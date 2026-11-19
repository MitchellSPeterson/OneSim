# compose-android ground-truth fixtures

Real Jetpack Compose (material3 1.4.0, compose-bom 2026.09.00) running `examples/compose-counter` and
`examples/compose-showcase` unmodified, built as one APK by `frameworks/compose/reference-app` and captured on the
Android 17 (API 37) Pixel 10a emulator through agent-device (T3 device tools). `scripts/compare-android.sh` renders the
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

No `counter-dark`: the counter's `MaterialTheme` has no dark scheme, so only the status-bar icons change.

## Recapture

```sh
# AGENT_DEVICE: the agent-device command returned by the T3 device tools' device_open for the Pixel 10a emulator.
AGENT_DEVICE=... GRADLE=gradle tests/fixtures/compose-android/capture.sh
```

Needs Gradle 9.3+ (AGP 9.1, built-in Kotlin), the Android 37 SDK platform and Java 17.

## Measured facts used by the renderer

- Text is drawn and measured at `floor(sp × 2.625)` px (22 sp -> 57 px, 14 sp -> 36 px), with GPOS kerning.
- Layout is in whole px: dp values round (`roundToPx`), text boxes round up, alignment offsets round.
- Elevation shadow = ambient (alpha 0.088, sigma 0.285·e) + spot (alpha 0.199, sigma 0.861·e, inset 0.686·e,
  shifted down by e·y / (398 dp − e)); fitted on the elevated card (1 dp) and extended FAB (6 dp).
- Status bar: clock 14 sp regular, box centred at (57.9, 26.9) dp; battery 23.8 x 12.9 dp pill + 1.6 x 6.5 dp nub
  ending at x 382.9 dp; wifi two arcs + dot in a 16 x 11.8 dp box; icons black on light apps, white on dark.
- Navigation handle 108 x 4 dp: black at 60% (light), white at 92% (dark).
