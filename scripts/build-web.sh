#!/usr/bin/env bash
# Usage: build-web.sh <app_path> [root_view]
#        build-web.sh --expo
# Builds a SwiftUI or Jetpack Compose app and the Rust renderer into a static site in target/web/
# (index.html, main.js, onesim.js, renderer.js, app.wasm, gpu/ = WebGPU presenter from onesim-web-gpu,
# plus guest/ = Kotlin/Wasm guest for Compose apps). See docs/web.md.
# --expo: the site (target/web-expo/) runs an Expo / React Native app's JavaScript in a Web Worker
# (guest/ = frameworks/react-native/guest); the app is chosen at load time with ?metro=URL or ?bundle=URL.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: $0 <app_path> [root_view] | $0 --expo" >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$1"; ROOT_VIEW="${2:-}"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
CACHE="$TARGET/web-cache"
OUT="${SIMCORE_WEB_OUT:-$TARGET/web}"  # serve-web.sh builds each example into its own directory
[ -n "${SIMCORE_WEB_OUT:-}" ] || [ "$1" != --expo ] || OUT="$TARGET/web-expo"
RUST_TARGET=wasm32-wasip1
GPU_TARGET=wasm32-unknown-unknown
die() { echo "error: $*" >&2; exit 1; }

# Detect framework from the app path (.kt -> Compose, otherwise SwiftUI).
FRAMEWORK=swiftui
if [ "$APP" = --expo ]; then
  FRAMEWORK=expo
elif [ -f "$APP" ]; then
  case "$APP" in *.kt) FRAMEWORK=compose ;; esac
elif [ -d "$APP" ]; then
  if [ -n "$(find "$APP" -name '*.kt' -type f -not -path '*/build/*' | head -1)" ]; then
    FRAMEWORK=compose
  fi
else
  die "app path not found: $APP"
fi

# 1. Rust targets and wasm-bindgen CLI (shared by SwiftUI and Compose).
rustup target list --installed 2>/dev/null | grep -qx "$RUST_TARGET" || die "missing Rust target; run: rustup target add $RUST_TARGET"
rustup target list --installed 2>/dev/null | grep -qx "$GPU_TARGET" || die "missing Rust target; run: rustup target add $GPU_TARGET"
WB_CRATE="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' "$ROOT/crates/onesim-web-gpu/Cargo.toml")"
command -v wasm-bindgen >/dev/null || die "wasm-bindgen CLI not found; run: cargo install wasm-bindgen-cli --version $WB_CRATE --locked"
WB_CLI="$(wasm-bindgen --version | awk '{print $2}')"
[ "$WB_CLI" = "$WB_CRATE" ] || die "wasm-bindgen CLI $WB_CLI != crate $WB_CRATE; run: cargo install wasm-bindgen-cli --version $WB_CRATE --locked --force"

mkdir -p "$CACHE" "$OUT"
LOG="$(mktemp)"; GEN=""; trap 'rm -f "$LOG"; [ -z "$GEN" ] || rm -rf "$GEN"' EXIT
run() { "$@" >>"$LOG" 2>&1 || { echo "build failed: $*" >&2; cat "$LOG" >&2; exit 1; }; }

build_gpu() {
  echo "rust: onesim-web-gpu ($GPU_TARGET)"
  mkdir -p "$OUT/gpu" "$CACHE/gpu"
  CARGO_PROFILE_RELEASE_OPT_LEVEL=s CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    run cargo build -p onesim-web-gpu --release --target "$GPU_TARGET" --manifest-path "$ROOT/Cargo.toml"
  local raw="$TARGET/$GPU_TARGET/release/onesim_web_gpu.wasm"
  if [ ! -f "$CACHE/gpu/onesim_web_gpu_bg.wasm" ] || [ "$raw" -nt "$CACHE/gpu/onesim_web_gpu_bg.wasm" ]; then
    run wasm-bindgen --target web --no-typescript --out-dir "$CACHE/gpu" --out-name onesim_web_gpu "$raw"
    if command -v wasm-opt >/dev/null; then
      # Binaryen 132+ can emit compact imports under --all-features. Chrome still gates them behind a flag.
      wasm-opt -Oz --all-features --disable-compact-imports "$CACHE/gpu/onesim_web_gpu_bg.wasm" -o "$CACHE/gpu/opt.wasm" >>"$LOG" 2>&1 \
        && mv "$CACHE/gpu/opt.wasm" "$CACHE/gpu/onesim_web_gpu_bg.wasm" || rm -f "$CACHE/gpu/opt.wasm"
    fi
  fi
  cp "$CACHE/gpu/onesim_web_gpu.js" "$CACHE/gpu/onesim_web_gpu_bg.wasm" "$OUT/gpu/"
}

