[SimCore](README.md) / [Documentation](docs/README.md)

# Contributing to SimCore

For a missing framework API, check the [coverage table](docs/compatibility/README.md), then open an [API request](https://github.com/MitchellSPeterson/SimCore/issues/new?template=api-request.yml) or pick up the issue linked from that row. Keep the issue link in the coverage row.

## Run the checks

From the repository root, run the Rust and Swift checks. The Swift wire test requires macOS and exercises guest messages, memory use, and stdout isolation.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
frameworks/swiftui/Tests/wire_test.sh
node scripts/api-coverage.mjs --check
```

For Expo changes, use the guest checks in the [React Native guide](frameworks/react-native/README.md). For renderer changes, run the pixel comparisons below.

## Update the docs with your change

| Change | Update |
| --- | --- |
| Protocol | Update the [protocol reference](docs/protocol.md) and [`simcore-protocol`](crates/simcore-protocol) together. Bump `PROTOCOL_VERSION` for a breaking change. |
| SwiftUI API | Update the [coverage row](docs/compatibility/swiftui/README.md) and record signatures and known limits in the [SwiftUI guest reference](docs/compatibility/swiftui/guest.md). |
| Expo / React Native API | Update the iOS and Android columns in the [coverage row](docs/compatibility/expo/README.md). Add native modules to [`guest/modules.js`](frameworks/react-native/guest/modules.js) with the matching `implemented`, `simulated`, or `unsupported` status. Unsupported modules must report an error. |
| Jetpack Compose API | Update the [coverage row](docs/compatibility/compose/README.md). |

Run `node scripts/api-coverage.mjs` after changing a coverage row. The CI check also verifies that every native module in the registry has a coverage row with a matching status.

Add a crate or abstraction when it has a real caller.

## Visual fidelity rule

A pixel counts as different when its difference luminance exceeds 24/255. Every compared state must stay below its threshold.

| Mode | Fonts and symbols | Threshold |
| --- | --- | ---: |
| iOS system, on macOS | Apple SF Pro and SF Symbols | **< 2%** |
| iOS fallback, on other hosts or with `SIMCORE_FALLBACK=1` | Bundled Inter and Phosphor | **< 3.5%** |
| Android | Bundled Roboto | **< 2%** |

Inter’s widths are fitted to SF Pro. The fallback threshold accounts for the remaining difference in glyph shapes. The maintainer-approved exceptions live in `STATE_MAX` in [`scripts/compare_ios.py`](scripts/compare_ios.py).

- The SwiftUI `typography-scrolled` and `typography-scrolled-dark` states have a temporary **< 5%** limit in fallback mode.
- The React Native showcase `text` state has a **< 2.5%** limit in system mode. Dense glyph rasterization accounts for the remaining difference.

Run these checks with Python 3 and Pillow. The Compose comparison also requires Java 17 or later.

```sh
scripts/compare-ios.sh
SIMCORE_FALLBACK=1 scripts/compare-ios.sh
scripts/compare-android.sh
```

The iOS commands run both releases by default. Set `SIMCORE_IOS=27.2` to check one release locally. Install the Expo example dependencies to include the React Native and Expo Router screenshots. See the [iOS comparison results](docs/ios-comparison.md) for examples and known gaps.

Fix the rendering when a state fails. Use `SIMCORE_MAX_DIFF` only for local investigation. Keep the checked-in thresholds and states.

## Add a visual reference

1. For a new iOS UI element, capture the state on [iOS 27.2 with iPhone 18 Pro](tests/fixtures/ios27/README.md) and [iOS 26.5 with iPhone 17 Pro](tests/fixtures/showcase-ios/README.md). Add a matching `shot` line to [`scripts/compare-ios.sh`](scripts/compare-ios.sh).
2. For a new Compose component, capture the state on the [Android 17 Pixel 10a emulator](tests/fixtures/compose-android/README.md) using [`capture.sh`](tests/fixtures/compose-android/capture.sh). Add a matching `shot` line to [`scripts/compare-android.sh`](scripts/compare-android.sh).
