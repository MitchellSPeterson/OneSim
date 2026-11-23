#!/usr/bin/env bash
# Render the Compose examples on OneSim in the same states as tests/fixtures/compose-android
# (real Android 17 Pixel 10a emulator, see its README) and build iOS-style side-by-side comparisons
# in target/compare/compose/. Same pass rule as compare-ios.sh (< 2% of pixels off). Needs Java 17+.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p onesim-cli
out=target/compare/compose; mkdir -p "$out/onesim"
shot() { local app=$1 name=$2; shift 2; ./target/debug/onesim run "examples/$app" "$@" --screenshot "$out/onesim/$name.png" >/dev/null; }
shot compose-counter  counter
shot compose-counter  counter-changed  --click Increment --click Increment --click Decrement
shot compose-showcase showcase
shot compose-showcase showcase-dark    --dark
# Switch has no label: tap its center (353,254 dp).
shot compose-showcase showcase-changed --click Filled --click Compose --click 353,254
shot compose-input input
shot compose-input input-changed --click Name --type Hello --key enter --drag 86,313,117,0
# Pointer input and interaction (px / 2.625 = dp, as captured): clicks, long and double click, draggable,
# draggable2D, pointerInput taps, scrollable, selectable and toggleable; then a finger held on the card (ripple).
shot compose-interaction interaction
shot compose-interaction interaction-changed --click 137.9048,165.7143 --click 137.9048,165.7143 \
  --long-press 137.9048,252.9524 --double-click 137.9048,252.9524 --click 137.9048,252.9524 --wait 400 \
  --drag 43.8095,314.6667,114.2857,0 --drag 43.8095,364.5714,114.2857,38.0952 --click 137.9048,508.5714 \
  --drag 137.9048,566.0952,114.2857,0 --click 139.8095,619.4286 --click 114.2857,664.7619
shot compose-interaction interaction-pressed --down 137.9048,165.7143 --wait 400
python3 scripts/compare_ios.py tests/fixtures/compose-android "$out/onesim" "$out"
