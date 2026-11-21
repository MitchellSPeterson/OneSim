# SwiftUI navigation references

Build `examples/swiftui-navigation` with `scripts/build-ios-sim.sh` and install the app on each pinned simulator. These PNGs and accessibility snapshots come from the iOS 26.5 iPhone 17 Pro. The matching iOS 27.2 iPhone 18 Pro captures are in `../ios27/swiftui-navigation-ios`.

Use light appearance. Capture `library` at launch. Tap About for `detail`, Legacy for `legacy`, and Search for `search`. On Detail, tap its title for `detail-menu`. Tap the search field for `search-active`. Relaunch between routes. Use agent-device to capture each accessibility snapshot and take a 3× screenshot with `--normalize-status-bar`. Repeat the screenshot until the Dynamic Island is visible.

`scripts/compare-ios.sh` runs these states in OneSim.
