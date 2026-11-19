[OneSim](../README.md) / [Documentation](README.md)

# OneSim in the browser

Run the SwiftUI counter or showcase in a browser tab. The app, SwiftUI module, and CPU renderer compile to one WebAssembly module. A second module presents through WebGPU when an adapter is available. The preview responds to taps, drag and wheel scrolling, and text input from the on-screen or physical keyboard.

## Install the toolchain

Install a swift.org toolchain and its matching WebAssembly SDK. Add both Rust Wasm targets and the version of
`wasm-bindgen-cli` pinned by `onesim-web-gpu`. CI uses Swift 6.4.2 and wasm-bindgen 0.2.130.

```sh
curl -LO https://download.swift.org/swift-6.4.2-release/xcode/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE-osx.pkg
installer -pkg swift-6.4.2-RELEASE-osx.pkg -target CurrentUserHomeDirectory   # -> ~/Library/Developer/Toolchains (or use swiftly)
PATH=~/Library/Developer/Toolchains/swift-6.4.2-RELEASE.xctoolchain/usr/bin:$PATH \
swift sdk install https://download.swift.org/swift-6.4.2-release/wasm-sdk/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE_wasm.artifactbundle.tar.gz \
  --checksum 83baca83553f8c443cda7b84800912e6e709c3acd2e7ebf277794ca5836c0da5
rustup target add wasm32-wasip1 wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
brew install binaryen    # optional: wasm-opt shrinks app.wasm ~25%; skipped (with a note) if absent or if it fails
```
Allow several GB of free space for the toolchain. The build script finds the toolchain and SDK. Set `SIMCORE_SWIFT_TOOLCHAIN` to select a specific toolchain.

## Build and open the preview

From the repository root, build the counter and serve the generated files.

```sh
scripts/build-web.sh examples/swiftui-counter CounterView
python3 -m http.server 8000 -d target/web
```

