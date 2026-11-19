# Rendering backends

Layout, hit testing and input are backend independent: the host builds one `Scene` and a `Renderer`
(`crates/simcore-render/src/backend.rs`) turns it into pixels.

| backend | what | availability |
|---|---|---|
| `cpu` | tiny-skia + cosmic-text, the deterministic reference (all pixel-diff gates use it) | always |
| `gpu` | wgpu + Vello (`crates/simcore-render-gpu`), render to texture and read back once per frame; Liquid Glass and scroll edges run as compute passes on an accumulation texture (`SIMCORE_GPU_GLASS=cpu` forces the CPU backdrop path) | `--features gpu` and a usable adapter |

The GPU backend lowers a `Scene` to an internal `DisplayList` in device pixels using `simcore_render::Builder`.
`simcore_render_gpu::Executor` runs that list into a texture. Its inputs include CPU-rasterized text, glyph and symbol
sprites. Native and browser presenters blit the texture directly. Headless GPU capture reads it back once after rendering
unless the frame needs a CPU backdrop pass. The executor builds for `wasm32-unknown-unknown` with
`--no-default-features`.

## Migrating to v0.3

1. Keep your existing app source and headless screenshot commands. Set `--renderer cpu` for deterministic captures
   when a shared environment might set `SIMCORE_RENDERER`.
2. Build with `--features window` for native preview. Unset `SIMCORE_RENDERER` or choose `--renderer auto` to try the
   GPU first. Build with `--features gpu` if you only need opt-in headless GPU captures.
3. Rebuild and deploy the complete `target/web/` directory for browser preview. `app.wasm` and `gpu/` share a versioned
   display list, so publish them together. Serve over HTTPS or localhost and use `?diag` for the active backend or
   `?renderer=cpu` to select Canvas 2D. [Browser setup](web.md)
4. Clients can keep using the same screenshot and input APIs. Use `render.backend` to select the screenshot renderer
   and read its diagnostics. [Client API](api.md)

## Selecting

`--renderer auto|gpu|cpu` (CLI, global) overrides `SIMCORE_RENDERER`; with neither set, headless capture uses `cpu` (the window is GPU first, see Window).

- `cpu`: always the reference rasterizer.
- `gpu`: the GPU or a clear error (`GPU renderer unavailable: <reason>`, or `built without the \`gpu\` feature`).
- `auto`: try the GPU, fall back to CPU with the reason recorded. Only an explicit choice (flag, env,
  `Host::set_renderer`, `render.backend`) selects it; unset means `cpu`, so default output is deterministic without a GPU.

An invalid value is an error naming it, reported at the first screenshot. Apps never choose a backend.

## Capture convention (every backend)

`Frame::rgba` is 8-bit sRGB RGBA, premultiplied alpha, rows top to bottom with no padding,
`width = round(scene.w * scale)`, `height = round(scene.h * scale)` (each at least 1). Full-screen scenes are
opaque, so premultiplied equals straight there. PNG encoding demultiplies.

## Recovery

Per frame, on the same scene (no guest restart): `SurfaceLost` retries once; `DeviceLost` re-creates the GPU
renderer once, then falls back to CPU with the reason recorded; `Unsupported` under `auto` renders that frame
on CPU. With an explicit `gpu`, `Unsupported` and other errors are reported.

## Diagnostics

`Host::renderer_diagnostics()` and the `render.backend` RPC (docs/api.md) report backend, adapter,
limitations, fallback reason, frame count and last-frame timings (`encode`, `gpu`, `readback`). The CLI
prints one line to stderr after `--screenshot` when `--renderer` or `SIMCORE_RENDERER` is set:

```
renderer: gpu (Apple M1 Pro (Metal))
renderer: cpu (fallback: GPU renderer unavailable: no adapter)
```

## Building with the GPU backend

```
cargo build -p simcore-cli --features gpu
simcore run App.swift --renderer gpu --screenshot out.png
```

The default build has no GPU dependencies. Native GPU builds honor wgpu's `WGPU_BACKEND` override
for adapter selection.

## Window

`simcore window <app>` (SwiftUI, Compose) and `simcore expo --url <metro> --window` open a live session in a native
window. Build with `cargo build -p simcore-cli --features window` (winit + softbuffer; implies `gpu`). Default builds
do not have it. The other options (`--device`, `--dark`, `--click` ... applied before the window opens) are those of `run`.

The window fits a fixed device viewport to the monitor. Render scale is the physical window width divided by the viewport
width in points. Hit testing maps cursor positions back to that viewport through the shared scene. The window accepts
clicks, wheel or trackpad scroll, live left-drag, text, Backspace and Enter. SwiftUI and Compose use the host's scroll
physics. SwiftUI sliders track the pointer and sheets follow a swipe. React Native receives live start, move and end
touches on the original Fabric target. Pointer moves and wheel events coalesce before the next draw.

