# SimCore in the browser

The SwiftUI counter runs entirely client-side: **one Wasm module** (Swift app + our `SwiftUI`
module + `crates/simcore-web`, which embeds `simcore-render`) drawing to a `<canvas>`. A static file
server is enough.

```
scripts/build-web.sh examples/swiftui-counter CounterView   # -> target/web/{index.html,main.js,app.wasm}
python3 -m http.server 8000 -d target/web                    # open http://localhost:8000/
```
Query params: `?scale=3` (device pixels per point, default `devicePixelRatio`), `?dark=1`.

## Toolchain (one-time)

Xcode's Swift cannot target Wasm. You need a swift.org toolchain and the Wasm SDK of the **same
release**, plus the Rust target. Tested with 6.4.2:

```
curl -LO https://download.swift.org/swift-6.4.2-release/xcode/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE-osx.pkg
installer -pkg swift-6.4.2-RELEASE-osx.pkg -target CurrentUserHomeDirectory   # -> ~/Library/Developer/Toolchains (or use swiftly)
PATH=~/Library/Developer/Toolchains/swift-6.4.2-RELEASE.xctoolchain/usr/bin:$PATH \
swift sdk install https://download.swift.org/swift-6.4.2-release/wasm-sdk/swift-6.4.2-RELEASE/swift-6.4.2-RELEASE_wasm.artifactbundle.tar.gz \
  --checksum 83baca83553f8c443cda7b84800912e6e709c3acd2e7ebf277794ca5836c0da5
rustup target add wasm32-wasip1
brew install binaryen    # optional: wasm-opt shrinks app.wasm ~25%
```
Needs several GB free during the toolchain install. The script finds the toolchain (set
`SIMCORE_SWIFT_TOOLCHAIN` to override) and the SDK and fails with install hints if either is missing.

## How it works

- **ABI.** `simcore-web` exports the `simcore_runtime_*` C ABI (protocol.md section C) in-process:
  `submit_tree` parses the ViewNode and renders it to an RGBA framebuffer; `run` returns at once.
- **Reactor.** Linked with `-mexec-model=reactor`. JS calls `_initialize`, then
  `simcore_web_init(scale, dark)`, which runs the Swift entry (`__main_argc_argv`: registers the app and
  *returns*; on WASI `_simcoreRun` does not block) and then pushes the environment, which makes Swift
  evaluate and submit the first tree. The instance stays alive; the browser is the event loop.
- **Events.** `simcore_web_pointer(kind, x, y)` (0 down, 1 up, 2 click; points) hit-tests the last
  scene, fires the action callback into Swift, which re-evaluates and submits; JS then redraws
  from `simcore_web_fb_ptr/len/width/height`.
- **Fixed profile:** iphone-17-pro (402x874 pt). Fonts and symbols are the bundled Inter and Phosphor
  (the same as `SIMCORE_FALLBACK=1`); pixel-identical to the native fallback render.
- **No Foundation on Wasm.** `Portable.swift` supplies `CGFloat`, `NSNull` and a small JSON writer
  under `#if os(WASI)`; native builds are unchanged.
- **Debug hook.** `window.simcore.lastTreeText()` returns the latest tree JSON.

## Gaps

Text input/keyboard, scrolling and the env callback's viewport changes are not wired in JS yet
(the Rust side ignores `scroll`/focus); fixed viewport; no `@main` verification (only the root-view
path was tested); `wasm-opt` flags assume binaryen 133.
