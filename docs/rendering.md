# Rendering backends

Layout, hit testing and input are backend independent: the host builds one `Scene` and a `Renderer`
(`crates/simcore-render/src/backend.rs`) turns it into pixels.

| backend | what | availability |
|---|---|---|
| `cpu` | tiny-skia + cosmic-text, the deterministic reference (all pixel-diff gates use it) | always |
| `gpu` | wgpu + Vello (`crates/simcore-render-gpu`), render to texture and read back once per frame; Liquid Glass and scroll edges run as compute passes on an accumulation texture (`SIMCORE_GPU_GLASS=cpu` forces the CPU backdrop path) | `--features gpu` and a usable adapter |

The GPU backend is a display-list pipeline: `simcore_render::Builder` lowers a `Scene` (fonts, layout, sprites) to a
`DisplayList` (`crates/simcore-render/src/display.rs`, device pixels, binary encoding), and `simcore_render_gpu::Executor`
runs it into a texture with no fonts or scene. `GpuRenderer` is Builder + Executor + one readback. The executor builds for
`wasm32-unknown-unknown` with `--no-default-features`.

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

The default build has no GPU dependencies.

## Window

`simcore window <app>` (SwiftUI, Compose) and `simcore expo --url <metro> --window` open a live session in a native
window. Build with `cargo build -p simcore-cli --features window` (winit + softbuffer; implies `gpu`). Default builds
do not have it. The other options (`--device`, `--dark`, `--click` ... applied before the window opens) are those of `run`.

The window shows the device viewport 1:1 in points (shrunk to fit a small monitor) and is not resizable: the guest lays
out for a device (insets, island, tab bar), so a resizable window would be a different device. The render scale is
`window pixel width / viewport width`; cursor pixels divide by it to give points. The same `Host` calls as the headless
steps handle input: click, wheel / trackpad (`scroll`, applied once per frame), left-drag (a live `drag` with release
momentum, and sheet swipes; React Native gets the whole gesture on release, no live tracking), typed text, Backspace and
Enter. Hit testing is the scene's, so it does not depend on the backend.

**Backend.** `--renderer` / `SIMCORE_RENDERER`: unset or `auto` is GPU first here (interactive, not headless), CPU when
no adapter; `gpu` fails at startup instead of falling back; `cpu` is tiny-skia through softbuffer. The GPU path is
`simcore_render_gpu::WindowRenderer`: the executor draws the display list into an offscreen `Rgba8Unorm` storage texture,
a fullscreen-triangle pass copies it to the swap chain (`Fifo` vsync, frame latency 2). The 8-bit values are already sRGB,
so a non-sRGB surface format is used (an sRGB one is viewed as its non-sRGB twin): no second gamma. Failures: an outdated
or lost swap chain is reconfigured and the frame retried (three in a row move to the CPU presenter); a lost device or any
other GPU error switches to the CPU presenter, the session keeps running, and the reason is in the title and on stderr.
A hidden window (minimized, covered, locked screen) skips the frame before any drawing work and is probed 4 times a second.

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

**Test hook.** `SIMCORE_WINDOW_SCRIPT` drives a window without OS input or screen capture (a locked screen hides
windows from both): steps `wait:MS`, `click:X,Y`, `scroll:X,Y,DX,DY`, `drag:X,Y,DX,DY`, `type:TEXT`, `key:enter`,
`toggle`, `dark`, `snap:PATH`, `quit`, separated by `;`, each run once the previous frame is drawn. `snap` renders the
scene through the same presenter (GPU: the blit pipeline and surface format) and writes a PNG.
