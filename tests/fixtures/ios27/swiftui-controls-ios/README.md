# swiftui-controls-ios ground-truth fixtures (iOS 27.2)

`examples/swiftui-controls` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI controls block) against
`onesim run examples/swiftui-controls --ios 27.2`. The iOS 26.5 set is in `../../swiftui-controls-ios`.

## Capture steps
Same as the 26.5 README: `scripts/build-ios-sim.sh`, then agent-device `install`, `settings appearance`, `open --relaunch`,
the same taps, and `screenshot --pixel-density 3 --normalize-status-bar` until the island is drawn on the light shots
(it shows in only ~5% of iOS 27 screenshots) and no status-bar back-link is shown.

## States
The 10 states of the 26.5 set, with the same taps and OneSim flags.

## Facts OneSim must match
The frames listed in the 26.5 README were measured on this release.
