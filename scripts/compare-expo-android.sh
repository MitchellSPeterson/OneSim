#!/usr/bin/env bash
# Compare the Expo showcase with Android 17 Pixel 10a references.
set -euo pipefail
cd "$(dirname "$0")/.."
fixtures=tests/fixtures/expo-android
out=target/compare/expo-android
app=examples/expo-showcase
metro_url=${EXPO_ANDROID_METRO_URL:-}

if [ ! -f "$fixtures/home.png" ]; then
  echo "Missing Android Expo references. See $fixtures/README.md." >&2
  exit 1
fi

mkdir -p "$out/onesim"
cargo build -q -p onesim-cli
if [ -z "$metro_url" ]; then
  if [ ! -d "$app/node_modules" ]; then
    echo "Run npm ci in $app before comparing Android Expo." >&2
    exit 1
  fi
  port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
  metro_url=http://localhost:$port
  (cd "$app" && CI=1 EXPO_OFFLINE=1 EXPO_NO_TELEMETRY=1 exec node node_modules/expo/bin/cli start --port "$port") >"$out/metro.log" 2>&1 &
  metro=$!
  trap 'kill "$metro" 2>/dev/null || true' EXIT
  for _ in $(seq 60); do
    curl -fsS "$metro_url/status" >/dev/null 2>&1 && break
    sleep 1
  done
  curl -fsS "$metro_url/status" >/dev/null
fi

shot() {
  local name=$1
  shift
  test -f "$fixtures/$name.png" || { echo "Missing $fixtures/$name.png" >&2; return 1; }
  ./target/debug/onesim expo --platform android --url "$metro_url" "$@" --screenshot "$out/onesim/$name.png" >/dev/null
}

shot home
shot home-dark --dark
shot controls --click Controls
shot controls-changed --click Controls --click Increment --click Alpha
shot scroll-scrolled --click Scroll --click Jump
shot image --click Image
shot visuals --click Visuals
python3 scripts/compare_ios.py "$fixtures" "$out/onesim" "$out"
