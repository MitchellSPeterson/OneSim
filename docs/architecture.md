# Architecture

```
client (DemoKit, CI, agent)
   │  A: JSON-lines API (`simcore serve`)            docs/protocol.md §A
   ▼
simcore-host ── sessions, build, process supervision, input routing
   │  uses simcore-render: framework layout → Scene → render, hit test, inspect
   │  B: JSON-lines guest wire over child stdin/stdout
   ▼
app process (one per session)
   SwiftUI (swiftc + libsimcore_runtime.a C ABI), React Native (Node + Fabric), or Compose (JVM)

Scene ─────┬─ CpuRenderer → tiny-skia + cosmic-text → RGBA → PNG / softbuffer / Canvas 2D
           └─ Builder → DisplayList → Executor (Vello + wgpu) → storage texture
                                                                 ├─ GPU capture readback
                                                                 └─ native swap chain / WebGPU canvas
```

## Split of responsibility

| Concern | Where | Why |
|---|---|---|
| Language semantics (generics, closures, property wrappers) | Swift module | Swift compiler already does this; Rust never interprets Swift. |
| State registry, view identity, action table | Swift module | State is tied to Swift values/closures. |
| Normalized view tree | Framework guest → JSON | Framework-specific structure in a shared protocol. |
| Layout | `simcore-render::layout::{swiftui,rn,compose}` | SwiftUI uses proposals, React Native uses Yoga semantics via taffy, and Compose uses constraints and measure/place. All three emit the same Scene. |
| Compose execution | `frameworks/compose` (JVM) | The real `androidx.compose.runtime` composes the app; SimCore's `ui`/`foundation`/`material3` reduce it to Box/Column/Row/Text + Modifier chains (protocol §K), laid out by `layout::compose` with Compose's constraint rules. |
| React Native execution | `frameworks/react-native/guest` (Node) | Runs the Metro bundle unchanged; RN's own Fabric renderer reconciles, the guest implements `nativeFabricUIManager` + TurboModules. See `frameworks/react-native/README.md`. |
| Scene, CPU raster, hit testing, inspection | `simcore-render` | Shared scene and input semantics. CPU rendering provides the deterministic reference. |
| Display-list building | `simcore-render::Builder` | Resolves paint order, clips, transforms, and device coordinates. CPU-rasterized sprites carry shaped text and symbols to the GPU. Resource IDs allow reuse across frames. |
| GPU execution | `simcore-render-gpu::Executor` | Vello handles vector and image drawing. wgpu compute passes composite Liquid Glass, its halo and scroll edges against the accumulated frame. |
| Presentation and recovery | `simcore-cli`, `simcore-web-gpu`, `web/renderer.js` | Own the swap chain or canvas, resize, scheduling and fallback. The host retains the session and scene when a presenter changes. |
| Sessions, isolation, external API | `simcore-host` / `simcore-cli` | Process per session: a guest crash marks that session `crashed` only. |

## Choices

- **CPU capture by default.** The CLI without optional features has no GPU or window dependencies. The same CPU rasterizer provides the visual regression reference for CPU and GPU backends. Native windows and browser previews select GPU when available. [Renderer behavior](rendering.md)
- **SwiftUI browser runtime.** Swift and the shared layout and CPU renderer run in `app.wasm` (WASI). A separate `wasm32-unknown-unknown` module decodes the display list for WebGPU. The display-list version is internal to the pair of modules. [Browser build](web.md)
- **JSON transport.** Kept behind small send/receive functions so a binary encoding can replace it without touching layout or Swift evaluation.
- **Whole-root re-evaluation** after a state change. Fine-grained invalidation is a later optimisation.
