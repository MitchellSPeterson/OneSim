#!/usr/bin/env bash
# Render the example apps on OneSim in the same states as the iOS reference screenshots
# and build side-by-side comparisons (iOS | OneSim | diff) in target/compare/ios<version>/.
# Requires Python 3 with Pillow (dev-only).
# SIMCORE_FALLBACK=1 forces the bundled Inter + Phosphor fallbacks (inherited by every `onesim run`,
# whose host process does the rendering); the capabilities line below shows what is actually in use.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p onesim-cli
printf '{"id":1,"method":"hello","params":{"protocol_version":1}}\n{"id":2,"method":"get_capabilities"}\n' | ./target/debug/onesim serve 2>/dev/null \
  | python3 -c 'import json,sys; c=json.loads(sys.stdin.readlines()[1])["result"]; print("fonts:", c["fonts"], "| symbols:", c["symbols"], "| SIMCORE_FALLBACK=" + "'"${SIMCORE_FALLBACK:-}"'")'
# SIMCORE_ONLY=swiftui|expo limits the run to one framework's states (scripts/affected.sh uses it).
only=${SIMCORE_ONLY:-all}
case $only in all|swiftui|expo) ;; *) echo "unknown SIMCORE_ONLY=$only (swiftui|expo)"; exit 2 ;; esac
want() { [ "$only" = all ] || [ "$only" = "$1" ]; }
# Build each guest once. Every shot still starts a fresh process with its own device and appearance.
guests=$(mktemp -d)
metro=
stop_metro() {
  if [ -n "$metro" ]; then
    pkill -P "$metro" 2>/dev/null || true
    kill "$metro" 2>/dev/null || true
    metro=
  fi
}
cleanup() { stop_metro; rm -rf "$guests"; }
trap cleanup EXIT
app="$guests/showcase"
tabs="$guests/tabs"
gest="$guests/gestures"
layout="$guests/layout"
if want swiftui; then
  scripts/build-swift-app.sh examples/swiftui-showcase "$app"
  scripts/build-swift-app.sh examples/swiftui-tabs "$tabs"
  scripts/build-swift-app.sh examples/swiftui-gestures "$gest"
  scripts/build-swift-app.sh examples/swiftui-layout "$layout"
