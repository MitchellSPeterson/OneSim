# swiftui-gestures-ios ground-truth fixtures

`examples/swiftui-gestures` built against Apple's SwiftUI and run on the iPhone 18 Pro simulator, iOS 27.2 (runtime 24B8117) (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-19. Compared by `scripts/compare-ios.sh` (SwiftUI gestures block) against
`onesim run examples/swiftui-gestures`. The iOS 26.5 set is in `../../swiftui-gestures-ios`. Captured on a dedicated "OneSim Gestures iPhone 18 Pro" simulator (same device type and runtime) while the shared one was busy.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-gestures target/ios/Gestures.app dev.simcore.gestures`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.gestures target/ios/Gestures.app`,
   `settings appearance light`, `open dev.simcore.gestures --relaunch`, wait 2 s. Every state starts from a relaunch.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn
   (light: pixel (603,100) black and unlike (300,100)). Dark: `settings appearance dark`, relaunch; the island is invisible there.
4. `snapshot --json > <name>.json` beside some PNGs records the accessibility frames.

## States
| state | iOS capture | OneSim |
|---|---|---|
| gestures | launch | (none) |
| gestures-dark | dark, launch | `--dark` |
| gestures-dragged | `gesture pan 201 269 100 0 600` (from "Drag me") | `--drag "Drag me,100,0"` |
| gestures-long-pressed | `longpress 201 393 1000` ("Hold") | `--long-press Hold` |
| gestures-double-tapped | `press 201 517 --count 2` ("Double-tap") | `--click Double-tap --click Double-tap` |
| gestures-pinched | `gesture pinch 1.5 201 654` ("Pinch or rotate") | `--pinch "Pinch or rotate,1.5"` |

## Facts OneSim must match (points)
- Non-scrolling content in a `NavigationStack` with a large title is centred below the bar: "Drag me" text at y 259, the
  `VStack(spacing: 16)` rows follow (texts at 321, 383, 445, 507, 569, 644, 718, 754).
- `.offset(x: 100)` moves the card's drawing and its touch area, not the layout: "Offset 100, 0" stays centred under it.
- DragGesture translation is measured from the touch-down point (`gesture pan` by 100 pt gives exactly 100).
- No rotation state: `agent-device gesture rotate` (and `transform` with an angle) never reaches SwiftUI's `RotateGesture` on the
  simulator (tried 30, 45, 90 and -60 degrees; the card stays "Not turned" while `pinch` works). Rotation is covered by the
  guest wire test (`frameworks/swiftui/Tests/GestureLab.swift`) and the host's touch synthesis tests instead.
