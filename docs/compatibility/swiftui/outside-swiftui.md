# Outside SwiftUI

Part of [SwiftUI API coverage](README.md): status legend, summary and how to add a row. Hand-maintained (not generated).

APIs SwiftUI apps use from other modules (Combine, Observation, UIKit), reimplemented in OneSim's `SwiftUI` module.

| API | Since | iOS | Issue | Notes |
|---|---|---|---|---|
| `ObservableObject` | 13 | 🟠 | | `objectWillChange` is a stub |
| `Published` | 13 | ✅ | | Any set or mutation marks the view dirty |
| `Observable` (macro) | 17 | ✅ | [#47](https://github.com/MitchellSPeterson/OneSim/issues/47) | The real Observation module (re-exported); a property read during evaluation re-renders the tree after the event that changes it, like `@State` |
| `Transferable` | 16 | 🟠 | [#45](https://github.com/MitchellSPeterson/OneSim/issues/45) | CoreTransferable, declared in OneSim's SwiftUI module: `TransferRepresentation` with its builder, `ProxyRepresentation`, `CodableRepresentation` (JSON), `DataRepresentation` and `FileRepresentation` (async closures run to completion), `exportingCondition`; `String`, `URL`, `Data`, `Image` conform. Items move as text: data that is not UTF-8 (images, binary files) has no text form |
| `UTType` | 14 | 🟠 | [#45](https://github.com/MitchellSPeterson/OneSim/issues/45) | UniformTypeIdentifiers' own type where the platform has it; on Linux and WASI guests OneSim declares the common types (`plainText`, `url`, `json`, `data`, `image`, …) with conformance |
| `NSItemProvider` | 11 | 🟠 | [#45](https://github.com/MitchellSPeterson/OneSim/issues/45) | Foundation's where it exists; on Linux and WASI guests a text-only provider (`loadObject(ofClass: String.self)`, `loadItem`) |
| `UIKeyboardType` | 2 | 🟠 | [#41](https://github.com/MitchellSPeterson/OneSim/issues/41) | OneSim declares this UIKit type in its SwiftUI module. See `.keyboardType` for rendered layouts. |
| `UITextAutocapitalizationType` | 2 | ✅ | [#41](https://github.com/MitchellSPeterson/OneSim/issues/41) | OneSim declares this UIKit type in its SwiftUI module. |
| `UITextContentType` | 10 | 🟠 | [#41](https://github.com/MitchellSPeterson/OneSim/issues/41) | OneSim declares this UIKit type in its SwiftUI module. Content hints do not trigger autofill. |
| `UIApplication` | 2 | 🟠 | [#49](https://github.com/MitchellSPeterson/OneSim/issues/49) | `shared`, `applicationState`, `delegate` (SwiftUI's forwarding delegate, as on iOS), the lifecycle notifications, `LaunchOptionsKey`, `OpenURLOptionsKey`; not on the web build |
| `UIApplicationDelegate` | 2 | 🟠 | [#49](https://github.com/MitchellSPeterson/OneSim/issues/49) | Launch and termination run (scene-based app: the active/background callbacks never do, as on iOS); no scene configuration; not on the web build |
