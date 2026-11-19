# SimCore in the browser

The SwiftUI app runs entirely client-side. One Wasm module contains the app, SwiftUI runtime, layout and
CPU renderer. A second Wasm module presents the display list through WebGPU when the browser offers an
adapter. Serve the complete build over HTTPS or localhost.

```
scripts/build-web.sh examples/swiftui-showcase               # @main App, no root view
scripts/build-web.sh examples/swiftui-counter CounterView    # or a root view
                                                             # -> target/web/{index.html,main.js,simcore.js,app.wasm}
python3 -m http.server 8000 -d target/web                    # open http://localhost:8000/
```
Query params: `?device=<name>` (a profile from `DEVICES` in `crates/simcore-render/src/device.rs`, listed by `sim.devices`; default the first, iphone-17-pro), `?width=&height=&scale=` (points / device
pixels per point; default scale fits the window x `devicePixelRatio`), `?dark=1` (default: the browser's
colour scheme), `?renderer=cpu|gpu|auto` (default auto; also localStorage `simcore.renderer`), `?diag` (show the
active renderer and any fallback reason in the corner). The canvas fits the window keeping the aspect ratio and re-renders crisply on resize.
Page API: `window.simcore.setViewport(w, h)`, `setAppearance('dark'|'light')` (both re-layout and push the
environment to Swift, as native), `click(label)`, `type(text)`, `key('backspace'|'enter')`, `scroll(x,y,dx,dy)`,
`inspect()`, `lastTreeText()`.

## Toolchain (one-time)

Xcode's Swift cannot target Wasm. You need a swift.org toolchain and the Wasm SDK of the **same
release**, plus the Rust target. Tested with 6.4.2:

```
curl -LO https://download.swift.org/swift-6.4.2-release/xcode/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE-osx.pkg
installer -pkg swift-6.4.2-RELEASE-osx.pkg -target CurrentUserHomeDirectory   # -> ~/Library/Developer/Toolchains (or use swiftly)
PATH=~/Library/Developer/Toolchains/swift-6.4.2-RELEASE.xctoolchain/usr/bin:$PATH \
swift sdk install https://download.swift.org/swift-6.4.2-release/wasm-sdk/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE_wasm.artifactbundle.tar.gz \
  --checksum 83baca83553f8c443cda7b84800912e6e709c3acd2e7ebf277794ca5836c0da5
rustup target add wasm32-wasip1 wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked   # must equal the wasm-bindgen pin in crates/simcore-web-gpu/Cargo.toml
brew install binaryen    # optional: wasm-opt shrinks app.wasm ~25%; skipped (with a note) if absent or if it fails
```
Needs several GB free during the toolchain install. The script finds the toolchain (set
`SIMCORE_SWIFT_TOOLCHAIN` to override) and the SDK and fails with install hints if either is missing.

## GPU rendering (WebGPU, CPU fallback)

Two Wasm modules. `app.wasm` (wasm32-wasip1: Swift + layout + fonts + CPU raster) stays the runtime. With
`simcore_web_set_output(1)` its `draw` lowers the scene with a persistent `simcore_render::Builder` to an encoded
`DisplayList` (`simcore_web_dl_ptr/len`) instead of pixels (mode 0, the default, is byte-identical to before).
Hit testing and interaction still come from the `Scene`. `gpu/simcore_web_gpu*.{js,wasm}`
(`crates/simcore-web-gpu`, wasm32-unknown-unknown + wasm-bindgen, built by `scripts/build-web.sh`) wraps
`simcore_render_gpu::Executor`: `await init(canvas)` -> `Gpu`; `gpu.render(bytes)` decodes, executes into an offscreen
`Rgba8Unorm` storage texture and blits it to the canvas (canvas textures cannot be storage textures; values are already
sRGB-encoded 8-bit, so the blit is a copy); `resize`, `info()` (JSON), `lost()`, `error()`, `free()`. The crate is
`#![cfg(target_arch = "wasm32")]` with wasm-only dependencies, so on native targets it is empty and
`cargo build/clippy --workspace` is unaffected; lint it with `cargo clippy -p simcore-web-gpu --target wasm32-unknown-unknown`.

`web/renderer.js` (`Presenter`) chooses the backend before the first frame: `renderer=cpu`, no `navigator.gpu`, a runtime
without display-list output, or any init failure -> CPU; otherwise GPU. After that, a render error, a lost device (polled
once a second, plus checked after every frame) or an uncaptured GPU error switches to CPU for good: `setOutput(0)`
(resets the builder), a fresh `<canvas>` (a canvas that got a `webgpu` context cannot give a `2d` one; input listeners are
on the `#stage` wrapper so they survive), and the same frame is shown with `putImageData`. The page is never blank.
Rendering stays event-driven: a frame is presented only when input or the animation tick changed something.
`window.simcoreRenderer` exposes `mode` (`gpu|cpu`), `reason`, `requested`, `info`, `frames`, `fallback()`.
Deploy `app.wasm`, the JavaScript and `gpu/` from the same build. Both Wasm modules use the internal display-list
version, and an incompatible pair triggers CPU fallback.

