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

# Normalize the status bar to match SimCore: 9:41, full wifi (no "no internet" mark), full battery, no notifications.
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

$AD settings appearance light >/dev/null
launch com.example.counter.MainActivity;   shot counter
tap "$(ref Increment)"; tap "$(ref Increment)"; tap "$(ref Decrement)"; shot counter-changed
launch com.example.showcase.ShowcaseActivity; shot showcase
# The Switch has no label: its center is 927,668 px (353,254 dp).
tap "$(ref Filled)"; tap "$(ref Compose)"; tap 927 668; shot showcase-changed
$AD settings appearance dark >/dev/null
launch com.example.showcase.ShowcaseActivity; shot showcase-dark
$AD settings appearance light >/dev/null
demo exit
