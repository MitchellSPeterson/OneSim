# devices ground-truth fixtures

Per-device chrome references for `DEVICES` in `crates/onesim-render/src/device.rs`, compared by `scripts/compare-ios.sh`
(`home` and `keyboard-shift` of `examples/swiftui-showcase`, rendered with `--device <slug>`; same < 2% / < 3.5% fallback gates).

Captured on iOS 26.5 simulators via agent-device, one simulator booted at a time: `screenshot --pixel-density 3 --normalize-status-bar`
(9:41, full battery) for `<state>.png`, raw `snapshot --json` for `<state>.json` (rects in pt; key rects are touch frames incl. gaps).
Dynamic Island appears only intermittently under status-bar normalization: retake until it shows (see `../showcase-ios/README.md`).
Notches are part of the bezel and absent from screenshots, so notch profiles have no `island`.
The 17 Pro reference stays in `../showcase-ios`; only it was captured with keyboard suggestion words.

| fixture slug | other models with identical geometry |
|---|---|
| iphone-16 | 15, 15 Pro, 14 Pro |
| iphone-16-plus | 15 Plus, 15 Pro Max, 14 Pro Max |
| iphone-17-pro-max | 16 Pro Max |
| iphone-air | - |
| iphone-16e | 17e, 14, 13, 13 Pro, 12, 12 Pro (notch) |
| iphone-14-plus | 13 Pro Max, 12 Pro Max (notch) |
| iphone-13-mini | 12 mini (375x812 pt at 2.88 px/pt, 1080x2340 px) |
| iphone-11-pro | - |
| iphone-11-pro-max | - |
| iphone-11 | (414x896 @2x) |
| iphone-se-3 | SE 2 (home button, classic status bar) |
| (showcase-ios) iphone-17-pro | 17, 16 Pro |
