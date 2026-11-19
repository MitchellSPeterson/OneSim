#!/usr/bin/env bash
# Usage: build-web.sh <app_path> [root_view]
# Builds a SwiftUI app + SimCore's SwiftUI module + the Rust renderer into ONE Wasm module and
# assembles a static site in target/web/ (index.html, main.js, simcore.js, renderer.js, app.wasm, gpu/ = the WebGPU
# presenter built from crates/simcore-web-gpu). See docs/web.md.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: $0 <app_path> [root_view]" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$1"; ROOT_VIEW="${2:-}"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
CACHE="$TARGET/web-cache"
OUT="$TARGET/web"
FW="$ROOT/frameworks/swiftui/Sources"
RUST_TARGET=wasm32-wasip1
die() { echo "error: $*" >&2; exit 1; }

# 1. A swift.org toolchain whose version exactly matches an installed Wasm Swift SDK.
#    (Xcode's swiftc cannot target Wasm; a Wasm SDK only works with its own release.)
SDKS="${SWIFT_SDKS_DIR:-$HOME/Library/org.swift.swiftpm/swift-sdks}"
[ -d "$SDKS" ] || SDKS="$HOME/Library/org.swift.swiftpm/swift-sdks"
[ -d "$SDKS" ] || SDKS="$HOME/.swiftpm/swift-sdks"
TC=""; TAG=""
for cand in ${SIMCORE_SWIFT_TOOLCHAIN:+"$SIMCORE_SWIFT_TOOLCHAIN"} "$HOME"/Library/Developer/Toolchains/swift-*-RELEASE.xctoolchain /Library/Developer/Toolchains/swift-*-RELEASE.xctoolchain; do
  [ -x "$cand/usr/bin/swiftc" ] || continue
  t="$(basename "$cand" .xctoolchain)"
  if [ -d "$SDKS/${t}_wasm.artifactbundle" ]; then TC="$cand"; TAG="$t"; fi  # last (newest) match wins
done
if [ -z "$TC" ]; then
  have_tc="$(ls -d "$HOME"/Library/Developer/Toolchains/swift-*-RELEASE.xctoolchain 2>/dev/null | xargs -n1 basename 2>/dev/null | tr '\n' ' ')"
  have_sdk="$(ls "$SDKS" 2>/dev/null | grep _wasm | tr '\n' ' ')"
  die "no swift.org toolchain with a matching Wasm SDK.
  toolchains: ${have_tc:-none}
  wasm SDKs:  ${have_sdk:-none}
  Install both from the same release, e.g. (see docs/web.md):
    installer -pkg swift-X.Y.Z-RELEASE-osx.pkg -target CurrentUserHomeDirectory
    swift sdk install https://download.swift.org/swift-X.Y.Z-release/wasm-sdk/swift-X.Y.Z-RELEASE/swift-X.Y.Z-RELEASE_wasm.artifactbundle.tar.gz --checksum <sha256>"
fi
SWIFTC="$TC/usr/bin/swiftc"
SDK="$(ls -d "$SDKS/${TAG}_wasm.artifactbundle"/*/wasm32-unknown-wasip1 | head -1)"
[ -d "$SDK/WASI.sdk" ] || die "Wasm SDK at $SDK has no WASI.sdk"
echo "swift: $TAG, sdk: $SDK"

# 2. Rust target.
rustup target list --installed 2>/dev/null | grep -qx "$RUST_TARGET" || die "missing Rust target; run: rustup target add $RUST_TARGET"

mkdir -p "$CACHE" "$OUT"
LOG="$(mktemp)"; GEN=""; trap 'rm -f "$LOG"; [ -z "$GEN" ] || rm -rf "$GEN"' EXIT
run() { "$@" >>"$LOG" 2>&1 || { echo "build failed: $*" >&2; cat "$LOG" >&2; exit 1; }; }
SW=(-swift-version 5 -target wasm32-unknown-wasip1 -sdk "$SDK/WASI.sdk"
    -resource-dir "$SDK/swift.xctoolchain/usr/lib/swift_static" -static-stdlib)

# 2b. GPU presenter (optional at runtime): simcore-web-gpu for wasm32-unknown-unknown + wasm-bindgen glue.
#     The wasm-bindgen CLI must match the `wasm-bindgen` crate version pinned in crates/simcore-web-gpu/Cargo.toml.
GPU_TARGET=wasm32-unknown-unknown
rustup target list --installed 2>/dev/null | grep -qx "$GPU_TARGET" || die "missing Rust target; run: rustup target add $GPU_TARGET"
WB_CRATE="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' "$ROOT/crates/simcore-web-gpu/Cargo.toml")"
command -v wasm-bindgen >/dev/null || die "wasm-bindgen CLI not found; run: cargo install wasm-bindgen-cli --version $WB_CRATE --locked"
WB_CLI="$(wasm-bindgen --version | awk '{print $2}')"
[ "$WB_CLI" = "$WB_CRATE" ] || die "wasm-bindgen CLI $WB_CLI != crate $WB_CRATE; run: cargo install wasm-bindgen-cli --version $WB_CRATE --locked --force"

# 3. Rust renderer + in-process runtime (staticlib); bundled Inter + Phosphor only. Speed-optimized
#    with Wasm SIMD: a 3x frame drops from ~13 ms (opt-level s, scalar) to ~5 ms for +2.5% size.
#    simd128 needs Chrome 91 / Firefox 89 / Safari 16.4.
echo "rust: simcore-web ($RUST_TARGET)"
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  CARGO_PROFILE_RELEASE_PANIC=abort CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+simd128" \
  run cargo build -p simcore-web --release --target "$RUST_TARGET" --manifest-path "$ROOT/Cargo.toml"
