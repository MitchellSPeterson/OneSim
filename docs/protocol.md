# SimCore Protocol (v1)

Three boundaries. All JSON is UTF-8; "JSON lines" = one JSON object per `\n`-terminated line.

```
client ──(A: external API, JSON lines over `simcore serve` stdin/stdout)──▶ simcore host
host   ──(B: guest wire, JSON lines over child process pipes)──▶ app process (guest)
guest  ──(C: C ABI, libsimcore_runtime.a)──▶ Swift compatibility module
```

The host owns layout, rendering, hit testing, screenshots and UI inspection.
The guest (one OS process per session) owns app code execution and state.

---

## A. External API (client ↔ host)

Request: `{"id": <u64>, "method": "<name>", "params": {...}}`
Response: `{"id": <u64>, "ok": true, "result": {...}}` or
`{"id": <u64>, "ok": false, "error": {"code": "<code>", "message": "..."}}`

Error codes: `bad_request`, `not_found` (unknown session), `invalid_state`,
`unsupported` (capability error), `build_failed`, `runtime_error`, `timeout`,
`protocol_version`.

First request MUST be `hello`: params `{"protocol_version": 1}` → result
`{"protocol_version": 1, "server": "simcore", "version": "<crate version>"}`.
Mismatched major version → `protocol_version` error.

| method | params | result |
|---|---|---|
| `get_capabilities` | – | `{"frameworks":["swiftui"],"platforms":["ios"],"commands":[...],"headless":true}` |
| `create_session` | `{"framework":"swiftui","platform":"ios","app_path":"<file or dir>","root_view":"CounterView"?,"viewport":{"width":393,"height":852}?,"scale":2.0?,"appearance":"light"\|"dark"?,"dynamic_type_size":"large"?,"device":"iphone-18-pro"?,"os_version":"27.2"?}` | `{"session_id":"s1"}` |
| `start_session` | `{"session_id"}` | `{"state":"running"}` (blocks until first tree or failure) |
| `stop_session` | `{"session_id"}` | `{"state":"stopped"}` |
| `get_session_status` | `{"session_id"}` | `{"session_id","state","framework","platform","app_path","viewport","scale","appearance","tree_version"}` |
| `set_viewport` | `{"session_id","width","height"}` | `{}` |
| `set_appearance` | `{"session_id","mode":"light"\|"dark"}` | `{}` |
| `send_pointer_event` | `{"session_id","event":{"kind":"down"\|"up"\|"click","x","y"}}` | `{"hit": <node id or null>, "action": <bool>}` |
| `send_keyboard_event` | `{"session_id","event":{...}}` | `unsupported` error in v1 |
| `capture_screenshot` | `{"session_id","path":"out.png"?}` | `{"width","height","path"}` or `{"width","height","png_base64"}` |
| `inspect_ui_tree` | `{"session_id"}` | `{"tree_version", "root": UiNode}` |
| `get_errors` | `{"session_id"}` | `{"errors":[{"message","source"}]}` |
| `shutdown` | – | `{}` then host stops all sessions and exits |

Coordinates are in points (logical). Screenshots are `viewport * scale` pixels.

Session states: `created → starting → running → stopping → stopped`, plus
`failed` (build/launch error) and `crashed` (guest exited unexpectedly).
`click` = down+up at the same point. After an action, `send_pointer_event`
returns only once the guest has submitted the next tree (or 2 s timeout), so
a following `capture_screenshot` reflects the update.

`UiNode`: `{"id","kind","frame":{"x","y","width","height"},"text"?,"role"?,"label"?,"action"?,"children":[...]}`
(`role`: `"button"`, `"text"`, `"image"`, `"none"`).

---

## B. Guest wire (host ↔ app process)

Host launches the app executable with env:
`SIMCORE_SESSION_ID`, `SIMCORE_VIEWPORT=393x852`, `SIMCORE_APPEARANCE=light|dark`.

The guest runtime (Rust staticlib) on start `dup()`s fd 1 for the protocol and
then `dup2(2, 1)`, so app `print()` output goes to stderr and never corrupts the
wire. Host reads guest stderr into the session's log/errors.

Guest → host:
- `{"type":"hello","protocol":1}`
- `{"type":"tree","version":<u64>,"root":<ViewNode>}`
- `{"type":"error","message":"..."}`

Host → guest (guest stdin):
- `{"type":"action","id":<u64>,"version":<u64>}` — `version` = tree version the
  action id came from. Guest ignores actions whose version is not current.
- `{"type":"environment","appearance":"light"|"dark","viewport":{"width","height"},"dynamicTypeSize"?:"large"}` — guest re-evaluates and sends a new tree. `dynamicTypeSize` is omitted at the default (`large`); the guest also gets `SIMCORE_DYNAMIC_TYPE` at launch. The host does the text sizing (below); the guest may later expose it as `@Environment(\.dynamicTypeSize)`.
- `{"type":"shutdown"}` — guest exits 0.

### ViewNode (normalized view tree, produced by Swift, consumed by Rust)

Every node: `{"kind": "...", "id": "<stable structural path>", "modifiers": [Modifier...]?, ...kind fields}`

| kind | fields |
|---|---|
| `text` | `"text": "Count: 1"` |
| `vstack` | `"spacing": f64?` (null = default 8), `"alignment": "leading"\|"center"\|"trailing"`, `"children": [...]` |
| `hstack` | `"spacing": f64?`, `"alignment": "top"\|"center"\|"bottom"`, `"children": [...]` |
| `zstack` | `"alignment": "center"\|"topLeading"\|"top"\|"topTrailing"\|"leading"\|"trailing"\|"bottomLeading"\|"bottom"\|"bottomTrailing"`, `"children"` |
| `spacer` | `"minLength": f64?` |
| `button` | `"action": <u64>`, `"label": <ViewNode>` |
| `color` | `"color": Color` (greedy fill) |
| `rectangle` | – (greedy, filled with current foreground color, default primary) |
| `empty` | – (EmptyView, zero size) |

Color: `{"r":0..1,"g":0..1,"b":0..1,"a":0..1}` or `{"semantic":"primary"|"secondary"|"accent"|"background"}` (host resolves per appearance).

Modifiers are applied in array order, innermost first (same as Swift source order):

| modifier | shape |
|---|---|
| padding | `{"padding":{"top","leading","bottom","trailing"}}` (Swift default edges = 16) |
| frame | `{"frame":{"width"?,"height"?,"minWidth"?,"maxWidth"?,"minHeight"?,"maxHeight"?,"alignment":"center"...}}` (`maxWidth: .infinity` → `1e308`... use JSON `"inf"` string) |
| foregroundColor | `{"foregroundColor": Color}` (inherited by descendants) |
| background | `{"background": ViewNode}` (sized to the modified view) |
| cornerRadius | `{"cornerRadius": f64}` (clips) |
| opacity | `{"opacity": f64}` |

