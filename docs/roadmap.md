# Roadmap

| Milestone | Status |
|---|---|
| 0 Repository bootstrap (workspace, CLI, logging, errors, protocol types, CI) | done |
| 1 Rust rendering prototype (`cargo run -p simcore-render --example counter [--features window -- --window]`) | done |
| 2 SwiftUI compatibility foundation (`import SwiftUI` unchanged) | done |
| 3 Stateful SwiftUI rendering (counter increments through Rust) | done |
| 4 Headless sessions + API (`simcore serve`) | done |
| WASM execution backend: counter runs in the browser, 0.0% pixel diff vs native (`scripts/build-web.sh`) | prototype |
| 5 DemoKit integration | not started: PRD section truncated, needs spec |
| v0.2 M0 Expo/RN spike: unmodified Expo app from Metro, React Native Fabric in a Node guest, Yoga-style layout (taffy), tap → state update (`simcore expo`) | done |
| v0.2 M1 core components: TextInput, ScrollView/FlatList events, Image, Networking, Switch, Modal, nested Text; 10 iOS reference states (`tests/fixtures/rn-showcase-ios`) | in progress: 9/10 pass; `text` 2.63% (Apple) / 4.44% (fallback) — glyph rasterization on dense text |
| v0.2 M2 Fast Refresh: Metro HMR + React Refresh in the guest, state preserved, syntax/render error report + recovery, full-reload fallback, `reload_session` API; median edit→tree 126 ms (`run-refresh.mjs`) | done (no redbox UI; render errors reset state) |
| v0.2 M3 Expo Router: unmodified create-expo-app template (NativeTabs, Reanimated 4/worklets, expo-image/symbols/glass) + native Stack app (push/pop, params, header back); 9 iOS reference states pass both modes | done (no transition animations, header subviews, large-title collapse) |
| v0.2 M4 native-module registry + JSON-RPC session API + events | not started |

Next SwiftUI surface: `@Binding`, `@ObservedObject`/`@StateObject`, `ScrollView`, `List`, `TextField` (needs keyboard events), `Toggle`, `NavigationStack`, sheets/alerts, animations, environment values.

Later: Jetpack Compose engine (own layout, Kotlin runtime out of process), React Native engine (Hermes + Fabric, Yoga layout), WASM execution backend (Wasmtime), binary wire encoding, native module registry (implemented / simulated / unsupported).
