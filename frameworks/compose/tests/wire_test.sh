#!/usr/bin/env bash
# Builds the test labs and the counter example and drives them over guest wire B/L/X: pointer input, navigation and
# system back, app bars, navigation bars and rails, drawers. Exit 0 = pass.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
build() { "$ROOT/frameworks/compose/scripts/build-app.sh" "$@"; }
build "$HERE/GestureLab.kt" "$OUT/lab/app" GestureLab
build "$ROOT/examples/compose-counter" "$OUT/counter/app"
python3 -I "$HERE/wire_test.py" "$OUT/lab/app" "$OUT/counter/app"
build "$HERE/NavLab.kt" "$OUT/nav/app" NavLab
python3 -I "$HERE/nav_wire_test.py" "$OUT/nav/app"
build "$HERE/AppBarLab.kt" "$OUT/appbar/app" AppBarLab
python3 -I "$HERE/appbar_wire_test.py" "$OUT/appbar/app"
build "$HERE/NavBarLab.kt" "$OUT/navbar/app" NavBarLab
python3 -I "$HERE/navbar_wire_test.py" "$OUT/navbar/app"
build "$HERE/DrawerLab.kt" "$OUT/drawer/app" DrawerLab
python3 -I "$HERE/drawer_wire_test.py" "$OUT/drawer/app"
