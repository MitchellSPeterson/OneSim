"""Compose guest wire test: Material 3 navigation bars and rails (NavBarLab.kt) driven over stdin/stdout.

Checks selection callbacks, the indicator-shaped ripple and its press mapping, disabled items, wide rail state
transitions, and the modal wide rail's scrim and system back (predictive back) dismissal.
Usage: navbar_wire_test.py [<NavBarLab launcher>]  (without one it builds NavBarLab.kt). Exit 0 = pass.
"""
import json, os, queue, subprocess, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
ENV = dict(os.environ, SIMCORE_SCALE="2.625", SIMCORE_INSETS="24,24")
D = 2.625

if len(sys.argv) > 1:
    exe = sys.argv[1]
else:
    out = tempfile.mkdtemp()
    exe = os.path.join(out, "app")
    subprocess.run([os.path.join(HERE, "..", "scripts", "build-app.sh"), os.path.join(HERE, "NavBarLab.kt"), exe, "NavBarLab"], check=True)


def spawn(path):
    p = subprocess.Popen([path], env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
    p.q = queue.Queue()

    def read():
        for line in p.stdout:
            p.q.put(json.loads(line))
        p.q.put(None)

    threading.Thread(target=read, daemon=True).start()
    return p


def send(p, **m):
    p.stdin.write(json.dumps(m) + "\n"); p.stdin.flush()


errors, events = [], []


def recv(p, kind="tree", timeout=15):
    while True:
        m = p.q.get(timeout=timeout)
        assert m is not None, "guest exited"
        if m["type"] == kind:
            return m
        if m["type"] == "error":
            errors.append(m["message"])
        if m["type"] == "event":
            events.append(m)


def settle(p, wait):
    time.sleep(wait)
    send(p, type="environment", appearance="light")
    t = recv(p)
    while True:
        try:
            m = p.q.get(timeout=0.3)
        except queue.Empty:
            return t
        assert m is not None, "guest exited"
        if m["type"] == "tree":
            t = m
        elif m["type"] == "error":
            errors.append(m["message"])
        elif m["type"] == "event":
            events.append(m)


def walk(n):
    yield n
    for c in n.get("children", []):
        yield from walk(c)


def mods(n):
    return n["props"].get("modifier", [])


def texts(n):
    return [m["props"]["text"] for m in walk(n) if m["component"] == "Text"]


def text(t, prefix):
    return next(s for s in texts(t["root"]) if s.startswith(prefix))


def path_to(t, label):
    """Nodes from the root down to the Text `label` (the last match: overlays come after the app)."""
    found = []

    def find(n, stack):
        if n["component"] == "Text" and n["props"].get("text") == label:
            found.append(stack + [n])
        for c in n.get("children", []):
            find(c, stack + [n])

    find(t["root"], [])
    assert found, label
    return found[-1]


def keys(n):
    out = []
    for m in mods(n):
        if "clickable" in m and "key" in m["clickable"]:
            out.append(m["clickable"]["key"])
        if "gesture" in m:
            out.append(m["gesture"]["key"])
    return out


def targets(t, label):
    out = []
    for n in reversed(path_to(t, label)):
        out += list(reversed(keys(n)))
    return out


def item(t, label):
    """The selectable item (the innermost node with a tab clickable) holding `label`."""
    return [n for n in path_to(t, label) if any(m.get("clickable", {}).get("role") == "tab" for m in mods(n))][-1]


def touch(p, ks, phase, tm, pt, frame):
    send(p, type="gesture", phase=phase, time=tm, points=[list(pt)], targets=[{"key": k, "frame": frame} for k in ks])
    return recv(p)


clock = [1000]


def tap(p, t, label, frame=(0, 0, 100, 80), at=None):
    ks = targets(t, label)
    assert ks, label
    pt = at or (frame[0] + frame[2] / 2, frame[1] + frame[3] / 2)
    clock[0] += 1000
    touch(p, ks, "start", clock[0], pt, list(frame))
    return touch(p, ks, "end", clock[0] + 48, pt, list(frame))


def back(p, phase, progress=0.0):
    clock[0] += 100
    send(p, type="back", phase=phase, progress=progress, edge="left", x=10, y=400, time=clock[0])
    return recv(p)


def ripples(t):
    return [(n, m["ripple"]) for n in walk(t["root"]) for m in mods(n) if "ripple" in m]


def check(cond, msg):
    if not cond:
        print("FAIL:", msg)
        sys.exit(1)


p = spawn(exe)
assert recv(p, "hello")["protocol"] == 1
t = recv(p)
check(text(t, "Sel") == "Sel 0", text(t, "Sel"))

# --- NavigationBarItem: a tap selects; the press ripple is drawn on the 56 x 32 indicator, mapped into its coordinates
ks = targets(t, "A1")
clock[0] += 1000
# Item 100 dp wide, pressed at its middle, 28 dp down: the indicator sits (100 - 56) / 2 = 22 dp in and 12 dp down.
t = touch(p, ks, "start", clock[0], (50, 28), [0, 0, 100, 80])
t = settle(p, 0.05)
rs = [r for _, r in ripples(t) if r.get("ripples")]
check(len(rs) == 1, f"one press ripple: {rs}")
o = rs[0]["ripples"][0]["origin"]
check(abs(o[0] - 28) < 0.6 and abs(o[1] - 16) < 0.6, f"ripple origin mapped into the indicator: {o}")
rn = next(n for n, r in ripples(t) if r.get("ripples"))
check({"width": 56} in mods(rn) and {"height": 32} in mods(rn), f"ripple layer is the indicator: {mods(rn)}")
check(any("clip" in m for m in mods(rn)), "ripple clipped to the pill")
touch(p, ks, "end", clock[0] + 48, (50, 28), [0, 0, 100, 80])
t = settle(p, 0.6)
check(text(t, "Sel") == "Sel 1", ("NavigationBarItem tap", text(t, "Sel")))
check(not [r for _, r in ripples(t) if r.get("ripples")], "ripple finished")
# The selected item's indicator is full width and opaque, the others have none.
sel_item = item(t, "A1")
inds = [n for n in walk(sel_item) if any("background" in m for m in mods(n)) and {"height": 32} in mods(n)]
check(len(inds) == 1 and {"width": 56} in mods(inds[0]), f"selected indicator 56 x 32: {[mods(n) for n in inds]}")
check(not [n for n in walk(item(t, "A0")) if any("background" in m for m in mods(n)) and {"height": 32} in mods(n)],
      "unselected item has no indicator")

# --- ShortNavigationBarItem and NavigationRailItem select; a disabled rail item does not
t = tap(p, t, "S4"); t = settle(p, 0.3)
check(text(t, "Sel") == "Sel 4", ("ShortNavigationBarItem tap", text(t, "Sel")))
t = tap(p, t, "R5", (0, 0, 80, 56)); t = settle(p, 0.3)
check(text(t, "Sel") == "Sel 5", ("NavigationRailItem tap", text(t, "Sel")))
r6 = item(t, "R6")
check(all(m["clickable"]["enabled"] is False for m in mods(r6) if "clickable" in m), "disabled rail item")
check(not keys(r6), f"no gesture key on a disabled item: {keys(r6)}")

# --- WideNavigationRail: toggling the state expands the rail; items switch to the start-icon layout (labelLarge)
w7 = next(n for n in walk(t["root"]) if n["component"] == "Text" and n["props"]["text"] == "W7")
check(w7["props"]["fontSize"] == 12, f"collapsed item label is labelMedium: {w7['props']}")
t = tap(p, t, "Toggle", (0, 0, 120, 40))
t = settle(p, 1.0)
check(text(t, "Wide") == "Wide Expanded Expanded", text(t, "Wide"))
w7 = next(n for n in walk(t["root"]) if n["component"] == "Text" and n["props"]["text"] == "W7")
check(w7["props"]["fontSize"] == 14, f"expanded item label is labelLarge: {w7['props']}")
t = tap(p, t, "W7", (0, 0, 200, 56)); t = settle(p, 0.3)
check(text(t, "Sel") == "Sel 7", ("WideNavigationRailItem tap", text(t, "Sel")))
t = tap(p, t, "Toggle", (0, 0, 120, 40)); t = settle(p, 1.0)
check(text(t, "Wide") == "Wide Collapsed Collapsed", text(t, "Wide"))


def scrim(t):
    """The modal scrim: a full-screen clickable with the 32% scrim color."""
    for n in walk(t["root"]):
        bg = [m["background"]["color"] for m in mods(n) if "background" in m]
        if bg and abs(bg[0][3] - 0.32) < 0.01 and keys(n):
            return n
    return None


# --- ModalWideNavigationRail: opens over the screen with a scrim; system back dismisses it
check(scrim(t) is None, "no scrim while collapsed")
check(texts(t["root"]).count("M8") == 1, "items in the collapsed rail")
t = tap(p, t, "OpenModal", (0, 0, 120, 40)); t = settle(p, 1.0)
check(text(t, "Modal") == "Modal Expanded Expanded", text(t, "Modal"))
check(scrim(t) is not None, "scrim shown")
check(texts(t["root"]).count("M8") == 1, "content moved into the modal rail")
check(t["root"]["children"][-1] is not None and "M8" in texts(t["root"]["children"][-1]), "modal rail drawn over the app")
# A cancelled back gesture leaves it open.
back(p, "start"); back(p, "progress", 0.5); back(p, "cancel")
t = settle(p, 0.6)
check(text(t, "Modal") == "Modal Expanded Expanded", ("cancelled back", text(t, "Modal")))
# A committed back gesture collapses it.
back(p, "start"); back(p, "progress", 0.3); back(p, "progress", 0.8); back(p, "commit", 0.8)
t = settle(p, 1.0)
check(text(t, "Modal") == "Modal Collapsed Collapsed", ("back dismisses", text(t, "Modal")))
check(scrim(t) is None, "scrim gone")
check(texts(t["root"]).count("M8") == 1, "content back in the collapsed rail")
check(not [e for e in events if e.get("name") == "activity.finished"], "back went to the rail, not the activity")
# Selecting an item in the open rail, then a scrim tap dismisses.
t = tap(p, t, "OpenModal", (0, 0, 120, 40)); t = settle(p, 1.0)
t = tap(p, t, "M9", (0, 0, 220, 56)); t = settle(p, 0.3)
check(text(t, "Sel") == "Sel 9", ("item in the modal rail", text(t, "Sel")))
s = scrim(t)
clock[0] += 1000
touch(p, keys(s), "start", clock[0], (380, 500), [0, 0, 411, 923])
touch(p, keys(s), "end", clock[0] + 48, (380, 500), [0, 0, 411, 923])
t = settle(p, 1.0)
check(text(t, "Modal") == "Modal Collapsed Collapsed", ("scrim tap dismisses", text(t, "Modal")))

# --- hideOnCollapse + shouldDismissOnBackPress = false: back does nothing (and does not reach the activity)
check("L10" not in texts(t["root"]), "hidden rail shows nothing collapsed")
t = tap(p, t, "OpenLocked", (0, 0, 120, 40)); t = settle(p, 1.0)
check(text(t, "Locked") == "Locked Expanded Expanded", text(t, "Locked"))
check("L10" in texts(t["root"]), "hidden rail slid in")
back(p, "commit")
t = settle(p, 0.6)
check(text(t, "Locked") == "Locked Expanded Expanded", ("back ignored", text(t, "Locked")))
check(not [e for e in events if e.get("name") == "activity.finished"], "back consumed by the rail window")
s = scrim(t)
check(s is not None, "locked rail scrim")
clock[0] += 1000
touch(p, keys(s), "start", clock[0], (380, 500), [0, 0, 411, 923])
touch(p, keys(s), "end", clock[0] + 48, (380, 500), [0, 0, 411, 923])
t = settle(p, 1.2)
check(text(t, "Locked") == "Locked Collapsed Collapsed", ("scrim tap", text(t, "Locked")))
check("L10" not in texts(t["root"]), "hidden again")

# --- with nothing open, back goes to the activity
back(p, "commit")
settle(p, 0.3)
check([e for e in events if e.get("name") == "activity.finished"], "back with no rail open finishes the activity")

check(not errors, errors)
send(p, type="shutdown")
print("navbar wire test: ok")
