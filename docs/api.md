[OneSim](../README.md) / [Documentation](README.md)

# JSON-RPC API v2

The API lets test runners, DemoKit, and other clients control a OneSim session over WebSocket. The [TypeScript SDK](../sdk/typescript/README.md) wraps this API. Both transports share the same host. For JSON lines over stdin and stdout, see the [v1 stdio API](protocol.md#a-external-api-client--host).

Start the WebSocket server from a checkout.

```sh
cargo run -p onesim-cli -- serve --listen 127.0.0.1:3030
```

Connect to `ws://127.0.0.1:3030/`. Each text frame carries one JSON-RPC 2.0 object. Clients share the host’s sessions, and the server sends events as JSON-RPC notifications.

> [!IMPORTANT]
> Keep the server bound to loopback. The API has no authentication.

## Handshake and versioning

First call on a connection MUST be `initialize`:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2.0","client":{"name":"demokit","version":"1.0"}}}
→ {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2.0","server":{"name":"onesim","version":"0.2.0"},
   "capabilities":{"frameworks":["swiftui","react-native"],"platforms":["ios"],"methods":[...],"events":[...],
                   "devices":[...names],"screenshotFormats":["png"],"fonts":"apple"|"fallback","symbols":"..."}}}
```

The server accepts any `2.x` and answers with its own version; another major → error `protocol_version`.
Any other method before `initialize` → error `invalid_state`.

## Errors

JSON-RPC errors use `code` -32602 (invalid params), -32601 (method not found), -32700 (parse),
-32600 (invalid request), or -32000 (OneSim error). `error.data.code` always carries the
OneSim code: `bad_request`, `not_found`, `invalid_state`, `unsupported`, `build_failed`,
`runtime_error`, `timeout`, `protocol_version`, `method_not_found`. `error.data` may add
`sessionId` and `details`.

## Sessions

A session is one running app: its own guest process, device configuration, state, and event
stream. Ids are strings (`"s1"`).

| method | params | result |
|---|---|---|
| `session.create` | `{"framework":"expo"\|"react-native"\|"swiftui"\|"compose","platform"?:"ios"\|"android","device"?,"osVersion"?:"27.2"\|"26.5","appearance"?,"dynamicTypeSize"?,"metroUrl"?,"appPath"?,"rootView"?,"viewport"?:{width,height},"scale"?,"load"?:false}` | `{"sessionId","status":"created"}`; with `"load": true` it also loads (like `runtime.load`) and returns `"status":"running"` |
| `session.list` | – | `{"sessions":[SessionInfo]}` |
| `session.get` | `{"sessionId"}` | `SessionInfo` |
| `session.destroy` | `{"sessionId"}` | `{}` (stops if running, frees the id) |

`SessionInfo`: `{"sessionId","framework","platform","device","viewport":{width,height},"scale","appearance","status","treeVersion","metroUrl"?,"appPath"?}`.
Expo uses iOS by default. Pass `"platform": "android"` for Expo on Android. SwiftUI targets iOS and Compose targets Android.
Device names accept a display name or profile slug. The defaults are `iphone-18-pro` on iOS and `pixel-10a` on Android.
`osVersion` selects iOS `"27.2"` or `"26.5"`, with `"27.2"` as the default. For Android, `session.get` reports `"17"`.

## Runtime lifecycle

Status values: `created`, `starting`, `running`, `paused`, `stopping`, `stopped`, `failed`, `crashed`.

| method | params | result |
|---|---|---|
| `runtime.load` | `{"sessionId","metroUrl"?,"appPath"?}` (overrides the create params) | `{"status":"running"}` after the first rendered tree |
| `runtime.reload` | `{"sessionId"}` | `{"status":"running"}` (React Native: full JS reload; SwiftUI: `unsupported`) |
| `runtime.pause` | `{"sessionId"}` | `{"status":"paused"}`: the guest process is suspended; input and reload fail with `invalid_state` until resumed; screenshots show the last frame |
| `runtime.resume` | `{"sessionId"}` | `{"status":"running"}` |
| `runtime.stop` | `{"sessionId"}` | `{"status":"stopped"}` |
| `runtime.status` | `{"sessionId"}` | `{"status","treeVersion","errors":[{"message","source"}]}` |

## Input

Targets are points `{"x","y"}` or a semantic selector `{"element": "<UiNode id>"}` /
`{"label": "<accessibility label>"}` (same resolution as the CLI `--click LABEL`). Every input
call returns after the app has settled (same rule as v1 `send_pointer_event`).

Live viewers: `input.tap`, `input.press`, `input.scroll`, `render.screenshot` and `render.frame` accept
`"settle": false`. React Native then skips every wait for the guest (the next tree, the quiet window:
150 ms to 2 s per call): touches return once sent, snapshots take one layout pass. Trees the guest
commits afterwards arrive as `render.updated`; fetch another frame then.
SwiftUI never waits, so the flag changes nothing there. Keep the default for deterministic screenshots.

| method | params | result |
|---|---|---|
| `input.tap` | target | `{"hit": <id or null>}` |
| `input.press` | target + `"durationMs"?` (default 500) | `{"hit"}` (down, wait, up) |
| `input.type` | `{"sessionId","text"}` (focused text field) | `{}` |
| `input.key` | `{"sessionId","key":"enter"\|"backspace"}` | `{}` |
| `input.scroll` | target + `{"dx","dy"}` points (positive dy reveals later content) | `{}` |
| `input.swipe` | `{"sessionId","from":{x,y},"to":{x,y},"durationMs"?}` | `{}` (v0.2: delivered as a scroll of `from - to` at `from`) |

All input params also carry `"sessionId"`.

## Rendering

| method | params | result |
|---|---|---|
| `render.screenshot` | `{"sessionId","format"?:"png","scale"?:number,"width"?,"height"?}` | `{"format":"png","width","height","data":"<base64>"}`; `width`/`height` resize the image (aspect kept if only one), `scale` overrides device pixels per point |
| `render.frame` | `{"sessionId"}` | `{"treeVersion","width","height","data"}` (screenshot + tree version, for streaming) |
| `render.backend` | `{"sessionId","renderer"?:"auto"\|"gpu"\|"cpu"}` | `{"backend":"cpu"\|"gpu","adapter","limitations":[...],"fallbackReason":string\|null,"frames","lastFrameMs":{"encode","gpu","readback"}}`; `renderer` switches this session's screenshot backend first (`auto` = try GPU, else CPU). See [rendering.md](rendering.md) |
| `render.tree` | `{"sessionId"}` | `{"treeVersion","root": UiNode}` |
| `render.inspect` | `{"sessionId"}` + `{"element"}` or `{"x","y"}` | `{"node": UiNode (with children), "path":[ids root→node]}` |

`UiNode`: `{"id","kind","role","label"?,"text"?,"value"?,"frame":{x,y,width,height},"action"?,"focused"?,"disabled"?,"accessibility"?,"children":[...]}`. `accessibility` is the extra semantics from [protocol section U](protocol.md).
`id` is stable for the life of the element (React Native: the reactTag; SwiftUI: the structural path).

## Device

| method | params | result |
|---|---|---|
| `device.list` | – | `{"devices":[{"name","displayName","viewport","scale","safeArea":{top,bottom}}]}` |
| `device.set` | `{"sessionId","device"}` | `SessionInfo` (viewport, scale and chrome change; the app gets a dimensions change) |
| `device.rotate` | `{"sessionId","orientation":"portrait"\|"landscape"}` | `unsupported` in v0.2 for anything but `portrait` |
| `device.appearance` | `{"sessionId","mode":"light"\|"dark"}` | `{}` |
| `device.keyboard` | `{"sessionId","visible":false}` | `{}` (hide = blur the focused field; `true` without a focused field → `invalid_state`) |

## Native modules (React Native)

| method | params | result |
|---|---|---|
| `modules.list` | `{"sessionId"}` | `{"modules":[{"name","package"?,"status":"implemented"\|"simulated"\|"unsupported","methods":[...],"events":[...],"permissions"?:[...]}]}` |
| `modules.simulate` | `{"sessionId","module","state":{...}}` | `{}`: set simulated platform state, e.g. `{"module":"location","state":{"latitude":37.33,"longitude":-122.03}}`, `{"module":"clipboard","state":{"text":"hi"}}`, `{"module":"permissions","state":{"location":"granted"\|"denied"\|"undetermined"}}`, `{"module":"securestore","state":{"token":"abc"}}` (seeds expo-secure-store; `null` deletes a key; not re-applied on reload) |

Unsupported calls print a diagnostic on the guest's stderr, e.g.

```
OneSim: ExpoCamera is not supported.

