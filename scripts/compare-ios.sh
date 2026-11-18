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
shot long-list           --click "Long list"
shot long-list-collapsed --click "Long list" --scroll 200,500,0,52
shot long-list-scrolled  --click "Long list" --scroll 200,500,0,775.66
rc=0
python3 scripts/compare_ios.py tests/fixtures/showcase-ios "$out/simcore" "$out" || rc=1
# React Native / Expo showcase: tests/fixtures/rn-showcase-ios (see its README). Needs node + `npm install` in the example.
rn=examples/expo-showcase
if [ -d "$rn/node_modules" ] && command -v npx >/dev/null; then
  rnout=target/compare/rn; mkdir -p "$rnout/simcore"
  port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
  (cd "$rn" && CI=1 exec npx expo start --port "$port" >"../../$rnout/metro.log" 2>&1) &
  metro=$!
  trap 'pkill -P $metro 2>/dev/null; kill $metro 2>/dev/null; true' EXIT
  for _ in $(seq 60); do curl -sf "http://localhost:$port/status" >/dev/null 2>&1 && break; sleep 1; done
  rnshot() { local name=$1; shift; ./target/debug/simcore expo --url "http://localhost:$port" "$@" --screenshot "$rnout/simcore/$name.png" >/dev/null || echo "simcore expo failed: $name"; }
  rnshot home
  rnshot layout           --click Layout
  rnshot text             --click Text
  rnshot controls         --click Controls
  rnshot controls-changed --click Controls --click Increment --click Increment --click Alpha
  rnshot controls-typed   --click Controls --click "Type here" --type Hello
  rnshot scroll           --click Scroll
  rnshot scroll-scrolled  --click Scroll --click Jump
  rnshot image            --click Image
  rnshot home-dark        --dark
  echo "== React Native (expo-showcase)"
  python3 scripts/compare_ios.py tests/fixtures/rn-showcase-ios "$rnout/simcore" "$rnout" || rc=1
else
  echo "== skipping React Native comparison (run npm install in $rn, needs node)"
fi
# Expo Router: tests/fixtures/rn-router-ios (see its README). One Metro per app; each group compares only the states it rendered.
rr_group() { # app-dir label; states are produced by the rr_states_* function named by $3
  local app=$1 label=$2 o=target/compare/$2
  if ! { [ -d "$app/node_modules" ] && command -v npx >/dev/null; }; then echo "== skipping $label comparison (run npm install in $app, needs node)"; return; fi
  rm -rf "$o"; mkdir -p "$o/simcore"
  local pt; pt=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
  (cd "$app" && CI=1 exec npx expo start --port "$pt" >"../../$o/metro.log" 2>&1) &
  local m=$!
  for _ in $(seq 60); do curl -sf "http://localhost:$pt/status" >/dev/null 2>&1 && break; sleep 1; done
  rrshot() { local n=$1; shift; ./target/debug/simcore expo --url "http://localhost:$pt" "$@" --screenshot "$o/simcore/$n.png" >/dev/null || echo "simcore expo failed: $n"; }
  "$3"
  pkill -P $m 2>/dev/null; kill $m 2>/dev/null
  echo "== Expo Router ($label)"
  python3 scripts/compare_ios.py tests/fixtures/rn-router-ios "$o/simcore" "$o" || rc=1
}
rr_states_tabs() {
  rrshot tabs-home
  rrshot tabs-explore          --click 244,822
  rrshot tabs-explore-expanded --click 244,822 --click 100,389
  rrshot tabs-home-dark        --dark
  rrshot tabs-explore-dark     --dark --click 244,822
}
rr_states_stack() {
  rrshot stack-list
  rrshot stack-detail      --click 200,192
  rrshot stack-back        --click 200,192 --click 64,84
  rrshot stack-detail-dark --dark --click 200,192
}
rr_group examples/expo-default tabs rr_states_tabs
rr_group examples/expo-router-stack stack rr_states_stack
# Per-device chrome: tests/fixtures/devices/<slug>/{home,keyboard-shift}.png (see its README).
for d in tests/fixtures/devices/*/; do
  slug=$(basename "$d"); dd="$out/devices/$slug"; mkdir -p "$dd/simcore"
  ./target/debug/simcore run "$app" --device "$slug" --screenshot "$dd/simcore/home.png" >/dev/null
  ./target/debug/simcore run "$app" --device "$slug" --click Todos --click "New todo" --screenshot "$dd/simcore/keyboard-shift.png" >/dev/null
  echo "== $slug"; python3 scripts/compare_ios.py "$d" "$dd/simcore" "$dd" || rc=1
done
exit $rc
