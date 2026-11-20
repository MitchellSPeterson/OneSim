# swiftui-layout-ios ground-truth fixtures (iOS 27.2)

`examples/swiftui-layout` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-19. Compared by `scripts/compare-ios.sh` (SwiftUI layout block) against
`onesim run examples/swiftui-layout --ios 27.2`. The iOS 26.5 set is in `../../swiftui-layout-ios`.

## Capture steps
Same as the 26.5 README: `scripts/build-ios-sim.sh`, then agent-device `install`, `settings appearance`, `open --relaunch`,
and `screenshot --pixel-density 3 --normalize-status-bar` until the island is drawn on the light shots.

## States
| state | iOS capture | OneSim |
|---|---|---|
| layout | launch | (none) |
| layout-dark | dark, launch | `--dark` |
| layout-paged | `gesture pan 346 658 -320 0 1800` on the horizontal scroller | `--scroll 201,658,320,0` |

## Facts OneSim must match (points)
Same frames as iOS 26.5: "A" at y 322, "Compact" at y 498, geometry text "370" at y 560, scroller at (16, 622) size 370 x 72.
Pages are 320 pt, so one exact page of scroll shows "Two" with a sliver of the next page.
