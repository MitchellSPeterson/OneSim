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
# SIMCORE_ONLY=swiftui|expo limits the run to one framework's states (scripts/ci-local.sh uses it).
only=${SIMCORE_ONLY:-all}
case $only in all|swiftui|expo) ;; *) echo "unknown SIMCORE_ONLY=$only (swiftui|expo)"; exit 2 ;; esac
want() { [ "$only" = all ] || [ "$only" = "$1" ]; }
# SIMCORE_SMOKE=1: a few representative states per framework (scripts/ci-local.sh fast level).
# Same fixtures and thresholds; the full run compares every state before merge.
smoke=${SIMCORE_SMOKE:-}
SMOKE_STATES=" home home-dark controls sheet todos-typed form controls-typed "
keep() { [ -z "$smoke" ] || [[ "$SMOKE_STATES" == *" $1 "* ]]; }
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
pages="$guests/pages"
gest="$guests/gestures"
shp="$guests/shapes"
layout="$guests/layout"
typeapp="$guests/type"
anim="$guests/animation"
controls="$guests/controls"
effects="$guests/effects"
textinput="$guests/text-input"
keypads="$guests/keypads"
pickers="$guests/pickers"
navigation="$guests/navigation"
forms="$guests/forms"
if want swiftui; then
  scripts/build-swift-app.sh examples/swiftui-showcase "$app"
fi
if want swiftui && [ -z "$smoke" ]; then
  scripts/build-swift-app.sh examples/swiftui-tabs "$tabs"
  scripts/build-swift-app.sh examples/swiftui-pages "$pages"
  scripts/build-swift-app.sh examples/swiftui-gestures "$gest"
  scripts/build-swift-app.sh examples/swiftui-shapes "$shp"
  scripts/build-swift-app.sh examples/swiftui-layout "$layout"
  scripts/build-swift-app.sh examples/swiftui-type "$typeapp"
  scripts/build-swift-app.sh examples/swiftui-animation "$anim"
  scripts/build-swift-app.sh examples/swiftui-controls "$controls"
  scripts/build-swift-app.sh examples/swiftui-effects "$effects"
  scripts/build-swift-app.sh examples/swiftui-text-input "$textinput"
  scripts/build-swift-app.sh examples/swiftui-keypads "$keypads"
  scripts/build-swift-app.sh examples/swiftui-pickers "$pickers"
  scripts/build-swift-app.sh examples/swiftui-navigation "$navigation"
  scripts/build-swift-app.sh examples/swiftui-forms "$forms"
