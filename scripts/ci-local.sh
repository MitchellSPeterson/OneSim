#!/usr/bin/env bash
# Run the CI checks. Local runs and the GitHub jobs share both the selection (scripts/ci_select.py) and the steps
# below: each CI job installs its toolchains, then calls `--check <name> --level <level>`. See docs/ci.md.
#
#   scripts/ci-local.sh --explain  [--level L] [--base REF]  what would run for this branch, and why
#   scripts/ci-local.sh --affected [--level L] [--base REF]  run the checks this branch needs (default level: premerge)
#   scripts/ci-local.sh --all                                every check at the full level (not portable: Linux/Windows)
#   scripts/ci-local.sh --check NAME [--level L]             one check's steps (rust, swiftui, compose, expo,
#                                                            web-swiftui, web-compose, web-expo)
#
# Levels: fast (each PR commit: compile, targeted tests, smoke states), premerge (before merge: full pixel-diff and
# web parity suites), full (nightly and releases). --base defaults to origin/main; local runs include uncommitted
# and untracked files. RUST_PACKAGES="a b" limits the rust check to those packages (empty: whole workspace).
set -euo pipefail
cd "$(dirname "$0")/.."

mode= level= base=origin/main check=
usage() { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
while [ $# -gt 0 ]; do
  case $1 in
    --explain) mode=explain ;;
    --affected) mode=affected ;;
    --all) mode=all ;;
    --check) mode=check; check=${2:?--check needs a name}; shift ;;
    --level) level=${2:?--level needs fast, premerge or full}; shift ;;
    --base) base=${2:?--base needs a ref}; shift ;;
    -h|--help) usage ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
  shift
done
[ -n "$mode" ] || usage

step() {
  [ -z "${GITHUB_ACTIONS:-}" ] || echo "::group::$*"
  echo "==> $*" >&2
  local rc=0
  "$@" || rc=$?
  [ -z "${GITHUB_ACTIONS:-}" ] || echo "::endgroup::"
  if [ $rc != 0 ]; then
    [ -z "${GITHUB_ACTIONS:-}" ] || echo "::error::failed ($rc): $*"
    echo "FAILED ($rc): $*" >&2
    return $rc
  fi
}
fast() { [ "$level" = fast ]; }
need_pillow() { python3 -c 'import PIL' 2>/dev/null || { echo "error: the pixel diffs need Pillow: python3 -m pip install pillow" >&2; exit 1; }; }
need_npm() { for d in "$@"; do [ -d "examples/$d/node_modules" ] || { echo "error: run npm ci --prefix examples/$d first" >&2; exit 1; }; done; }

# Assert on captured output: `cmd | grep -q` fails under pipefail when grep exits before cmd finishes writing.
expect() { # expect <text> <command...>: run it, fail unless its stdout contains <text>
  local want=$1 out; shift
  out=$("$@") || { printf '%s\n' "$out" >&2; return 1; }
  grep -qF -- "$want" <<<"$out" || { printf '%s\n' "$out" >&2; echo "expected $want" >&2; return 1; }
}
swiftui_counter() {
  expect '"Count: 1"' cargo run -q -p onesim-cli -- run examples/swiftui-counter --root-view CounterView \
    --click Increment --inspect
}
compose_counter() {
  expect '"Count: 1"' cargo run -q -p onesim-cli -- run examples/compose-counter \
    --click Increment --click Increment --click Decrement --inspect
}
fake_guest_navigation() {
  cargo build -q -p onesim-host --bin onesim-fake-guest
  mkdir -p target/showcase-render
  ln -sf "$PWD/target/debug/onesim-fake-guest" target/debug/fake-nav-guest
  expect '"Buy mil"' cargo run -q -p onesim-cli -- run target/debug/fake-nav-guest \
    --click Todos --click "New todo" --type "Buy milk" --key backspace --click Add --inspect \
    --screenshot target/showcase-render/ci-todos.png
}

check_rust() {
  local sel=(--workspace) p
  if [ -n "${RUST_PACKAGES:-}" ]; then sel=(); for p in $RUST_PACKAGES; do sel+=(-p "$p"); done; fi
  step cargo build "${sel[@]}"
  SIMCORE_SKIP_GPU_TESTS=1 step cargo test "${sel[@]}"
  step cargo clippy "${sel[@]}" --all-targets -- -D warnings
  # The renderer, host and CLI end to end: the windowed build, the showcase PNGs and the fake guest.
  if [ -z "${RUST_PACKAGES:-}" ] || [[ " $RUST_PACKAGES " == *" onesim-cli "* ]]; then
    step cargo build -p onesim-render --examples --features window
    step cargo clippy -p onesim-cli --features window --all-targets -- -D warnings
    step cargo run -q -p onesim-render --example showcase -- --out target/showcase-render
    step fake_guest_navigation
  fi
}

