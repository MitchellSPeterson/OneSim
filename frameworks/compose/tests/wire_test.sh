#!/usr/bin/env bash
# Builds GestureLab and the counter example and drives them over guest wire B/L (pointer input). Exit 0 = pass.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
"$ROOT/frameworks/compose/scripts/build-app.sh" "$HERE/GestureLab.kt" "$OUT/lab/app" GestureLab
"$ROOT/frameworks/compose/scripts/build-app.sh" "$ROOT/examples/compose-counter" "$OUT/counter/app"
python3 -I "$HERE/wire_test.py" "$OUT/lab/app" "$OUT/counter/app"
