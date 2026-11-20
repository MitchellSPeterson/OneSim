# swiftui-type-ios ground-truth fixtures (iOS 27.2)

`examples/swiftui-type` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI type block) against
`onesim run examples/swiftui-type --ios 27.2`. The iOS 26.5 set is in `../../swiftui-type-ios`.

## Capture steps
Same as the 26.5 README: `scripts/build-ios-sim.sh`, then agent-device `install`, `settings appearance`, `open --relaunch`,
and `screenshot --pixel-density 3 --normalize-status-bar` until the island is drawn on the light shot.
Light: pixel (603,100) black and unlike (301,100). Dark: the island is invisible; the status bar has no previous-app control.
The clock is 9:41.

## States
| state | iOS capture | OneSim |
|---|---|---|
| type | launch | (none) |
| type-dark | dark, launch | `--dark` |

## Facts OneSim must match (points)
Same rows as iOS 26.5. Middle truncation of the sample sentence is `The quic…e lazy dog`.
