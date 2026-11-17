#!/usr/bin/env bash
# Pixel parity gate, browser backend vs native: render the 13 showcase states of
# scripts/compare-ios.sh through the Node harness (target/web/app.wasm, bundled Inter + Phosphor)
# and through native `SIMCORE_FALLBACK=1 simcore run`, then require every state < 2% differing
# pixels (compare_ios.py metric). Build first: scripts/build-web.sh examples/swiftui-showcase
# Requires Python 3 with Pillow (dev-only). Output: target/compare-web/{native,web}/*.png + sheets.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f target/web/app.wasm ] || { echo "error: target/web/app.wasm missing; run scripts/build-web.sh examples/swiftui-showcase" >&2; exit 2; }
cargo build -q -p simcore-cli
out=target/compare-web; rm -rf "$out"; mkdir -p "$out/native" "$out/web"
app=examples/swiftui-showcase
shot() {
  local name=$1; shift
  SIMCORE_FALLBACK=1 ./target/debug/simcore run "$app" "$@" --screenshot "$out/native/$name.png" >/dev/null 2>&1
  node web/test/run.mjs "$@" --screenshot "$out/web/$name.png" 2>/dev/null
}
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
# native is the reference; both sides are fallback fonts, so the 2% rule applies (never loosen it)
SIMCORE_MAX_DIFF=2.0 python3 scripts/compare_ios.py "$out/native" "$out/web" "$out"
