#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
guest="$tmp/guest"
if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then guest="$guest.cmd"; fi
frameworks/compose/scripts/build-app.sh frameworks/compose/tests/TextWireTest.kt "$guest" TestText
python3 - "$guest" <<'PY'
import json
import subprocess
import sys

messages = [
    {"type": "text_layout", "version": 1, "layouts": [{"id": 1, "geometry": "80,18,1;0;0,5,5,0,60,0,18,14,0,0,1,0,10,0;w,0,1,3,5;0"}]},
    {"type": "text_click", "id": 1, "version": 0, "offset": 3},
    {"type": "text_click", "id": 1, "version": 3, "offset": 4},
    {"type": "shutdown"},
]
guest = subprocess.run([sys.argv[1]], input="".join(json.dumps(m) + "\n" for m in messages),
                       text=True, capture_output=True, check=True, timeout=30)
trees = [m for m in map(json.loads, guest.stdout.splitlines()) if m["type"] == "tree"]
assert [m["version"] for m in trees] == [1, 2, 3, 4], (guest.stdout, guest.stderr)

def nodes(root):
    yield root
    for child in root.get("children", []):
        yield from nodes(child)

texts = [[n["props"] for n in nodes(m["root"]) if n["component"] == "Text"] for m in trees]
first, status, layout = texts[0]
assert first["text"] == "A😀Go" and first["textClick"] == 1, first
assert first["textLayout"] == 1 and layout["text"] == "Layout lines: 0", texts[0]
assert first["spans"] == [{"start": 3, "end": 5, "color": [0, 0, 1, 1]}], first
assert "selectionColor" in first and "selectionColor" not in status, (first, status)
assert texts[1][2]["text"] == "Layout lines: 1", texts[1]
assert texts[2][1]["text"] == "offset=-1 route=null", texts[2]
assert texts[3][1]["text"] == "offset=4 route=details", texts[3]
print("Compose text wire test passed")
PY
