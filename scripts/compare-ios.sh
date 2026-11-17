#!/usr/bin/env bash
# Render the showcase on SimCore in the same states as tests/fixtures/showcase-ios
# and build side-by-side comparisons (iOS | SimCore | diff) in target/compare/.
# Requires Python 3 with Pillow (dev-only).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p simcore-cli
out=target/compare; mkdir -p "$out/simcore"
app=examples/swiftui-showcase
shot() { local name=$1; shift; ./target/debug/simcore run "$app" "$@" --screenshot "$out/simcore/$name.png" >/dev/null; }
shot home
shot typography       --click Typography
shot shapes           --click Shapes
shot todos            --click Todos
shot todos-toggled    --click Todos --click "Ship the SwiftUI subset"
shot todos-typed      --click Todos --click "Ship the SwiftUI subset" --click "New todo" --type "Buy milk"
shot controls         --click Controls
shot controls-changed --click Controls --click Notifications --click More --click More
shot home-dark        --dark
shot symbols          --click Symbols
shot symbols-dark     --click Symbols --dark
shot keyboard-shift   --click Todos --click "New todo"
shot keyboard-dark    --dark --click Todos --click "New todo" --type "Buy milk"
python3 scripts/compare_ios.py tests/fixtures/showcase-ios "$out/simcore" "$out"
