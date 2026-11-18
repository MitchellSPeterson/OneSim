# Architecture

```
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

## Split of responsibility

| Concern | Where | Why |
|---|---|---|
| Language semantics (generics, closures, property wrappers) | Swift module | Swift compiler already does this; Rust never interprets Swift. |
| State registry, view identity, action table | Swift module | State is tied to Swift values/closures. |
| Normalized view tree | Swift → JSON | Framework-specific but language-neutral. |
| Layout | `simcore-render::layout::{swiftui,rn}` | Proposal-based SwiftUI semantics; React Native uses Yoga flexbox semantics via taffy. Both emit the same Scene. |
| React Native execution | `frameworks/react-native/guest` (Node) | Runs the Metro bundle unchanged; RN's own Fabric renderer reconciles, the guest implements `nativeFabricUIManager` + TurboModules. See `frameworks/react-native/README.md`. |
| Scene, raster, hit test, inspection | `simcore-render` | Framework-neutral; shared by every future framework. Liquid Glass is a backdrop effect in `glass.rs` (CPU, deterministic, wasm-safe); nodes are in paint order so a glass node reads the pixels already drawn. |
| Sessions, isolation, external API | `simcore-host` / `simcore-cli` | Process per session: a guest crash marks that session `crashed` only. |

## Choices

- **Renderer: tiny-skia + cosmic-text (CPU).** Headless by construction, no GPU/driver/Chromium, deterministic screenshots for CI, small dependency set. Vello/wgpu can be added as a second backend behind the same Scene when GPU throughput matters.
- **Native execution first.** Apps compile with `swiftc` against our `SwiftUI` module (`-I` search path shadows Apple's). WASM is a later execution backend; nothing in the host assumes native beyond "spawn a process that speaks wire B".
- **JSON transport.** Kept behind small send/receive functions so a binary encoding can replace it without touching layout or Swift evaluation.
- **Whole-root re-evaluation** after a state change. Fine-grained invalidation is a later optimisation.
