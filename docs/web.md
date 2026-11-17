# SimCore in the browser

The SwiftUI counter runs entirely client-side: **one Wasm module** (Swift app + our `SwiftUI`
module + `crates/simcore-web`, which embeds `simcore-render`) drawing to a `<canvas>`. A static file
server is enough.

```
scripts/build-web.sh examples/swiftui-showcase               # @main App, no root view
scripts/build-web.sh examples/swiftui-counter CounterView    # or a root view
                                                             # -> target/web/{index.html,main.js,simcore.js,app.wasm}
python3 -m http.server 8000 -d target/web                    # open http://localhost:8000/
```
Query params: `?device=iphone-17-pro` (default; the only profile), `?width=&height=&scale=` (points / device
pixels per point; default scale fits the window x `devicePixelRatio`), `?dark=1` (default: the browser's
colour scheme). The canvas fits the window keeping the aspect ratio and re-renders crisply on resize.
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
rustup target add wasm32-wasip1
brew install binaryen    # optional: wasm-opt shrinks app.wasm ~25%; skipped (with a note) if absent or if it fails
```
Needs several GB free during the toolchain install. The script finds the toolchain (set
`SIMCORE_SWIFT_TOOLCHAIN` to override) and the SDK and fails with install hints if either is missing.

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
  2 click; points), `simcore_web_scroll(x,y,dx,dy)`, `simcore_web_key(kind,len)` (text / `backspace` / `enter`
  from the scratch buffer; only while a field is focused), `set_viewport`, `set_dark`, `set_scale`, `focused`,
  `inspect`, `find`, and `fb_ptr/len/width/height`. `web/simcore.js` wraps them (shared by browser and Node).
- **Input (main.js).** Pointer events map CSS px to points; moving more than 8 px turns a press into a
  drag-scroll; `wheel` deltaY > 0 reveals later content; `keydown` types printable keys, Backspace, Enter while a
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
scripts/compare-web.sh                                # the 13 showcase states: Node harness vs native `SIMCORE_FALLBACK=1`, each < 2%
```
CI (`web` job in `.github/workflows/ci.yml`) installs the 6.4.2 toolchain + Wasm SDK (downloads cached),
builds both examples and runs all of the above.

## Gaps

One device profile (iphone-17-pro chrome; other sizes via `?width=&height=`). The software keyboard has no
number/emoji layers (as native). No IME/composition input and no multi-touch. `wasm-opt` flags assume binaryen 133+.
