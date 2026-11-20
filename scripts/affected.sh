#!/usr/bin/env bash
# Work out which gates a change needs, from the files it touches. CI and local runs share this mapping.
#
#   scripts/affected.sh                 print the affected groups (diff against the merge base with origin/main)
#   scripts/affected.sh --run           run the gates for those groups
#   scripts/affected.sh --all [--run]   every group, as on main
#   scripts/affected.sh --base <ref>    diff against another ref
#   scripts/affected.sh --paths         read the changed paths from stdin instead of git (to test the mapping)
#   scripts/affected.sh --github-output write group=true|false lines to $GITHUB_OUTPUT (CI)
#
# Groups: rust, swiftui, compose, expo, web. The api-coverage check always runs.
# Shared code (crates, protocol, fixtures tooling, CI itself) selects every group, and so does any path
# the table below does not know: an unmapped file runs everything rather than nothing.
set -euo pipefail
cd "$(dirname "$0")/.."

GROUPS_ALL="rust swiftui compose expo web"

# First match wins. Groups "all" (every group) and "none" (no gate beyond api-coverage).
# Globs are Python fnmatch: `*` also matches `/`.
RULES='
all      crates/*
all      Cargo.toml
all      Cargo.lock
all      assets/*
all      docs/protocol.md
all      docs/compatibility/versions.json
all      scripts/compare_ios.py
all      scripts/compare-ios.sh
all      scripts/affected.sh
all      .github/*
swiftui  frameworks/swiftui/*
swiftui  examples/swiftui-*
swiftui  scripts/build-swift-app.sh
swiftui  scripts/build-ios-sim.sh
swiftui  tests/fixtures/showcase-ios/*
swiftui  tests/fixtures/swiftui-tabs-ios/*
swiftui  tests/fixtures/swiftui-gestures-ios/*
swiftui  tests/fixtures/devices/*
swiftui  tests/fixtures/ios27/showcase-ios/*
swiftui  tests/fixtures/ios27/swiftui-tabs-ios/*
swiftui  tests/fixtures/ios27/swiftui-gestures-ios/*
swiftui  tests/fixtures/ios27/devices/*
compose  frameworks/compose/*
compose  examples/compose-*
compose  tests/fixtures/compose-android/*
expo     frameworks/react-native/*
expo     examples/expo-*
expo     scripts/compare-expo-*
expo     tests/fixtures/rn-*
expo     tests/fixtures/expo-*
expo     tests/fixtures/ios27/rn-*
expo     tests/fixtures/ios27/expo-*
web      web/*
web      scripts/build-web.sh
web      scripts/compare-web.sh
none     docs/*
none     *.md
none     LICENSE
none     sdk/*
none     examples/demokit-integration/*
none     scripts/api-coverage.mjs
none     scripts/*-inventory.mjs
'

base=origin/main all=0 run=0 gh=0 stdin=0
while [ $# -gt 0 ]; do
  case $1 in
    --all) all=1 ;;
    --run) run=1 ;;
    --github-output) gh=1 ;;
    --paths) stdin=1 ;;
    --base) base=$2; shift ;;
    *) echo "usage: $0 [--all] [--run] [--base <ref>] [--paths] [--github-output]" >&2; exit 2 ;;
  esac
  shift
done

if [ $all = 1 ]; then
  groups=$GROUPS_ALL
else
  changed() {
    if [ $stdin = 1 ]; then cat; return; fi
    mb=$(git merge-base "$base" HEAD) || { echo "error: no merge base with $base (fetch it, or pass --all)" >&2; exit 2; }
    # Committed changes since the merge base, plus uncommitted and untracked work for local runs.
    git diff --name-only "$mb"; git ls-files --others --exclude-standard
  }
  groups=$(changed | sort -u \
    | RULES=$RULES ALL=$GROUPS_ALL python3 -c '
import fnmatch, os, sys
rules = [l.split() for l in os.environ["RULES"].strip().splitlines()]
hit = set()
for path in filter(None, sys.stdin.read().splitlines()):
    group = next((g for g, glob in rules if fnmatch.fnmatchcase(path, glob)), "all")
    if group != "none":
        print(f"{path} -> {group}", file=sys.stderr)
    hit |= set(os.environ["ALL"].split()) if group == "all" else {group} - {"none"}
# The web build compiles the SwiftUI framework to Wasm, so SwiftUI changes also run the web gates.
if "swiftui" in hit:
    hit.add("web")
# Every framework runs on the Rust host, so any of them needs the Rust build and tests.
if hit:
    hit.add("rust")
print(" ".join(g for g in os.environ["ALL"].split() if g in hit))
')
fi

echo "affected: ${groups:-none (api-coverage only)}" >&2
has() { [[ " $groups " == *" $1 "* ]]; }

if [ $gh = 1 ]; then
  for g in $GROUPS_ALL; do
    if has "$g"; then echo "$g=true"; else echo "$g=false"; fi
  done >>"${GITHUB_OUTPUT:?--github-output needs GITHUB_OUTPUT}"
fi

[ $run = 1 ] || exit 0

step() { echo "==> $*" >&2; "$@"; }
step node scripts/api-coverage.mjs --check
if has rust; then
  step cargo build --workspace
  SIMCORE_SKIP_GPU_TESTS=1 step cargo test --workspace
  step cargo clippy --workspace --all-targets -- -D warnings
fi
if has swiftui; then
  step frameworks/swiftui/Tests/wire_test.sh
  SIMCORE_ONLY=swiftui step scripts/compare-ios.sh
  SIMCORE_ONLY=swiftui SIMCORE_FALLBACK=1 step scripts/compare-ios.sh
fi
if has expo; then
  for d in expo-showcase expo-router-stack expo-default expo-ui; do
    [ -d "examples/$d/node_modules" ] || { echo "error: run npm ci --prefix examples/$d first (the Expo gates skip apps without it)" >&2; exit 1; }
  done
  SIMCORE_ONLY=expo step scripts/compare-ios.sh
  SIMCORE_ONLY=expo SIMCORE_FALLBACK=1 step scripts/compare-ios.sh
  step scripts/compare-expo-android.sh
fi
if has compose; then
  step scripts/compare-android.sh
fi
if has web; then
  # Needs the swift.org toolchain with the Wasm SDK (see the web job in .github/workflows/ci.yml).
  step scripts/build-web.sh examples/swiftui-showcase
  step node web/test/run.mjs --suite showcase
  step scripts/compare-web.sh
fi