Infinite values are encoded as the string `"inf"`.

Default style (host): system font 17pt, primary = black/white, accent = #007AFF
(light) / #0A84FF (dark), background = white / black. Default button style
renders its label in accent color. Root view is proposed the full viewport and
centered in it.

---

## C. C ABI (`crates/simcore-runtime`, header `simcore_runtime.h`)

```c
#include <stdint.h>
#include <stddef.h>

// Called on the thread that is inside simcore_runtime_run().
typedef void (*simcore_action_cb)(void *ctx, uint64_t action_id);
typedef void (*simcore_env_cb)(void *ctx, const char *appearance, double width, double height);

// Must be called once, first. Sets up the protocol fd and sends hello.
// Returns 0 on success, nonzero on failure (see simcore_last_error).
int32_t simcore_runtime_init(void);

// Register callbacks. ctx is passed back verbatim; it must stay valid until run returns.
void simcore_runtime_set_callbacks(void *ctx, simcore_action_cb on_action, simcore_env_cb on_env);

// Send a view tree. Rust copies `json` before returning; caller keeps ownership.
// `version` must increase monotonically. Returns 0 on success.
int32_t simcore_submit_tree(const uint8_t *json, size_t len, uint64_t version);

// Report a non-fatal error to the host. Rust copies `msg`.
void simcore_report_error(const char *msg);

// Blocking event loop: reads host messages, invokes callbacks on this thread,
// returns 0 after `shutdown` or stdin EOF.
int32_t simcore_runtime_run(void);

// Thread-local last error; pointer valid until the next simcore_* call on this thread. Do not free.
const char *simcore_last_error(void);
```

All Swift values cross as bytes/C strings; no Swift objects or pointers to
temporaries are retained by Rust. Single-threaded: all calls from one thread.

---

## D. v1 additions: showcase API surface (additive, protocol version stays 1)

Hosts ignore unknown modifier keys. Guests never send kinds a host doesn't list in `get_capabilities`.

### New ViewNode kinds

| kind | fields |
|---|---|
| `shape` | `"shape": "rectangle"\|"circle"\|"capsule"\|"roundedRectangle"`, `"cornerRadius"?`, `"fill"?: Color`, `"stroke"?: {"color": Color, "lineWidth"}`. Greedy like `rectangle`. Neither fill nor stroke means fill with the current foreground color. (`rectangle` kind stays accepted.) |
| `divider` | – (hairline, 1px at device scale, separator color; spans the cross axis of the enclosing stack) |
| `scroll` | `"axis": "vertical"\|"horizontal"`, `"child": ViewNode`. Child is proposed unbounded on the axis. Host keeps the scroll offset per node id across trees; it changes via `scroll` pointer events. Clips. |
| `list` | `"style": "insetGrouped"\|"plain"`, `"sections": [{"header": String?, "footer": String?, "rows": [ViewNode]}]`. Rows outside any `Section` form their own headerless section. `ForEach`/`if` inside lists are already flattened into rows by the guest. Scrolls like `scroll` (vertical). |
| `navigation` | `"title": String?`, `"displayMode": "large"\|"inline"`, `"back": {"action": u64, "label": String}?`, `"content": ViewNode`. The top of the stack only. The guest owns the push/pop path. |
| `toggle` | `"isOn": bool`, `"action": u64` (flips it), `"label": ViewNode`, `"disabled"?: bool` |
| `stepper` | `"label": ViewNode`, `"decrement": u64?`, `"increment": u64?` (null = at bound, rendered disabled) |
| `text_field` | `"placeholder": String`, `"text": String`, `"field": u64` |

### `button` additions

`"style": "automatic"|"plain"|"bordered"|"borderedProminent"|"navigationLink"` (default automatic) and `"disabled": bool`. A `navigationLink` button is a row with a trailing chevron. Its label is primary-colored, not accent.

### New modifiers

| modifier | shape | notes |
|---|---|---|
| font | `{"font": {"style": "largeTitle"\|"title"\|"title2"\|"title3"\|"headline"\|"subheadline"\|"body"\|"callout"\|"footnote"\|"caption"\|"caption2"}}` or `{"font": {"size": f64, "weight": Weight}}` | inherited |
| fontWeight | `{"fontWeight": Weight}` (`bold()` → `"bold"`) | inherited; Weight = `ultraLight`…`black` |
| strikethrough | `{"strikethrough": bool}` | inherited |
| multilineTextAlignment | `{"multilineTextAlignment": "leading"\|"center"\|"trailing"}` | inherited |
| foregroundColor | also used for `foregroundStyle` | Semantic adds `"tertiary"`, `"separator"`, `"groupedBackground"`, `"secondaryGroupedBackground"` |
| overlay | `{"overlay": {"view": ViewNode, "alignment": Alignment}}` | proposed the modified view's size, drawn above |
| shadow | `{"shadow": {"color": Color, "radius", "x", "y"}}` | SwiftUI default color is black at 0.33 opacity |
| navigationTitle | `{"navigationTitle": String}` | informational; the guest also lifts it into `navigation.title` |

Text styles and weights follow Apple's iOS Dynamic Type "Large" defaults (body 17, largeTitle 34, title 28, title2 22, title3 20, headline 17 semibold, subheadline 15, callout 16, footnote 13, caption 12, caption2 11).

### Input additions

- Pointer event kind `"scroll"`: `{"kind":"scroll","x","y","dx","dy"}` in points. Scrolls the innermost scroll or list under the point, clamped.
- Tapping a `text_field` focuses it. The host tracks focus by node id. Tapping elsewhere unfocuses.
- `send_keyboard_event` is now supported: `{"kind":"text","text":"abc"}` inserts at the end, and `{"kind":"key","key":"backspace"|"enter"}` edits or ends editing (enter unfocuses). It returns `unsupported` only for unknown keys, and `invalid_state` when nothing is focused.
- Guest wire, host → guest: `{"type":"text_input","field": u64, "version": u64, "text": "<full new text>"}`. The guest sets the bound String, re-evaluates, and sends a tree. It is version-gated like `action`.

### Device profile and platform chrome (host)

`create_session` accepts a `"device"` profile name (default `"iphone-18-pro"`: viewport 402×874 pt, scale 3, safe area top 62 / bottom 34). Every iPhone of the iOS 26.5 and 27.2 Simulators is a profile (iOS 27 status bar and keyboard height per profile where measured, `Device::ios27`) (`iphone-16`, `iphone-air`, `iphone-13-mini`, `iphone-se-3`, ...; the full list is `DEVICES` / `get_capabilities` `devices`); notch models have no `island`, the SE has a home button (classic status bar, no bottom inset, bottom-row keyboard icons). Lists, cards and titles use a 20 pt side margin on screens 414 pt or wider, else 16. Content is laid out inside the safe area. The screen background, a `navigation` bar background and a `list` background extend to the screen edges. The host draws an iOS status bar (time "9:41", battery) in the top inset. `viewport` and `scale` params still override. Profiles (geometry, safe area, island, status bar, keyboard height) live in `DEVICES` in `crates/simcore-render/src/device.rs`, shared with the Wasm build, and are listed by `get_capabilities` `devices`.

