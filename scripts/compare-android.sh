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
shot compose-text text
# Pixel coordinates 156,412 for the tap and 122,598 for the held selection at 2.625x.
shot compose-text text-tapped --click 59.43,156.95
shot compose-text text-selected --click 59.43,156.95 --hold 46.48,227.81
shot compose-text text-toolbar --click 59.43,156.95 --long-press 46.48,227.81
shot compose-text text-inline --root-view InlineTextDemo
shot compose-text text-rich --root-view RichTextDemo
shot compose-text text-link-focused --root-view RichTextDemo --key tab
shot compose-text text-link-pressed --root-view RichTextDemo --key tab --hold 57.14,329.52
shot compose-text text-effects --root-view TextEffectsDemo
shot compose-text text-paint --root-view TextPaintDemo
python3 scripts/compare_ios.py tests/fixtures/compose-android "$out/onesim" "$out"
