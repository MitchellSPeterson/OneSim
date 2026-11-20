[OneSim](README.md) / [Documentation](docs/README.md)

# Contributing to OneSim

For a missing framework API, check the [coverage table](docs/compatibility/README.md), then open an [API request](https://github.com/MitchellSPeterson/OneSim/issues/new?template=api-request.yml) or pick up the issue linked from that row. Keep the issue link in the coverage row.

## Pick up a coverage batch

Missing APIs are grouped into batches by platform and area, as sub-issues of four parents: [SwiftUI (iOS)](https://github.com/MitchellSPeterson/OneSim/issues/30), [Expo / React Native (iOS)](https://github.com/MitchellSPeterson/OneSim/issues/31), [Expo / React Native (Android)](https://github.com/MitchellSPeterson/OneSim/issues/32) and [Jetpack Compose (Android)](https://github.com/MitchellSPeterson/OneSim/issues/33). Each batch is one pull request into `main`, and its acceptance criteria are the checklist.

1. Pick a batch with no open blockers and no assignee, then assign yourself:

   ```sh
   gh api --paginate 'repos/MitchellSPeterson/OneSim/issues?labels=ready-for-human&state=open&assignee=none&per_page=100' \
     --jq '.[] | select(.issue_dependencies_summary.blocked_by == 0) | "#\(.number) \(.title)"'
   ```

2. Branch from `main` and start from a fresh context (a new agent session, or `/clear`).
3. Build it test-first. With the agent skills, run `/implement #<issue>`: it runs `/tdd`, the test suite and `/code-review`. Behaviour without a visible result needs a test that fails if it breaks.
4. For new UI, [add a visual reference](#add-a-visual-reference) and keep it under the [visual fidelity rule](#visual-fidelity-rule).
5. Open a pull request with `Closes #<issue>` and before/after screenshots. CI runs fast checks for what the change touches on every commit. Add the `ready-to-merge` label when the PR is ready: that runs the full pre-merge suites, and they run again on any later commit ([CI](docs/ci.md)). Merging closes the issue and unblocks the batches that wait on it.

Found an API that isn't in any table? Add a ⬜ row and open a new issue instead of growing the batch. Leave blocking links as they are.

## Run the checks

To run only the checks your branch needs, use `scripts/ci-local.sh`. It diffs against `origin/main` and maps the changed paths to checks with the same selector as CI (`scripts/ci_select.py`). A SwiftUI-only change skips the Android, Expo and other web checks. Rust crates follow the Cargo dependency graph, so a change to `onesim-web` skips the native comparisons. See [CI](docs/ci.md) for the levels and the required checks.

```sh
scripts/ci-local.sh --explain               # what would run, and why
scripts/ci-local.sh --affected --level fast # quick checks: compile, targeted tests, smoke states
scripts/ci-local.sh --affected              # the pre-merge checks: every affected suite in full
scripts/ci-local.sh --all                   # everything, as nightly
```

`SIMCORE_ONLY=swiftui` or `SIMCORE_ONLY=expo` limits `scripts/compare-ios.sh` to one framework's states.

The full set of checks:

From the repository root, run the Rust and Swift checks. The Swift wire test requires macOS and exercises guest messages, memory use, and stdout isolation.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
frameworks/swiftui/Tests/wire_test.sh
node scripts/api-coverage.mjs --check
```

For Expo changes, use the guest checks in the [React Native guide](frameworks/react-native/README.md). For renderer changes, run the pixel comparisons below.

For native GPU changes, run the display-list and rendering tests on a machine with a usable wgpu adapter.

```sh
cargo test -p onesim-render-gpu
cargo clippy -p onesim-cli --features window --all-targets -- -D warnings
```

For WebGPU changes, build the SwiftUI showcase and run `node web/test/presenter.mjs --app`. The presenter test also exercises Canvas 2D fallback. [Browser test setup](docs/web.md#testing)

## Update the docs with your change

| Change | Update |
| --- | --- |
| Protocol | Update the [protocol reference](docs/protocol.md) and [`onesim-protocol`](crates/onesim-protocol) together. Bump `PROTOCOL_VERSION` for a breaking change. |
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

The reference comparisons use the CPU renderer. In a shell with a shared `SIMCORE_RENDERER` setting, set it to `cpu` before running the scripts.

```sh
scripts/compare-ios.sh
SIMCORE_FALLBACK=1 scripts/compare-ios.sh
scripts/compare-android.sh
scripts/compare-expo-android.sh
```

The iOS commands check iOS 27.2, the only release gated for now. Set `SIMCORE_IOS="26.5 27.2"` to also check the iOS 26.5 references. Install the Expo example dependencies to include the React Native and Expo Router screenshots. The Android Expo command requires `npm ci` in `examples/expo-showcase`. See the [iOS comparison results](docs/ios-comparison.md) for examples and known gaps.

Fix the rendering when a state fails. Use `SIMCORE_MAX_DIFF` only for local investigation. Keep the checked-in thresholds and states.

## Add a visual reference

1. For a new iOS UI element, capture the state on [iOS 27.2 with iPhone 18 Pro](tests/fixtures/ios27/README.md). An [iOS 26.5 with iPhone 17 Pro](tests/fixtures/showcase-ios/README.md) capture is optional while only iOS 27.2 is gated. Add a matching `shot` line to [`scripts/compare-ios.sh`](scripts/compare-ios.sh).
2. For a new Compose component, capture the state on the [Android 17 Pixel 10a emulator](tests/fixtures/compose-android/README.md) using [`capture.sh`](tests/fixtures/compose-android/capture.sh). Add a matching `shot` line to [`scripts/compare-android.sh`](scripts/compare-android.sh).
3. For a React Native Android component, use the [Expo Android capture guide](tests/fixtures/expo-android/README.md) and add a matching `shot` line to [`scripts/compare-expo-android.sh`](scripts/compare-expo-android.sh).
