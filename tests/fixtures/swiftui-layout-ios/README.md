# swiftui-layout-ios ground-truth fixtures

`examples/swiftui-layout` built against Apple's SwiftUI and run on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-19. Compared by `scripts/compare-ios.sh` (SwiftUI layout block) against
`onesim run examples/swiftui-layout`. The iOS 27.2 set is in `../ios27/swiftui-layout-ios`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-layout target/ios/Layout.app dev.simcore.layout`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.layout target/ios/Layout.app`,
   `settings appearance light`, `open dev.simcore.layout --relaunch`, wait 2 s. Every state starts from a relaunch.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn
   (light: pixel (603,100) black and unlike (300,100)). Dark: `settings appearance dark`, relaunch; the island is invisible there.
4. `snapshot --json > layout.json` records the accessibility frames.

## States
| state | iOS capture | OneSim |
|---|---|---|
| layout | launch | (none) |
| layout-dark | dark, launch | `--dark` |
| layout-paged | `gesture pan 360 658 -320 0 1800` on the horizontal scroller | `--scroll 201,658,320,0` |

## Facts OneSim must match (points)
- Large-title `NavigationStack`. The `VStack` is centred in the area under the bar: "A" text at y 322, "Compact" at y 498,
  the geometry width "370" at y 560, the horizontal scroller at (16, 622) size 370 x 72.
- `Grid` cells are 64 x 36 with 8 pt spacing. "E" spans two columns and stays 64 pt wide.
- `LazyVGrid` uses two flexible columns. `ViewThatFits` shows "Compact". `GeometryReader` in a height-80 frame reports 370.
- Pages are 320 pt wide, so the 370 pt scroller shows a sliver of the next page. `--scroll` does not tick, so the paged
  shot moves exactly 320 pt and lands on "Two" (the view-aligned stop). The iOS scroll-indicator value at that offset is 54%.
