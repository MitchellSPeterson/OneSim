# Agent notes

- Contract between Swift guest, Rust host and clients: `docs/protocol.md`. Change it and `onesim-protocol` together.
- Gates: `cargo build/test/clippy --workspace -- -D warnings`, `frameworks/swiftui/Tests/wire_test.sh`, `scripts/compare-ios.sh`, `SIMCORE_FALLBACK=1 scripts/compare-ios.sh`, `scripts/compare-android.sh` (Compose, needs Java 17+).
- **Pixel-diff rule:** when matching UI to iOS, every compared state must be **< 2%** in `scripts/compare-ios.sh` (Apple fonts/symbols), and **< 3.5%** in fallback mode (`SIMCORE_FALLBACK=1`, bundled Inter + Phosphor), or the work is not done. Never loosen the threshold or remove states to pass (the only exceptions are the maintainer-approved ones in `STATE_MAX`, `scripts/compare_ios.py`). New UI elements get an iOS reference state plus a `shot` line. Details are in CONTRIBUTING.md.
- iOS references: one set per supported release, pinned with every other target version in `docs/compatibility/versions.json` (`node scripts/api-coverage.mjs --check` catches drift): iOS 27.2 on the iPhone 18 Pro in `tests/fixtures/ios27/` (the default, `--ios 27.2`) and iOS 26.5 on the iPhone 17 Pro in `tests/fixtures/` (`--ios 26.5`). Both must pass; a look that differs between releases goes in `crates/onesim-render/src/look.rs` (or a per-release branch next to the code it changes), never by editing the other release's fit. Captured via the T3 device tools (agent-device). Status-bar normalization makes the Dynamic Island appear only intermittently (on iOS 27 in ~5% of shots), so retake until it shows (see `tests/fixtures/showcase-ios/README.md`).
- Android references: Pixel 10a emulator, Android 17, same < 2% rule in `scripts/compare-android.sh`; captured with `tests/fixtures/compose-android/capture.sh` (agent-device from the T3 device tools, real APK of the examples from `frameworks/compose/reference-app`).

## Agent skills

### Issue tracker

GitHub Issues in MitchellSPeterson/OneSim, via `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`, created lazily. See `docs/agents/domain.md`.
