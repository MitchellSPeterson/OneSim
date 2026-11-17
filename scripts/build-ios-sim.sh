#!/usr/bin/env bash
# Usage: build-ios-sim.sh <swift sources dir> <out .app path> [bundle id]
# Builds real SwiftUI sources into an ad-hoc signed iOS Simulator .app.
set -euo pipefail
[ $# -ge 2 ] || { echo "usage: $0 <sources dir> <out .app> [bundle id]" >&2; exit 2; }
SRC="$1"; APP="$2"; BID="${3:-dev.simcore.showcase}"
NAME="$(basename "$APP" .app)"
rm -rf "$APP"; mkdir -p "$APP"
xcrun --sdk iphonesimulator swiftc -parse-as-library -target arm64-apple-ios17.0-simulator -O \
  -o "$APP/$NAME" "$SRC"/*.swift
cat > "$APP/Info.plist" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$BID</string>
<key>CFBundleExecutable</key><string>$NAME</string>
<key>CFBundleName</key><string>$NAME</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>MinimumOSVersion</key><string>17.0</string>
<key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>
<key>UIDeviceFamily</key><array><integer>1</integer></array>
<key>LSRequiresIPhoneOS</key><true/>
<key>UILaunchScreen</key><dict/>
<key>UIApplicationSceneManifest</key><dict><key>UIApplicationSupportsMultipleScenes</key><false/></dict>
<key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationPortrait</string></array>
</dict></plist>
PL
codesign -s - --force "$APP"
echo "built $APP"
