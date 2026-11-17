# Contributing

- `cargo build --workspace && cargo test --workspace && cargo clippy --workspace` must pass.
- Swift side: `frameworks/swiftui/Tests/wire_test.sh` must pass (macOS).
- Protocol changes: update `docs/protocol.md` and `simcore-protocol` together; breaking changes bump `PROTOCOL_VERSION`.
- New SwiftUI API: add it to `docs/swiftui-compatibility.md`.
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
- Don't raise the threshold or drop a state to make a change pass; fix the rendering. Use `SIMCORE_MAX_DIFF` only to explore locally.
