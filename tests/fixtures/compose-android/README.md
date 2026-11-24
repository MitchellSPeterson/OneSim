# compose-android ground-truth fixtures

Real Jetpack Compose (material3 1.4.0, compose-bom 2026.09.00) running `examples/compose-counter`,
`examples/compose-showcase`, `examples/compose-input`, and `examples/compose-text` unmodified, built as one APK by `frameworks/compose/reference-app` and captured on the
Android 17 (API 37) Pixel 10a emulator with agent-device. The text states used a local Pixel 10a emulator. `scripts/compare-android.sh` renders the
same states on OneSim and requires every state to differ in < 2% of pixels (`scripts/compare_ios.py`).

- Screen 1080 x 2424 px, density 2.625 (420 dpi) -> 411.43 x 923.43 dp. Status bar inset 142 px, navigation bar 63 px.
- Status bar normalized with SystemUI demo mode: clock 09:41, wifi level 4 (`fully`, no "no internet" mark), battery
  100%, no cellular, no notification icons.
- Animations off (`agent-device settings animations off`); each shot waits 1.5 s after the last tap so ripples settle.

| state | app | steps |
|---|---|---|
| counter | counter | launch |
| counter-changed | counter | Increment, Increment, Decrement ("Count: 1") |
| showcase | showcase | launch (light) |
| showcase-changed | showcase | Filled, Compose FAB ("Tapped 2 times"), switch off (927,668 px = 353,254 dp) |
| showcase-dark | showcase | launch with system dark theme |
| input | input | launch with two text fields and a stepped slider |
| input-changed | input | enter NameHello in the first field, dismiss the keyboard, and drag the slider to 75 |
| text | text | launch with styled text and a selection container |
| text-tapped | text | tap (156,412) px in ClickableText, which reports offset 4 |
| text-selected | text | after the tap, hold (122,598) px to select the first word, then capture while held |
| text-toolbar | text | release the hold to show the selection toolbar and handles |
| text-inline | text | launch the inline composable example and display the reported placeholder width |

No `counter-dark`: the counter's `MaterialTheme` has no dark scheme, so only the status-bar icons change.

## Recapture

```sh
# AGENT_DEVICE: the agent-device command returned by the T3 device tools' device_open for the Pixel 10a emulator.
AGENT_DEVICE=... GRADLE=gradle tests/fixtures/compose-android/capture.sh
```

Pass `--text-only` to recapture just the text states.

Needs Gradle 9.3+ (AGP 9.1, built-in Kotlin), the Android 37 SDK platform and Java 17.

## Measured facts used by the renderer

- Text is drawn and measured at `floor(sp × 2.625)` px (22 sp -> 57 px, 14 sp -> 36 px), with GPOS kerning.
- Layout is in whole px: dp values round (`roundToPx`), text boxes round up, alignment offsets round.
- Elevation shadow = ambient (alpha 0.088, sigma 0.285·e) + spot (alpha 0.199, sigma 0.861·e, inset 0.686·e,
  shifted down by e·y / (398 dp − e)); fitted on the elevated card (1 dp) and extended FAB (6 dp).
- Status bar: clock 14 sp regular, box centred at (57.9, 26.9) dp; battery 23.8 x 12.9 dp pill + 1.6 x 6.5 dp nub
  ending at x 382.9 dp; wifi two arcs + dot in a 16 x 11.8 dp box; icons black on light apps, white on dark.
- Navigation handle 108 x 4 dp: black at 60% (light), white at 92% (dark).


`text-rich` covers nested bullets, locale-aware uppercase, step-based and custom sizing,
and a bundled font. `text-link-focused` and `text-link-pressed` cover the link highlight
on keyboard focus and while the pointer is down. The sample font is a subset of Inter
and uses the repository's [Inter license](../../../assets/fonts/OFL.txt).

`text-effects` covers gradient and size-aware shader ink, a blurred text shadow, a dashed
gradient marker, a custom path marker, an XML font family, geometric text transforms, outline strokes, and
automatic hyphenation.

`text-paint` covers image shaders and chained and stamped path effects. The demo verifies
synchronous measurement, placeholder geometry, intrinsic widths, and translated,
clipped bitmap painting before it reports the measured line count.

`text-direction` covers mixed-direction text, neutral text in an RTL environment, and
the difference between relative and physical alignment.
