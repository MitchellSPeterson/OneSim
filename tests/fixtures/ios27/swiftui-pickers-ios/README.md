# SwiftUI picker references

Captured from `examples/swiftui-pickers` on the iPhone 18 Pro simulator running
iOS 27.2. Screenshots are 1206 × 2622 px. The app uses GMT and fixed sample
dates so picker values stay the same across runs.

Build the reference with:

```sh
scripts/build-ios-sim.sh examples/swiftui-pickers /tmp/OneSimPickers.app dev.simcore.pickers
```

Install the app on the pinned device and open it through agent-device. Start
each capture with a relaunch. Capture with `screenshot --pixel-density 3
--normalize-status-bar` and repeat until the Dynamic Island is visible.

| State | Actions after launch |
| --- | --- |
| `pickers` | Home |
| `pickers-date` | Tap the Start date at (213, 241) |
| `pickers-time` | Tap the Start time at (322, 241) |
| `graphical` | Tap Graphical |
| `wheel` | Tap Wheel |
| `multi` | Tap Multiple |
| `colors` | Tap the Tint color well at (352, 492) |
