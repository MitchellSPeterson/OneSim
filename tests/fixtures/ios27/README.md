# iOS 27.2 reference screenshots

The iOS 27 reference set, captured 2026-11-18/19 on the **iPhone 18 Pro** simulator, iOS 27.2 (24B8117), Xcode 27.2
(402 x 874 pt, scale 3, PNGs 1206 x 2622). Same apps, states and capture method as the iOS 26.5 set one level up
(`../showcase-ios`, `../swiftui-tabs-ios`, `../swiftui-gestures-ios`, `../swiftui-shapes-ios`, `../swiftui-type-ios`, `../rn-showcase-ios`, `../rn-router-ios`, `../devices`); each subfolder's README
lists only what differs. `scripts/compare-ios.sh` renders every state with `--ios 27.2 --device iphone-18-pro`.

| folder | app | states |
|---|---|---|
| `showcase-ios` | `examples/swiftui-showcase` | the 37 states of `../showcase-ios` |
| `swiftui-tabs-ios` | `examples/swiftui-tabs` | the 5 states of `../swiftui-tabs-ios` |
| `swiftui-gestures-ios` | `examples/swiftui-gestures` | the 6 states of `../swiftui-gestures-ios` |
| `swiftui-shapes-ios` | `examples/swiftui-shapes` | the 2 states of `../swiftui-shapes-ios` |
| `swiftui-type-ios` | `examples/swiftui-type` | the 2 states of `../swiftui-type-ios` |
| `rn-showcase-ios` | `examples/expo-showcase` in Expo Go 58.0.7 | the 18 states of `../rn-showcase-ios` |
| `rn-router-ios` | `examples/expo-default`, `examples/expo-router-stack` in Expo Go 58.0.7 | the 9 states of `../rn-router-ios` |
| `devices` | `examples/swiftui-showcase` `home` + `keyboard-shift` | iPhone 18 Pro Max, 17e, Duo |

## Capture notes (differences from iOS 26.5)
- Dynamic Island: with `--normalize-status-bar` it shows in only ~5% of iOS 27 screenshots (vs ~50% on 26.5). Keep
  re-shooting (up to 150 tries; light: pixel (603,100) black and unlike (301,100)). Dark PNGs cannot show it.
- `agent-device type` sometimes leaves the keyboard dismissed on iOS 27. Check the snapshot has `Typing Predictions`
  before the screenshot and redo the state if not (keyboard-dark, controls-typed, keyboard-avoiding(-dark) were retaken).
- Scrolls: use slow exact drags (`scroll down --pixels N --duration-ms 1500..2500`), not flings. typography-scrolled(-dark):
  `--pixels 50` = 40 pt (Large Title 132 -> 92 at Dynamic Type AX5); long-list-collapsed `--pixels 62` = 52 pt;
  long-list-scrolled `--pixels 700 --duration-ms 2500` = 687.67 pt (Row 11 at y 40.67; compare-ios.sh uses that offset).
- Expo Go 58.0.7 for the simulator comes from `https://api.expo.dev/v2/versions` (`sdkVersions["58.0.0"].iosClientUrl`),
  installed with `agent-device install host.exp.Exponent Exponent.app`. Open projects from Expo Go's home (Recently
  opened) or with Expo Go in front: opening the `exp://` URL from another app adds a status-bar back-link. Turn the Tools
  button off once (gear -> Tools button).

## What iOS 27 draws differently (implemented in `crates/onesim-render/src/look.rs` and per-release branches)
- Inset-grouped lists: row content, headers and separators 20 pt inside the card on every width (26.5: the 16/20 pt margin).
- Status bar (18 Pro): clock 18 pt centred at x 81; solid green battery with a round nub, chunkier wifi arcs, smaller
  signal dots; island 95 pt wide (x 153.3..248.3). Other devices' status bars moved too (see `devices/`).
- Keyboard: 7 pt shorter on the 6.1/6.3" phones (panel top 546 vs 539), 2 pt taller on the Max; letters follow shift
  (lower case after typing); the first suggestion is the typed word in quotes ("milk"); dark panel 32 / keys 68.
- Liquid Glass: no outer shadow; a ~0.7 pt dark hairline just outside the shape (strongest on vertical edges); highlights
  along the top and bottom edges; a more saturated, brighter backdrop (fits in `glass.rs`). Dark glass brightens.
- Scroll edge: content under a bar is veiled ~0.88 right down to the bar's bottom, with a 1 px hairline there, and
  nothing below it (26.5: a veil fading over 32 pt).
- Alerts / confirmation popovers in dark: lighter buttons, dimmer message, pinker destructive label; the back
  button is not grayed under a presentation.
- Tab bar: the selected item's capsule sinks into the bar (light 225, dark 10; 26.5 raised it).
- Expo Go shows no home indicator.
