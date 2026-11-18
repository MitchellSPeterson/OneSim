# Agent notes

- Contract between Swift guest, Rust host and clients: `docs/protocol.md`. Change it and `simcore-protocol` together.
- Gates: `cargo build/test/clippy --workspace -- -D warnings`, `frameworks/swiftui/Tests/wire_test.sh`, `scripts/compare-ios.sh`, `SIMCORE_FALLBACK=1 scripts/compare-ios.sh`.
- **Pixel-diff rule:** when matching UI to iOS, every compared state must be **< 2%** in `scripts/compare-ios.sh` (Apple fonts/symbols), and **< 3.5%** in fallback mode (`SIMCORE_FALLBACK=1`, bundled Inter + Phosphor), or the work is not done. Never loosen the threshold or remove states to pass (the only exceptions are the maintainer-approved ones in `STATE_MAX`, `scripts/compare_ios.py`). New UI elements get an iOS reference state plus a `shot` line. Details are in CONTRIBUTING.md.
- iOS references: iPhone 17 Pro, iOS 26.5, captured via the T3 device tools (agent-device). Status-bar normalization makes the Dynamic Island appear only intermittently, so retake until it shows (see `tests/fixtures/showcase-ios/README.md`).
