# Jetpack Compose engine (preview)

Runs unmodified Jetpack Compose apps (an Activity calling `setContent { }`, or any top-level `@Composable`)
headlessly as Android, with no emulator, Gradle or Android SDK.

```sh
cargo run -p onesim-cli -- run examples/compose-counter --click Increment --screenshot counter.png
cargo run -p onesim-cli -- run path/to/ui --root-view Greeting    # a @Composable fun with no parameters
```

## How it works

- **Toolchain** (`scripts/toolchain.sh`): downloads the pinned Kotlin compiler (it bundles the Compose compiler
  plugin) and the real `androidx.compose.runtime` desktop/JVM jars into `~/.cache/simcore/compose`
  (`SIMCORE_COMPOSE_TOOLCHAIN`). Only Java 17+ is needed.
- **Real runtime, own UI layer.** Composition, `remember`, snapshots, recomposition and effects are AndroidX's own
  code. Everything above it (`androidx.compose.ui`, `foundation`, `foundation.layout`, `material3`,
  `activity-compose`, `android.os.Bundle`, `android.util.Log`) is reimplemented in `src/`. Material components are
  built out of a few layout primitives (`Box`, `Column`, `Row`, `Text`) and a Modifier chain, as in AndroidX.
- **Build** (`scripts/build-app.sh`): compiles `src/` once (cached by content hash in `target/compose/`), then the
  app's `.kt` files, and writes a launcher that runs `onesim.compose.Guest <Activity>` on the JVM.
- **Guest** (`src/onesim/compose/Guest.kt`): instantiates the Activity, runs `onCreate`, composes with a
  `LayoutApplier`, and speaks protocol sections B and K over stdin/stdout: a `tree` after every change, `action`
  for taps, `environment` for dark mode (`isSystemInDarkTheme()`).
- **Host** (`crates/onesim-render/src/layout/compose.rs`): Compose's constraint-based measure/place
  (padding, size, fill, wrap, weights, arrangements, alignment, `minimumInteractiveComponentSize`), drawing,
  hit testing, the Android status bar and gesture handle. Text is Roboto (bundled, OFL).

## Status

Coverage per API: [docs/compatibility/compose](../../docs/compatibility/compose/README.md). Device: Pixel 10a
(Android 17, 411.43 x 923.43 dp at 2.625x; status-bar and navigation-bar insets measured on the emulator).

Pixel-checked against the real thing: `scripts/compare-android.sh` renders the examples in the states captured from
a real APK of the same sources (`reference-app/`, material3 1.4.0) on the Android 17 emulator
(`tests/fixtures/compose-android`); every state must differ in < 2% of pixels, like iOS.

Not yet: `Icon`/`Image`, text fields, lazy lists and scrolling, navigation, dialogs, animations, ripples,
`AnnotatedString`, custom fonts, resources (`R.string`, `painterResource`), `ViewModel`, Expo / React Native on Android.