check_swiftui() {
  step frameworks/swiftui/Tests/wire_test.sh
  step swiftui_counter
  step python3 scripts/test-animation-behavior.py
  need_pillow
  step python3 -B scripts/test_compare_animation.py
  if fast; then
    SIMCORE_ONLY=swiftui SIMCORE_SMOKE=1 step scripts/compare-ios.sh
  else
    SIMCORE_ONLY=swiftui step scripts/compare-ios.sh
    SIMCORE_ONLY=swiftui SIMCORE_FALLBACK=1 step scripts/compare-ios.sh
  fi
}

check_compose() {
  step compose_counter
  if ! fast; then
    need_pillow
    step scripts/compare-android.sh
  fi
}

check_expo() {
  need_pillow
  if fast; then
    need_npm expo-showcase
    SIMCORE_ONLY=expo SIMCORE_SMOKE=1 step scripts/compare-ios.sh
  else
    need_npm expo-showcase expo-router-stack expo-default expo-ui expo-apis
    step cargo build -q -p onesim-cli # run-apis answers the guest's host requests with `onesim guest-request`
    step node frameworks/react-native/guest/run-apis.mjs
    SIMCORE_ONLY=expo step scripts/compare-ios.sh
    SIMCORE_ONLY=expo SIMCORE_FALLBACK=1 step scripts/compare-ios.sh
    step scripts/compare-expo-android.sh
  fi
}

# The web checks need the Rust Wasm targets and wasm-bindgen-cli; SwiftUI also needs the swift.org toolchain with
# its Wasm SDK, Compose needs Java 17+, and the presenter and parity steps need Chrome (CHROME=). See docs/web.md.
check_web_swiftui() {
  step cargo clippy -p onesim-web-gpu --target wasm32-unknown-unknown -- -D warnings
  step scripts/build-web.sh examples/swiftui-counter CounterView
  step node web/test/run.mjs --suite counter
  if ! fast; then
    step scripts/build-web.sh examples/swiftui-showcase
    step node web/test/run.mjs --suite showcase
    step node web/test/presenter.mjs --app
    need_pillow
    step scripts/compare-web.sh
  fi
}

check_web_compose() {
  step scripts/build-web.sh examples/compose-counter
  step node web/test/run.mjs --suite compose-counter
  if ! fast; then
    step scripts/build-web.sh examples/compose-showcase
    step node web/test/run.mjs --suite compose-showcase
    step scripts/build-web.sh examples/compose-input
    step node web/test/run.mjs --suite compose-input
    step node web/test/presenter.mjs --compose
    need_pillow
    step scripts/compare-web-compose.sh
  fi
}

check_web_expo() {
  step cargo clippy -p onesim-web --features js-guest --target wasm32-wasip1 -- -D warnings
  step scripts/build-web.sh --expo
  if ! fast; then
    need_pillow
    need_npm expo-showcase
    step scripts/compare-web-expo.sh
  fi
}

run_check() {
  case $1 in
    rust|swiftui|compose|expo|web-swiftui|web-compose|web-expo) "check_${1//-/_}" ;;
    portable) echo "portable runs on the Linux and Windows CI runners only (.github/workflows/checks.yml)" >&2 ;;
    *) echo "unknown check: $1" >&2; exit 2 ;;
  esac
}

case $mode in
  check)
    level=${level:-premerge}
    run_check "$check"
    ;;
  explain)
    python3 scripts/ci_select.py explain --level "${level:-premerge}" --base "$base"
    ;;
  affected|all)
    [ $mode = all ] && level=full
    level=${level:-premerge}
    python3 scripts/ci_select.py explain --level "$level" --base "$base"
    plan=$(python3 scripts/ci_select.py plan --level "$level" --base "$base")
    # The plan job's checks, then every selected check.
    step node scripts/api-coverage.mjs --check
    step python3 -B scripts/test_ci_select.py
    export RUST_PACKAGES
    RUST_PACKAGES=$(python3 -c 'import json,sys; print(" ".join(json.load(sys.stdin)["rust_packages"] or []))' <<<"$plan")
    for c in $(python3 -c 'import json,sys; print(" ".join(c for c, on in json.load(sys.stdin)["checks"].items() if on))' <<<"$plan"); do
      [ "$c" = portable ] && { echo "skipping portable (Linux/Windows CI only)" >&2; continue; }
      echo "==== $c ($level)" >&2
      run_check "$c"
    done
    echo "all selected checks passed ($level)" >&2
    ;;
esac
