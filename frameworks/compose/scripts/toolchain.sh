#!/usr/bin/env bash
# Fetch the pinned Kotlin compiler and the Compose runtime jars the Compose guest needs.
# Prints the toolchain directory. Idempotent: existing files are kept.
#
# Layout of $SIMCORE_COMPOSE_TOOLCHAIN (default ~/.cache/simcore/compose):
#   kotlinc-<ver>/       Kotlin compiler distribution (lib/compose-compiler-plugin.jar is the Compose plugin)
#   jars/*.jar           Compose runtime + its runtime dependencies (no Gradle)
set -euo pipefail

KOTLIN=2.4.21
COMPOSE_RUNTIME=1.13.0-beta01
COLLECTION=1.6.0
COROUTINES=1.9.0

DIR="${SIMCORE_COMPOSE_TOOLCHAIN:-$HOME/.cache/simcore/compose}"
mkdir -p "$DIR/jars"

if [ ! -x "$DIR/kotlinc-$KOTLIN/bin/kotlinc" ]; then
  echo "simcore: downloading Kotlin $KOTLIN" >&2
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
fetch() { # repo group artifact version
  local repo=$1 group=$2 artifact=$3 version=$4
  local out="$DIR/jars/$artifact-$version.jar"
  [ -s "$out" ] && return 0
  echo "simcore: downloading $group:$artifact:$version" >&2
  curl -fsSL -o "$out.part" "$repo/${group//.//}/$artifact/$version/$artifact-$version.jar"
  mv "$out.part" "$out"
}
fetch "$GOOGLE" androidx.compose.runtime runtime-desktop "$COMPOSE_RUNTIME"
fetch "$GOOGLE" androidx.compose.runtime runtime-annotation-jvm "$COMPOSE_RUNTIME"
fetch "$GOOGLE" androidx.compose.runtime runtime-saveable-desktop "$COMPOSE_RUNTIME"
fetch "$GOOGLE" androidx.collection collection-jvm "$COLLECTION"
fetch "$CENTRAL" org.jetbrains.kotlinx kotlinx-coroutines-core-jvm "$COROUTINES"

echo "$DIR"