Module: expo-camera
Method: requestCameraPermissionsAsync
```

and reject (async) / throw (sync) with the first line as the message; one `runtime.error` and one
`module.unsupported` event per module + method. Simulated `simulate` state survives reloads.

Notes: `session.create` with `load: true` that fails keeps the session (`error.data.sessionId`).
`render.screenshot` with both `width` and `height` fits inside both, keeping the aspect ratio;
images over 8192 px are rejected (`bad_request`). `treeVersion` is 0 before the first tree.

## Events

Clients subscribe per session; the server pushes JSON-RPC notifications:

```json
{"jsonrpc":"2.0","method":"event","params":{"sessionId":"s1","seq":12,"name":"render.updated","time":1731900000123,"data":{"treeVersion":7}}}
```

| method | params | result |
|---|---|---|
| `events.subscribe` | `{"sessionId","names"?:[...],"since"?:<seq>}` | `{"subscribed":true,"lastSeq"}`; with `since`, buffered events with `seq > since` are replayed first, in order (buffer: last 1000 per session) |
| `events.unsubscribe` | `{"sessionId"}` | `{}` |

`seq` increases by 1 per event within a session; delivery order is `seq` order. A client that
reconnects calls `events.subscribe` with the last `seq` it saw.

| event | data |
|---|---|
| `runtime.ready` | `{"treeVersion"}` first tree after load or reload |
| `runtime.error` | `{"message","source"}` guest/host error (guest stderr logging is not an event; it stays in `runtime.status.errors`) |
| `runtime.stopped` | `{"reason":"stopped"\|"crashed"\|"failed"}` (`failed` = exited before its first tree) |
| `runtime.paused` / `runtime.resumed` | `{}` |
| `bundle.loaded` / `bundle.reloaded` | `{"url"?}` |
| `bundle.error` | `{"message"}` |
| `refresh.applied` | `{}` Fast Refresh update applied |
| `render.updated` | `{"treeVersion"}` coalesced: at most one per 16 ms |
| `navigation.changed` | `{"route","params"?,"stack"?:[routes]}` React Navigation / Expo Router focused route |
| `module.unsupported` | `{"module","method","package"?}` once per module+method |
| `module.called` | `{"module","method","args"}` simulated modules with observable side effects (haptics, clipboard writes, SwiftUI `openURL` with `module` `OpenURLAction`); `module` is the native module name (`ExpoHaptics`) |

## Guest wire additions (host ↔ React Native guest)

- Guest → host `{"type":"modules","modules":[...]}` once after boot (same shape as `modules.list`).
- Guest → host `{"type":"event","name":"navigation.changed"|"module.unsupported"|"module.called",...,"detail":{...}}` (existing event line).
- Host → guest `{"type":"simulate","module":"...","state":{...}}`.