optimize_wasm() {
  local src=$1 dst=$2
  if command -v wasm-opt >/dev/null; then
    echo "wasm-opt -Oz"
    # Optimisation is optional: if this wasm-opt rejects the flags or the module, keep the unoptimised build.
    if wasm-opt -Oz --enable-bulk-memory --enable-sign-ext --enable-mutable-globals --enable-nontrapping-float-to-int \
        --enable-simd --enable-reference-types --enable-multivalue "$src" -o "$dst.opt.wasm" >>"$LOG" 2>&1; then
      mv "$dst.opt.wasm" "$dst"
    else
      rm -f "$dst.opt.wasm"
      [ "$src" -ef "$dst" ] || cp "$src" "$dst"
      echo "note: wasm-opt failed - keeping the unoptimised module (see below)"; tail -5 "$LOG"
    fi
  else
    [ "$src" -ef "$dst" ] || cp "$src" "$dst"
    echo "note: wasm-opt not found (brew install binaryen) - skipping size optimization"
  fi
}

if [ "$FRAMEWORK" != swiftui ]; then
  # 2. Standalone Rust renderer module (cdylib, JS-hosted guest imports; React Native mode uses none of them).
  echo "rust: onesim-web ($RUST_TARGET, js-guest)"
  CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_PANIC=abort CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+simd128" \
    run cargo rustc -p onesim-web --lib --crate-type cdylib --features js-guest --release --target "$RUST_TARGET" \
    --manifest-path "$ROOT/Cargo.toml" -- -C link-arg=-z -C link-arg=stack-size=8388608
  RAW_WASM="$TARGET/$RUST_TARGET/release/onesim_web.wasm"
  CACHED_APP="$CACHE/app-js-guest.wasm"
  if [ ! -f "$CACHED_APP" ] || [ "$RAW_WASM" -nt "$CACHED_APP" ]; then
    optimize_wasm "$RAW_WASM" "$CACHED_APP"
  fi
  cp "$CACHED_APP" "$OUT/app.wasm"
  build_gpu
fi

