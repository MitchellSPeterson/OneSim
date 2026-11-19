[SimCore](../README.md) / [Documentation](README.md)

# Architecture

SimCore separates app execution from rendering. Each native session runs app code in a guest process. The Rust host receives the guest’s view tree, lays it out, and sends input back to the guest.

## A SwiftUI session

```text
client (DemoKit, CI, agent)
   │  A: JSON-lines API (`simcore serve`)            docs/protocol.md §A
   ▼
simcore-host ── sessions, build, process supervision
   │  uses simcore-render: layout (SwiftUI semantics) → Scene → tiny-skia raster, hit test, inspect
   │  B: JSON-lines guest wire over child stdin/stdout  §B
   ▼
app process (one per session)
   Swift app code  ──import SwiftUI──▶  frameworks/swiftui (our "SwiftUI" module)
                                          │ C: C ABI  §C
                                          ▼
                                       libsimcore_runtime.a (Rust)
```

The Swift compiler resolves `import SwiftUI` to [SimCore’s SwiftUI module](compatibility/swiftui/guest.md). The module evaluates views with Swift’s own generics, closures, and property wrappers. It submits a normalized view tree to the host through the Rust guest runtime. The host lays out the tree, rasterizes it with tiny-skia and cosmic-text, and routes actions back to Swift closures. See the [protocol reference](protocol.md) for the messages at each boundary.

## Framework guests

| Concern | Where | Why |
|---|---|---|
| Language semantics (generics, closures, property wrappers) | Swift module | Swift compiler already does this; Rust never interprets Swift. |
| State registry, view identity, action table | Swift module | State is tied to Swift values/closures. |
| Normalized view tree | Swift → JSON | Framework-specific but language-neutral. |
| Layout | `simcore-render::layout` | SwiftUI proposes sizes, React Native uses Yoga flexbox semantics via taffy, and Compose measures and places content under constraints. All three emit the same scene type. |
| Compose execution | [Compose guest](../frameworks/compose/README.md) (JVM) | The AndroidX Compose runtime composes the app. SimCore’s `ui`, `foundation`, and `material3` libraries reduce it to layout nodes and modifier chains. |
| React Native execution | [React Native guest](../frameworks/react-native/README.md) (Node.js) | The guest loads the Metro bundle unchanged and implements `nativeFabricUIManager` and TurboModules while Fabric reconciles the UI. |
| Scene, rasterization, hit testing, and inspection | `simcore-render` | The renderer handles all three frameworks. It draws nodes in paint order so Liquid Glass can sample the pixels already drawn behind it. |
| Sessions and the external API | `simcore-host` and `simcore-cli` | Each session has its own process. If a guest exits unexpectedly, the host marks that session `crashed`. |

## Design choices

- The CPU renderer uses tiny-skia and cosmic-text to produce deterministic screenshots for CI. The scene type can support another rendering backend.
- Native guests communicate over JSON lines. The [browser build](web.md) links the SwiftUI app, guest runtime, and renderer together and calls guest functions in-process.
- The SwiftUI guest re-evaluates the root view after each state change.

## Repository layout

| Directory | Responsibility |
| --- | --- |
| [`simcore-cli`](../crates/simcore-cli) | The `simcore` command, including `run`, `expo`, and `serve`. |
| [`simcore-host`](../crates/simcore-host) | Sessions, guest builds, and process supervision. |
| [`simcore-render`](../crates/simcore-render) | Layout, rasterization, hit testing, and inspection. |
| [`simcore-protocol`](../crates/simcore-protocol) | Shared wire and API types. |
| [`simcore-runtime`](../crates/simcore-runtime) | Rust guest static library and C ABI. |
| [`simcore-web`](../crates/simcore-web) | In-process runtime and renderer for WebAssembly. |
| [`frameworks`](../frameworks) | SwiftUI, React Native, and Compose guest implementations. |

## Fonts and symbols

On macOS, the renderer uses system SF Pro and SF Symbols. On other hosts, or when you set `SIMCORE_FALLBACK=1`, it uses bundled Inter and maps SF Symbol names to Phosphor Icons. The stdio API’s `get_capabilities` method reports the active `fonts` and `symbols` modes.

Inter and Roboto are licensed under the SIL Open Font License 1.1. Phosphor Icons are MIT-licensed. [Inter license](../assets/fonts/OFL.txt) · [Roboto license](../assets/fonts/Roboto-OFL.txt) · [Phosphor license](../assets/symbols/phosphor/LICENSE)