fi
# Every supported iOS release has its own reference set, captured on its pinned runtime and device
# (docs/compatibility/versions.json): 26.5 on the iPhone 17 Pro in tests/fixtures/, 27.2 on the iPhone 18 Pro
# in tests/fixtures/ios27/. SIMCORE_IOS="27.2" limits the run to one release.
rc=0
for ios in ${SIMCORE_IOS:-26.5 27.2}; do
  case $ios in
    # ll: scroll offset of the long-list-scrolled reference (a fling on 26.5, an exact drag on 27.2).
    26.5) fx=tests/fixtures dev=iphone-17-pro ll=775.66 ;;
    27.2) fx=tests/fixtures/ios27 dev=iphone-18-pro ll=687.67 ;;
    *) echo "unknown iOS $ios"; exit 2 ;;
  esac
  base=(--ios "$ios" --device "$dev")
  out=target/compare/ios$ios; mkdir -p "$out/onesim"
  echo "==== iOS $ios ($dev, $fx)"
  export SIMCORE_COMPARE_IOS=$ios
  if want swiftui; then
    shot() { local name=$1; shift; ./target/debug/onesim run "$app" "${base[@]}" "$@" --screenshot "$out/onesim/$name.png" >/dev/null; }
    shot home
    shot typography       --click Typography
    shot shapes           --click Shapes
    shot todos            --click Todos
    shot todos-toggled    --click Todos --click "Ship the SwiftUI subset"
    shot todos-typed      --click Todos --click "Ship the SwiftUI subset" --click "New todo" --type "Buy milk"
    shot controls         --click Controls
    shot controls-changed --click Controls --click Notifications --click More --click More
    shot alert            --click Controls --click "Show alert"
    shot alert-dark       --dark --click Controls --click "Show alert"
    shot sheet            --click Controls --click "Show sheet"
    shot sheet-dark       --dark --click Controls --click "Show sheet"
    shot sheet-medium      --click Controls --click "Show half sheet"
    shot sheet-medium-dark --dark --click Controls --click "Show half sheet"
    shot sheet-nested      --click Controls --click "Show half sheet" --click "Open another"
    shot sheet-nested-dark --dark --click Controls --click "Show half sheet" --click "Open another"
    shot dialog            --click Controls --click "Show dialog"
    shot dialog-dark       --dark --click Controls --click "Show dialog"
    shot cover             --click Controls --click "Show cover"
    shot cover-dark        --dark --click Controls --click "Show cover"
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
    shot long-list-scrolled  --click "Long list" --scroll 200,500,0,$ll
    shot form         --click Form
    shot form-dark    --dark --click Form
    shot form-menu    --click Form --click Flavor
    shot form-changed --click Form --click Flavor --click Chocolate --click Large --drag 170,552,300,0
    python3 scripts/compare_ios.py $fx/showcase-ios "$out/onesim" "$out" || rc=1
    # SwiftUI TabView: tests/fixtures/swiftui-tabs-ios (see its README). Tab items by point: Home 115,822, Counter 201,822, Settings 287,822.
    tout=$out/swiftui-tabs; mkdir -p "$tout/onesim"
    tshot() { local name=$1; shift; ./target/debug/onesim run "$tabs" "${base[@]}" "$@" --screenshot "$tout/onesim/$name.png" >/dev/null; }
    tshot tabs-home
    tshot tabs-home-dark     --dark
    tshot tabs-second        --click 201,822 --click Increment
    tshot tabs-pushed        --click Apple
    tshot tabs-settings-dark --dark --click 287,822
    echo "== SwiftUI TabView (swiftui-tabs, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-tabs-ios "$tout/onesim" "$tout" || rc=1
    # SwiftUI gestures: tests/fixtures/swiftui-gestures-ios (see its README). Targets by label.
    gout=$out/swiftui-gestures; mkdir -p "$gout/onesim"
    gshot() { local name=$1; shift; ./target/debug/onesim run "$gest" "${base[@]}" "$@" --screenshot "$gout/onesim/$name.png" >/dev/null; }
    gshot gestures
    gshot gestures-dark          --dark
    gshot gestures-dragged       --drag "Drag me,100,0"
    gshot gestures-long-pressed  --long-press Hold
    gshot gestures-double-tapped --click Double-tap --click Double-tap
    gshot gestures-pinched       --pinch "Pinch or rotate,1.5"
    echo "== SwiftUI gestures (swiftui-gestures, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-gestures-ios "$gout/onesim" "$gout" || rc=1
    # SwiftUI layout: tests/fixtures/swiftui-layout-ios (see its README). Pages are 320pt; --scroll does not tick.
    lout=$out/swiftui-layout; mkdir -p "$lout/onesim"
    lshot() { local name=$1; shift; ./target/debug/onesim run "$layout" "${base[@]}" "$@" --screenshot "$lout/onesim/$name.png" >/dev/null; }
    lshot layout
    lshot layout-dark --dark
    lshot layout-paged --scroll 201,658,320,0
    echo "== SwiftUI layout (swiftui-layout, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-layout-ios "$lout/onesim" "$lout" || rc=1
  fi
  # React Native / Expo showcase: tests/fixtures/rn-showcase-ios (see its README). Needs node + `npm install` in the example.
  if want expo; then
    rn=examples/expo-showcase
    if [ -d "$rn/node_modules" ] && command -v npx >/dev/null; then
      rnout=$out/rn; mkdir -p "$rnout/onesim"
      port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
      (cd "$rn" && CI=1 exec npx expo start --port "$port" >"../../$rnout/metro.log" 2>&1) &
      metro=$!
      for _ in $(seq 60); do curl -sf "http://localhost:$port/status" >/dev/null 2>&1 && break; sleep 1; done
      rnshot() { local name=$1; shift; ./target/debug/onesim expo --url "http://localhost:$port" "${base[@]}" "$@" --screenshot "$rnout/onesim/$name.png" >/dev/null || echo "onesim expo failed: $name"; }
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
      rnshot alert            --click Controls --click Count
      rnshot alert-dark       --dark --click Controls --click Count
      rnshot keyboard-avoiding      --click Showcase --click Message --type Hello
      rnshot keyboard-avoiding-dark --dark --click Showcase --click Message --type Hello
      rnshot visuals             --click Visuals
      rnshot visuals-dark        --dark --click Visuals
      rnshot refresh-pulled      --click Refresh --scroll 200,300,0,-150
      rnshot refresh-pulled-dark --dark --click Refresh --scroll 200,300,0,-150
      echo "== React Native (expo-showcase, iOS $ios)"
      python3 scripts/compare_ios.py $fx/rn-showcase-ios "$rnout/onesim" "$rnout" || rc=1
      stop_metro
    else
      echo "== skipping React Native comparison (run npm install in $rn, needs node)"
    fi
    # Expo Router: tests/fixtures/rn-router-ios (see its README). One Metro per app; each group compares only the states it rendered.
    rr_group() { # app-dir label; states are produced by the rr_states_* function named by $3
      local app=$1 label=$2 o=$out/$2
      if ! { [ -d "$app/node_modules" ] && command -v npx >/dev/null; }; then echo "== skipping $label comparison (run npm install in $app, needs node)"; return; fi
      rm -rf "$o"; mkdir -p "$o/onesim"
      local pt; pt=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
      (cd "$app" && CI=1 exec npx expo start --port "$pt" >"../../$o/metro.log" 2>&1) &
      local m=$!
      for _ in $(seq 60); do curl -sf "http://localhost:$pt/status" >/dev/null 2>&1 && break; sleep 1; done
      rrshot() { local n=$1; shift; ./target/debug/onesim expo --url "http://localhost:$pt" "${base[@]}" "$@" --screenshot "$o/onesim/$n.png" >/dev/null || echo "onesim expo failed: $n"; }
      "$3"
      pkill -P $m 2>/dev/null; kill $m 2>/dev/null
      echo "== Expo Router ($label, iOS $ios)"
      python3 scripts/compare_ios.py $fx/rn-router-ios "$o/onesim" "$o" || rc=1
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
    if [ -d examples/expo-ui/node_modules ]; then
      scripts/compare-expo-ui.sh "$ios" || rc=1
    fi
  fi
  # Per-device chrome: tests/fixtures/devices/<slug>/{home,keyboard-shift}.png (see its README).
  for d in $fx/devices/*/; do
    { [ -d "$d" ] && want swiftui; } || continue
    slug=$(basename "$d"); dd="$out/devices/$slug"; mkdir -p "$dd/onesim"
    ./target/debug/onesim run "$app" --ios "$ios" --device "$slug" --screenshot "$dd/onesim/home.png" >/dev/null
    ./target/debug/onesim run "$app" --ios "$ios" --device "$slug" --click Todos --click "New todo" --screenshot "$dd/onesim/keyboard-shift.png" >/dev/null
    echo "== $slug (iOS $ios)"; python3 scripts/compare_ios.py "$d" "$dd/onesim" "$dd" || rc=1
  done
done
exit $rc
