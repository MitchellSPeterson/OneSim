#!/usr/bin/env bash
# Build a Jetpack Compose app for SimCore: build-app.sh <app.kt|dir> <out> [RootComposable]
#
# Compiles SimCore's Compose framework (frameworks/compose/src, cached by content hash) and the app's .kt files with
# the Kotlin Compose compiler plugin against the real androidx.compose.runtime, then writes <out>: an executable
# launcher that runs the guest (`java ... simcore.compose.Guest <Activity>`).
# The activity is the app's `ComponentActivity` subclass, or a generated one calling RootComposable() in setContent.
set -euo pipefail

APP="$1"
OUT="$2"
ROOT="${3:-}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
HOME_DIR="$(cd "$HERE/../.." && pwd)"
TC="$("$HERE/scripts/toolchain.sh")"
KOTLINC="$(ls -d "$TC"/kotlinc-*/ | tail -1)"
PLUGIN="$KOTLINC/lib/compose-compiler-plugin.jar"
STDLIB="$KOTLINC/lib/kotlin-stdlib.jar"
CP="$(ls "$TC"/jars/*.jar | tr '\n' ':')"
KFLAGS=(-jvm-target 17 -Xplugin="$PLUGIN" -nowarn -Xsuppress-version-warnings)

# 1. Framework (cached by the hash of its sources and the toolchain).
HASH="$( (cd "$HERE/src" && find . -name '*.kt' -type f | LC_ALL=C sort | xargs shasum; echo "$KOTLINC") | shasum | cut -c1-16)"
FW_DIR="$HOME_DIR/target/compose/framework-$HASH"
if [ ! -f "$FW_DIR/simcore-compose.jar" ]; then
  echo "simcore: compiling the Compose framework" >&2
  rm -rf "$FW_DIR.tmp" && mkdir -p "$FW_DIR.tmp"
  "$KOTLINC/bin/kotlinc" "${KFLAGS[@]}" -cp "$CP" -module-name simcore-compose \
    $(find "$HERE/src" -name '*.kt' -type f) -d "$FW_DIR.tmp/simcore-compose.jar"
  rm -rf "$FW_DIR" && mv "$FW_DIR.tmp" "$FW_DIR"
fi
FW="$FW_DIR/simcore-compose.jar"

# 2. App sources.
if [ -d "$APP" ]; then
  SOURCES=()
  while IFS= read -r f; do SOURCES+=("$f"); done < <(find "$APP" -name '*.kt' -type f -not -path '*/build/*' | LC_ALL=C sort)
else
  SOURCES=("$APP")
fi
[ ${#SOURCES[@]} -gt 0 ] || { echo "simcore: no .kt files in $APP" >&2; exit 1; }

pkg_of() { sed -n 's/^[[:space:]]*package[[:space:]]\{1,\}\([A-Za-z0-9_.]*\).*/\1/p' "$1" | head -1; }

BUILD="$(dirname "$OUT")/compose-build"
rm -rf "$BUILD" && mkdir -p "$BUILD"
if [ -n "$ROOT" ]; then
  FILE="$(grep -l -E "fun[[:space:]]+$ROOT[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
  [ -n "$FILE" ] || { echo "simcore: no fun $ROOT() in $APP" >&2; exit 1; }
  PKG="$(pkg_of "$FILE")"
  IMPORT=""; [ -n "$PKG" ] && IMPORT="import $PKG.$ROOT"
  cat > "$BUILD/SimcoreEntry.kt" <<KT
package simcore.entry

import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
$IMPORT

class SimcoreEntryActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { $ROOT() }
    }
}
KT
  SOURCES+=("$BUILD/SimcoreEntry.kt")
  ACTIVITY=simcore.entry.SimcoreEntryActivity
else
  LINE="$(grep -h -E "class[[:space:]]+[A-Za-z0-9_]+[[:space:]]*:[[:space:]]*ComponentActivity[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
  [ -n "$LINE" ] || { echo "simcore: no ComponentActivity subclass in $APP (pass a root composable with --root-view)" >&2; exit 1; }
  NAME="$(echo "$LINE" | sed -E 's/.*class[[:space:]]+([A-Za-z0-9_]+).*/\1/')"
  FILE="$(grep -l -E "class[[:space:]]+$NAME[[:space:]]*:" "${SOURCES[@]}" | head -1)"
  PKG="$(pkg_of "$FILE")"
  ACTIVITY="${PKG:+$PKG.}$NAME"
fi

"$KOTLINC/bin/kotlinc" "${KFLAGS[@]}" -cp "$CP$FW" -module-name app "${SOURCES[@]}" -d "$BUILD/app.jar"

cat > "$OUT" <<SH
#!/bin/sh
exec java -Xshare:auto -XX:TieredStopAtLevel=1 -cp "$BUILD/app.jar:$FW:$CP$STDLIB" simcore.compose.Guest $ACTIVITY
SH
chmod +x "$OUT"
