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
| `create_session` | `{"framework":"swiftui","platform":"ios","app_path":"<file or dir>","root_view":"CounterView"?,"viewport":{"width":393,"height":852}?,"scale":2.0?,"appearance":"light"\|"dark"?}` | `{"session_id":"s1"}` |
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
- `{"type":"environment","appearance":"light"|"dark","viewport":{"width","height"}}` — guest re-evaluates and sends a new tree.
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

`create_session` accepts `"device": "iphone-17-pro"` (default). Profile: viewport 402×874 pt, scale 3, safe area top 62 / bottom 34. Content is laid out inside the safe area. The screen background, a `navigation` bar background and a `list` background extend to the screen edges. The host draws an iOS status bar (time "9:41", battery) in the top inset. `viewport` and `scale` params still override.

The guest reads `SIMCORE_APPEARANCE` at launch for `@Environment(\.colorScheme)`.

---

## E. v1 additions: SF Symbols, keyboard, Dynamic Island, shadows (additive)

### New ViewNode kinds

| kind | fields |
|---|---|
| `image` | `"systemName": String`, `"resizable": bool`, `"contentMode"?: "fit"\|"fill"`, `"scale"?: "small"\|"medium"\|"large"` (imageScale). A non-resizable image sizes from the inherited font (point size and weight) and its scale, like a glyph. A resizable image is greedy, or keeps its aspect ratio when `contentMode` is set. It is tinted with the inherited foreground color. |
| `label` | `"title": ViewNode`, `"icon": ViewNode`. iOS `Label`: icon then title. In a `list` row the icon sits in a fixed-width leading column and is tinted with the accent color unless a foregroundColor is set. The title stays primary. |

New modifier: `{"imageScale": "small"|"medium"|"large"}` (inherited). `Font` also supports `.weight(_:)` on text styles, emitted as `{"font": {"style": "footnote", "weight": "semibold"}}`.

Symbol rendering is a **host native-API capability**. On macOS hosts, symbols are rasterized from the system's SF Symbols, and nothing Apple-licensed is shipped in the repo. Other hosts draw a placeholder and report `unsupported_symbol` in `get_errors`. `get_capabilities` lists `"symbols": "system"|"placeholder"`.

### Software keyboard (host chrome)

When a `text_field` is focused, the host draws the iOS keyboard for the device profile (light or dark). The keyboard rises from the bottom, so the bottom safe-area inset becomes the keyboard height (keyboard avoidance), and focused content stays above it. Key taps are ordinary pointer clicks on the keyboard:
- Letter, space and backspace keys edit the text through `text_input`.
- Return is `enter`: it unfocuses and the keyboard hides.
- Shift toggles case. Auto-capitalization turns shift on at the start of the text and after ". ".

`send_keyboard_event` keeps working and bypasses the keys. `inspect_ui_tree` includes the keyboard as `kind: "keyboard"` with a child per key (`role: "key"`, `label`). The CLI `--click q` matches a key label when the keyboard is up.

### Dynamic Island and shadows

- Device profiles have `"island": {"x","y","width","height"}` (pt); the iPhone 17 Pro has one. The host draws it as a black capsule above everything, matching the iOS Simulator screenshots.
- `shadow` is a true Gaussian blur, matching CALayer shadow rendering for SwiftUI's `shadow(radius:)` (radius → blur). It is computed offscreen per node and cached.
