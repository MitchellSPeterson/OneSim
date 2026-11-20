#!/usr/bin/env bash
# Pixel parity gate, Expo in the browser vs native: render examples/expo-showcase states through the browser build
# (target/web-expo: the guest in a Web Worker, CPU renderer, bundled Inter + Phosphor) and through native
# `SIMCORE_FALLBACK=1 onesim expo`, then require every state < 2% differing pixels (compare_ios.py metric).
# Needs: scripts/build-web.sh --expo, npm ci --prefix examples/expo-showcase, Chrome (CHROME=), Python 3 with Pillow.
# EXPO_SHOWCASE_METRO_URL reuses a running Metro. Output: target/compare-web-expo/{native,web}/*.png + sheets.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f target/web-expo/app.wasm ] || { echo "error: target/web-expo/app.wasm missing; run scripts/build-web.sh --expo" >&2; exit 2; }
app=examples/expo-showcase
out=target/compare-web-expo; rm -rf "$out"; mkdir -p "$out/native" "$out/web"
cargo build -q -p onesim-cli
url=${EXPO_SHOWCASE_METRO_URL:-}
if [ -z "$url" ]; then
  test -d "$app/node_modules" || { echo "error: run npm ci --prefix $app first" >&2; exit 1; }
  port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
  url=http://localhost:$port
  (cd "$app" && CI=1 EXPO_OFFLINE=1 EXPO_NO_TELEMETRY=1 exec node node_modules/expo/bin/cli start --port "$port") >"$out/metro.log" 2>&1 &
  metro=$!
  trap 'kill "$metro" 2>/dev/null || true' EXIT
  for _ in $(seq 60); do curl -fsS "$url/status" >/dev/null 2>&1 && break; sleep 1; done
  curl -fsS "$url/status" >/dev/null
fi
# Same device, release and scale on both sides (native defaults: iPhone 18 Pro, iOS 27.2, 3x).
device=(--device iphone-18-pro --ios 27.2)
shot() {
  local name=$1; shift
  SIMCORE_FALLBACK=1 ./target/debug/onesim expo --url "$url" "${device[@]}" "$@" --screenshot "$out/native/$name.png" >/dev/null 2>&1
  node web/test/expo.mjs --metro "$url" "${device[@]}" --scale 3 --renderer "${renderer:-cpu}" "$@" --screenshot "$out/web/$name.png" >/dev/null
}
shot home
shot controls       --click Controls
shot controls-count --click Controls --click Increment
shot typed          --click Controls --click "Type here" --type "Hello web"
shot scroll         --click Scroll
shot scrolled       --click Scroll --scroll 200,500,0,400
shot text           --click Text
shot layout         --click Layout
shot image          --click Image
shot visuals        --click Visuals
shot home-dark      --dark
# The WebGPU presenter must match too (headless Chrome has an adapter on macOS; EXPO_WEB_GPU=0 skips it elsewhere).
[ "${EXPO_WEB_GPU:-1}" = 0 ] || renderer=gpu shot image-gpu --click Image
# native is the reference; both sides use the fallback fonts, so the 2% rule applies (never loosen it)
SIMCORE_MAX_DIFF=2.0 python3 scripts/compare_ios.py "$out/native" "$out/web" "$out"
