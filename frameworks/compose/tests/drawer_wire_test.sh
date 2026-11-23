#!/usr/bin/env bash
# Builds DrawerLab and drives Material 3 navigation drawers over the guest wire (DrawerState, drags, scrim, system
# back). Exit 0 = pass.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
"$ROOT/frameworks/compose/scripts/build-app.sh" "$HERE/DrawerLab.kt" "$OUT/lab/app" DrawerLab
python3 -I "$HERE/drawer_wire_test.py" "$OUT/lab/app"
