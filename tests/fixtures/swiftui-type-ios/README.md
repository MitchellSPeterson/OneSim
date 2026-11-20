# swiftui-type-ios ground-truth fixtures

`examples/swiftui-type` built against Apple's SwiftUI and run on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI type block) against
`onesim run examples/swiftui-type`. The iOS 27.2 set is in `../ios27/swiftui-type-ios`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-type target/ios/Type.app dev.simcore.type`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.type target/ios/Type.app`,
   `settings appearance light`, `open dev.simcore.type --relaunch`, wait 2 s. Every state starts from a relaunch.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn
   (light: pixel (603,100) black and unlike (300,100)). Dark: `settings appearance dark`, relaunch; the island is invisible there.
   A previous-app control in the status bar means `home`, then `open --relaunch`, then another shot.

## States
| state | iOS capture | OneSim |
|---|---|---|
| type | launch | (none) |
| type-dark | dark, launch | `--dark` |

## Facts OneSim must match (points)
- Large-title `NavigationStack`. The two stacks sit under the bar, centred, 16 pt padding, 8 pt row spacing.
- Each row is a 76 pt caption and a 17 pt sample: tracking, kerning, baseline offset, serif and rounded, condensed width,
  tabular digits, secondary text scale, head / tail / middle truncation of one line in a 160 pt frame, a dotted underline,
  uppercase, three symbol modes, and an 8 x 8 image centred in a 32 pt frame.
