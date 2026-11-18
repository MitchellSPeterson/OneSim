# swiftui-tabs-ios ground-truth fixtures

`examples/swiftui-tabs` built against Apple's SwiftUI and run on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3,
PNGs 1206 x 2622). Compared by `scripts/compare-ios.sh` (SwiftUI TabView block) against `simcore run examples/swiftui-tabs`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-tabs target/ios/Tabs.app dev.simcore.tabs`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.tabs target/ios/Tabs.app`,
   `settings appearance light`, `open dev.simcore.tabs --relaunch`, wait 3 s. Taps are `press X Y` in points; wait 2-3 s after each.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn
   (light: pixel (603,100) black and unlike (300,100); 1-10 tries). Dark: `settings appearance dark`, relaunch; the island is invisible there.
4. `snapshot --json > <name>.json` beside a PNG records the accessibility frames.

## States
| state | iOS capture | SimCore |
|---|---|---|
| tabs-home | launch | (none) |
| tabs-home-dark | dark, launch | `--dark` |
| tabs-second | press Counter tab (201,822), press Increment once | `--click 201,822 --click Increment` |
| tabs-pushed | launch, press Apple row (200,193) | `--click Apple` |
| tabs-settings-dark | dark, launch, press Settings tab (287,822) | `--dark --click 287,822` |

## Facts SimCore must match (points)
- Same floating glass tab bar as `../rn-router-ios` (NativeTabs): a11y TabBar frame 0,791 402x83; with three tabs the
  items are 94x54 at x 68 / 154 / 240 (a11y y 795; drawn 797 in the PNGs), capsule 274 wide.
- Tab bars draw the `.fill` variant of each symbol (a11y names `home`, `favorite`, `gearshape.fill`); selected = accent symbol and title
  on a gray capsule, others black / white.
- `.badge(3)`: red disc 18 pt (x 296..314, y 797..815 in PNG pixels / 3), light #FF383C, dark #FF4245, white 12 pt digit.
- Content insets: lists run under the bar to the bottom; non-scrolling content centres between the top inset (62 pt, plus any nav bar)
  and y 791 ("Count: 1" / "You picked Apple").
