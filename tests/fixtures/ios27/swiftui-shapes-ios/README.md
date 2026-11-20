# swiftui-shapes-ios ground-truth fixtures

`examples/swiftui-shapes` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (runtime 24B8117) (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-19. Compared by `scripts/compare-ios.sh` (SwiftUI shapes block) against
`onesim run examples/swiftui-shapes`. The iOS 26.5 set is in `../../swiftui-shapes-ios`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-shapes target/ios/Shapes.app dev.simcore.shapes`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.shapes target/ios/Shapes.app`,
   `settings appearance light`, `open dev.simcore.shapes --relaunch`, wait 2 s.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the full Dynamic Island is drawn
   (light: pixels `(603, 95..130)` black and `(300, 100)` white). Dark: `settings appearance dark`, relaunch.
4. `snapshot --json > <name>.json` records the accessibility frames.

## States
| state | iOS capture | OneSim |
|---|---|---|
| shapes | launch | (none) |
| shapes-dark | dark, launch | `--dark` |

## What this exercises
- `Ellipse` (true oval, not a capsule)
- `UnevenRoundedRectangle` (continuous corners, four distinct radii)
- `InsettableShape.strokeBorder` chained on `ShapeView` (`.fill(...).strokeBorder(...)`)
- `ContainerRelativeShape` and `ConcentricRectangle` inside `.containerShape(RoundedRectangle(cornerRadius: 26))` with 10 pt padding
- Custom `Shape` (`StarShape.path(in:)`) via host-to-guest `layout` feedback
- `AnyShape` with `.rotation` and `.scale`, stroked with `StrokeStyle` (dash, round cap, round join)
- `Path` with `FillStyle(eoFill: true)` cutting a concentric hole
- `Shape.trim`, `.offset`, `.union`, `.subtracting`, and `.intersection`