The guest reads `SIMCORE_APPEARANCE` at launch for `@Environment(\.colorScheme)`.

---

## E. v1 additions: SF Symbols, keyboard, Dynamic Island, shadows (additive)

### New ViewNode kinds

| kind | fields |
|---|---|
| `image` | `"systemName": String`, `"resizable": bool`, `"contentMode"?: "fit"\|"fill"`, `"scale"?: "small"\|"medium"\|"large"` (imageScale). A non-resizable image sizes from the inherited font (point size and weight) and its scale, like a glyph. A resizable image is greedy, or keeps its aspect ratio when `contentMode` is set. It is tinted with the inherited foreground color. |
| `label` | `"title": ViewNode`, `"icon": ViewNode`. iOS `Label`: icon then title. In a `list` row the icon sits in a fixed-width leading column and is tinted with the accent color unless a foregroundColor is set. The title stays primary. |

New modifier: `{"imageScale": "small"|"medium"|"large"}` (inherited). `Font` also supports `.weight(_:)` on text styles, emitted as `{"font": {"style": "footnote", "weight": "semibold"}}`.

Symbol rendering is a **host native-API capability**. On macOS hosts, symbols are rasterized from the system's SF Symbols, and nothing Apple-licensed is shipped in the repo. `get_capabilities` lists `"symbols": "system"|"bundled"|"placeholder"` and `"fonts": "system"|"bundled"`.

**Fallback chain (open-licensed; nothing Apple-licensed is bundled).** When Apple's resources are unavailable (non-macOS host) or `SIMCORE_FALLBACK=1` is set in the host process's environment (so macOS CI can test this path), the host uses:
- Fonts `"bundled"`: **Inter** (`assets/fonts/InterVariable.ttf`, SIL OFL 1.1, `assets/fonts/OFL.txt`), embedded in `simcore-render`, variable `wght` axis, with per-size letter-spacing (`inter_tracking_em`) fitted to the iOS/SF widths in `tests/fixtures/showcase-ios`. The SF path (`/System/Library/Fonts/SFNS.ttf`) is unchanged.
- Symbols `"bundled"`: **Phosphor Icons** (`assets/symbols/phosphor/{regular,bold,fill}`, MIT, `LICENSE`), rendered with resvg into the same alpha-mask pipeline as AppKit symbols. `crates/simcore-render/src/symbols_map.rs` maps SF Symbol name to Phosphor icon (~270 names). A `.fill` suffix selects the fill style, weight semibold or heavier the bold style, otherwise regular; `.circle` variants map to Phosphor's `*-circle` icons where they exist. The layout box approximates AppKit's (ink = icon units x 1.25 x pt / 256 x imageScale, plus a margin), so it differs from AppKit per glyph by a few percent.
- A name missing from the table (or any name when `"symbols"` is `"placeholder"`) draws the placeholder below and reports `unsupported_symbol`.

`Color` rgba gains an optional `"dark": [r, g, b]` (0..1): in dark appearance the host uses it instead of `r,g,b` (alpha unchanged), so adaptive system colors (`.red`, `.pink`, `.green`, `.blue`, ...) follow iOS 26 in dark mode.

Symbol sizing (matches iOS 26 frames, `tests/fixtures/showcase-ios/symbols.json`): the layout box of a non-resizable image is the symbol's
natural size at (inherited font size, weight, imageScale), i.e. AppKit's `NSImage.size` for `NSImage.SymbolConfiguration(pointSize:weight:scale:)`
(17pt regular star.fill = 22x20 box; iOS reports the 18x17.6 ink inside it). imageScale small/medium/large changes the glyph (~0.78/1.0/1.29x),
not the font. A resizable image takes the proposal; with `contentMode` it keeps the symbol's box aspect ratio (`fit` shrinks to fit,
`fill` occupies the whole proposal and crops). The glyph is rasterized at device scale into an alpha mask, tinted with the foreground color
and cached by (name, size, weight, scale, device scale, target size). In a list row, `label` puts the icon (imageScale large) centered in a
24pt leading column and starts the title 16pt later (title at row content x + 40); the row separator aligns with the title.
Rendering is monochrome only (multicolor, hierarchical and palette modes are out of scope). For a name with no mapping the
host draws a rounded-square outline placeholder and records one `get_errors` entry per name, `{"source":"host","message":"unsupported_symbol: ..."}`.

### Software keyboard (host chrome)

When a `text_field` is focused, the host draws the iOS keyboard for the device profile (light or dark). The keyboard rises from the bottom, so the bottom safe-area inset becomes the keyboard height (keyboard avoidance), and focused content stays above it. Key taps are ordinary pointer clicks on the keyboard:
- Letter, space and backspace keys edit the text through `text_input`.
- Return is `enter`: it unfocuses and the keyboard hides.
- Shift toggles case. Auto-capitalization turns shift on at the start of the text and after ". ".

`send_keyboard_event` keeps working and bypasses the keys. `inspect_ui_tree` includes the keyboard as `kind: "keyboard"` with a child per key (`role: "key"`, `label`). The CLI `--click q` matches a key label when the keyboard is up.

### Dynamic Island and shadows

