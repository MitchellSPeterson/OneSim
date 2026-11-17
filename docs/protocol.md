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
