#!/usr/bin/env bash
# Compare the same Expo UI screen against each platform's native reference.
set -euo pipefail
cd "$(dirname "$0")/.."

case "${1:-}" in
  26.5) fixtures=tests/fixtures/expo-ui-ios; args=(--ios 26.5 --device iphone-17-pro); input=(--key enter --drag 79,414,110,0); scroll_y=480 ;;
  27.2) fixtures=tests/fixtures/ios27/expo-ui-ios; args=(--ios 27.2 --device iphone-18-pro); input=(--key enter); scroll_y=480 ;;
  android) fixtures=tests/fixtures/expo-ui-android; args=(--platform android); input=(--drag 79,440,110,0); scroll_y=510 ;;
  *) echo "Usage: $0 {26.5|27.2|android}" >&2; exit 2 ;;
esac

app=examples/expo-ui
out=target/compare/expo-ui/$1
url=${EXPO_UI_METRO_URL:-}
mkdir -p "$out/onesim"
cargo build -q -p onesim-cli
if [ -z "$url" ]; then
  test -d "$app/node_modules" || { echo "Run npm ci in $app." >&2; exit 1; }
  port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
  url=http://localhost:$port
  (cd "$app" && CI=1 EXPO_OFFLINE=1 EXPO_NO_TELEMETRY=1 exec node node_modules/expo/bin/cli start --port "$port") >"$out/metro.log" 2>&1 &
  metro=$!
  trap 'kill "$metro" 2>/dev/null || true' EXIT
  for _ in $(seq 60); do
    curl -fsS "$url/status" >/dev/null 2>&1 && break
    sleep 1
  done
  curl -fsS "$url/status" >/dev/null
fi

shot() {
  local name=$1
  shift
  test -f "$fixtures/$name.png"
  ./target/debug/onesim expo --url "$url" "${args[@]}" "$@" --inspect \
    --screenshot "$out/onesim/$name.png" >"$out/$name.json"
}

changed=(--click Increment --click Notifications --click Name --type Hello "${input[@]}")
shot home
shot changed "${changed[@]}"
shot scrolled "${changed[@]}" --scroll "100,$scroll_y,0,400" --click "Embedded 0"
python3 - "$out" "$1" <<'PY'
import json
import sys
from pathlib import Path

def texts(node):
    for key in ("text", "label"):
        if key in node:
            yield node[key]
    for child in node["children"]:
        yield from texts(child)

out, platform = Path(sys.argv[1]), sys.argv[2]
changed = {"Count 1", "Notifications on", "Name: Hello" if platform == "android" else "Saved Hello",
           "Volume 25" if platform == "27.2" else "Volume 75"}
for state, expected in {
    "home": {"Expo UI", "Below the host"},
    "changed": changed,
    "scrolled": changed | {"Embedded 1"},
}.items():
    missing = expected - set(texts(json.loads((out / f"{state}.json").read_text())))
    if missing:
        raise SystemExit(f"{platform} {state}: missing {', '.join(sorted(missing))}")
PY
SIMCORE_COMPARE_IOS="$1" python3 scripts/compare_ios.py "$fixtures" "$out/onesim" "$out"