fi
# Every supported iOS release has its own reference set, captured on its pinned runtime and device
# (docs/compatibility/versions.json): 27.2 on the iPhone 18 Pro in tests/fixtures/ios27/, 26.5 on the iPhone 17 Pro
# in tests/fixtures/. Only 27.2 is gated for now (the default); SIMCORE_IOS="26.5 27.2" also checks 26.5.
rc=0
for ios in ${SIMCORE_IOS:-27.2}; do
  case $ios in
    # ll: scroll offset of the long-list-scrolled reference (a fling on 26.5, an exact drag on 27.2).
    26.5) fx=tests/fixtures dev=iphone-17-pro ll=775.66 ;;
    27.2) fx=tests/fixtures/ios27 dev=iphone-18-pro ll=687.67 ;;
    *) echo "unknown iOS $ios"; exit 2 ;;
  esac
  base=(--ios "$ios" --device "$dev")
  out=target/compare/ios$ios${smoke:+-smoke}; [ -z "$smoke" ] || rm -rf "$out"; mkdir -p "$out/onesim"
  echo "==== iOS $ios ($dev, $fx)"
  export SIMCORE_COMPARE_IOS=$ios
  if want swiftui; then
    shot() { local name=$1; shift; keep "$name" || return 0; ./target/debug/onesim run "$app" "${base[@]}" "$@" --screenshot "$out/onesim/$name.png" >/dev/null; }
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
  fi
  if want swiftui && [ -z "$smoke" ]; then
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

    pout=$out/swiftui-pages; mkdir -p "$pout/onesim"
    pfx=$fx/swiftui-pages-ios
    pshot() {
      local name=$1; shift
      ./target/debug/onesim run "$pages" "${base[@]}" "$@" --screenshot "$pout/onesim/$name.png" >/dev/null
      if [ ! -f "$pfx/$name.png" ]; then echo "Missing iOS reference: $pfx/$name.png" >&2; rc=1; fi
    }
    pshot pages-first
    pshot pages-programmatic --click Next
    pshot pages-swiped       --click Next --drag 315,430,-250,0
    pshot pages-inbox        --click Inbox
    echo "== SwiftUI page TabView (swiftui-pages, iOS $ios)"
    python3 scripts/compare_ios.py "$pfx" "$pout/onesim" "$pout" || rc=1
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
    # SwiftUI shapes & paths: tests/fixtures/swiftui-shapes-ios (see its README).
    sout=$out/swiftui-shapes; mkdir -p "$sout/onesim"
    sshot() { local name=$1; shift; ./target/debug/onesim run "$shp" "${base[@]}" "$@" --screenshot "$sout/onesim/$name.png" >/dev/null; }
    sshot shapes
    sshot shapes-dark --dark
    echo "== SwiftUI shapes (swiftui-shapes, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-shapes-ios "$sout/onesim" "$sout" || rc=1
    # SwiftUI layout: tests/fixtures/swiftui-layout-ios (see its README). Pages are 320pt; --scroll does not tick.
    lout=$out/swiftui-layout; mkdir -p "$lout/onesim"
    lshot() { local name=$1; shift; ./target/debug/onesim run "$layout" "${base[@]}" "$@" --screenshot "$lout/onesim/$name.png" >/dev/null; }
    lshot layout
    lshot layout-dark --dark
    lshot layout-paged --scroll 201,658,320,0
    echo "== SwiftUI layout (swiftui-layout, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-layout-ios "$lout/onesim" "$lout" || rc=1
    # SwiftUI type: tests/fixtures/swiftui-type-ios (see its README).
    yout=$out/swiftui-type; mkdir -p "$yout/onesim"
    yshot() { local name=$1; shift; ./target/debug/onesim run "$typeapp" "${base[@]}" "$@" --screenshot "$yout/onesim/$name.png" >/dev/null; }
    yshot type
    yshot type-dark --dark
    echo "== SwiftUI type (swiftui-type, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-type-ios "$yout/onesim" "$yout" || rc=1
    # SwiftUI animation: tests/fixtures/swiftui-animation-ios (see its README). --wait 600 settles all spring/ease animations.
    aout=$out/swiftui-animation; mkdir -p "$aout/onesim"
    ashot() { local name=$1; shift; ./target/debug/onesim run "$anim" "${base[@]}" "$@" --screenshot "$aout/onesim/$name.png" >/dev/null; }
    ashot animation
    ashot animation-dark   --dark
    ashot animation-active --click Beta --click "Toggle Card" --click Increment --wait 600
    echo "== SwiftUI animation (swiftui-animation, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-animation-ios "$aout/onesim" "$aout" || rc=1
    # SwiftUI controls: tests/fixtures/swiftui-controls-ios (see its README). Targets by label, picker rows by point.
    cout=$out/swiftui-controls; mkdir -p "$cout/onesim"
    cshot() { local name=$1; shift; ./target/debug/onesim run "$controls" "${base[@]}" "$@" --screenshot "$cout/onesim/$name.png" >/dev/null; }
    cshot controls
    cshot controls-dark     --dark
    cshot controls-menu     --click Actions
    cshot controls-changed  --click Bold --click "I agree" --click "Pill 0"
    cshot pickers           --click Pickers
    cshot pickers-dark      --dark --click Pickers
    cshot pickers-pushed    --click Pickers --click Size
    cshot pickers-changed   --click Pickers --click Green --click Star --click 10:00
    cshot indicators        --click Indicators
    cshot indicators-dark   --dark --click Indicators
    echo "== SwiftUI controls (swiftui-controls, iOS $ios)"
    python3 scripts/compare_ios.py $fx/swiftui-controls-ios "$cout/onesim" "$cout" || rc=1
    eout=$out/swiftui-effects; mkdir -p "$eout/onesim"
    eshot() {
      local name=$1; shift
      ./target/debug/onesim run "$effects" "${base[@]}" "$@" --screenshot "$eout/onesim/$name.png" >/dev/null
      if [ ! -f "$fx/swiftui-effects-ios/$name.png" ]; then echo "Missing iOS reference: $fx/swiftui-effects-ios/$name.png" >&2; rc=1; fi
    }
    eshot effects
    eshot effects-dark --dark
    echo "== SwiftUI gradients, Canvas and visual effects (iOS $ios)"
    python3 scripts/compare_ios.py "$fx/swiftui-effects-ios" "$eout/onesim" "$eout" || rc=1

    iout=$out/swiftui-text-input; mkdir -p "$iout/onesim"
    ishot() {
      local guest=$1 name=$2; shift 2
      ./target/debug/onesim run "$guest" "${base[@]}" "$@" --screenshot "$iout/onesim/$name.png" >/dev/null
      if [ ! -f "$fx/swiftui-text-input-ios/$name.png" ]; then echo "Missing iOS reference: $fx/swiftui-text-input-ios/$name.png" >&2; rc=1; fi
    }
    ishot "$textinput" text-input
    ishot "$textinput" text-input-focused --click "Full name"
    ishot "$keypads" keypad-number --click Number
    ishot "$keypads" keypad-decimal --click Decimal
    ishot "$keypads" keypad-phone --click Phone
    ishot "$keypads" keypad-phone-symbols --click Phone --click '+*#'
    echo "== SwiftUI text input (iOS $ios)"
    python3 scripts/compare_ios.py "$fx/swiftui-text-input-ios" "$iout/onesim" "$iout" || rc=1
    # Date and color pickers. Each recipe must have an Apple SwiftUI reference.
    pkout=$out/swiftui-pickers
    pkfx=$fx/swiftui-pickers-ios
    mkdir -p "$pkout/onesim"
    pshot() {
      local name=$1
      shift
      if [ ! -f "$pkfx/$name.png" ]; then
        echo "Missing iOS $ios picker reference: $pkfx/$name.png" >&2
        rc=1
        return
      fi
      ./target/debug/onesim run "$pickers" "${base[@]}" "$@" --screenshot "$pkout/onesim/$name.png" >/dev/null
    }
    pshot pickers
    pshot pickers-date --click 213,241
    pshot pickers-time --click 322,241
    pshot graphical    --click Graphical
    pshot wheel        --click Wheel
    pshot multi        --click Multiple
    pshot colors       --click 352,492
    echo "== SwiftUI date and color pickers (iOS $ios)"
    python3 scripts/compare_ios.py "$pkfx" "$pkout/onesim" "$pkout" || rc=1
    nout=$out/swiftui-navigation
    nfx=$fx/swiftui-navigation-ios
    mkdir -p "$nout/onesim"
    nshot() {
      local name=$1
      shift
      if [ ! -f "$nfx/$name.png" ]; then
        echo "Missing iOS $ios navigation reference: $nfx/$name.png" >&2
        rc=1
        return
      fi
      ./target/debug/onesim run "$navigation" "${base[@]}" "$@" --screenshot "$nout/onesim/$name.png" >/dev/null
    }
    nshot library
    nshot detail --click About
    nshot detail-menu --click About --click 188,84
    nshot legacy --click Legacy
    nshot search --click Search
    nshot search-active --click Search --click 100,138
    echo "== SwiftUI navigation (iOS $ios)"
    python3 scripts/compare_ios.py "$nfx" "$nout/onesim" "$nout" || rc=1
    # Forms and containers (tests/fixtures/ios27/swiftui-forms-ios/README.md). Screens below the fold are reached
    # after a scroll; toggles and swipes animate, so those states wait for them to settle.
    fmout=$out/swiftui-forms
    fmfx=$fx/swiftui-forms-ios
    mkdir -p "$fmout/onesim"
    fshot() {
      local name=$1
      shift
      if [ ! -f "$fmfx/$name.png" ]; then
        echo "Missing iOS $ios forms reference: $fmfx/$name.png" >&2
        rc=1
        return
      fi
      ./target/debug/onesim run "$forms" "${base[@]}" "$@" --screenshot "$fmout/onesim/$name.png" >/dev/null
    }
    down=(--scroll 200,500,0,400)
    fshot forms
    fshot forms-dark       --dark
    fshot forms-expanded   --click Advanced --wait 1000
    fshot grouped          --click Grouped
    fshot plain            --click Plain
    fshot inset            --click Inset
    fshot sidebar          --click Sidebar
    fshot sidebar-dark     --dark --click Sidebar
    fshot sidebar-expanded --click Sidebar --click 200,298 --wait 1000
    fshot boxes            --click Boxes
    fshot boxes-dark       --dark --click Boxes
    fshot columns          "${down[@]}" --click Columns
    fshot empty            "${down[@]}" --click Empty
    fshot empty-search     "${down[@]}" --click Empty --click Search
    fshot outline          "${down[@]}" --click Outline
    fshot outline-expanded "${down[@]}" --click Outline --click Documents --wait 1000
    fshot edit             "${down[@]}" --click Edit
    fshot edit-active      "${down[@]}" --click Edit --click Edit --wait 1000
    fshot edit-delete      "${down[@]}" --click Edit --click Edit --wait 1000 --click 47,177 --wait 1000
    fshot swipe-trailing   "${down[@]}" --click Swipe --drag 300,177,-150,0
    fshot swipe-leading    "${down[@]}" --click Swipe --drag 100,177,100,0
    fshot table            "${down[@]}" --click Table
    echo "== SwiftUI forms and containers (iOS $ios)"
    python3 scripts/compare_ios.py "$fmfx" "$fmout/onesim" "$fmout" || rc=1
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
      rnshot() { local name=$1; shift; keep "$name" || return 0; ./target/debug/onesim expo --url "http://localhost:$port" "${base[@]}" "$@" --screenshot "$rnout/onesim/$name.png" >/dev/null || echo "onesim expo failed: $name"; }
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
    [ -z "$smoke" ] || continue
    rr_group examples/expo-default tabs rr_states_tabs
    rr_group examples/expo-router-stack stack rr_states_stack
    if [ -d examples/expo-ui/node_modules ]; then
      scripts/compare-expo-ui.sh "$ios" || rc=1
    fi
  fi
  # Per-device chrome: tests/fixtures/devices/<slug>/{home,keyboard-shift}.png (see its README).
  for d in $fx/devices/*/; do
    { [ -d "$d" ] && want swiftui && [ -z "$smoke" ]; } || continue
    slug=$(basename "$d"); dd="$out/devices/$slug"; mkdir -p "$dd/onesim"
    ./target/debug/onesim run "$app" --ios "$ios" --device "$slug" --screenshot "$dd/onesim/home.png" >/dev/null
    ./target/debug/onesim run "$app" --ios "$ios" --device "$slug" --click Todos --click "New todo" --screenshot "$dd/onesim/keyboard-shift.png" >/dev/null
    echo "== $slug (iOS $ios)"; python3 scripts/compare_ios.py "$d" "$dd/onesim" "$dd" || rc=1
  done
done
exit $rc
