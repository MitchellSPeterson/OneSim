# Contributing

- `cargo build --workspace && cargo test --workspace && cargo clippy --workspace` must pass.
- Swift side: `frameworks/swiftui/Tests/wire_test.sh` must pass (macOS).
- Protocol changes: update `docs/protocol.md` and `simcore-protocol` together; breaking changes bump `PROTOCOL_VERSION`.
- New SwiftUI API: add it to `docs/swiftui-compatibility.md`.
- No crate or abstraction without a real user of it.

## Visual fidelity rule

UI matched against the iOS Simulator by pixel diff passes only when **every state is < 2%** (pixels whose difference luminance exceeds 24/255).
- Check: `scripts/compare-ios.sh`. It exits non-zero on any state at or above 2%, and CI enforces it.
- A new UI element needs an iOS reference state in `tests/fixtures/showcase-ios/` (see its README for capture steps) and a matching `shot` line in `scripts/compare-ios.sh`.
- Don't raise the threshold or drop a state to make a change pass; fix the rendering. Use `SIMCORE_MAX_DIFF` only to explore locally.
