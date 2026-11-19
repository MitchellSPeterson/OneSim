#!/usr/bin/env bash
# Usage: build-swift-app.sh <app_path> <out_exe> [root_view]
# Builds a SwiftUI app against OneSim's own `SwiftUI` module (not Apple's).
set -euo pipefail
[ $# -ge 2 ] || { echo "usage: $0 <app_path> <out_exe> [root_view]" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$1"; OUT="$2"; ROOT_VIEW="${3:-}"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
CACHE="$TARGET/swiftui-cache"
FW="$ROOT/frameworks/swiftui/Sources"
LIB="$TARGET/release/libonesim_runtime.a"
LOG="$(mktemp)"; GEN=""; trap 'rm -f "$LOG"; [ -z "$GEN" ] || rm -rf "$GEN"' EXIT
fail() { echo "build failed:" >&2; cat "$LOG" >&2; exit 1; }
run() { "$@" >>"$LOG" 2>&1 || fail; }

mkdir -p "$CACHE"
# 1. Rust runtime (cargo no-ops when up to date).
run cargo build -p onesim-runtime --release --manifest-path "$ROOT/Cargo.toml"

# 2. SwiftUI module (rebuilt when any input is newer than the object).
if [ ! -f "$CACHE/SwiftUI.o" ] || [ -n "$(find "$FW" "$ROOT/crates/onesim-runtime/include" -type f -newer "$CACHE/SwiftUI.o" | head -1)" ]; then
  run swiftc -swift-version 5 -parse-as-library -module-name SwiftUI -O -wmo \
    -emit-module -emit-module-path "$CACHE/SwiftUI.swiftmodule" -emit-object -o "$CACHE/SwiftUI.o" \
    -I "$FW/OneSimBridge" "$FW"/SwiftUI/*.swift
fi

# 3. App sources.
if [ -d "$APP" ]; then SRCS=("$APP"/*.swift); else SRCS=("$APP"); fi
[ -e "${SRCS[0]}" ] || { echo "no .swift sources in $APP" >&2; exit 1; }
EXTRA=(); PAL=()
if ! grep -qs '^[[:space:]]*@main' "${SRCS[@]}" && ! printf '%s\n' "${SRCS[@]}" | grep -q '/main\.swift$'; then
  [ -n "$ROOT_VIEW" ] || { echo "no @main in sources and no root_view given" >&2; exit 1; }
  GEN="$CACHE/gen-$$"; mkdir -p "$GEN"
  printf 'import SwiftUI\n_onesimRun(%s())\n' "$ROOT_VIEW" > "$GEN/main.swift"
  EXTRA=("$GEN/main.swift")
elif grep -qs '^[[:space:]]*@main' "${SRCS[@]}"; then PAL=(-parse-as-library); fi
mkdir -p "$(dirname "$OUT")"
# `-I $CACHE` is searched before the SDK's frameworks, so `import SwiftUI` resolves to ours.
run swiftc -swift-version 5 -O ${PAL[@]+"${PAL[@]}"} -module-name OneSimApp -I "$CACHE" -I "$FW/OneSimBridge" \
  "${SRCS[@]}" ${EXTRA[@]+"${EXTRA[@]}"} "$CACHE/SwiftUI.o" "$LIB" -o "$OUT"
[ -z "${GEN:-}" ] || rm -rf "$GEN"