**Backend.** `--renderer` / `SIMCORE_RENDERER`: unset or `auto` is GPU first here (interactive, not headless), CPU when
no adapter; `gpu` fails at startup instead of falling back; `cpu` is tiny-skia through softbuffer. The GPU path is
`simcore_render_gpu::WindowRenderer`: the executor draws the display list into an offscreen `Rgba8Unorm` storage texture,
a fullscreen-triangle pass copies it to the swap chain (`Fifo` vsync, frame latency 2). The 8-bit values are already sRGB,
so a non-sRGB surface format is used (an sRGB one is viewed as its non-sRGB twin): no second gamma. Failures: an outdated
or lost swap chain is reconfigured and the frame retried (three in a row move to the CPU presenter); a lost device or any
other GPU error switches to the CPU presenter, the session keeps running, and the reason is in the title and on stderr.
An occluded swap chain skips GPU paint and is probed at most four times a second, including during momentum.

**Frame scheduler** (`crates/simcore-cli/src/window/scheduler.rs`, pure and unit tested). Invalidations are input,
resize / scale change, appearance, renderer switch and guest trees (`render.updated` events wake the loop; a tree no newer
than the one already drawn is ignored). They coalesce into one draw, at most one per display refresh interval. While
momentum or a bounce is in flight (`Host::tick`) the scheduler keeps drawing every interval and stops when it ends. With
nothing dirty it is idle: the loop sleeps in `ControlFlow::Wait` until the next OS event. React Native sessions run in
live mode (no waiting for the guest after the first layout), so their animations draw at the rate the guest sends trees.

| key | |
|---|---|
| Cmd+R / F5 | switch GPU and CPU presenters live (same session and scene) |
| Cmd+D | toggle dark appearance |
| Cmd+S | write `simcore-window.png` (the window as presented) |

**Diagnostics.** The title and a stderr line give backend, adapter, surface format and any fallback reason.
`SIMCORE_FRAME_STATS=1` prints one line per second:

```
frames=45 idle_frames=0 skipped=0 wakeups=135 backend=cpu ms p50/p95: total 6.8/7.1 guest 0.1/0.1 scene 0.0/0.1 list 0.0/0.0 exec 1.8/1.9 present 4.9/5.1
```

`guest` is host input handling and ticks (guest round trips), `scene` layout, `list` the display list (GPU), `exec` GPU
submission or the CPU raster, `present` the swap-chain acquire (vsync wait) and present, or the softbuffer copy.
`idle_frames` counts draws nothing asked for (an OS-initiated redraw) and stays 0; `wakeups` counts event-loop wakeups,
about one per second from the stats timer itself when idle.

**Test hook.** `SIMCORE_WINDOW_SCRIPT` drives a window and uses the same presenter and blit pipeline to write PNGs.
Separate steps with `;`. Steps include `wait:MS`, `click:X,Y`, `scroll:X,Y,DX,DY`, `drag:X,Y,DX,DY`, `type:TEXT`,
`key:enter`, `toggle`, `dark`, `snap:PATH` and `quit`. Use `drag-begin:X,Y`, `drag-move:X,Y` and `drag-end:X,Y`
to inspect the app during a gesture.

## v0.3 verification matrix

| Target | Preferred and fallback | Verification |
|---|---|---|
| macOS native | Metal + Vello or tiny-skia via softbuffer | Real Metal captures, window blit snapshots, live scripted inputs and backend switching. Device destruction and replay verified headlessly. Restricting wgpu to DX12 on macOS exercised automatic CPU fallback in the CLI and window. A locked display prevented visible swap-chain pacing and input-latency measurements. |
| Windows native | wgpu adapter selection (DX12 or Vulkan where available), CPU fallback | Native window and GPU adapter path untested for v0.3. |
| Linux native | wgpu adapter selection (typically Vulkan), CPU fallback | Native window, hardware and software adapters untested for v0.3. |
| Chrome WebGPU | Vello + WebGPU or Canvas 2D | Real SwiftUI Wasm input, resize, 2× and 3× presentation, device destruction and retained session verified in headless Chrome on macOS. CPU fallback verified with no WebGPU API, no adapter, no canvas context, malformed data and an uncaptured device error. |
| Safari and Firefox | WebGPU when an adapter is offered, Canvas 2D fallback | GPU and fallback paths untested on these browsers for v0.3. |
| Headless CI | CPU screenshots | SwiftUI, Expo and Compose regression scripts use CPU. Native hardware tests can be enabled locally by leaving `SIMCORE_SKIP_GPU_TESTS` unset. The web job checks WebGPU when an adapter is available. |

## Limitations

- The GPU path keeps text, symbol and glyph shaping on the CPU and caches the uploaded sprites. It rebuilds the display
  list and paints the full frame after invalidation. Damage-region rendering and a retained scene diff remain future work.
- Vector antialiasing, sampling, gradients and shadows have small CPU/GPU pixel differences. The iOS and Android gates
  use CPU screenshots. GPU parity has separate CPU-vs-GPU tests.
- Backdrop clips are limited by the device storage buffer. Large frame or glass scratch textures depend on adapter
  limits. Native GPU rendering can composite unsupported backdrop operations on the CPU and re-upload the result.
  The browser falls back to Canvas 2D if the GPU cannot execute a frame.
- Native screenshot capture waits for a readback. Interactive GPU presentation stays on the GPU unless a backdrop pass
  requires CPU fallback. Use the benchmark report to compare capture cost with the measured limits of window presentation.
  [v0.3 GPU benchmarks](benchmarks/v0.3-gpu.md)
