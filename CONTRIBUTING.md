# Contributing

- `cargo build --workspace && cargo test --workspace && cargo clippy --workspace` must pass.
- Swift side: `frameworks/swiftui/Tests/wire_test.sh` must pass (macOS).
- Protocol changes: update `docs/protocol.md` and `simcore-protocol` together; breaking changes bump `PROTOCOL_VERSION`.
- New SwiftUI API: add it to `docs/swiftui-compatibility.md`.
- No crate or abstraction without a real user of it.
