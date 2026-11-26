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

scripts/build-web.sh examples/compose-interaction
shot compose-interaction interaction
shot compose-interaction interaction-changed --click 137.9048,165.7143 --click 137.9048,165.7143 \
  --long-press 137.9048,252.9524 --double-click 137.9048,252.9524 --click 137.9048,252.9524 --wait 400 \
  --drag 43.8095,314.6667,114.2857,0 --drag 43.8095,364.5714,114.2857,38.0952 --click 137.9048,508.5714 \
  --drag 137.9048,566.0952,114.2857,0 --click 139.8095,619.4286 --click 114.2857,664.7619
shot compose-interaction interaction-pressed --down 137.9048,165.7143 --wait 400

SIMCORE_MAX_DIFF=2.0 python3 scripts/compare_ios.py "$out/native" "$out/web" "$out"
