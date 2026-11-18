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
for exe in counter entry showcase dataflow presentation; do
  if otool -L "$OUT/$exe" | grep -q 'SwiftUI.framework'; then echo "FAIL: $exe links Apple SwiftUI" >&2; exit 1; fi
done
/usr/bin/python3 -I "$(dirname "$0")/wire_test.py" "$OUT/counter" "$OUT/entry" "$OUT/showcase" "$OUT/dataflow" "$OUT/presentation"
