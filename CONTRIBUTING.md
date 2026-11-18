# Contributing

- `cargo build --workspace && cargo test --workspace && cargo clippy --workspace` must pass.
- Swift side: `frameworks/swiftui/Tests/wire_test.sh` must pass (macOS).
- Protocol changes: update `docs/protocol.md` and `simcore-protocol` together; breaking changes bump `PROTOCOL_VERSION`.
- New or changed Expo / React Native API: update its row in `docs/compatibility/expo/` (the area file) (iOS and Android columns, link the issue) and run `node scripts/api-coverage.mjs`. CI runs it with `--check`, which fails when the summary is stale or a module in `frameworks/react-native/guest/modules.js` is missing from the docs or disagrees with its status. Native modules go in that registry with status `implemented` / `simulated` / `unsupported`; never report fake success.
- New or changed SwiftUI API: update its row in `docs/compatibility/swiftui/` (the area file; signatures and limits go in `guest.md`) and run `node scripts/api-coverage.mjs`.
- New or changed Jetpack Compose API: update its row in `docs/compatibility/compose/` (the area file) and run `node scripts/api-coverage.mjs`.
- Missing API: open an [API request](https://github.com/MitchellSPeterson/SimCore/issues/new?template=api-request.yml) issue and link it from the coverage row. Expo guest checks: `node frameworks/react-native/guest/run-*.mjs` (see `frameworks/react-native/README.md`).
- No crate or abstraction without a real user of it.

## Visual fidelity rule

UI matched against the iOS Simulator by pixel diff passes only when **every state is under the threshold**. A pixel counts as different when its difference luminance exceeds 24/255.

| Mode | Fonts / symbols | Threshold |
|---|---|---|
| system (macOS) | Apple SF Pro + SF Symbols | **< 2%** |
| fallback (non-macOS, or `SIMCORE_FALLBACK=1`) | bundled Inter + Phosphor | **< 3.5%** |

The fallback allowance is the measured, untunable glyph-shape gap: Inter's widths are fitted to SF and fallback runs up to ~1.3 pt above system mode.
- Check: `scripts/compare-ios.sh` and `SIMCORE_FALLBACK=1 scripts/compare-ios.sh`. Each exits non-zero on any failing state, and CI runs both.
- A new UI element needs an iOS reference state in `tests/fixtures/showcase-ios/` (see its README for capture steps) and a matching `shot` line in `scripts/compare-ios.sh`.
- Exceptions (maintainer-approved, `STATE_MAX` in `scripts/compare_ios.py`): `typography-scrolled(-dark)` in fallback mode have a temporary < 5% limit (see docs/ios-comparison.md Known gaps); `rn-showcase-ios/text` in system mode has < 2.5% (dense text; layout matches within 1–2 px, the rest is glyph rasterization).
- Don't raise the threshold or drop a state to make a change pass; fix the rendering. Use `SIMCORE_MAX_DIFF` only to explore locally.
