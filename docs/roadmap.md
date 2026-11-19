[OneSim](../README.md) / [Documentation](README.md)

# Roadmap

For current support by API and platform, see the [compatibility tables](compatibility/README.md).

## Milestones

| Milestone | Status |
|---|---|
| 0 Repository bootstrap (workspace, CLI, logging, errors, protocol types, CI) | done |
| 1 Rust rendering prototype (`cargo run -p onesim-render --example counter [--features window -- --window]`) | done |
| 2 SwiftUI compatibility foundation (`import SwiftUI` unchanged) | done |
| 3 Stateful SwiftUI rendering (counter increments through Rust) | done |
| 4 Headless sessions + API (`onesim serve`) | done |
| WASM execution backend: counter runs in the browser, 0.0% pixel diff vs native (`scripts/build-web.sh`) | prototype |
| 5 DemoKit integration | not started: PRD section truncated, needs spec |
| v0.2 M0 Expo/RN spike: unmodified Expo app from Metro, React Native Fabric in a Node guest, Yoga-style layout (taffy), tap → state update (`onesim expo`) | done |
| v0.2 M1 core components: TextInput, ScrollView and FlatList events, Image, networking, Switch, Modal, and nested Text | In progress. Dense text exceeds the fallback pixel-diff limit. See [known gaps](ios-comparison.md#known-gaps). |
| v0.2 M2 Fast Refresh: Metro HMR + React Refresh in the guest, state preserved, syntax/render error report + recovery, full-reload fallback, `reload_session` API; median edit→tree 126 ms (`run-refresh.mjs`) | done (no redbox UI; render errors reset state) |
| v0.2 M3 Expo Router: unmodified create-expo-app template (NativeTabs, Reanimated 4/worklets, expo-image/symbols/glass) + native Stack app (push/pop, params, header back); 9 iOS reference states pass both modes | done (no transition animations, header subviews, large-title collapse) |
| v0.2 M4 native-module registry (implemented/simulated/unsupported, PRD diagnostics; clipboard, haptics, location, permissions simulated), JSON-RPC 2.0 over WebSocket (`onesim serve --listen`, `docs/api.md`), per-session ordered events with replay, TypeScript SDK (`sdk/typescript`), DemoKit-style example (`examples/demokit-integration`) | done |
| Compose C0: unmodified Compose app (Activity + `setContent`) on the real Compose runtime, own `ui`/`foundation`/`material3`, Compose measure/place in the host, Pixel 10a device with Android status bar and gesture handle (`onesim run examples/compose-counter`) | done (Android 17 emulator pixel gate: `scripts/compare-android.sh`) |

The React Native showcase’s dense text state is the remaining visual fidelity gap for v0.2. The Expo guest runs on Node and emits `navigation.changed` for Expo Router. Push and pop animations, redbox UI, and landscape remain unsupported.

SwiftUI since v0.1: presentations (alerts, sheets, dialogs, covers), value navigation, environment objects, lifecycle, `onChange`, `onTapGesture`, `@AppStorage`, lazy stacks, `Form`, `Picker`, `Slider`, `ProgressView`, `SecureField`, `LabeledContent`, `TabView`/`Tab`/`.badge` ([compatibility](compatibility/swiftui/README.md)).

## Next framework work

### SwiftUI

Add `.task` with async runtime wakeups, and `@Observable` and `@Bindable` through a macro plugin. Other planned APIs include `.toolbar`, `ToolbarItem`, `@FocusState`, `LazyVGrid`, `Grid`, `Image("asset")`, `.searchable`, `.swipeActions`, `.refreshable`, and animations.

### Jetpack Compose

Add `Icon` with Material Symbols, `TextField`, `LazyColumn` and scrolling, `AlertDialog`, and navigation-compose.

### Longer term

The roadmap also includes expanding the Expo Android preview, a React Native engine using Hermes, a Wasmtime execution backend, and binary wire encoding.

## Apple frameworks beyond SwiftUI

OneSim's SwiftUI is a reimplementation of SwiftUI in Swift. We will extend the same approach to the frameworks SwiftUI apps reach for, so their previews run unchanged: Charts, SpriteKit, SceneKit, RealityKit, ARKit, MapKit, StoreKit, MusicKit, PencilKit, WidgetKit, UIKit, CoreGraphics, Metal and Vision. None of these are started yet; each gets its own milestone, compatibility notes and iOS reference states like the SwiftUI work.
