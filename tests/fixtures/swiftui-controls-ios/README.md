# swiftui-controls-ios ground-truth fixtures

`examples/swiftui-controls` built against Apple's SwiftUI and run on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3,
PNGs 1206 x 2622), captured 2026-11-20. Compared by `scripts/compare-ios.sh` (SwiftUI controls block) against
`onesim run examples/swiftui-controls`. The iOS 27.2 set is in `../ios27/swiftui-controls-ios`.

## Capture steps
1. `scripts/build-ios-sim.sh examples/swiftui-controls target/ios/Controls.app dev.simcore.controls`
2. With the T3 device tools (`device_open`, then its `agent-device` command): `install dev.simcore.controls target/ios/Controls.app`,
   `settings appearance light|dark`, `open dev.simcore.controls --relaunch`, wait 2.5 s, then the taps below (1 s apart).
   Every state starts from a relaunch.
3. Each PNG: `screenshot <name>.png --pixel-density 3 --normalize-status-bar` (absolute path), retaken (relaunching) until the
   Dynamic Island is drawn on light shots (pixel (603,100) black and unlike (301,100)) and no status-bar back-link
   ("< Expo Go" under the clock, left when another app opened this one) is shown.
4. `snapshot --json > <name>.json` records the accessibility frames.

## States
| state | iOS capture (taps in points) | OneSim |
|---|---|---|
| controls | launch | (none) |
| controls-dark | dark, launch | `--dark` |
| controls-menu | hold 201,304 (Actions) | `--long-press Actions` |
| controls-changed | press "Bold", 201,620 (I agree), 201,519 (Pill 0) | `--click Bold --click "I agree" --click "Pill 0"` |
| pickers | press "Pickers" | `--click Pickers` |
| pickers-dark | dark, press "Pickers" | `--dark --click Pickers` |
| pickers-pushed | press "Pickers", 200,449 (Size) | `--click Pickers --click Size` |
| pickers-changed | press "Pickers", 200,287 (Green), 311,563 (Star), 201,804 (10:00 on the wheel) | `--click Pickers --click Green --click Star --click 10:00` |
| indicators | press "Indicators" | `--click Indicators` |
| indicators-dark | dark, press "Indicators" | `--dark --click Indicators` |

## Facts OneSim must match (points, measured on iOS 27.2; 26.5 matches within a point)
- `Menu`: a 250 pt glass card centred on its button, top 10.5 pt above the button's centre; 42 pt rows, 10 pt padding;
  icons centred 39.5 pt in, text at 63 pt; a `Divider` opens a 21 pt gap with a 1 pt separator inset 24 pt; destructive red.
- `.toggleStyle(.button)`: bordered-button metrics (12 x 7 pt padding), accent label; on, an accent capsule at ~16%.
- `NavigationLink` outside a `List` is a plain accent button.
- Linear `ProgressView`: label 4.67 pt above the 4 pt bar; `currentValueLabel` (12 pt, secondary) 4 pt below it.
  `.circular` with a value is the spinner.
- `Gauge`: default and `.linearCapacity` 16 pt capacity bar (track (239,239,240)), label centred 10.4 pt above,
  min/max labels 8 pt beside, value 6.3 pt below; `.accessoryLinearCapacity` 4 pt bar, label leading 4 pt above;
  `.accessoryLinear` 8 pt black bar with a 7.33 pt dot in a 16 pt cut-out, no label; circular styles 58 pt with a
  5.7 pt stroke: capacity ring (track black at 35%) from the top, open arc +-140 degrees with a dot at the value.
- Wheel picker in a `Form` row: 216 pt tall; the selection in a 32 pt band inset 16.67 pt; rows 30 / 56 / 75 pt from the
  centre, squashed to 0.915 / 0.745 / 0.45; unselected rows black at ~35%.
- Slider value labels sit 11 pt from the track.
