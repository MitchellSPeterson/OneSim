#!/usr/bin/env bash
# Use the agent-device command pinned to the Android 17 Pixel 10a.
set -euo pipefail
cd "$(dirname "$0")/../../.."
: "${AGENT_DEVICE:?set AGENT_DEVICE to the agent-device command returned by device_open}"
url=${EXPO_ANDROID_LAUNCH_URL:-exp://127.0.0.1:8081}
out=tests/fixtures/expo-android

launch() { "$AGENT_DEVICE" open host.exp.exponent "$url" --relaunch >/dev/null; }
ref() {
  local result
  for _ in $(seq 40); do
    result=$("$AGENT_DEVICE" snapshot -i | awk -v label="\"$1\"" 'index($0, label) && !found { print $1; found = 1 }')
    if [ -n "$result" ]; then
      printf '%s\n' "$result"
      return
    fi
    sleep 0.5
  done
  echo "Could not find $1 on the emulator" >&2
  return 1
}
tap() {
  local target
  target=$(ref "$1")
  "$AGENT_DEVICE" press "$target" --settle >/dev/null
}
shot() { "$AGENT_DEVICE" screenshot "$out/$1.png" >/dev/null; }

launch
"$AGENT_DEVICE" settings appearance light >/dev/null
trap '"$AGENT_DEVICE" settings appearance light >/dev/null' EXIT
ref Controls >/dev/null
shot home
tap Controls
shot controls
tap Increment
tap Alpha
shot controls-changed
tap Back
tap Scroll
tap Jump
shot scroll-scrolled
tap Back
tap Image
shot image
tap Back
tap Visuals
shot visuals
"$AGENT_DEVICE" settings appearance dark >/dev/null
launch
ref Controls >/dev/null
shot home-dark
