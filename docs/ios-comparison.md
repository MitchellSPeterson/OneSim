# SimCore vs iOS Simulator

Same unmodified source (`examples/swiftui-showcase/Showcase.swift`) rendered by:
- **iOS 26.5 Simulator**, iPhone 17 Pro (Apple's SwiftUI), shots in `tests/fixtures/showcase-ios/`
- **SimCore** (SimCore's SwiftUI module + Rust renderer, headless), 402×874 @3x

Reproduce: `scripts/compare-ios.sh` (needs Python + Pillow). Re-capture iOS references: `scripts/build-ios-sim.sh`, then follow `tests/fixtures/showcase-ios/README.md`.

| state | pixels >10% off | mean abs diff | notes |
|---|---:|---:|---|
| controls-changed | 1.9% | 3.8 | status bar icons, text antialiasing |
| controls | 1.8% | 3.8 | status bar icons, text antialiasing |
| home-dark | 0.8% | 1.0 | status bar icons, text antialiasing |
| home | 1.9% | 4.0 | status bar icons, text antialiasing |
| shapes | 0.4% | 0.7 | status bar icons, text antialiasing |
| todos-toggled | 1.8% | 3.5 | status bar icons, text antialiasing |
| todos-typed | 1.6% | 8.3 | iOS software keyboard not simulated |
| todos | 0.6% | 0.6 | status bar icons, text antialiasing |
| typography | 2.8% | 5.3 | glyph-width drift (no SF `opsz` axis) |

Every state also differs by the Dynamic Island and the charging bolt. SimCore doesn't draw either.

![home](ios-comparison/cmp-home.png)
![controls](ios-comparison/cmp-controls-changed.png)
![todos](ios-comparison/cmp-todos-typed.png)
![shapes](ios-comparison/cmp-shapes.png)
![typography](ios-comparison/cmp-typography.png)
![dark](ios-comparison/cmp-home-dark.png)

## Fixed by this comparison

- **Spacers in stacks are sized last.** Before, `HStack { Text; Spacer }` split the space evenly, so text wrapped at half width.
- **System colors now use the iOS 26 palette.** Red #FF383C, orange #FF8D28, blue #0088FF, purple #CB30E0.

## Known gaps

- No software keyboard.
- The Liquid Glass back button is an approximation.
- Text widths can drift 1–2 px from iOS.
- Shadows use a box blur.
- Corners are circular, not continuous.
