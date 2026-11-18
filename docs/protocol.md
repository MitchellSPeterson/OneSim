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
| `create_session` | `{"framework":"swiftui","platform":"ios","app_path":"<file or dir>","root_view":"CounterView"?,"viewport":{"width":393,"height":852}?,"scale":2.0?,"appearance":"light"\|"dark"?,"dynamic_type_size":"large"?}` | `{"session_id":"s1"}` |
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

`create_session` accepts a `"device"` profile name (default `"iphone-17-pro"`: viewport 402×874 pt, scale 3, safe area top 62 / bottom 34). Every iPhone of the iOS 26.5 Simulator is a profile (`iphone-16`, `iphone-air`, `iphone-13-mini`, `iphone-se-3`, ...; the full list is `DEVICES` / `get_capabilities` `devices`); notch models have no `island`, the SE has a home button (classic status bar, no bottom inset, bottom-row keyboard icons). Lists, cards and titles use a 20 pt side margin on screens 414 pt or wider, else 16. Content is laid out inside the safe area. The screen background, a `navigation` bar background and a `list` background extend to the screen edges. The host draws an iOS status bar (time "9:41", battery) in the top inset. `viewport` and `scale` params still override. Profiles (geometry, safe area, island, status bar, keyboard height) live in `DEVICES` in `crates/simcore-render/src/device.rs`, shared with the Wasm build, and are listed by `get_capabilities` `devices`.

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

- Device profiles have `"island": {"x","y","width","height"}` (pt); the iPhone 17 Pro has one. The host draws it as a black capsule above everything, matching the iOS Simulator screenshots.
- `shadow` is a true Gaussian blur, matching CALayer shadow rendering for SwiftUI's `shadow(radius:)` (radius → blur). It is computed offscreen per node and cached.

---

## F. v1 additions: Liquid Glass and Dynamic Type (additive)

### Dynamic Type

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
