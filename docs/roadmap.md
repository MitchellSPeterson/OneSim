# Roadmap

| Milestone | Status |
|---|---|
| 0 Repository bootstrap (workspace, CLI, logging, errors, protocol types, CI) | done |
| 1 Rust rendering prototype (`cargo run -p simcore-render --example counter [--features window -- --window]`) | done |
| 2 SwiftUI compatibility foundation (`import SwiftUI` unchanged) | done |
| 3 Stateful SwiftUI rendering (counter increments through Rust) | done |
| 4 Headless sessions + API (`simcore serve`) | done |
| 5 DemoKit integration | not started: PRD section truncated, needs spec |

Next SwiftUI surface: `@Binding`, `@ObservedObject`/`@StateObject`, `ScrollView`, `List`, `TextField` (needs keyboard events), `Toggle`, `NavigationStack`, sheets/alerts, animations, environment values.

Later: Jetpack Compose engine (own layout, Kotlin runtime out of process), React Native engine (Hermes + Fabric, Yoga layout), WASM execution backend (Wasmtime), binary wire encoding, native module registry (implemented / simulated / unsupported).
