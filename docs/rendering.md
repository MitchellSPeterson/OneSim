# Rendering backends

Layout, hit testing and input are backend independent: the host builds one `Scene` and a `Renderer`
(`crates/simcore-render/src/backend.rs`) turns it into pixels.

| backend | what | availability |
|---|---|---|
| `cpu` | tiny-skia + cosmic-text, the deterministic reference (all pixel-diff gates use it) | always |
| `gpu` | wgpu + Vello (`crates/simcore-render-gpu`), render to texture and read back once per frame; Liquid Glass and scroll edges run as compute passes on an accumulation texture (`SIMCORE_GPU_GLASS=cpu` forces the CPU backdrop path) | `--features gpu` and a usable adapter |

## Selecting

`--renderer auto|gpu|cpu` (CLI, global) overrides `SIMCORE_RENDERER`; with neither set, headless capture uses `cpu`.

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
