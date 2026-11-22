#!/usr/bin/env python3
"""End-to-end deterministic animation & interactive behavior tests for real SwiftUI apps.

Builds examples/swiftui-{animation,gestures,showcase,tabs} against OneSim's SwiftUI module
and drives them over `onesim serve` (v1 JSON-lines with `tick`, `wait`, `send_drag_phase`,
`send_pointer_event`, `inspect_ui_tree`). No simulator required; runs in CI `check_swiftui`.
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ONESIM = ROOT / "target/debug/onesim"


class OneSimClient:
    def __init__(self):
        self.proc = subprocess.Popen(
            [str(ONESIM), "serve"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.seq = 0
        self.call("hello", {"protocol_version": 1})

    def call(self, method, params=None):
        self.seq += 1
        req = {"id": self.seq, "method": method}
        if params is not None:
            req["params"] = params
        assert self.proc.stdin and self.proc.stdout
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            err = self.proc.stderr.read() if self.proc.stderr else ""
            raise RuntimeError(f"onesim serve exited unexpectedly on {method}: {err}")
        resp = json.loads(line)
        if not resp.get("ok"):
            raise RuntimeError(f"{method} failed: {resp.get('error')}")
        return resp["result"]

    def close(self):
        try:
            self.call("shutdown")
        except Exception:
            pass
        self.proc.wait(timeout=5)


def walk(node):
    yield node
    for c in node.get("children", []):
        yield from walk(c)


def find_node(root, pred):
    for n in walk(root):
        if pred(n):
            return n
    return None


def find_all(root, pred):
    return [n for n in walk(root) if pred(n)]


def center(node):
    f = node["frame"]
    return f["x"] + f["width"] / 2.0, f["y"] + f["height"] / 2.0


def click_label(cli, sid, label):
    tree = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    n = find_node(tree, lambda x: x.get("label") == label or x.get("text") == label)
    assert n is not None, f"node with label/text {label!r} not found"
    x, y = center(n)
    return cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": x, "y": y}})


def start_app(cli, app_path):
    sid = cli.call("create_session", {"framework": "swiftui", "app_path": str(app_path), "prebuilt": True})["session_id"]
    cli.call("start_session", {"session_id": sid})
    return sid


def test_swiftui_animation(cli, app_path):
    print("  [1/4] swiftui-animation: matchedGeometryEffect, transition, contentTransition, interruption")
    sid = start_app(cli, app_path)

    # 1. matchedGeometryEffect: Alpha -> Beta slides smoothly across ticks, not single-frame snap.
    tree0 = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    alpha = find_node(tree0, lambda n: n.get("text") == "Alpha")
    beta = find_node(tree0, lambda n: n.get("text") == "Beta")
    gamma = find_node(tree0, lambda n: n.get("text") == "Gamma")
    assert alpha and beta and gamma
    # Find the blue capsule (shape behind Alpha).
    pill_before = find_node(tree0, lambda n: n.get("kind") == "shape" and abs(n["frame"]["x"] - alpha["frame"]["x"]) < 25 and abs(n["frame"]["y"] - alpha["frame"]["y"]) < 20)
    assert pill_before is not None, "initial matchedGeometryEffect pill not found"
    x_alpha = pill_before["frame"]["x"]

    click_label(cli, sid, "Beta")
    xs = []
    for _ in range(25):
        r = cli.call("tick", {"session_id": sid, "dt_ms": 16.0})
        tree = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
        # Find pill(s) near the tab bar y
        pills = find_all(tree, lambda n: n.get("kind") == "shape" and abs(n["frame"]["y"] - pill_before["frame"]["y"]) < 10 and n["frame"]["height"] < 45)
        if pills:
            xs.append(pills[-1]["frame"]["x"])
        if not r["animating"]:
            break
    assert len(xs) >= 5, f"expected smooth multi-frame pill slide, got {len(xs)} frames"
    x_beta = xs[-1]
    assert x_beta > x_alpha + 40.0, f"pill should move right from Alpha ({x_alpha:.1f}) to Beta ({x_beta:.1f})"
    assert any(x_alpha + 10.0 < x < x_beta - 10.0 for x in xs), f"pill must pass through intermediate positions: {xs[:10]}"
    # Regression test (discovered via iOS 27.2 filmstrip diff): matchedGeometryEffect pill must stay
    # opaque mid-flight rather than dipping to 75% alpha from double-crossfade.
    click_label(cli, sid, "Alpha")
    cli.call("tick", {"session_id": sid, "dt_ms": 112.0})
    tree_mg = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    pills_mg = find_all(tree_mg, lambda n: n.get("kind") == "shape" and abs(n["frame"]["y"] - pill_before["frame"]["y"]) < 10 and n["frame"]["height"] < 45)
    assert pills_mg and pills_mg[-1].get("opacity", 1.0) >= 0.99, f"matchedGeometryEffect pill must stay opaque mid-flight: {pills_mg}"
    cli.call("wait", {"session_id": sid, "ms": 600.0})
    click_label(cli, sid, "Beta")
    cli.call("wait", {"session_id": sid, "ms": 600.0})

    # Interruption mid-flight: Beta -> Gamma, then at t=112ms redirect to Alpha without teleporting.
    click_label(cli, sid, "Gamma")
    cli.call("wait", {"session_id": sid, "ms": 112.0})
    click_label(cli, sid, "Alpha")
    cli.call("wait", {"session_id": sid, "ms": 700.0})
    tree_settled = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    pills_end = find_all(tree_settled, lambda n: n.get("kind") == "shape" and abs(n["frame"]["y"] - pill_before["frame"]["y"]) < 10 and n["frame"]["height"] < 45)
    assert pills_end and abs(pills_end[-1]["frame"]["x"] - x_alpha) < 1.5, f"interrupted pill must settle back at Alpha ({x_alpha:.1f}), got {pills_end}"

    # 2. Asymmetric .transition(.move(edge: .bottom).combined(with: .opacity)) on Toggle Card.
    click_label(cli, sid, "Toggle Card")
    cli.call("tick", {"session_id": sid, "dt_ms": 80.0})
    tree_mid = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    card_mid = find_node(tree_mid, lambda n: n.get("text") == "Animated Card")
    assert card_mid is not None, "Animated Card should exist mid-insertion"
    mid_y = card_mid["frame"]["y"]
    mid_op = card_mid.get("opacity", 1.0)
    assert mid_op < 0.95, f"insertion transition should fade in mid-flight, got opacity={mid_op}"

    cli.call("wait", {"session_id": sid, "ms": 400.0})
    tree_end = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    card_end = find_node(tree_end, lambda n: n.get("text") == "Animated Card")
    assert card_end is not None
    # Regression test (discovered via iOS 27.2 filmstrip diff): .move(edge: .bottom) shifts by the card's
    # own height (~56pt), not the full 874pt screen height.
    dy = mid_y - card_end["frame"]["y"]
    assert 15.0 < dy < 80.0, f"card should enter from ~1 card-height below (not full screen): mid_y={mid_y:.1f}, end_y={card_end['frame']['y']:.1f}, dy={dy:.1f}"
    assert card_end.get("opacity", 1.0) >= 0.99

    # Removal transition settles cleanly.
    click_label(cli, sid, "Toggle Card")
    cli.call("wait", {"session_id": sid, "ms": 400.0})
    tree_gone = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_gone, lambda n: n.get("text") == "Animated Card") is None, "card should be gone after removal settles"

    # 3. .contentTransition(.numericText()) on Increment: incoming node rolls up from below & fades in,
    # while outgoing ghost stays hidden from UI tree inspection.
    click_label(cli, sid, "Increment")
    cli.call("tick", {"session_id": sid, "dt_ms": 60.0})
    tree_ct = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    cnt1_mid = find_node(tree_ct, lambda n: n.get("text") == "Count: 1")
    assert cnt1_mid is not None, "Count: 1 should exist mid-transition"
    assert cnt1_mid.get("opacity", 1.0) < 0.95, f"Count: 1 should fade in mid-transition, got opacity={cnt1_mid.get('opacity')}"
    cli.call("wait", {"session_id": sid, "ms": 400.0})
    tree_ct_end = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    cnt1_end = find_node(tree_ct_end, lambda n: n.get("text") == "Count: 1")
    assert cnt1_end is not None
    assert cnt1_mid["frame"]["y"] > cnt1_end["frame"]["y"] + 1.0, (
        f"numericText should roll up from below: mid_y={cnt1_mid['frame']['y']:.2f}, end_y={cnt1_end['frame']['y']:.2f}"
    )
    assert cnt1_end.get("opacity", 1.0) >= 0.99

    cli.call("stop_session", {"session_id": sid})


def test_swiftui_gestures(cli, app_path):
    print("  [2/4] swiftui-gestures: incremental drag (@GestureState live tracking) & double-tap")
    sid = start_app(cli, app_path)
    tree0 = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    drag_node = find_node(tree0, lambda n: n.get("text") == "Drag me")
    assert drag_node is not None
    x0, y0 = center(drag_node)

    cli.call("send_drag_phase", {"session_id": sid, "phase": "begin", "x": x0, "y": y0, "time_ms": 0.0})
    for step in range(1, 6):
        dx = step * 20.0
        cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": x0 + dx, "y": y0, "time_ms": step * 16.0})
        tree = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
        moved = find_node(tree, lambda n: n.get("text") == "Drag me")
        assert moved is not None
        mx, _ = center(moved)
        assert abs(mx - (x0 + dx)) < 2.0, f"step {step}: Drag me should track finger live ({x0 + dx:.1f}), got {mx:.1f}"
    cli.call("send_drag_phase", {"session_id": sid, "phase": "end", "x": x0 + 100.0, "y": y0, "time_ms": 96.0})
    tree_end = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_end, lambda n: n.get("text") == "Offset 100, 0") is not None, "committed drag offset missing"

    # Double tap
    click_label(cli, sid, "Double-tap")
    click_label(cli, sid, "Double-tap")
    tree_dt = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_dt, lambda n: n.get("text") == "Double taps: 1") is not None

    cli.call("stop_session", {"session_id": sid})


def test_swiftui_showcase(cli, app_path):
    print("  [3/4] swiftui-showcase: half-sheet drag snap-back vs dismiss, modal hit-testing, scroll momentum")
    sid = start_app(cli, app_path)

    # Navigate to Controls and open half sheet.
    click_label(cli, sid, "Controls")
    tree_ctrl = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    more_btn = find_node(tree_ctrl, lambda n: n.get("label") == "More" or n.get("text") == "More")
    assert more_btn is not None
    more_x, more_y = center(more_btn)

    click_label(cli, sid, "Show half sheet")
    tree_sh = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    open_another = find_node(tree_sh, lambda n: n.get("label") == "Open another" or n.get("text") == "Open another")
    assert open_another is not None, "half sheet should be presented"

    # Underlying More button is blocked while sheet is open.
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": more_x, "y": more_y}})
    tree_still = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_still, lambda n: n.get("text") == "Volume: 4") is None

    # Small drag down on sheet (40pt) -> snaps back, sheet stays open.
    grab_y = open_another["frame"]["y"] - 40.0
    cli.call("send_drag_phase", {"session_id": sid, "phase": "begin", "x": 200.0, "y": grab_y, "time_ms": 0.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": grab_y + 40.0, "time_ms": 32.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "end", "x": 200.0, "y": grab_y + 40.0, "time_ms": 200.0})
    tree_snap = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_snap, lambda n: n.get("label") == "Open another" or n.get("text") == "Open another") is not None, "small drag must snap back"

    # Large drag down (320pt) -> dismisses sheet.
    cli.call("send_drag_phase", {"session_id": sid, "phase": "begin", "x": 200.0, "y": grab_y, "time_ms": 300.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": grab_y + 160.0, "time_ms": 332.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": grab_y + 320.0, "time_ms": 364.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "end", "x": 200.0, "y": grab_y + 320.0, "time_ms": 396.0})
    tree_dismissed = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_dismissed, lambda n: n.get("label") == "Open another") is None, "large drag down must dismiss sheet"

    # Underlying More button is hittable again!
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": more_x, "y": more_y}})
    tree_after = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_after, lambda n: n.get("text") == "4") is not None, "underlying More button must work after sheet dismisses"

    # Navigate back and test Long list fling momentum.
    back = find_node(tree_after, lambda n: n.get("id", "").endswith("#back"))
    assert back is not None
    bx, by = center(back)
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": bx, "y": by}})
    click_label(cli, sid, "Long list")
    tree_ll = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    row1 = find_node(tree_ll, lambda n: n.get("text") == "Row 1")
    assert row1 is not None
    y_start = row1["frame"]["y"]

    cli.call("send_drag_phase", {"session_id": sid, "phase": "begin", "x": 200.0, "y": 550.0, "time_ms": 500.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": 490.0, "time_ms": 516.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": 430.0, "time_ms": 532.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "move", "x": 200.0, "y": 370.0, "time_ms": 548.0})
    cli.call("send_drag_phase", {"session_id": sid, "phase": "end", "x": 200.0, "y": 370.0, "time_ms": 556.0})
    y_rel = find_node(cli.call("inspect_ui_tree", {"session_id": sid})["root"], lambda n: n.get("text") == "Row 1")["frame"]["y"]
    assert y_rel < y_start - 100.0, f"drag should scroll Long list: {y_start:.1f} -> {y_rel:.1f}"

    coasted = False
    for _ in range(120):
        r = cli.call("tick", {"session_id": sid, "dt_ms": 16.0})
        y_now = find_node(cli.call("inspect_ui_tree", {"session_id": sid})["root"], lambda n: n.get("text") == "Row 1")["frame"]["y"]
        if y_now < y_rel - 10.0:
            coasted = True
        if not r["animating"]:
            break
    assert coasted, "Long list fling must coast with momentum after release"

    cli.call("stop_session", {"session_id": sid})


def test_swiftui_tabs(cli, app_path):
    print("  [4/4] swiftui-tabs: tab switching preserves state & rapid taps")
    sid = start_app(cli, app_path)
    # Tab items on iPhone 18 Pro: Home 115,822, Counter 201,822, Settings 287,822
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": 201.0, "y": 822.0}})
    for _ in range(3):
        click_label(cli, sid, "Increment")
    tree_c = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_c, lambda n: n.get("text") == "Count: 3") is not None

    # Switch to Home and back to Counter -> state must be preserved.
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": 115.0, "y": 822.0}})
    cli.call("send_pointer_event", {"session_id": sid, "event": {"kind": "click", "x": 201.0, "y": 822.0}})
    tree_back = cli.call("inspect_ui_tree", {"session_id": sid})["root"]
    assert find_node(tree_back, lambda n: n.get("text") == "Count: 3") is not None, "tab state must survive tab switch"

    cli.call("stop_session", {"session_id": sid})


def main():
    subprocess.run(["cargo", "build", "-q", "-p", "onesim-cli"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="onesim-anim-behavior-") as tmp:
        tmp = Path(tmp)
        apps = {
            "animation": tmp / "animation",
            "gestures": tmp / "gestures",
            "showcase": tmp / "showcase",
            "tabs": tmp / "tabs",
        }
        for name, out in apps.items():
            subprocess.run(
                [str(ROOT / "scripts/build-swift-app.sh"), str(ROOT / f"examples/swiftui-{name}"), str(out)],
                cwd=ROOT,
                check=True,
                stdout=subprocess.DEVNULL,
            )
        print("Running SwiftUI animation & interactive behavior suite:")
        cli = OneSimClient()
        try:
            test_swiftui_animation(cli, apps["animation"])
            test_swiftui_gestures(cli, apps["gestures"])
            test_swiftui_showcase(cli, apps["showcase"])
            test_swiftui_tabs(cli, apps["tabs"])
        finally:
            cli.close()
    print("All 4 SwiftUI animation & behavior suites passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
