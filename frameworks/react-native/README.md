# React Native / Expo (v0.2, Milestone 0)

SimCore runs a real, unmodified Expo app's JavaScript from Metro. React and React Native's
own Fabric renderer (bundled in the app) do reconciliation; SimCore implements the *native*
side that RN's JavaScript expects on iOS with the new architecture (bridgeless).

```
Metro ──bundle──▶ guest/host.js (Node worker)
                    RN JS + ReactFabric ──▶ nativeFabricUIManager (shim) ──▶ tree JSON ──▶ simcore-host
                    TurboModules / expo.modules (shims)                       ◀── touch / layout / environment
```

Run it:

```
cd examples/expo-counter && npm install && CI=1 npx expo start --port 8081 &
simcore expo --url http://localhost:8081 --screenshot out.png
node frameworks/react-native/guest/run-standalone.mjs   # guest-only check: tap increments the counter
```

## Decisions

- **Engine: Node (V8) in a separate process, not Hermes yet.** PRD §4.3 allows an external
  process. Node is already on every dev machine running Metro, needs no native build, and
  runs Metro's dev bundles unchanged. Hermes stays the target for parity (bytecode, engine
  quirks); swapping it in only replaces the process behind wire B. QuickJS: no advantage
  over Node here and a weaker debugger story.
- **Fabric host interface, not the legacy bridge.** `nativeFabricUIManager` is a small,
  stable surface (create/clone/append/completeRoot/registerEventHandler/measure). React owns
  identity and lifecycle; the guest forwards host nodes keyed by `reactTag`.
- **Layout in Rust (`simcore-render/src/layout/rn.rs`, taffy)**, with Yoga defaults set per
  node, so headless, native and WASM render paths share it.
- **The bundle runs in a plain worker global**, not `node:vm`: RN's lazy global polyfills
  break inside a vm context. Reload = new worker.
- **Unsupported native modules fail loudly**: `SimCore: <Module> is not supported (method: m)`
  on stderr plus one `error` message per module. Nothing reports fake success.

Wire contract: `docs/protocol.md` §G.

## Shimmed native surface

Real behavior: `RN$Bridgeless`, `RN$registerCallableModule`, Fabric UIManager core, timers /
rAF / microtasks, PlatformConstants (iOS 26.5), DeviceInfo/Dimensions (from the session
viewport), Appearance, SourceCode, I18nManager, AppState, SettingsManager, ExceptionsManager
(→ `error`), AccessibilityInfo (all off), NativeDOMCxx (from host frames), WebSocketModule,
`expo` JSI classes (EventEmitter, SharedObject, SharedRef, NativeModule), ExponentConstants
(the Metro manifest).

Stubs: StatusBarManager, KeyboardObserver, DevSettings, DevMenu, LogBox, RedBox, Timing,
RNCSafeAreaContext (iPhone 17 Pro insets 62/34), ExpoKeepAwake.

Also real (Milestone 1): Networking (`fetch`, XMLHttpRequest, `expo/fetch`; HTTP runs on the
guest main thread because RN's polyfills break Node `fetch` inside the worker), minimal
BlobModule / FileReaderModule, ImageLoader (shared download cache), TextInput, ScrollView and
Switch events, Modal `onShow`.

Unsupported (diagnostic): Linking.openURL, Alert, ActionSheet, Clipboard, Share, Vibration,
push notifications, layout animations, cookies (`clearCookies` returns false).

## Known gaps (next milestones)

- **Keyboard avoidance**: no `keyboardWillShow` events, so KeyboardAvoidingView does not move.
  Tapping outside a TextInput does not blur it (same as RN without `Keyboard.dismiss`).
- **Image**: PNG/JPEG draw; GIF/WebP load (size, events) but do not draw. `tintColor`, `repeat` ignored.
- **Fast Refresh** works (Metro `/hot` + React Refresh, on by default, `SIMCORE_HMR=0` off;
  check: `node frameworks/react-native/guest/run-refresh.mjs`). No redbox UI: errors go to
  `get_errors`. A render error unmounts the root, so recovery remounts with fresh state.
  A dropped `/hot` socket is not retried.
- **Animations**: NativeAnimated absent → JS-driven, one tree per frame.
- Trees are sent whole on every commit; incremental mutations are a later wire change.
