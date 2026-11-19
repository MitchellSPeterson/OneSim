# showcase-ios (iOS 27.2) ground-truth fixtures

`examples/swiftui-showcase` built with `scripts/build-ios-sim.sh examples/swiftui-showcase target/ios/Showcase.app` (Xcode 27.2)
and captured on the iPhone 18 Pro simulator, iOS 27.2 (24B8117), via agent-device: one `<name>.png` + `<name>.json` per state,
the same 37 states and taps as `../../showcase-ios/README.md` (compare-ios.sh `shot` lines). See `../README.md` for the
capture notes and the list of iOS 27 differences.

Measured on these captures (pt):
- Rows: labels at x 36 (26.5: 32), chevrons at x 356.7 (360.7), trailing values 4 pt further in; cards, headers, row heights unchanged.
- Keyboard (`todos-typed`): predictions bar 546..590, keys from y 597 (54 tall, 39.3 wide), emoji/dictation unchanged at y 805.7.
- Glass (light, centre pixels): purple (203,48,224) -> (255,133,255), pink (255,45,85) -> (255,131,169), blue (0,136,255) -> (84,204,255),
  back button over (242,242,247) -> (246,246,250); dark: black -> 32, purple (219,52,242) -> (245,61,255). Clear: purple -> (237,73,255).
- Alert dark: card ~(35..43), buttons (71,71,73), message (120,121,124), destructive (255,103,106). Popover dark: buttons (80,80,82),
  message (128,130,134), destructive (255,109,112).
