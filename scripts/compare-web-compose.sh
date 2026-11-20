#!/usr/bin/env bash
# Pixel parity gate, browser Compose (Kotlin/Wasm) vs native Compose (JVM): render every state from
# scripts/compare-android.sh through target/web/ (Kotlin/Wasm guest + onesim-web) and through native
# `onesim run`, then require every state < 2% differing pixels (compare_ios.py metric).
# Requires Rust Wasm targets, wasm-bindgen-cli, Java 17+, Node 22+, and Python 3 with Pillow.
# Output: target/compare-web-compose/{native,web}/*.png + sheets.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p onesim-cli
out=target/compare-web-compose
rm -rf "$out"
mkdir -p "$out/native" "$out/web"

shot() {
  local app=$1 name=$2; shift 2
  ./target/debug/onesim run "examples/$app" "$@" --screenshot "$out/native/$name.png" >/dev/null 2>&1
  node web/test/run.mjs "$@" --screenshot "$out/web/$name.png" 2>/dev/null
}

scripts/build-web.sh examples/compose-counter
shot compose-counter  counter
shot compose-counter  counter-changed  --click Increment --click Increment --click Decrement

scripts/build-web.sh examples/compose-showcase
shot compose-showcase showcase
shot compose-showcase showcase-dark    --dark
# Switch has no label: tap its center (353,254 dp).
shot compose-showcase showcase-changed --click Filled --click Compose --click 353,254

scripts/build-web.sh examples/compose-input
shot compose-input    input
shot compose-input    input-changed    --click Name --type Hello --key enter --drag 86,313,117,0

SIMCORE_MAX_DIFF=2.0 python3 scripts/compare_ios.py "$out/native" "$out/web" "$out"
