#!/usr/bin/env bash
# Render the showcase on SimCore in the same states as tests/fixtures/showcase-ios
# and build side-by-side comparisons (iOS | SimCore | diff) in target/compare/.
# Requires Python 3 with Pillow (dev-only).
# SIMCORE_FALLBACK=1 forces the bundled Inter + Phosphor fallbacks (inherited by every `simcore run`,
# whose host process does the rendering); the capabilities line below shows what is actually in use.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p simcore-cli
printf '{"id":1,"method":"hello","params":{"protocol_version":1}}\n{"id":2,"method":"get_capabilities"}\n' | ./target/debug/simcore serve 2>/dev/null \
  | python3 -c 'import json,sys; c=json.loads(sys.stdin.readlines()[1])["result"]; print("fonts:", c["fonts"], "| symbols:", c["symbols"], "| SIMCORE_FALLBACK=" + "'"${SIMCORE_FALLBACK:-}"'")'
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
shot glass            --click Glass
shot glass-dark       --dark --click Glass
shot glass-pressed    --click Glass --click "Glass 0"
shot typography-scrolled      --dynamic-type accessibility5 --click Typography --scroll 200,500,0,40
shot typography-scrolled-dark --dynamic-type accessibility5 --dark --click Typography --scroll 200,500,0,40
python3 scripts/compare_ios.py tests/fixtures/showcase-ios "$out/simcore" "$out"
