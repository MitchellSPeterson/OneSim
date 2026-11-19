# devices (iOS 27.2) ground-truth fixtures

Per-device chrome references for `DEVICES` in `crates/simcore-render/src/device.rs` on iOS 27.2 (`home` and `keyboard-shift`
of `examples/swiftui-showcase`, rendered with `--ios 27.2 --device <slug>`). Captured like `../../devices/README.md`,
one simulator per device; all iOS 27 keyboards were captured with the "I / The / I'm" suggestions.

| fixture slug | runtime | measured |
|---|---|---|
| iphone-18-pro-max | 27.2 (24B8117) | 440x956 @3x; island x 172.3..267.3; clock 71.7..107; battery right edge 393.7; keyboard top 609 |
| iphone-17e | 27.2 (24B8117) | 390x844 @3x (notch); clock 46.3..78; battery right edge 363.3; keyboard top 516 |
| iphone-duo | 27.2 seed (24B91016s, the iPhone Duo-only runtime) | unfolded 951x669 @2x, see below |
| (showcase-ios) iphone-18-pro | 27.2 (24B8117) | the default; island x 153.3..248.3 |

iPhone Duo (unfolded; the folded state is not modelled): content is 887 pt wide; the right 64 pt column holds the status
bar (17 pt clock centred at 902.5,40; a green battery ring r 19 around the wifi glyph, bolt in the gap at the top; four
signal dots on an arc at y 92..98) and the back button (48 pt glass circle centred at 902.75,147.75). Navigation bar 24..82
with a 28 pt bold title at x 20, y 31. Home indicator 372..597 x 656.3, 4.7 tall. Keyboard: full-width panel from y 405
(26 pt top corners), "..." more button, three 267 pt suggestion slots from x 75, an 801 pt key block (key pitch 79.2,
caps inset 2 pt / 8 pt, 38.5 pt tall; rows at +49 / +100.7 / +152.7 / +204.3), bottom row 123 / emoji / space / dictation / return.
