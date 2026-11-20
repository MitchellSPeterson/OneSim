# swiftui-animation-ios ground-truth fixtures (iOS 27.2)

`examples/swiftui-animation` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI animation block) against
`onesim run examples/swiftui-animation --ios 27.2`. The iOS 26.5 set is in `../../swiftui-animation-ios`.

## Capture steps
Same as the 26.5 README: `scripts/build-ios-sim.sh`, then agent-device `install`, `settings appearance`, `open --relaunch`,
and `screenshot --pixel-density 3 --normalize-status-bar` until the island is drawn on the light shots.

## States
| state | iOS capture | OneSim |
|---|---|---|
| animation | launch | (none) |
| animation-dark | dark, launch | `--dark` |
| animation-active | tap `Beta`, `Toggle Card`, `Increment` and wait to settle | `--click Beta --click "Toggle Card" --click Increment --wait 600` |

## Facts OneSim must match (points)
Same frames and settled layout as iOS 26.5: segment pill (`Alpha` -> `Beta`) via `.matchedGeometryEffect(id: "tab", in: ns)`, `Toggle Card` inserts the 240 x 56 green `.transition(.asymmetric(...))` card, `Increment` updates `Count: 0` -> `Count: 1` with `.contentTransition(.numericText())`, and `PhaseAnimator`, `KeyframeAnimator`, and `TimelineView(.explicit([fixedDate]))` render in their resting states.
