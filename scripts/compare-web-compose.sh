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

scripts/build-web.sh examples/compose-text
shot compose-text text
shot compose-text text-tapped --click 59.43,156.95
shot compose-text text-selected --click 59.43,156.95 --drag 24.38,227.81,45.72,64
shot compose-text text-held --click 59.43,156.95 --hold 46.48,227.81
shot compose-text text-toolbar --click 59.43,156.95 --long-press 46.48,227.81
node web/test/run.mjs --click 59.43,156.95 --expect "Tapped offset: 4"

scripts/build-web.sh examples/compose-text InlineTextDemo
./target/debug/onesim run examples/compose-text --root-view InlineTextDemo --screenshot "$out/native/text-inline.png" >/dev/null 2>&1
node web/test/run.mjs --screenshot "$out/web/text-inline.png" --expect "Inline width: 63 px"

scripts/build-web.sh examples/compose-text RichTextDemo
./target/debug/onesim run examples/compose-text --root-view RichTextDemo --screenshot "$out/native/text-rich.png" >/dev/null 2>&1
node web/test/run.mjs --screenshot "$out/web/text-rich.png" --expect "Fitted size: 23 sp"
./target/debug/onesim run examples/compose-text --root-view RichTextDemo --key tab --screenshot "$out/native/text-link-focused.png" >/dev/null 2>&1
node web/test/run.mjs --key tab --screenshot "$out/web/text-link-focused.png"
./target/debug/onesim run examples/compose-text --root-view RichTextDemo --key tab --hold 57.14,329.52 --screenshot "$out/native/text-link-pressed.png" >/dev/null 2>&1
node web/test/run.mjs --key tab --hold 57.14,329.52 --screenshot "$out/web/text-link-pressed.png"
node web/test/run.mjs --key tab --key enter --expect "Activated: 1"
node web/test/run.mjs --expect "Custom size: 18 sp"

scripts/build-web.sh examples/compose-text TextEffectsDemo
./target/debug/onesim run examples/compose-text --root-view TextEffectsDemo --screenshot "$out/native/text-effects.png" >/dev/null 2>&1
node web/test/run.mjs --screenshot "$out/web/text-effects.png" --expect "Measured shader"
scripts/build-web.sh examples/compose-text TextPaintDemo
./target/debug/onesim run examples/compose-text --root-view TextPaintDemo --screenshot "$out/native/text-paint.png" >/dev/null 2>&1
node web/test/run.mjs --screenshot "$out/web/text-paint.png" --expect "Measured 2 lines, 1 placeholder"
scripts/build-web.sh examples/compose-text TextDirectionDemo
./target/debug/onesim run examples/compose-text --root-view TextDirectionDemo --screenshot "$out/native/text-direction.png" >/dev/null 2>&1
node web/test/run.mjs --screenshot "$out/web/text-direction.png" --expect "ABC (123)"

scripts/build-web.sh frameworks/compose/tests/TextWireTest.kt TestText
node web/test/run.mjs --expect "Layout lines: 1"

scripts/build-web.sh examples/compose-input
shot compose-input    input
shot compose-input    input-changed    --click Name --type Hello --key enter --drag 86,313,117,0

SIMCORE_MAX_DIFF=2.0 python3 scripts/compare_ios.py "$out/native" "$out/web" "$out"
