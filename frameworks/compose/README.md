# Jetpack Compose engine (preview)

Runs unmodified Jetpack Compose apps (an Activity calling `setContent { }`, or any top-level `@Composable`)
headlessly as Android, with no emulator, Gradle or Android SDK.

```sh
cargo run -p onesim-cli -- run examples/compose-counter --click Increment --screenshot counter.png
cargo run -p onesim-cli -- run path/to/ui --root-view Greeting    # a @Composable fun with no parameters
```

## How it works

- **Toolchain** (`scripts/toolchain.sh`): downloads the pinned Kotlin compiler (it bundles the Compose compiler
  plugin) and the real `androidx.compose.runtime` desktop/JVM jars (or `wasmJs` klibs with `--wasm`) into
  `~/.cache/simcore/compose` (`SIMCORE_COMPOSE_TOOLCHAIN`). Only Java 17+ is needed.
- **Real runtime, own UI layer.** Composition, `remember`, snapshots, recomposition and effects are AndroidX's own
  code. Everything above it (`androidx.compose.ui`, `foundation`, `foundation.layout`, `material3`,
  `activity-compose`, `android.os.Bundle`, `android.util.Log`) is reimplemented in `src/` as common Kotlin shared
  between the JVM and Kotlin/Wasm targets. Material components are built out of a few layout primitives
  (`Box`, `Column`, `Row`, `Text`) and a Modifier chain, as in AndroidX.
- **Build** (`scripts/build-app.sh` for JVM; `scripts/build-web.sh` for browser Wasm): compiles `src/` plus the
  platform guest (`jvm/` or `wasm/`, cached by content hash), then the app's `.kt` files. On the JVM it writes a
  launcher running `onesim.compose.JvmGuest <Activity>`. On the web it compiles with `kotlinc-wasm` and a generated
  entry point into `target/web/guest/` paired with `onesim-web` (see [docs/web.md](../../docs/web.md)).
- **Guest** (`src/onesim/compose/Guest.kt`, `jvm/onesim/compose/JvmGuest.kt`, `wasm/onesim/compose/WasmGuest.kt`):
  instantiates the Activity, runs `onCreate`, composes with a `LayoutApplier`, and delivers trees/events over
  stdin/stdout JSON lines (JVM) or JS exports (Wasm).
- **Host** (`crates/onesim-render/src/layout/compose.rs`): Compose's constraint-based measure/place
  (padding, size, fill, wrap, weights, arrangements, alignment, `minimumInteractiveComponentSize`), drawing,
  hit testing, the Android status bar and gesture handle. Text is Roboto (bundled, OFL).

## Status

Coverage per API: [docs/compatibility/compose](../../docs/compatibility/compose/README.md). Device: Pixel 10a
(Android 17, 411.43 x 923.43 dp at 2.625x; status-bar and navigation-bar insets measured on the emulator).

Pixel-checked against the real thing: `scripts/compare-android.sh` renders the examples in the states captured from
a real APK of the same sources (`reference-app/`, material3 1.4.0) on the Android 17 emulator
(`tests/fixtures/compose-android`); every state must differ in < 2% of pixels, like iOS.

`BasicTextField` and Material 3 `TextField` accept string edits from the host keyboard. Material 3
`Slider` supports a value range and discrete steps, with input from clicks and drags.

`Text` and `BasicText` accept styled `AnnotatedString` content and report text layout geometry.
`rememberTextMeasurer` and `MultiParagraph` use the same host shaper for synchronous measurement on
the JVM and in the browser. `MultiParagraph.paint` can draw to a mutable sRGB bitmap.
`ClickableText` reports UTF-16 offsets. `SelectionContainer` highlights text across enabled
children on drag or long press and supplies touch handles, a magnifier, and a selection toolbar.
`DisableSelection` excludes its subtree. Copy uses the desktop or browser clipboard. Read aloud
uses macOS speech, Windows System.Speech, Speech Dispatcher on Linux, or browser speech synthesis.

Not yet: `Icon`/`Image`, lazy lists and scrolling, navigation, dialogs, animations, ripples,
`R.string`, `painterResource`, `ViewModel`.


### Bundled fonts

Place `.ttf`, `.otf`, `.ttc`, or XML font families in `res/font` or the Android module's
`src/main/res/font`. XML families can declare local faces with weights, styles, and collection
indexes. Provider-backed XML uses its declared system family when available.
Use `FontFamily(Font(R.font.my_font))` in `Text` or `SpanStyle`. OneSim's source-only build
generates `R.font` in the activity or root composable's package. Set the manifest's package
when the Android resource package differs.

Place PNG or JPEG bitmaps in `res/drawable` or `res/drawable-nodpi` to generate `R.drawable` values.
`ImageBitmap.imageResource` loads them for `ImageShader`, which tiles the pixels into text or bullet markers.
