#!/usr/bin/env bash
# Builds the examples and drives them over guest wire B. Exit 0 = pass.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-counter/CounterView.swift" "$OUT/counter" CounterView
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-app-entry" "$OUT/entry"
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-showcase" "$OUT/showcase"
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-dataflow" "$OUT/dataflow"
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-presentation" "$OUT/presentation"
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-tabs" "$OUT/tabs"
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/TabsModern.swift" "$OUT/tabsmodern" TabsModern
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/GestureLab.swift" "$OUT/gesturelab" GestureLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/ShapeLab.swift" "$OUT/shapelab" ShapeLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/LayoutLab.swift" "$OUT/layoutlab" LayoutLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/StateLab.swift" "$OUT/statelab" StateLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/TypeLab.swift" "$OUT/typelab"
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/AnimationLab.swift" "$OUT/animlab" AnimationLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/ControlLab.swift" "$OUT/controllab" ControlLab
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-pages" "$OUT/pages"
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/EffectsLab.swift" "$OUT/effectslab" EffectsLab
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/TextInputLab.swift" "$OUT/textinputlab" TextInputLab
"$ROOT/scripts/build-swift-app.sh" "$ROOT/examples/swiftui-pickers" "$OUT/pickers"
"$ROOT/scripts/build-swift-app.sh" "$(dirname "$0")/PickerLab.swift" "$OUT/pickerlab" PickerLab
for exe in counter entry showcase dataflow presentation tabs tabsmodern gesturelab layoutlab shapelab typelab animlab statelab controllab pages effectslab textinputlab pickers pickerlab; do
  if otool -L "$OUT/$exe" | grep -q 'SwiftUI.framework'; then echo "FAIL: $exe links Apple SwiftUI" >&2; exit 1; fi
done
/usr/bin/python3 -I "$(dirname "$0")/wire_test.py" "$OUT/counter" "$OUT/entry" "$OUT/showcase" "$OUT/dataflow" "$OUT/presentation" "$OUT/tabs" "$OUT/tabsmodern" "$OUT/gesturelab" "$OUT/layoutlab" "$OUT/shapelab" "$OUT/typelab" "$OUT/animlab" "$OUT/statelab" "$OUT/controllab" "$OUT/pages" "$OUT/effectslab" "$OUT/textinputlab" "$OUT/pickers" "$OUT/pickerlab"