if [ "$FRAMEWORK" = expo ]; then
  rm -rf "$OUT/guest" "$OUT/expo" && mkdir -p "$OUT/guest" "$OUT/expo"
  cp "$ROOT"/frameworks/react-native/guest/*.js "$OUT/guest/"
  cp "$ROOT"/web/expo/*.js "$OUT/expo/"
  cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/onesim.js" "$ROOT/web/renderer.js" "$OUT/"
  echo "built $OUT ($(wc -c < "$OUT/app.wasm" | tr -d ' ') bytes app.wasm, $(wc -c < "$OUT/gpu/onesim_web_gpu_bg.wasm" | tr -d ' ') bytes gpu)"
  exit 0
fi

if [ "$FRAMEWORK" = compose ]; then

  # 3. Kotlin/Wasm toolchain + cached Compose framework klib.
  COMPOSE_FW="$ROOT/frameworks/compose"
  TC="$("$COMPOSE_FW/scripts/toolchain.sh" --wasm)"
  KOTLINC="$(ls -d "$TC"/kotlinc-*/ | tail -1)"
  PLUGIN="$KOTLINC/lib/compose-compiler-plugin.jar"
  LIBS="$KOTLINC/lib/kotlin-stdlib-wasm-js.klib:$(ls "$TC"/klibs/*.klib | LC_ALL=C sort | tr '\n' ':')"
  LIBS="${LIBS%:}"
  SHA=shasum; command -v shasum >/dev/null || SHA=sha256sum

  HASH="$( (cd "$COMPOSE_FW" && find src wasm -name '*.kt' -type f | LC_ALL=C sort | xargs $SHA; echo "$KOTLINC") | $SHA | cut -c1-16)"
  FW_DIR="$CACHE/compose-$HASH"
  FW_KLIB="$FW_DIR/onesim-compose.klib"
  if [ ! -f "$FW_KLIB" ]; then
    echo "kotlin/wasm: Compose framework"
    rm -rf "$FW_DIR.tmp" && mkdir -p "$FW_DIR.tmp"
    COMMON_FILES="$(find "$COMPOSE_FW/src" -name '*.kt' -type f | LC_ALL=C sort | tr '\n' ',')"
    COMMON_FILES="${COMMON_FILES%,}"
    FW_SRCS=()
    while IFS= read -r f; do FW_SRCS+=("$f"); done < <(find "$COMPOSE_FW/src" "$COMPOSE_FW/wasm" -name '*.kt' -type f | LC_ALL=C sort)
    run "$KOTLINC/bin/kotlinc-wasm" -Xwasm-target=wasm-js -Xmulti-platform -Xcommon-sources="$COMMON_FILES" \
      -Xplugin="$PLUGIN" -nowarn -Xsuppress-version-warnings \
      -libraries "$LIBS" -ir-output-dir "$FW_DIR.tmp" -ir-output-name onesim-compose -Xir-module-name=onesim-compose \
      "${FW_SRCS[@]}"
    rm -rf "$FW_DIR" && mv "$FW_DIR.tmp" "$FW_DIR"
  fi

  # 4. App sources + compile-time generated entry point (no JVM reflection on Wasm).
  if [ -d "$APP" ]; then
    SOURCES=()
    while IFS= read -r f; do SOURCES+=("$f"); done < <(find "$APP" -name '*.kt' -type f -not -path '*/build/*' | LC_ALL=C sort)
  else
    SOURCES=("$APP")
  fi
  [ ${#SOURCES[@]} -gt 0 ] || die "no .kt files in $APP"

  pkg_of() { sed -n 's/^[[:space:]]*package[[:space:]]\{1,\}\([A-Za-z0-9_.]*\).*/\1/p' "$1" | head -1; }

  GEN="$CACHE/gen-$$"; mkdir -p "$GEN"
  if [ -n "$ROOT_VIEW" ]; then
    FILE="$(grep -l -E "fun[[:space:]]+$ROOT_VIEW[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
    [ -n "$FILE" ] || die "no fun $ROOT_VIEW() in $APP"
    PKG="$(pkg_of "$FILE")"
    IMPORT="import androidx.activity.compose.setContent"; [ -n "$PKG" ] && IMPORT="$IMPORT"$'\n'"import $PKG.$ROOT_VIEW"
    ACTIVITY_EXPR="OneSimEntryActivity()"
    ACTIVITY_DECL="$(cat <<KT
class OneSimEntryActivity : androidx.activity.ComponentActivity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { $ROOT_VIEW() }
    }
}
KT
)"
  else
    LINE="$(grep -h -E "class[[:space:]]+[A-Za-z0-9_]+[[:space:]]*:[[:space:]]*ComponentActivity[[:space:]]*\(" "${SOURCES[@]}" | head -1 || true)"
    [ -n "$LINE" ] || die "no ComponentActivity subclass in $APP (pass a root composable as second argument)"
    NAME="$(echo "$LINE" | sed -E 's/.*class[[:space:]]+([A-Za-z0-9_]+).*/\1/')"
    FILE="$(grep -l -E "class[[:space:]]+$NAME[[:space:]]*:" "${SOURCES[@]}" | head -1)"
    PKG="$(pkg_of "$FILE")"
    IMPORT=""; [ -n "$PKG" ] && IMPORT="import $PKG.$NAME"
    ACTIVITY_EXPR="$NAME()"
    ACTIVITY_DECL=""
  fi

  source "$COMPOSE_FW/scripts/android-resources.sh"
  emit_android_resources "$APP" "$PKG" "$GEN/OneSimResources.kt"
  if [ -f "$GEN/OneSimResources.kt" ]; then SOURCES+=("$GEN/OneSimResources.kt"); fi

  cat > "$GEN/OneSimWebEntry.kt" <<KT
package onesim.entry

import onesim.compose.WasmGuest
import kotlin.js.ExperimentalJsExport
import kotlin.js.JsAny
import kotlin.js.JsExport
$IMPORT

$ACTIVITY_DECL

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimPlatform(): String = "android"

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimStart(dark: Boolean, insetTop: Float, insetBottom: Float, density: Float, host: JsAny): String =
    WasmGuest.start(dark, insetTop, insetBottom, density, host) { $ACTIVITY_EXPR }

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimAction(id: Double): String = WasmGuest.onAction(id.toLong())

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimTextInput(field: Double, text: String): String = WasmGuest.onTextInput(field.toLong(), text)

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimTextClick(id: Double, offset: Int): String = WasmGuest.onTextClick(id.toLong(), offset)

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimTextLayouts(message: String): String = WasmGuest.onTextLayouts(message)

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimEnvironment(dark: Boolean, insetTop: Float, insetBottom: Float): String =
    WasmGuest.onEnvironment(dark, insetTop, insetBottom)

@OptIn(ExperimentalJsExport::class)
@JsExport
fun onesimVersion(): Double = WasmGuest.version()
KT
  SOURCES+=("$GEN/OneSimWebEntry.kt")

  echo "kotlin/wasm: app + link"
  ALL_LIBS="$LIBS:$FW_KLIB"
  run "$KOTLINC/bin/kotlinc-wasm" -Xwasm-target=wasm-js -Xplugin="$PLUGIN" -nowarn -Xsuppress-version-warnings \
    -libraries "$ALL_LIBS" -ir-output-dir "$GEN" -ir-output-name guest -Xir-module-name=guest \
    "${SOURCES[@]}"

  rm -rf "$OUT/guest" && mkdir -p "$OUT/guest"
  run "$KOTLINC/bin/kotlinc-wasm" -Xwasm-target=wasm-js -Xir-produce-js -Xir-dce -nowarn -Xsuppress-version-warnings \
    -libraries "$ALL_LIBS" -Xinclude="$GEN/guest.klib" \
    -ir-output-dir "$OUT/guest" -ir-output-name guest -main=noCall

  cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/onesim.js" "$ROOT/web/renderer.js" "$OUT/"
  echo "built $OUT ($(wc -c < "$OUT/app.wasm" | tr -d ' ') bytes app.wasm, $(wc -c < "$OUT/guest/guest.wasm" | tr -d ' ') bytes guest.wasm, $(wc -c < "$OUT/gpu/onesim_web_gpu_bg.wasm" | tr -d ' ') bytes gpu)"
  exit 0
fi

# ---- SwiftUI web build -------------------------------------------------------------------
FW="$ROOT/frameworks/swiftui/Sources"

# 2. A swift.org toolchain whose version exactly matches an installed Wasm Swift SDK.
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

SW=(-swift-version 5 -target wasm32-unknown-wasip1 -sdk "$SDK/WASI.sdk"
    -resource-dir "$SDK/swift.xctoolchain/usr/lib/swift_static" -static-stdlib)

# 3. Rust renderer + in-process runtime (staticlib); bundled Inter + Phosphor only. Speed-optimized
#    with Wasm SIMD: a 3x frame drops from ~13 ms (opt-level s, scalar) to ~5 ms for +2.5% size.
#    simd128 needs Chrome 91 / Firefox 89 / Safari 16.4.
echo "rust: onesim-web ($RUST_TARGET)"
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  CARGO_PROFILE_RELEASE_PANIC=abort CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+simd128" \
  run cargo build -p onesim-web --release --target "$RUST_TARGET" --manifest-path "$ROOT/Cargo.toml"
LIB="$TARGET/$RUST_TARGET/release/libonesim_web.a"

build_gpu

# 4. SwiftUI module for Wasm (no Foundation).
if [ ! -f "$CACHE/SwiftUI.o" ] || [ -n "$(find "$FW" "$ROOT/crates/onesim-runtime/include" -type f -newer "$CACHE/SwiftUI.o" | head -1)" ]; then
  echo "swift: SwiftUI module"
  run "$SWIFTC" "${SW[@]}" -parse-as-library -module-name SwiftUI -Osize -wmo \
    -emit-module -emit-module-path "$CACHE/SwiftUI.swiftmodule" -emit-object -o "$CACHE/SwiftUI.o" \
    -I "$FW/OneSimBridge" "$FW"/SwiftUI/*.swift
fi

# 5. App sources (same @main / root-view rules as build-swift-app.sh).
if [ -d "$APP" ]; then SRCS=("$APP"/*.swift); else SRCS=("$APP"); fi
[ -e "${SRCS[0]}" ] || die "no .swift sources in $APP"
EXTRA=(); PAL=()
if ! grep -qs '^[[:space:]]*@main' "${SRCS[@]}" && ! printf '%s\n' "${SRCS[@]}" | grep -q '/main\.swift$'; then
  [ -n "$ROOT_VIEW" ] || die "no @main in sources and no root_view given"
  GEN="$CACHE/gen-$$"; mkdir -p "$GEN"
  printf 'import SwiftUI\n_onesimRun(%s())\n' "$ROOT_VIEW" > "$GEN/main.swift"
  EXTRA=("$GEN/main.swift")
elif grep -qs '^[[:space:]]*@main' "${SRCS[@]}"; then PAL=(-parse-as-library); fi

# 6. Link ONE reactor module: Swift (main = register app, returns) + Rust (exports for JS).
echo "swift: app + link"
EXPORTS=(simcore_web_init simcore_web_set_ios simcore_web_set_dark simcore_web_set_dynamic_type simcore_web_set_viewport simcore_web_set_scale
         simcore_web_pointer simcore_web_scroll simcore_web_drag simcore_web_tick simcore_web_key simcore_web_focused simcore_web_devices simcore_web_set_device simcore_web_buf simcore_web_buf_len
         simcore_web_inspect simcore_web_find simcore_web_fb_ptr simcore_web_fb_len simcore_web_fb_width
         simcore_web_fb_height simcore_web_tree_ptr simcore_web_tree_len simcore_web_set_output simcore_web_frame_version simcore_web_dl_ptr simcore_web_dl_len)
XL=(); for e in "${EXPORTS[@]}"; do XL+=(-Xlinker "--export=$e"); done
run "$SWIFTC" "${SW[@]}" -Osize ${PAL[@]+"${PAL[@]}"} -module-name OneSimApp -I "$CACHE" -I "$FW/OneSimBridge" \
  "${SRCS[@]}" ${EXTRA[@]+"${EXTRA[@]}"} "$CACHE/SwiftUI.o" "$LIB" \
  -Xclang-linker -mexec-model=reactor \
  -Xclang-linker "-resource-dir=$SDK/swift.xctoolchain/usr/lib/swift_static/clang" \
  -Xlinker -z -Xlinker stack-size=8388608 -Xlinker --gc-sections -Xlinker --strip-all \
  ${XL[@]+"${XL[@]}"} -o "$OUT/app.wasm"

optimize_wasm "$OUT/app.wasm" "$OUT/app.wasm"
rm -rf "$OUT/guest"
cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/onesim.js" "$ROOT/web/renderer.js" "$OUT/"
echo "built $OUT ($(wc -c < "$OUT/app.wasm" | tr -d ' ') bytes app.wasm, $(wc -c < "$OUT/gpu/onesim_web_gpu_bg.wasm" | tr -d ' ') bytes gpu)"
