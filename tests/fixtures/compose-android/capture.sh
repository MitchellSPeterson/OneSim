#!/usr/bin/env bash
# Capture the Android reference states for scripts/compare-android.sh on a real emulator.
# Usage: AGENT_DEVICE=<agent-device command from the T3 device tools' device_open> tests/fixtures/compose-android/capture.sh
# Needs: a booted Pixel 10a emulator (Android 17, API 37), adb on PATH or $ANDROID_HOME, Java 17+.
set -euo pipefail
cd "$(dirname "$0")/../../.."
: "${AGENT_DEVICE:?set AGENT_DEVICE to the agent-device command returned by device_open}"
AD=$AGENT_DEVICE
ADB=${ADB:-$(command -v adb || echo "${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb")}
out=tests/fixtures/compose-android
pkg=dev.simcore.reference

# Build and install the examples as a real APK (frameworks/compose/reference-app).
(cd frameworks/compose/reference-app && echo "sdk.dir=${ANDROID_HOME:-$HOME/Library/Android/sdk}" > local.properties && ${GRADLE:-gradle} -q assembleDebug)
"$ADB" install -r frameworks/compose/reference-app/app/build/outputs/apk/debug/app-debug.apk >/dev/null

# Normalize the status bar to match OneSim: 9:41, full wifi (no "no internet" mark), full battery, no notifications.
"$ADB" shell settings put global sysui_demo_allowed 1
demo() { "$ADB" shell am broadcast -a com.android.systemui.demo -e command "$@" >/dev/null; }
demo enter
demo clock -e hhmm 0941
demo battery -e level 100 -e plugged false
demo network -e wifi show -e level 4 -e fully true -e mobile hide
demo notifications -e visible false
demo status -e volume hide -e bluetooth hide -e location hide -e alarm hide -e sync hide -e mute hide -e speakerphone hide
$AD settings animations off >/dev/null

launch() { $AD open $pkg --activity "$pkg/$1" --relaunch >/dev/null; $AD wait 1500 >/dev/null 2>&1 || true; }
ref() { $AD snapshot -i 2>/dev/null | grep "\"$1\"" | head -1 | awk '{print $1}'; }
tap() { $AD press "$@" --settle >/dev/null; }
shot() { $AD wait 1500 >/dev/null 2>&1 || true; $AD screenshot "$out/$1.png" >/dev/null; echo "captured $1"; }

# A finger drag in 8 equal moves (fractional px, like OneSim's synthesized drag), held still at the end before it
# lifts so Android's input resampling does not leave an extrapolated overshoot (the lift's own movement is ignored).
drag() {
  "$ADB" shell "$(awk -v x="$1" -v y="$2" -v dx="$3" -v dy="$4" 'BEGIN { s = "input motionevent DOWN " x " " y
    for (i = 1; i <= 8; i++) s = s "; input motionevent MOVE " x + dx * i / 8 " " y + dy * i / 8
    e = " " x + dx " " y + dy; print s "; sleep 0.3; input motionevent MOVE" e "; sleep 0.3; input motionevent UP" e }')"
}
w() { $AD wait "$1" >/dev/null 2>&1 || true; }
# examples/compose-interaction (all coordinates in px; OneSim's shot lines use px / 2.625).
capture_interaction() {
  launch com.example.interaction.InteractionActivity; shot interaction
  tap 362 435; w 600; tap 362 435; w 600                           # card: Clicks 2
  $AD longpress 362 664 1000 >/dev/null; w 600                     # long click
  $AD press 362 664 --count 2 --interval-ms 60 >/dev/null; w 600   # double click
  tap 362 664; w 800                                               # click (after the double tap timeout)
  drag 115 826 300 0; drag 115 957 300 100                         # draggable, draggable2D
  tap 362 1335; w 500                                              # pointerInput detectTapGestures
  drag 362 1486 300 0; w 800                                       # scrollable (clamped at 315 px)
  tap 367 1626; tap 300 1745                                       # selectable "Two", toggleable Wi-Fi row
  shot interaction-changed
  # A finger held on the card: the pressed ripple at full spread and collectIsPressedAsState.
  launch com.example.interaction.InteractionActivity
  "$ADB" shell input motionevent DOWN 362 435; shot interaction-pressed; "$ADB" shell input motionevent UP 362 435
}

$AD settings appearance light >/dev/null
# SIMCORE_CAPTURE_ONLY=interaction recaptures just the interaction states.
if [ "${SIMCORE_CAPTURE_ONLY:-}" = interaction ]; then capture_interaction; demo exit; exit 0; fi
launch com.example.counter.MainActivity;   shot counter
tap "$(ref Increment)"; tap "$(ref Increment)"; tap "$(ref Decrement)"; shot counter-changed
launch com.example.showcase.ShowcaseActivity; shot showcase
# The Switch has no label: its center is 927,668 px (353,254 dp).
tap "$(ref Filled)"; tap "$(ref Compose)"; tap 927 668; shot showcase-changed
$AD settings appearance dark >/dev/null
launch com.example.showcase.ShowcaseActivity; shot showcase-dark
$AD settings appearance light >/dev/null
launch com.example.input.InputActivity; shot input
$AD fill "$(ref Name)" NameHello --settle >/dev/null
$AD keyboard dismiss >/dev/null
$AD swipe 226 822 532 822 >/dev/null
shot input-changed
capture_interaction
demo exit
