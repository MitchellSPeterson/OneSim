# swiftui-animation-ios ground-truth fixtures

`examples/swiftui-animation` built against Apple's SwiftUI and run on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI animation block) against
`onesim run examples/swiftui-animation`. The iOS 27.2 set is in `../ios27/swiftui-animation-ios`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-animation target/ios/Animation.app dev.simcore.animation`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.animation target/ios/Animation.app`,
   `settings appearance light`, `open dev.simcore.animation --relaunch`, wait 2 s. Every state starts from a relaunch.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn
   (light: pixel (603,100) black and unlike (300,100)). Dark: `settings appearance dark`, relaunch; the island is invisible there.
4. `snapshot --json > animation.json` and `animation-active.json` record the accessibility frames.

## States
| state | iOS capture | OneSim |
|---|---|---|
| animation | launch | (none) |
| animation-dark | dark, launch | `--dark` |
| animation-active | tap `Beta`, `Toggle Card`, `Increment` and wait to settle | `--click Beta --click "Toggle Card" --click Increment --wait 600` |

## Facts OneSim must match (points)
- Large-title `NavigationStack` titled "Animation". The `VStack(spacing: 16)` is centred vertically under the bar.
- Segment row (`Alpha`, `Beta`, `Gamma`) uses `.matchedGeometryEffect(id: "tab", in: ns)` on a capsule background; `Alpha` starts active at (76, 364) 76 x 36, and tapping `Beta` moves the capsule to (160, 328) 67 x 36 once the card opens.
- `Toggle Card` and `Increment` sit side by side at y 416 (launch) / y 380 (active).
- Tapping `Toggle Card` inserts the 240 x 56 green card ("Animated Card" at y 448) with `.transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity), removal: .opacity))`.
- Tapping `Increment` transitions `Count: 0` -> `Count: 1` with `.contentTransition(.numericText())`.
- `PhaseAnimator` ("Phase Ready" at y 520 / 557), `KeyframeAnimator` ("Keyframe Ready" at y 580 / 617), and `TimelineView(.explicit([fixedDate]))` ("Timeline 0" at y 629 / 665) render in their resting states.