## How it works

- **ABI.** `simcore-web` exports the `simcore_runtime_*` C ABI (protocol.md section C) in-process:
  `submit_tree` parses the ViewNode and renders it to an RGBA framebuffer; `run` returns at once.
- **Reactor.** Linked with `-mexec-model=reactor`. JS calls `_initialize`, then
  `simcore_web_init(w, h, scale, dark)`, which runs the Swift entry (`__main_argc_argv`: registers the app and
  *returns*; works for `_simcoreRun(Root())` mains and `@main App`) and then pushes the environment, which
  makes Swift evaluate and submit the first tree. The instance stays alive; the browser is the event loop.
- **Interaction.** `simcore_render::Interaction` is the one state machine (scroll offsets, focus, shift,
  press target) shared with the native host. It maps a pointer/keyboard event + the scene to an `Effect`
  (`None | Action{id,version} | TextInput{field,version,text} | Redraw`). The host sends actions and text over
  the guest wire; `simcore-web` calls the Swift action/text callbacks directly and redraws. Label lookup
  (`resolve_label`, the CLI's `--click LABEL`) lives next to it.
- **JS exports** (small, scratch-buffer based for strings): `simcore_web_pointer(kind,x,y)` (0 down, 1 up,
  2 click; points), `simcore_web_scroll(x,y,dx,dy)`, `simcore_web_drag(phase,x,y,t_ms)` (0 begin at the
  touch-down point, 1 move, 2 end), `simcore_web_tick(dt_ms)` (advance momentum/bounce; 1 while moving),
  `simcore_web_key(kind,len)` (text / `backspace` / `enter`
  from the scratch buffer; only while a field is focused), `set_viewport`, `set_dark`, `set_dynamic_type` (index into the 12 Dynamic Type sizes), `set_scale`, `focused`,
  `inspect`, `find`, and `fb_ptr/len/width/height`. `web/simcore.js` wraps them (shared by browser and Node).
- **Input (main.js).** Pointer events map CSS px to points; moving more than 8 px turns a press into a
  touch drag (`Interaction::drag`: rubber band past the edges, then UIScrollView momentum and a critically
  damped bounce after lifting; a touch on moving content stops it). A `requestAnimationFrame` loop calls
  `tick` after any input until it returns 0. `wheel` scrolls directly (clamped, no physics: trackpads
  already coast) and deltaY > 0 reveals later content; `keydown` types printable keys, Backspace, Enter while a
  field is focused (tapping a field shows the on-canvas keyboard, whose keys also type).
- **Fonts and symbols** are the bundled Inter and Phosphor (the same as `SIMCORE_FALLBACK=1`); the output is
  pixel-identical to the native fallback render (`scripts/compare-web.sh`).
- **No Foundation on Wasm.** `Portable.swift` supplies `CGFloat`, `NSNull` and a small JSON writer
  under `#if os(WASI)`; native builds are unchanged.

## Testing

```
node web/test/run.mjs --suite counter|showcase        # assertions on the built target/web/app.wasm (Node >= 20, no deps)
node web/test/run.mjs --click Todos --click "New todo" --type "Buy milk" --screenshot out.png --expect "Buy milk"
                                                      # CLI-style steps: --click LABEL|X,Y --type --key --scroll X,Y,DX,DY --dark --width/--height/--scale --inspect
node web/test/presenter.mjs                           # headless Chrome with GPU fixtures, resize, init failure and device loss
node web/test/presenter.mjs --app                     # also test the built showcase, idle and retained state after device loss
scripts/compare-web.sh                               # 33 showcase states: Node harness vs native bundled fonts, each < 2%
```
The `web` CI job installs Swift 6.4.2 and its Wasm SDK and uses wasm-bindgen-cli 0.2.130. It lints the WebGPU
target, builds both examples and runs the tests above. Chrome tests still verify CPU output on a runner without
a WebGPU adapter.

## Gaps

Device profiles come from `DEVICES` (`crates/simcore-render/src/device.rs`, shared with the native host); other sizes via `?width=&height=`. The software keyboard has no
number/emoji layers (as native). No IME/composition input and no multi-touch. `wasm-opt` flags assume binaryen 133+.