Open [http://localhost:8000/](http://localhost:8000/) and tap **Increment**. For the showcase, run `scripts/build-web.sh examples/swiftui-showcase`. It uses an `@main App` entry point, so omit the root view argument.

<p align="center">
  <img src="web/counter-in-browser.png" width="260" alt="The SwiftUI counter running in a browser canvas">
</p>

Serve the complete `target/web/` directory over HTTPS or localhost. Deploy `app.wasm`,
`gpu/onesim_web_gpu.js`, `gpu/onesim_web_gpu_bg.wasm`, and the page files from the same build.

## Configure the viewport

| Query parameter | Effect |
| --- | --- |
| `?device=<name>` | Selects a profile. The default is `iphone-18-pro`. Find the available profiles in `window.onesim.sim.devices`. |
| `?width=402&height=874&scale=3` | Sets width and height in points and scale in pixels per point. By default, scale follows the window size and `devicePixelRatio`. |
| `?dark=1` | Selects dark mode. The default follows the browser’s color scheme. |
| `?renderer=cpu\|gpu\|auto` | `cpu` uses Canvas 2D. `gpu` and `auto` try WebGPU with Canvas 2D fallback. The query parameter takes precedence over `localStorage["simcore.renderer"]`. |
| `?diag` | Shows the active renderer and any fallback reason. |

The canvas preserves its aspect ratio and redraws when the window resizes.

## Use the page API

Call `window.onesim.setViewport(w, h)` or `window.onesim.setAppearance('dark'|'light')` to update the layout and Swift environment. To drive the app, call `click(label)`, `type(text)`, `key('backspace'|'enter')`, or `scroll(x,y,dx,dy)` on the same object. Read the UI tree with `inspect()` or `lastTreeText()`.

## GPU rendering

Two Wasm modules. `app.wasm` (wasm32-wasip1: Swift + layout + fonts + CPU raster) stays the runtime. With
`simcore_web_set_output(1)` its `draw` lowers the scene with a persistent `onesim_render::Builder` to an encoded
`DisplayList` (`simcore_web_dl_ptr/len`) instead of pixels (mode 0 produces CPU pixels).
Hit testing and interaction still come from the `Scene`. `gpu/onesim_web_gpu*.{js,wasm}`
(`crates/onesim-web-gpu`, wasm32-unknown-unknown + wasm-bindgen, built by `scripts/build-web.sh`) wraps
`onesim_render_gpu::Executor`: `await init(canvas)` -> `Gpu`; `gpu.render(bytes)` decodes, executes into an offscreen
`Rgba8Unorm` storage texture and blits it to the canvas (canvas textures cannot be storage textures; values are already
sRGB-encoded 8-bit, so the blit is a copy); `resize`, `info()` (JSON), `lost()`, `error()`, `free()`. The crate is
`#![cfg(target_arch = "wasm32")]` with wasm-only dependencies, so on native targets it is empty and
`cargo build/clippy --workspace` is unaffected; lint it with `cargo clippy -p onesim-web-gpu --target wasm32-unknown-unknown`.

`web/renderer.js` (`Presenter`) chooses the backend before the first frame: `renderer=cpu`, no `navigator.gpu`, a runtime
without display-list output, or any init failure -> CPU; otherwise GPU. After that, a render error, a lost device (polled
once a second, plus checked after every frame) or an uncaptured GPU error switches to CPU for good: `setOutput(0)`
(resets the builder), a fresh `<canvas>` (a canvas that got a `webgpu` context cannot give a `2d` one; input listeners are
on the `#stage` wrapper so they survive), and the same frame is shown with `putImageData`.
Rendering stays event-driven: a frame is presented only when input or the animation tick changed something.
`window.onesimRenderer` exposes `mode` (`gpu|cpu`), `reason`, `requested`, `info`, `frames`, `fallback()`.
Deploy `app.wasm`, the JavaScript and `gpu/` from the same build. Both Wasm modules use the internal display-list
version, and an incompatible pair triggers CPU fallback.

## How it works

- **ABI.** `onesim-web` exports the [`simcore_runtime_*` C ABI](protocol.md#c-c-abi-cratesonesim-runtime-header-simcore_runtimeh) in-process.
  `submit_tree` parses the ViewNode and renders it to an RGBA framebuffer. `run` returns at once.
- **Reactor.** Linked with `-mexec-model=reactor`. JS calls `_initialize`, then
  `simcore_web_init(w, h, scale, dark)`, which runs the Swift entry (`__main_argc_argv`: registers the app and
  *returns*; works for `_onesimRun(Root())` mains and `@main App`) and then pushes the environment, which
  makes Swift evaluate and submit the first tree. The instance stays alive; the browser is the event loop.
- **Interaction.** `onesim_render::Interaction` is the one state machine (scroll offsets, focus, shift,
  press target) shared with the native host. It maps a pointer/keyboard event + the scene to an `Effect`
  (`None | Action{id,version} | TextInput{field,version,text} | Redraw`). The host sends actions and text over
  the guest wire; `onesim-web` calls the Swift action/text callbacks directly and redraws. Label lookup
  (`resolve_label`, the CLI's `--click LABEL`) lives next to it.
- **JS exports** (small, scratch-buffer based for strings): `simcore_web_pointer(kind,x,y)` (0 down, 1 up,
  2 click; points), `simcore_web_scroll(x,y,dx,dy)`, `simcore_web_drag(phase,x,y,t_ms)` (0 begin at the
  touch-down point, 1 move, 2 end), `simcore_web_tick(dt_ms)` (advance momentum/bounce; 1 while moving),
  `simcore_web_key(kind,len)` (text / `backspace` / `enter`
  from the scratch buffer; only while a field is focused), `set_viewport`, `set_dark`, `set_ios` (0 = 26.5, 1 = 27.2),
  `set_dynamic_type` (index into the 12 Dynamic Type sizes), `set_scale`, `focused`,
  `inspect`, `find`, and `fb_ptr/len/width/height`. The browser page and Node harness share the `web/onesim.js` driver.
- **Input (main.js).** Pointer events map CSS px to points; moving more than 8 px turns a press into a
  touch drag (`Interaction::drag`: rubber band past the edges, then UIScrollView momentum and a critically
  damped bounce after lifting; a touch on moving content stops it). A `requestAnimationFrame` loop calls
  `tick` after any input until it returns 0. `wheel` scrolls directly (clamped, no physics: trackpads
  already coast) and deltaY > 0 reveals later content; `keydown` types printable keys, Backspace, Enter while a
  field is focused (tapping a field shows the on-canvas keyboard, whose keys also type).
- **Fonts and symbols** use bundled Inter and Phosphor. `scripts/compare-web.sh` compares the browser's CPU output with native `SIMCORE_FALLBACK=1` captures.
- **No Foundation on Wasm.** `Portable.swift` supplies `CGFloat`, `NSNull` and a small JSON writer
  under `#if os(WASI)`; native builds are unchanged.

## Testing

Use Node.js 22 or later for the test commands below. The presenter tests also need Chrome or Chromium. Run the
suite that matches the example you built.

```sh
node web/test/run.mjs --suite counter                 # use --suite showcase after building the showcase
node web/test/run.mjs --click Todos --click "New todo" --type "Buy milk" --screenshot out.png --expect "Buy milk"
                                                      # CLI-style steps: --click LABEL|X,Y --type --key --scroll X,Y,DX,DY --dark --width/--height/--scale --inspect
node web/test/presenter.mjs                            # headless Chrome GPU and CPU fallback checks
node web/test/presenter.mjs --app                      # check the built showcase and state retained after device loss
scripts/compare-web.sh                                # compare each showcase state with native fallback, each < 2%
```
The `web` CI job installs Swift 6.4.2, its Wasm SDK, and wasm-bindgen-cli 0.2.130. It builds both examples,
tests the Chrome presenter, and compares screenshots. CPU fallback is tested even when the runner has no WebGPU adapter.

## Gaps

Device profiles come from `DEVICES` (`crates/onesim-render/src/device.rs`, shared with the native host); other sizes via `?width=&height=`. The software keyboard has no
number/emoji layers (as native). No IME/composition input and no multi-touch. `wasm-opt` flags assume binaryen 133+.
