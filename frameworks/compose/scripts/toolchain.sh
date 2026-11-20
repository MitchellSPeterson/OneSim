#!/usr/bin/env bash
# Fetch the pinned Kotlin compiler and the Compose runtime artifacts the Compose guest needs.
# Prints the toolchain directory. Idempotent: existing files are kept.
#
# Usage: toolchain.sh [--wasm]
#   (default)  Fetch Kotlin + JVM jars for the native guest.
#   --wasm     Fetch Kotlin + wasmJs klibs for the browser build.
#
# Layout of $SIMCORE_COMPOSE_TOOLCHAIN (default ~/.cache/simcore/compose):
#   kotlinc-<ver>/       Kotlin compiler distribution (lib/compose-compiler-plugin.jar is the Compose plugin)
#   jars/*.jar           Compose runtime + its JVM runtime dependencies (no Gradle)
#   klibs/*.klib         Compose runtime + its wasmJs dependencies (no Gradle)
set -euo pipefail

WASM=0
if [ "${1:-}" = "--wasm" ]; then WASM=1; fi

KOTLIN=2.4.21
COMPOSE_RUNTIME=1.13.0-beta01
COLLECTION=1.6.0
COROUTINES=1.9.0
ANNOTATION=1.9.1
ATOMICFU=0.28.0
BROWSER=0.5.0

DIR="${SIMCORE_COMPOSE_TOOLCHAIN:-$HOME/.cache/simcore/compose}"
mkdir -p "$DIR/jars" "$DIR/klibs"

if [ ! -x "$DIR/kotlinc-$KOTLIN/bin/kotlinc" ]; then
  echo "onesim: downloading Kotlin $KOTLIN" >&2
  tmp="$(mktemp -d)"
  curl -fsSL -o "$tmp/kotlinc.zip" \
    "https://github.com/JetBrains/kotlin/releases/download/v$KOTLIN/kotlin-compiler-$KOTLIN.zip"
  unzip -q "$tmp/kotlinc.zip" -d "$tmp"
  rm -rf "$DIR/kotlinc-$KOTLIN"
  mv "$tmp/kotlinc" "$DIR/kotlinc-$KOTLIN"
  rm -rf "$tmp"
fi

GOOGLE=https://dl.google.com/android/maven2
CENTRAL=https://repo1.maven.org/maven2
fetch() { # subdir ext repo group artifact version
  local subdir=$1 ext=$2 repo=$3 group=$4 artifact=$5 version=$6
  local out="$DIR/$subdir/$artifact-$version.$ext"
  [ -s "$out" ] && return 0
  echo "onesim: downloading $group:$artifact:$version" >&2
  curl -fsSL -o "$out.part" "$repo/${group//.//}/$artifact/$version/$artifact-$version.$ext"
  mv "$out.part" "$out"
}

if [ "$WASM" = 1 ]; then
  fetch klibs klib "$GOOGLE" androidx.compose.runtime runtime-wasm-js "$COMPOSE_RUNTIME"
  fetch klibs klib "$GOOGLE" androidx.compose.runtime runtime-annotation-wasm-js "$COMPOSE_RUNTIME"
  fetch klibs klib "$GOOGLE" androidx.compose.runtime runtime-saveable-wasm-js "$COMPOSE_RUNTIME"
  fetch klibs klib "$GOOGLE" androidx.collection collection-wasm-js "$COLLECTION"
  fetch klibs klib "$GOOGLE" androidx.annotation annotation-wasm-js "$ANNOTATION"
  fetch klibs klib "$CENTRAL" org.jetbrains.kotlinx kotlinx-coroutines-core-wasm-js "$COROUTINES"
  fetch klibs klib "$CENTRAL" org.jetbrains.kotlinx atomicfu-wasm-js "$ATOMICFU"
  fetch klibs klib "$CENTRAL" org.jetbrains.kotlinx kotlinx-browser-wasm-js "$BROWSER"
else
  fetch jars jar "$GOOGLE" androidx.compose.runtime runtime-desktop "$COMPOSE_RUNTIME"
  fetch jars jar "$GOOGLE" androidx.compose.runtime runtime-annotation-jvm "$COMPOSE_RUNTIME"
  fetch jars jar "$GOOGLE" androidx.compose.runtime runtime-saveable-desktop "$COMPOSE_RUNTIME"
  fetch jars jar "$GOOGLE" androidx.collection collection-jvm "$COLLECTION"
  fetch jars jar "$CENTRAL" org.jetbrains.kotlinx kotlinx-coroutines-core-jvm "$COROUTINES"
fi

echo "$DIR"