- Device profiles have `"island": {"x","y","width","height"}` (pt); the iPhone 17 and 18 Pro have one (the 18 Pro's is 95 pt wide). The host draws it as a black capsule above everything, matching the iOS Simulator screenshots.
- `shadow` is a true Gaussian blur, matching CALayer shadow rendering for SwiftUI's `shadow(radius:)` (radius → blur). It is computed offscreen per node and cached.

---

## F. v1 additions: Liquid Glass and Dynamic Type (additive)

### Dynamic Type

`create_session` accepts `"os_version"` (alias `osVersion`): the simulated iOS release, `"27.2"` (default) or `"26.5"` (`"27"` / `"26"` also parse). It selects the release's look in the host (list row padding, keyboard, Liquid Glass, status bar, scroll edge, alert colors: `crates/simcore-render/src/look.rs` and per-release branches beside the code) and is passed to guests as `SIMCORE_OS_VERSION` / `SIMCORE_OS_BUILD` (React Native reports it as `Platform.Version`, `expo-device` `osVersion`), with the profile slug as `SIMCORE_DEVICE`. `status` reports `os_version`. CLI: `simcore run|expo --ios 26.5`; web: `simcore_web_set_ios(index)` (0 = 26.5, 1 = 27.2). Each release is pinned to the Simulator runtime its references were captured on (`docs/compatibility/versions.json`).

`create_session` accepts `"dynamic_type_size"` (alias `dynamicTypeSize`): `xSmall|small|medium|large|xLarge|xxLarge|xxxLarge|accessibility1..accessibility5` (default `large`). CLI: `simcore run --dynamic-type SIZE`; web: `?dynamicType=SIZE` / `sim.init({dynamicType})` / `sim.setDynamicType(size)`.
- The host resolves the point size of every text *style* (`.largeTitle` ... `.caption2`, and unstyled text = `.body`) from Apple's Dynamic Type table (iOS, 12 sizes x 11 styles; `TextStyle::size_at`). `.system(size:)` is fixed. Nav bar titles, list headers, symbols and control sizes do not scale in v1.
- Above 34pt the host uses leading `1.12 x size + 2.6` (measured: 53pt body -> 62pt lines) and per-size tracking fitted to the fixtures.
- Verified against `typography-scrolled.json`: that fixture was captured at `accessibility-extra-extra-extra-large`, which is `accessibility5` (60/58/53/43pt for large title / title / body / caption), not accessibility3.

### Modifier

`{"glass": {"variant": "regular"|"clear"|"identity", "tint"?: Color, "interactive"?: bool, "shape": GlassShape}}` comes from `.glassEffect(_ glass: Glass = .regular, in shape: some Shape = .capsule)`.
- `GlassShape` is `{"shape": "capsule"|"circle"|"rect"|"roundedRectangle"|"ellipse", "cornerRadius"?: f64}`. The default is capsule. The Swift side emits `rect` for `.rect(cornerRadius:)` / `RoundedRectangle`; `circle`, `capsule` and `ellipse` all render as a full-radius pill (a non-square `circle`/`ellipse` is a capsule in v1).
- The glass is drawn behind the modified view's content and sized to it. `identity` draws nothing.
- `GlassEffectContainer` children are emitted as normal nodes. Merging and morphing between glass shapes is out of scope in v1. `interactive` is carried but has no visual effect (the simulator's press state is not distinguishable in fixtures).

### Button styles

`button.style` adds `"glass"` and `"glassProminent"`.
- glass: capsule-shaped regular glass (12/7 pt padding like bordered), label in primary.
- glassProminent: capsule regular glass tinted with the accent color, label in white.

### Rendering (host)

Liquid Glass is a **backdrop effect**: a glass node samples the pixels already rendered beneath it (scene nodes are in paint order), inside its shape. In order, the host:
1. blurs the backdrop (Gaussian, sigma 3pt; 1pt for `clear`) with edge replication;
2. applies a lens in a ~3pt rim band: samples are pulled `30pt * (1 - d/3pt)^2` from further inside, where `d` is the depth from the outline; the interior is undistorted (fixtures show a flat interior);
3. transforms color per variant (below);
4. mixes in the tint (opaque: tint color shifted by -17/255 in light and +8/255 in dark, scaled by the tint alpha);
5. adds a 1.2px specular rim (white mix up to 50%, favouring the top-left/bottom-right) and a soft outer shadow (black 0.13, sigma 12pt, dy +5pt, light only).

| variant | light | dark |
|---|---|---|
| regular | `0.43 * raw + 0.575 * 255` | `0.57 * raw + 17` |
| clear | `raw + 24` (sat 1.03) | `raw + 24` |

Everything is deterministic CPU code (no threads or SIMD), so it also runs in the Wasm build. The snapshot is taken before the node's own shadow is painted.

System chrome uses the same effect:
- The navigation back button is a regular glass circle (44pt).
- **Scroll-edge effect.** Scrollers under a navigation bar span the whole screen with the bar as a top inset, so content scrolls under the bar and status bar. Once scrolled (offset > 0) the host veils content toward the background color: alpha 0.42 down to the bar's bottom edge, then 0.27@+4pt, 0.16@+6, 0.095@+12, 0.04@+20, 0.016@+28, 0@+32 (piecewise linear), plus a 3/255 darkening (light only) fading out by bar+12. There is no blur. Content under the bars is not tappable; scrolling over the status bar does nothing.
- The keyboard panel is not glass in the fixtures and is unchanged.

---

## G. v1 additions: Text runs, button roles, presentation layers (additive)

- `text` may carry `"runs": [{"text", "font"?: Font, "fontWeight"?: Weight, "foregroundColor"?: Color, "italic"?: bool, "underline"?: bool, "strikethrough"?: bool}]` (from `Text + Text`, or a lone `Text` with `.italic()` / `.underline()`); `"text"` is the plain concatenation (labels, inspection). Unset fields inherit the environment. Runs lay out and wrap as one paragraph; line metrics come from the largest run. Italic is the font's oblique (synthesized when the face has none); underline is drawn 2pt below the baseline, 1pt thick, in the run's color, spanning the run's glyphs. A lone styled `Text` without italic/underline still uses plain modifiers.
- `button` gains `"role"?: "cancel"|"destructive"`.
- New kind `presentation`: `{"kind":"presentation","id","content": ViewNode, "layers": [Layer]}`. The guest emits it as the root only while something is presented (otherwise the tree is unchanged); `content` is the normal tree and `layers` run bottom to top. A layer presented from inside another layer's content comes after it. `Layer` is one of:

| layer | fields |
|---|---|
| `sheet` | `"content"`, `"detents"?: [Detent]`, `"dragIndicator"?: bool`, `"dismiss"?: u64` |
| `cover` | `"content"` (`.fullScreenCover`) |
| `alert` | `"title"`, `"message"?: ViewNode`, `"actions": [button ViewNode]` |
| `dialog` | `"title"`, `"titleVisible"`, `"message"?`, `"actions"`, `"anchor"?: node id`, `"dismiss"?: u64` (`.confirmationDialog`) |

  `Detent` is `{"kind":"medium"|"large"}`, `{"kind":"fraction","value"}` (of the large detent's height) or `{"kind":"height","value"}` (points); any order, the host sorts by height and starts at the smallest. Empty = large only. `dismiss` is a guest action that clears the binding (the guest then runs `onDismiss`); a sheet without it is `interactiveDismissDisabled`. `presentationDetents` / `presentationDragIndicator` / `interactiveDismissDisabled` are lifted off the sheet content's top node by the guest and never appear as modifiers. `actions` are buttons in source order; the host orders them. A dialog gets a Cancel button appended when the app gave none; its `dismiss` is that button's action id (tapping outside the popover cancels), and `anchor` is the id of the view the `.confirmationDialog` modifier is attached to. Tapping any alert/dialog button runs its action id and the guest clears the binding. The guest evaluates presented content under `<path>/sheet`, `<path>/cover` and `<path>/alert`, so its `@State` lives only while shown; `onDismiss` fires through the lifecycle machinery when the layer stops being evaluated (dismiss(), a swipe, or app code clearing the binding). A host that receives `presentation` nested below the root ignores the layer.
- Layer ids are `layer<i>` (index in `layers`). The host keeps a detent and a drag position per sheet layer id, like scroll offsets, in `Interaction::sheets`, and passes them to layout as `SceneParams::sheets`.
- Pointer event kind `"drag"`: `{"kind":"drag","x","y","dx","dy"}` is one whole finger gesture from (x, y) by (dx, dy) points (CLI `--drag X,Y,DX,DY`; the web test harness too). A touch UI uses `Interaction::drag` (begin/move/end) directly. A drag inside a sheet card moves the sheet unless the sheet is at its largest detent over content that can still scroll that way (then the content scrolls). On release the sheet settles at the nearest detent (after projecting 0.15 s of finger velocity), or, dragged below the dismiss line (25% of the way from the smallest detent to the screen bottom), runs its `dismiss` action. Sheets cannot be dragged above their largest detent. No animation: settled states only.
- Host rendering (`SHEET_*`, `ALERT_*`, `POPOVER_*` constants at the top of `crates/simcore-render/src/layout/swiftui.rs`, all measured on the iOS 26.5 simulator):
  - A black scrim (0.20 light / 0.46 dark) dims `content`; a `scrim` node marks the modal boundary: only nodes of the topmost layer can be tapped or scrolled. Prominent tints in what lies under a layer desaturate to gray.
  - **Sheet**: at its largest detent a full-width card from the top safe inset (62pt; 10pt lower per sheet under it, which also share a single dim) to the bottom, top radius 40, grouped background (elevated in dark). Below its largest detent it floats: inset 8pt at the sides and bottom, top radius 36, Liquid Glass fill, list backgrounds transparent over it. Its content is the large-sheet layout (402 wide) scaled by 0.96 about the card's top center (rows 355 x 49.9, text 0.96x), as the simulator reports. A sheet up over a screen flattens that screen's back-button glass to a barely visible disc; alerts keep it. The optional drag indicator is 34.7x5 at 4.67pt below the top.
  - **Cover**: opaque system background edge to edge (no scrim), content placed like the root.
  - **Alert**: centered glass card (320 wide), left-aligned title and message, capsule buttons (two side by side with cancel first; more stacked with cancel last).
  - **Dialog**: popover card 240 wide pointing at the anchor's label (the notch tip touches the label's end; the card starts 12.5pt later): to its right when there is room, else above, else below. No dim; capsule buttons stacked; no Cancel button shown.
- `PROTOCOL_VERSION` stays 1.

---

## H. v1 additions: React Native / Expo (additive)

Guest: `node frameworks/react-native/guest/host.js` (see `frameworks/react-native/README.md`).
It loads the app bundle from Metro and runs React Native's own Fabric renderer; no build step.

### API

- `create_session` accepts `"framework": "react-native"` (alias `"expo"`), `"platform": "ios"`
  (default) or `"android"`, and `"metro_url"` (alias `metroUrl`, default `http://localhost:8081`).
  The guest loads its app from Metro. Android defaults to the `pixel-10a` profile and reports
  `os_version: "17"`. Devices must match the platform selected at session creation.
- `get_capabilities.frameworks` includes `"react-native"`.
- `send_pointer_event` on an RN session: `down` → touch `start` on the deepest node at the
  point, `up` → touch `end` on the same target, `click` = both. After `end` the call waits for
  the next tree (2 s); a tap that changes nothing is not an error. `drag` → touch `start` at
  (x, y), 8 `move`s along (dx, dy) (timestamps 16 ms apart), `end`, all on the start's target.
- CLI: `simcore expo [--platform ios|android] [--url URL] [--device NAME] [--headless] [--appearance light|dark] [--screenshot out.png] [--click X,Y ...]`.

### Guest launch env

`SIMCORE_METRO_URL`, `SIMCORE_PLATFORM` (`ios` or `android`), `SIMCORE_VIEWPORT`, `SIMCORE_APPEARANCE`,
`SIMCORE_SESSION_ID`, `SIMCORE_SCALE`, and `SIMCORE_INSETS=top,bottom` (profile insets used on Android).
Optional: `SIMCORE_HMR=1` (connect Metro `/hot`), `SIMCORE_DEBUG=1`.
The guest loads `launchAsset.url` from the Expo manifest at `GET <metro>/` with `expo-platform` set
to the session platform. Plain Metro uses `/index.bundle?platform=<platform>&dev=true&minify=false`.

### ViewNode kind `rn`

`{"kind":"rn","id":"<reactTag>","component":"<Fabric viewName>","props":{...},"children":[...]}`

- The root is synthetic: `component: "RootView"`, `id: "1"` (surface rootTag 1), fills the viewport.
- `props` is Fabric's flat attribute payload merged across clones: style keys are top level,
  colors are `processColor` numbers (`0xAARRGGBB`), functions are dropped, event handlers
  appear as `true` flags (`onClick`, `onStartShouldSetResponder`, ...).
- Components: `RCTView`, `RCTText` (paragraph; text = concatenated `RCTRawText.props.text`),
  `RCTVirtualText` (nested span), `RCTRawText`, `RCTScrollView` (`horizontal` prop),
  `RCTImageView` (`source: [{uri,width,height,scale}]`, placeholder for now). Other names are
  laid out as a View and reported once as `unsupported_component: <name>`.
- Layout follows Yoga (column, `flexShrink: 0`, `alignContent: flex-start`, border-box);
  default text: system 14pt, black. Scene node ids are reactTags, so hit tests return tags.

Android uses density-independent pixels and Roboto. Its Fabric names include `AndroidTextInput`,
`AndroidSwitch` (`on`, `enabled`, `trackTintColor`, `thumbTintColor`), `AndroidProgressBar`,
`AndroidHorizontalScrollView` with `AndroidHorizontalScrollContentView`, and
`AndroidSwipeRefreshLayout` wrapping a scroller. The guest responds to existing focus, text input,
scroll, and touch messages for these nodes. A switch tap dispatches `topChange` with `value`.
The Android status bar and navigation handle are drawn over the edge-to-edge surface.
`__simcoreStatusBarStyle` (`default`, `dark-content`, or `light-content`) and
`__simcoreStatusBarHidden` (boolean) are optional Android `RootView` props from `StatusBarManager`.
Android dispatches `keyboardDidShow` and `keyboardDidHide` as simulated focus notifications.

### Host → guest (additions)

- `{"type":"touch","phase":"start"|"move"|"end"|"cancel","tag":u64,"x","y","lx","ly","timestamp"}`
  — page coords (pt), `lx/ly` relative to the target. Guest dispatches
  `topTouchStart/Move/End/Cancel`; RN's responder system bubbles from the target.
- `{"type":"layout","frames":{"<tag>":[x,y,w,h]}}` — page frames after each layout. Answers
  `measure*` and fires `onLayout`. Optional: guest works without it (zeros).
- `{"type":"reload"}` — full reload (fresh JS global).
- `environment` → Appearance `appearanceChanged`, Dimensions `didUpdateDimensions`.

Unsupported native modules: stderr `SimCore: <Module> is not supported (method: <m>)` plus one
`{"type":"error"}` per module (surfaced by `get_errors`). Nothing reports fake success.

### Milestone 1 additions (TextInput, ScrollView, Image, Switch, Modal)

**TextInput** (`RCTSinglelineTextInputView`, `RCTMultilineTextInputView`). The guest owns the
current text per tag and emits it as `props.__simcoreText` (string; empty when none), next to
`placeholder`, `placeholderTextColor`, `editable`, `secureTextEntry`, `multiline`. The host
treats these nodes as text fields (`field` = tag, value = `__simcoreText`) and drives them with
its existing focus + software-keyboard machinery.
- Host → guest `{"type":"focus","tag"}` / `{"type":"blur","tag"}` when host focus changes
  → `topFocus` / `topBlur` (+ `topEndEditing` on blur).
- Host → guest `text_input` with `field` = tag and the full new `text` → guest updates its text,
  dispatches `topChange` `{text, eventCount, target}` (and `topSelectionChange` at the end of
  the text), and emits a new tree.
- Host → guest `{"type":"keyboard","height":<pt>}` when the software keyboard appears (height =
  the device keyboard as drawn, `kb_base_h + safe_bottom`) or hides (`0`). Sent after `focus` / `blur`
  only when visibility changes (moving focus between fields keeps it up), including `hide_keyboard`
  and return. Guest emits `KeyboardObserver` events `keyboardWillChangeFrame`, `keyboardWill{Show,Hide}`,
  `keyboardDidChangeFrame`, `keyboardDid{Show,Hide}` with `{startCoordinates, endCoordinates:
  {screenX, screenY, width, height}, duration: 0, easing: "keyboard", isEventFromThisApp: true}`
  (window coordinates; hidden frame = `screenY` at the window bottom). `duration: 0` because the
  host renders settled states (`KeyboardAvoidingView` skips LayoutAnimation).
- Host → guest `{"type":"submit","tag"}` on return → `topSubmitEditing` `{text}`.
  `submitBehavior: "submit"` retains focus and `"blurAndSubmit"` blurs. With no explicit behavior,
  single-line inputs blur and multiline inputs retain focus.
- Guest → host `{"type":"focus_request","tag":<u64>|null}` when JS calls the `focus` / `blur`
  commands (null = blur); host focuses/unfocuses that field and shows/hides the keyboard.
  `setTextAndSelection` / `setNativeProps({text})` update the guest's text and re-emit.

**ScrollView** (`RCTScrollView`). Host keeps the offset.
- Host → guest `{"type":"scroll","tag","phase":"begin"|"scroll"|"end","x","y","contentWidth","contentHeight","width","height"}`
  (x/y = content offset) → `topScrollBeginDrag` / `topScroll` / `topScrollEndDrag` +
  `topMomentumScrollEnd`, nativeEvent `{contentOffset,contentSize,layoutMeasurement,contentInset:{0,0,0,0},zoomScale:1}`.
  One `--scroll` gesture = begin, scroll, end.
  A vertical scroll pulling down past the top (offset already 0) is reported like iOS rubber-banding:
  begin at 0, `scroll` with the negative overscroll `y`, end at 0. Pulled 100 pt or more, the guest fires
  `topRefresh` on the scroll view's `PullToRefreshView` child (RN `RefreshControl`) unless it is already refreshing.
- Guest → host `{"type":"scroll_to","tag","x","y"}` for the `scrollTo` / `scrollToEnd` commands
  (`scrollToEnd`: `x`/`y` = `"end"`); host sets the offset (clamped), re-renders and replies
  with a `scroll` message (phase `end`).
- `contentOffset` prop sets the initial offset.

**Image** (`RCTImageView`). The guest downloads `source[0].uri` (http(s), `data:`, Metro
assets) with Node `fetch` into `$TMPDIR/simcore-images/<sha1(uri)>.<ext>` and adds
`props.__simcoreImage = {"path","width","height"}` (pixels) to the node, dispatching
`topLoadStart`, `topLoad` `{source:{uri,width,height}}`, `topLoadEnd` (or `topError`
`{error}`). Host decodes PNG/JPEG from `path` and draws with `resizeMode`
`cover` (default) / `contain` / `stretch` / `center`, clipped to `borderRadius`; until
`__simcoreImage` exists it draws only the background. `ImageLoader.getSize`/`prefetch` use the
same cache.

**Switch** (`RCTSwitch`, props `value`, `onTintColor`, `thumbTintColor`, `disabled`). Host draws
the iOS 26 switch (same look and size as SwiftUI `Toggle`: 63×28, knob 37×24). The guest handles the tap: a touch `end`
whose target is an `RCTSwitch` (not disabled) dispatches `topChange` `{value: !value, target}`.

**Modal** (`RCTModalHostView`; RN's Fabric name `ModalHostView` is renamed by the guest; props `visible`, `animationType`, `transparent`,
`presentationStyle`). Its children are laid out against the full viewport and painted above
everything else (transparent → no backdrop; otherwise background color of the app default).
`visible: false` or absent node → nothing. Guest dispatches `topShow` after first emit with it
visible; `topRequestClose` is not generated (no hardware back on iOS).

**ActivityIndicator** (`ActivityIndicatorView`, props `animating`, `hidesWhenStopped`, `color`, `size`; size from style:
20×20 small, 36×36 large). Host draws the iOS spinner's 8 spokes at rest (a static frame); stopped + `hidesWhenStopped` → nothing.

**RefreshControl** (`PullToRefreshView`, iOS: first child of its `RCTScrollView`; props `refreshing`, `tintColor`).
Outside the content flow. While `refreshing` the scroll view opens a band above the content (extra top inset) with a spinner
centred in it. Pull-to-refresh: see ScrollView overscroll.

**SafeAreaView** (core, `SafeAreaView`). After a first layout pass its padding becomes the part of the safe-area insets its
frame overlaps (replacing its own padding), then the tree is laid out again.

**expo-linear-gradient** (`ViewManagerAdapter_ExpoLinearGradient`, props `colors` (processed ints), `locations`,
`startPoint` / `endPoint` `[x, y]` in the unit box, default top centre → bottom centre). Drawn like CAGradientLayer.

**expo-blur** (`ViewManagerAdapter_ExpoBlur_ExpoBlurView`, props `tint`, `intensity` 0...100). A flat material: backdrop
blur plus a colour transform per tint, scaled by intensity (no Liquid Glass lens/rim).

### Milestone 2 additions (Fast Refresh, reload)

- API `reload_session` `{"session_id"}` → `{"state"}`: full JS reload (fresh JS global, React
  state lost); returns after the reloaded app's first tree (30 s timeout → `timeout`).
- The guest connects Metro's `/hot` socket by default (`SIMCORE_HMR=0` disables). Edits are
  applied by Metro HMR + React Refresh inside the guest; the host just receives new trees.
  Updates that React Refresh cannot apply trigger a full reload in the guest.
- Guest → host `{"type":"event","name":"bundle.loaded"|"bundle.reloaded"|"bundle.error"|"refresh.applied","detail":{...}}`.
  The host currently only logs these (client event stream: Milestone 4). Bundle and runtime
  errors are also sent as `error` (→ `get_errors`).

### Milestone 3 additions (Expo Router: native stack, native tabs, safe area)

Native components (react-native-screens, safe-area-context, expo views) arrive as ordinary `rn`
nodes; the host draws the platform chrome.

- `RNSScreenStack` > `RNSScreen`* (last child = top) > `RNSScreenContentWrapper` (content) +
  `RNSScreenStackHeaderConfig` (header props: `title`, `hidden`, `largeTitle`, `hideBackButton`,
  `backTitle` (absent → previous screen's title), `backTitleVisible`, `color`, `backgroundColor`,
  `titleColor`, `titleFontSize`, `titleFontWeight`, `translucent`; children
  `RNSScreenStackHeaderSubview` `type` left/right/center/title/back/searchBar). Host draws the
  top screen and an iOS 26 navigation bar (same look as SwiftUI `NavigationStack`).
- `RNSTabsHostIOS` (`navStateRequest.selectedScreenKey`, `tabBarHidden`, `tabBarTintColor`) >
  `RNSTabsScreenIOS` (`screenKey`, `title`, `badgeValue`, `iconType` image/sfSymbol/xcasset,
  `iconResourceName`, guest-added `__simcoreIconImage` / `__simcoreSelectedIconImage`
  `{path,width,height}`). Host draws the selected tab's content and the iOS 26 floating Liquid
  Glass tab bar.
- `RNCSafeAreaProvider`, `RNCSafeAreaView` (`edges` `{top,left,bottom,right}`: `additive`|`off`
  → add the safe-area inset as padding), `RNSSafeAreaView`.
- `ViewManagerAdapter_ExpoImage` (`source`, `contentFit`, `tintColor`, guest-added
  `__simcoreImage`), `ViewManagerAdapter_SymbolModule` (`name`, `weight`, `scale`, `tintColor` as
  CSS string, size from style), `ViewManagerAdapter_ExpoGlassEffect_GlassView`
  (`glassEffectStyle`, `tintColor`, `isInteractive`, radii), `…_GlassContainer` (`spacing`).
- Root `props.backgroundColor`: `expo-system-ui` background.
- Safe-area insets come from the viewport height (874 → 62/34, 852 → 59/34, 844 → 47/34,
  812 → 44/34, else 20/0); the guest sends them to `RNCSafeAreaProvider` (`topInsetsChange`).

Host → guest:
- `{"type":"select_tab","tag":<RNSTabsHostIOS tag>,"key":"<screenKey>"}` (or `"index":n`) when a
  tab bar item is tapped. The guest fires `topTabSelected`; JS then updates
  `navStateRequest.selectedScreenKey`, which the host follows.
- `{"type":"native_back","tag":<RNSScreenStack tag>}` when the header back button is tapped. The
  guest fires the iOS pop events (`topDismissed` …); JS removes the screen on its next commit.

JS-driven push/pop needs no message. The guest spaces appear events by 350 ms
(`SIMCORE_NAV_MS`, 0 = instant); the host does not animate transitions yet.

### Milestone 4 additions (modules, simulation, v2 API)

- Guest → host `{"type":"modules","modules":[{name,package,status,methods,events,permissions}]}`
  after every (re)load; host → guest `{"type":"simulate","module","state"}`. Event lines add
  `navigation.changed` (`{route,params,stack}`, Expo Router pathnames), `module.unsupported`,
  `module.called`.
- The v2 JSON-RPC/WebSocket API (`simcore serve --listen ADDR`) is specified in `docs/api.md`.

### `Alert.alert` (AlertManager)

- While alerts are up the guest emits the root as a section G `presentation`: `content` is the normal `rn` root and
  `layers` holds one `alert` layer per open alert (oldest first): `title`, `message` (a `text` node, when given) and one
  `button` per RN button (`role` `cancel` / `destructive` from the button `style`; no buttons = one "OK"). The host lays
  out `content` as RN and draws the layers exactly as SwiftUI's `.alert` (scrim, card, button order).
- A tap on an alert button is sent as `{"type":"action","id","version"}` (as for SwiftUI). The guest drops that alert,
  re-emits the tree and calls the `alertWithArgs` callback with the button key (Alert.js runs that button's `onPress`).
- `Alert.prompt` (`type` other than `default`) is unsupported (diagnostic).

### react-native-gesture-handler (issue #10)

- No wire change: the guest recognizes Tap / Pan / LongPress / native-button gestures itself from
  `touch` start/move/end/cancel (one finger, RNGH thresholds: pan `minDist` 10 pt or its
  `activeOffset*`/`failOffset*`, tap `maxDuration` 500 ms, long press `minDuration` 500 ms). When a
  gesture activates, RN's responder gets `topTouchCancel` and the rest of that touch skips RN.
- `RNGestureHandlerButton` is laid out as a View; `RNGestureHandlerDetector` (`display: contents`)
  is not emitted: its children are emitted in its place (any `display: contents` view is).

## I. v1 additions: form controls (additive, protocol version stays 1)

- `text_field` gains `"secure"?: bool` (`SecureField`): the host draws one bullet per character; `text` is still the real value.
- New kind `picker`: `{"kind":"picker","id","label": ViewNode,"style":"menu"|"segmented","options":[{"text","action"}],"selected": index|null,"open"?: u64,"disabled"}`. `options` come from the content children carrying `.tag(_:)` (their first text); `selected` indexes the option whose tag equals the selection (null when none does). Running an option's `action` selects it. Menu style (also `.automatic`) is a row: label leading, selected text and `chevron.up.chevron.down` trailing, accent tinted; `open` toggles the menu. Segmented style is a capsule segmented control spanning the proposed width.
- New layer `menu` (in `presentation.layers`, section G): `{"kind":"menu","anchor": picker id,"items":[{"text","action","checked"?: bool}],"dismiss": u64}`, present while a menu picker is open. The host draws a glass card at the anchor; an item's action selects and closes, `dismiss` (tap outside) only closes.
- New kind `slider`: `{"kind":"slider","id","value","min","max","step"?,"field"?: u64,"disabled"}`. The host sets the value by sending `text_input` (section B) to `field` with the number as decimal text; the guest clamps it to `min...max` and snaps it to `step`. A tap on the slider or a `drag` pointer gesture that starts on it sends the value under the point where the finger ends (thumb centre travels between half a thumb width from each end). Disabled sliders have no `field`.
- New kind `progress`: `{"kind":"progress","id","value": fraction 0...1 | null,"label"?: ViewNode}` (`ProgressView`). With a value: a linear capsule bar (label above it). Null: the activity indicator, drawn as a static frame (label below it).
- `LabeledContent` and `Form` add no wire kinds: they are an `hstack` (label, spacer, content with `foregroundColor` secondary) and an inset-grouped `list`.
- Inspection: the picker row is a `button` labelled with the picker label (its `value` is the selected text), segments and menu items are `button`s labelled with their text, a slider has role `slider` and its value as `value`.

## J. v1 additions: tab views (additive, protocol version stays 1)

- New kind `tabview` (`TabView`): `{"kind":"tabview","id","selected": index,"tabs":[{"title","systemImage"?: SF symbol,"badge"?: string,"action": u64}],"content": ViewNode}`. Only the selected tab's content is sent. Running a tab's `action` (the ordinary `action` message, section B) selects it; no new host-to-guest message. Guest paths: tab `i` lives at `<tabview>/t/<i>` and the guest keeps the hidden tabs' state alive, so `@State` and a `NavigationStack`'s path persist across switches.
- Host: the content is placed like a root view with a bottom safe-area inset of 83 pt (scrollers and navigation stacks run under the bar, other content is centred above it), then the iOS 26 floating Liquid Glass tab bar shared with Expo Router native tabs (section H, milestone 3): a capsule 20 pt above the bottom, 94 x 54 pt items at an 86 pt pitch, the selected one on a gray capsule with an accent `<symbol>.fill`, `badge` as a red 18 pt capsule at the icon's top right. Items are buttons `<tabview>#tab-<i>` labelled with the title.

## K. v1 additions: Jetpack Compose on Android (additive, protocol version stays 1)

Guest: a JVM process running the app's Kotlin code compiled with the Kotlin Compose compiler plugin against the
**real** `androidx.compose.runtime` (composition, snapshots, recomposition) and SimCore's own reimplementation of
`androidx.compose.ui` / `foundation` / `material3` / `activity-compose` (`frameworks/compose`). Material components
are built in Kotlin out of the ui primitives below, as in AndroidX, so the wire only carries **layout primitives
plus a modifier chain**; the host lays them out with Compose's constraint-based measure/place model
(`simcore-render::layout::compose`).

### API

- `create_session` accepts `"framework": "compose"` with `"platform": "android"` (default for compose), `app_path`
  = a `.kt` file or a directory of them. `root_view` names a top-level `@Composable` function with no parameters;
  without it the guest launches the app's `ComponentActivity` subclass (`onCreate` → `setContent { … }`).
- Default device for compose sessions: `pixel-10a` (Android).
- CLI: `simcore run <dir with .kt files> [--root-view Name]` picks compose automatically.

### Guest launch env

Section B env plus `SIMCORE_PLATFORM=android`, `SIMCORE_SCALE` (density), `SIMCORE_INSETS=top,bottom` (dp; status
bar and gesture navigation bar, which an edge-to-edge app draws under).

### ViewNode kind `compose`

`{"kind":"compose","id":"<slot path>","component":"Column"|"Row"|"Box"|"Text","props":{...},"children":[...]}`

Units: dp (= scene points), sp = dp (font scale 1). Colors: `[r,g,b,a]` floats 0...1 (resolved by the guest, so the
Material color scheme and dark theme are guest-side). Shapes: `{"corners":[tl,tr,br,bl]}` in dp, or
`{"percent":[tl,tr,br,bl]}` of the shorter side (`CircleShape` = 50%); absent = rectangle.

| component | props |
|---|---|
| `Box` | `contentAlignment`: `[h, v]` biases -1...1 (default `[-1,-1]` TopStart), `propagateMinConstraints`: bool |
| `Column` | `arrangement`: Arrangement, `alignment`: horizontal bias -1...1 (Start = -1) |
| `Row` | `arrangement`: Arrangement, `alignment`: vertical bias -1...1 (Top = -1) |
| `Text` | `text`, `color`, `fontSize`, `lineHeight` (sp), `fontWeight` (100...900), `letterSpacing` (sp), `textAlign`: `start`\|`center`\|`end`, `maxLines`, `softWrap` |

Arrangement: `{"type":"start"|"end"|"center"|"spaceBetween"|"spaceAround"|"spaceEvenly"}` or
`{"type":"spacedBy","space":dp,"align":bias}` (main axis; Column `start` = Top).

`props.modifier` is the Modifier chain in source order (first = outermost):

| entry | meaning (Compose) |
|---|---|
| `{"padding":[start,top,end,bottom]}` | `padding` |
| `{"width":dp}` / `{"height":dp}` | `width` / `height` (`size` = both), coerced into the incoming constraints |
| `{"widthIn":[min,max]}` / `{"heightIn":[min,max]}` | `widthIn` / `heightIn` (`null` = unspecified) |
| `{"requiredWidth":dp}` / `{"requiredHeight":dp}` | `requiredWidth` / `requiredHeight` |
| `{"defaultMinSize":[w,h]}` | `defaultMinSize` (`null` = unspecified) |
| `{"fillMaxWidth":f}` / `{"fillMaxHeight":f}` | `fillMaxWidth` / `fillMaxHeight` (`fillMaxSize` = both) |
| `{"wrapContent":[h,v]}` | `wrapContentSize(align)` (biases) |
| `{"minInteractive":dp}` | `minimumInteractiveComponentSize` (at least dp per side, content centred, may exceed the constraints) |
| `{"align":[h,v]}` / `{"align":bias}` | `BoxScope.align` / `RowScope`/`ColumnScope.align` (parent data) |
| `{"weight":w,"fill":bool}` | `RowScope`/`ColumnScope.weight` (parent data, not a layer) |
| `{"offset":[x,y]}` | `offset` |
| `{"background":{"color","shape"?}}` | `background` |
| `{"border":{"width","color","shape"?}}` | `border` |
| `{"clip":shape}` | `clip` |
| `{"shadow":{"elevation","shape"?}}` | `shadow` (Material tonal shadow, drawn as a soft drop shadow) |
| `{"alpha":f}` | `alpha` |
| `{"clickable":{"action":u64,"enabled":bool,"role"?:"button"\|"switch"\|"checkbox"\|"radio"\|"tab","label"?}}` | `clickable` / `toggleable` / `selectable` |

Host → guest: the section B `action` (tap on a `clickable`), `environment` (appearance → `isSystemInDarkTheme()`),
`shutdown`. The guest re-composes and sends a new `tree` after each, once the Recomposer is idle.

### Android chrome (host)

The host draws the Android status bar (clock "9:41" left, wifi and battery right, as in the emulator's demo mode) over
the top inset and the gesture navigation handle centred in the bottom inset; app content is laid out full screen
(edge-to-edge). Layout is in whole device pixels like Compose (`roundToPx` for dp values, text sizes rounded up), and
text is drawn at a whole-pixel font size (Android floors `sp × density`). Elevation draws Android's ambient + spot
shadow pair (the spot shadow shifts down with distance from the top of the screen).
