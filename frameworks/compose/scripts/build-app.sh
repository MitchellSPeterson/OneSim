#!/usr/bin/env bash
# Build a Jetpack Compose app for OneSim: build-app.sh <app.kt|dir> <out> [RootComposable]
#
# Compiles OneSim's Compose framework (frameworks/compose/src, cached by content hash) and the app's .kt files with
# the Kotlin Compose compiler plugin against the real androidx.compose.runtime, then writes <out>: an executable
# launcher that runs the guest (`java ... onesim.compose.Guest <Activity>`).
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
SHA=shasum; command -v shasum >/dev/null || SHA=sha256sum # minimal Linux and Git Bash lack shasum
KFLAGS=(-jvm-target 17 -Xplugin="$PLUGIN" -nowarn -Xsuppress-version-warnings)

# 1. Framework (cached by the hash of its sources and the toolchain).
HASH="$( (cd "$HERE" && find src jvm -name '*.kt' -type f | LC_ALL=C sort | xargs $SHA; echo "$KOTLINC") | $SHA | cut -c1-16)"
FW_DIR="$HOME_DIR/target/compose/framework-$HASH"
if [ ! -f "$FW_DIR/onesim-compose.jar" ]; then
  echo "onesim: compiling the Compose framework" >&2
  rm -rf "$FW_DIR.tmp" && mkdir -p "$FW_DIR.tmp"
  "$KOTLINC/bin/kotlinc" "${KFLAGS[@]}" -cp "$CP" -module-name onesim-compose \
    $(find "$HERE/src" "$HERE/jvm" -name '*.kt' -type f) -d "$FW_DIR.tmp/onesim-compose.jar"
  rm -rf "$FW_DIR" && mv "$FW_DIR.tmp" "$FW_DIR"
fi
FW="$FW_DIR/onesim-compose.jar"

# 2. App sources.
if [ -d "$APP" ]; then
  SOURCES=()
  while IFS= read -r f; do SOURCES+=("$f"); done < <(find "$APP" -name '*.kt' -type f -not -path '*/build/*' | LC_ALL=C sort)
else
  SOURCES=("$APP")
fi
[ ${#SOURCES[@]} -gt 0 ] || { echo "onesim: no .kt files in $APP" >&2; exit 1; }

pkg_of() { sed -n 's/^[[:space:]]*package[[:space:]]\{1,\}\([A-Za-z0-9_.]*\).*/\1/p' "$1" | head -1; }

BUILD="$(dirname "$OUT")/compose-build"
rm -rf "$BUILD" && mkdir -p "$BUILD"
if [ -n "$ROOT" ]; then
  FILE="$(grep -l -E "fun[[:space:]]+$ROOT[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
  [ -n "$FILE" ] || { echo "onesim: no fun $ROOT() in $APP" >&2; exit 1; }
  PKG="$(pkg_of "$FILE")"
  IMPORT=""; [ -n "$PKG" ] && IMPORT="import $PKG.$ROOT"
  cat > "$BUILD/OneSimEntry.kt" <<KT
package onesim.entry

import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
$IMPORT

class OneSimEntryActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { $ROOT() }
    }
}
KT
  SOURCES+=("$BUILD/OneSimEntry.kt")
  ACTIVITY=onesim.entry.OneSimEntryActivity
else
  LINE="$(grep -h -E "class[[:space:]]+[A-Za-z0-9_]+[[:space:]]*:[[:space:]]*ComponentActivity[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
  [ -n "$LINE" ] || { echo "onesim: no ComponentActivity subclass in $APP (pass a root composable with --root-view)" >&2; exit 1; }
  NAME="$(echo "$LINE" | sed -E 's/.*class[[:space:]]+([A-Za-z0-9_]+).*/\1/')"
  FILE="$(grep -l -E "class[[:space:]]+$NAME[[:space:]]*:" "${SOURCES[@]}" | head -1)"
  PKG="$(pkg_of "$FILE")"
  ACTIVITY="${PKG:+$PKG.}$NAME"
fi

"$KOTLINC/bin/kotlinc" "${KFLAGS[@]}" -cp "$CP$FW" -module-name app "${SOURCES[@]}" -d "$BUILD/app.jar"

JAVA_ARGS="-Xshare:auto -XX:TieredStopAtLevel=1 -cp"
if [[ "$OUT" == *.cmd ]]; then # Windows (Git Bash): a batch launcher with a native `;` classpath
  printf '@java %s "%s" onesim.compose.JvmGuest %s\r\n' "$JAVA_ARGS" "$(for f in "$BUILD/app.jar" "$FW" "$TC"/jars/*.jar "$STDLIB"; do cygpath -w "$f"; done | tr '\n' ';')" "$ACTIVITY" > "$OUT"
else
  printf '#!/bin/sh\nexec java %s "%s" onesim.compose.JvmGuest %s\n' "$JAVA_ARGS" "$BUILD/app.jar:$FW:$CP$STDLIB" "$ACTIVITY" > "$OUT"
  chmod +x "$OUT"
fi
