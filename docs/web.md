[SimCore](../README.md) / [Documentation](README.md)

# SimCore in the browser

Run the SwiftUI counter or showcase in a browser tab. The app, SwiftUI module, and Rust renderer compile to one WebAssembly module that draws to a canvas. The preview responds to taps, drag and wheel scrolling, and text input from the on-screen or physical keyboard.

## Install the toolchain

Install a swift.org toolchain and its matching WebAssembly SDK, then add Rust’s `wasm32-wasip1` target. CI uses Swift 6.4.2.

```sh
curl -LO https://download.swift.org/swift-6.4.2-release/xcode/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE-osx.pkg
installer -pkg swift-6.4.2-RELEASE-osx.pkg -target CurrentUserHomeDirectory   # -> ~/Library/Developer/Toolchains (or use swiftly)
PATH=~/Library/Developer/Toolchains/swift-6.4.2-RELEASE.xctoolchain/usr/bin:$PATH \
swift sdk install https://download.swift.org/swift-6.4.2-release/wasm-sdk/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE_wasm.artifactbundle.tar.gz \
  --checksum 83baca83553f8c443cda7b84800912e6e709c3acd2e7ebf277794ca5836c0da5
rustup target add wasm32-wasip1
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

The output in `target/web/` contains `index.html`, `main.js`, `simcore.js`, and `app.wasm`. Any static file server can serve these files.

## Configure the viewport

| Query parameter | Effect |
| --- | --- |
| `?device=<name>` | Selects a profile. The default is `iphone-18-pro`. Find the available profiles in `window.simcore.sim.devices`. |
| `?width=402&height=874&scale=3` | Sets width and height in points and scale in pixels per point. By default, scale follows the window size and `devicePixelRatio`. |
| `?dark=1` | Selects dark mode. The default follows the browser’s color scheme. |

The canvas preserves its aspect ratio and redraws when the window resizes.

## Use the page API

Call `window.simcore.setViewport(w, h)` or `window.simcore.setAppearance('dark'|'light')` to update the layout and Swift environment. To drive the app, call `click(label)`, `type(text)`, `key('backspace'|'enter')`, or `scroll(x,y,dx,dy)` on the same object. Read the UI tree with `inspect()` or `lastTreeText()`.

## How it works

- **ABI.** `simcore-web` exports the [`simcore_runtime_*` C ABI](protocol.md#c-c-abi-cratessimcore-runtime-header-simcore_runtimeh) in-process.
  `submit_tree` parses the ViewNode and renders it to an RGBA framebuffer. `run` returns at once.
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

Use Node.js 20 or later to test the compiled module. Run the suite that matches the example you built.

```sh
node web/test/run.mjs --suite counter                 # use --suite showcase after building the showcase
node web/test/run.mjs --click Todos --click "New todo" --type "Buy milk" --screenshot out.png --expect "Buy milk"
                                                      # CLI-style steps: --click LABEL|X,Y --type --key --scroll X,Y,DX,DY --dark --width/--height/--scale --inspect
scripts/compare-web.sh                                # compare each showcase state with native fallback, each < 2%
```
CI (`web` job in `.github/workflows/ci.yml`) installs the 6.4.2 toolchain + Wasm SDK (downloads cached),
builds both examples and runs all of the above.

## Gaps

Device profiles come from `DEVICES` (`crates/simcore-render/src/device.rs`, shared with the native host); other sizes via `?width=&height=`. The software keyboard has no
number/emoji layers (as native). No IME/composition input and no multi-touch. `wasm-opt` flags assume binaryen 133+.
