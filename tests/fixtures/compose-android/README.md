# compose-android ground-truth fixtures

Real Jetpack Compose (compose-bom 2026.09.00 with material3 1.5.0-beta01, navigation-compose 2.10.0-rc01) running
`examples/compose-counter`, `examples/compose-showcase`, `examples/compose-input`, `examples/compose-interaction` and
`examples/compose-navigation` unmodified, built as one APK by `frameworks/compose/reference-app` and captured on the
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
| input | input | launch with two text fields and a stepped slider |
| input-changed | input | enter NameHello in the first field, dismiss the keyboard, and drag the slider to 75 |
| interaction | interaction | launch: clickable card, combined clickable, draggable / draggable2D boxes, pointerInput, scrollable, selectable group, toggleable row |
| interaction-changed | interaction | card clicked twice, long press, double tap, then a tap; 300 px drags (2D: 300, 100); tap at 299, 61; 300 px scroll; "Two"; Wi-Fi off |
| interaction-pressed | interaction | a finger held on the card (`adb shell input motionevent DOWN`): pressed ripple and `collectIsPressedAsState` |

No `counter-dark`: the counter's `MaterialTheme` has no dark scheme, so only the status-bar icons change.

Drags are injected as `input motionevent` DOWN, 8 equal MOVEs (fractional px), a resting MOVE, UP: AndroidX ignores
the movement carried by the lift, and Android's input resampling would otherwise leave an extrapolated overshoot.
Android 12+ draws the pressed ripple with the platform `RippleDrawable` shader (faint sparkles and a soft edge); OneSim
draws Material's plain circle, within the threshold.

## Recapture

```sh
# AGENT_DEVICE: the agent-device command returned by the T3 device tools' device_open for the Pixel 10a emulator.
AGENT_DEVICE=... GRADLE=gradle tests/fixtures/compose-android/capture.sh
```

`SIMCORE_CAPTURE_ONLY=interaction` recaptures only the interaction states. Needs Gradle 9.3+ (AGP 9.1, built-in Kotlin), the Android 37 SDK platform and Java 17.

## Measured facts used by the renderer

- Text is drawn and measured at `floor(sp × 2.625)` px (22 sp -> 57 px, 14 sp -> 36 px), with GPOS kerning.
- Layout is in whole px: dp values round (`roundToPx`), text boxes round up, alignment offsets round.
- Elevation shadow = ambient (alpha 0.088, sigma 0.285·e) + spot (alpha 0.199, sigma 0.861·e, inset 0.686·e,
  shifted down by e·y / (398 dp − e)); fitted on the elevated card (1 dp) and extended FAB (6 dp).
- Status bar: clock 14 sp regular, box centred at (57.9, 26.9) dp; battery 23.8 x 12.9 dp pill + 1.6 x 6.5 dp nub
  ending at x 382.9 dp; wifi two arcs + dot in a 16 x 11.8 dp box; icons black on light apps, white on dark.
- Navigation handle 108 x 4 dp: black at 60% (light), white at 92% (dark).
| navigation | navigation | launch: medium top app bar, primary tab row, NavHost inbox, navigation bar |
| navigation-detail | navigation | Social tab, "Social message 2" (NavHost pushes `message/2`) |
| navigation-changed | navigation | then the back key (pops to the inbox), then the Search bar item |
| drawer | navigation | DrawerActivity: "Open drawer" (modal drawer sheet with items and a badge) |
| bars | navigation | BarsActivity: large top app bar, secondary scrollable tabs, navigation rail with a header FAB, bottom app bar |
| bars-changed | navigation | Collapse (top bar to its collapsed height), Podcasts tab (row scrolls to centre it), Saved rail item |
| expressive | navigation | ExpressiveActivity: medium flexible top app bar with a subtitle, expanded wide navigation rail, flexible bottom app bar, short navigation bar |

`SIMCORE_CAPTURE_ONLY=navigation` recaptures just the navigation states.