LIB="$TARGET/$RUST_TARGET/release/libsimcore_web.a"

echo "rust: simcore-web-gpu ($GPU_TARGET)"
mkdir -p "$OUT/gpu"
CARGO_PROFILE_RELEASE_OPT_LEVEL=s CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  run cargo build -p simcore-web-gpu --release --target "$GPU_TARGET" --manifest-path "$ROOT/Cargo.toml"
run wasm-bindgen --target web --no-typescript --out-dir "$OUT/gpu" --out-name simcore_web_gpu \
  "$TARGET/$GPU_TARGET/release/simcore_web_gpu.wasm"
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --all-features "$OUT/gpu/simcore_web_gpu_bg.wasm" -o "$OUT/gpu/opt.wasm" >>"$LOG" 2>&1 \
    && mv "$OUT/gpu/opt.wasm" "$OUT/gpu/simcore_web_gpu_bg.wasm" || rm -f "$OUT/gpu/opt.wasm"
fi

# 4. SwiftUI module for Wasm (no Foundation).
if [ ! -f "$CACHE/SwiftUI.o" ] || [ -n "$(find "$FW" "$ROOT/crates/simcore-runtime/include" -type f -newer "$CACHE/SwiftUI.o" | head -1)" ]; then
  echo "swift: SwiftUI module"
  run "$SWIFTC" "${SW[@]}" -parse-as-library -module-name SwiftUI -Osize -wmo \
    -emit-module -emit-module-path "$CACHE/SwiftUI.swiftmodule" -emit-object -o "$CACHE/SwiftUI.o" \
    -I "$FW/SimCoreBridge" "$FW"/SwiftUI/*.swift
fi

# 5. App sources (same @main / root-view rules as build-swift-app.sh).
if [ -d "$APP" ]; then SRCS=("$APP"/*.swift); else SRCS=("$APP"); fi
[ -e "${SRCS[0]}" ] || die "no .swift sources in $APP"
EXTRA=(); PAL=()
if ! grep -qs '^[[:space:]]*@main' "${SRCS[@]}" && ! printf '%s\n' "${SRCS[@]}" | grep -q '/main\.swift$'; then
  [ -n "$ROOT_VIEW" ] || die "no @main in sources and no root_view given"
  GEN="$CACHE/gen-$$"; mkdir -p "$GEN"
  printf 'import SwiftUI\n_simcoreRun(%s())\n' "$ROOT_VIEW" > "$GEN/main.swift"
  EXTRA=("$GEN/main.swift")
elif grep -qs '^[[:space:]]*@main' "${SRCS[@]}"; then PAL=(-parse-as-library); fi

# 6. Link ONE reactor module: Swift (main = register app, returns) + Rust (exports for JS).
echo "swift: app + link"
EXPORTS=(simcore_web_init simcore_web_set_ios simcore_web_set_dark simcore_web_set_dynamic_type simcore_web_set_viewport simcore_web_set_scale
         simcore_web_pointer simcore_web_scroll simcore_web_drag simcore_web_tick simcore_web_key simcore_web_focused simcore_web_devices simcore_web_set_device simcore_web_buf simcore_web_buf_len
         simcore_web_inspect simcore_web_find simcore_web_fb_ptr simcore_web_fb_len simcore_web_fb_width
         simcore_web_fb_height simcore_web_tree_ptr simcore_web_tree_len simcore_web_set_output simcore_web_dl_ptr simcore_web_dl_len)
XL=(); for e in "${EXPORTS[@]}"; do XL+=(-Xlinker "--export=$e"); done
run "$SWIFTC" "${SW[@]}" -Osize ${PAL[@]+"${PAL[@]}"} -module-name SimCoreApp -I "$CACHE" -I "$FW/SimCoreBridge" \
  "${SRCS[@]}" ${EXTRA[@]+"${EXTRA[@]}"} "$CACHE/SwiftUI.o" "$LIB" \
  -Xclang-linker -mexec-model=reactor \
  -Xclang-linker "-resource-dir=$SDK/swift.xctoolchain/usr/lib/swift_static/clang" \
  -Xlinker -z -Xlinker stack-size=8388608 -Xlinker --gc-sections -Xlinker --strip-all \
  ${XL[@]+"${XL[@]}"} -o "$OUT/app.wasm"

if command -v wasm-opt >/dev/null; then
  echo "wasm-opt -Oz"
  # Optimisation is optional: if this wasm-opt rejects the flags or the module, keep the unoptimised build.
  if wasm-opt -Oz --enable-bulk-memory --enable-sign-ext --enable-mutable-globals --enable-nontrapping-float-to-int \
      --enable-simd --enable-reference-types --enable-multivalue "$OUT/app.wasm" -o "$OUT/app.opt.wasm" >>"$LOG" 2>&1; then
    mv "$OUT/app.opt.wasm" "$OUT/app.wasm"
  else
    rm -f "$OUT/app.opt.wasm"; echo "note: wasm-opt failed - keeping the unoptimised module (see below)"; tail -5 "$LOG"
  fi
else
  echo "note: wasm-opt not found (brew install binaryen) - skipping size optimization"
fi
cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/simcore.js" "$ROOT/web/renderer.js" "$OUT/"
echo "built $OUT ($(wc -c < "$OUT/app.wasm" | tr -d ' ') bytes app.wasm, $(wc -c < "$OUT/gpu/simcore_web_gpu_bg.wasm" | tr -d ' ') bytes gpu)"
