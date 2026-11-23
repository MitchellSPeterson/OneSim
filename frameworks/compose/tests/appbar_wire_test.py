"""Compose guest wire test: Material 3 app bar scroll behaviours (nested scroll, snapping, flings, state saving).

Drives tests/AppBarLab.kt over stdin/stdout with section L `gesture` touches on the list's scrollable and on the bars.
Usage: appbar_wire_test.py <AppBarLab launcher>. Exit 0 = pass.
"""
import json, os, queue, subprocess, sys, threading, time

ENV = dict(os.environ, SIMCORE_SCALE="2.625")
D = 2.625
SLOP = 21  # 8 dp touch slop in px


def spawn(exe):
    p = subprocess.Popen([exe], env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
    p.q = queue.Queue()

    def read():
        for line in p.stdout:
            p.q.put(json.loads(line))
        p.q.put(None)

    threading.Thread(target=read, daemon=True).start()
    return p


def send(p, **m):
    p.stdin.write(json.dumps(m) + "\n"); p.stdin.flush()


errors = []


def recv(p, kind="tree", timeout=10):
    while True:
        m = p.q.get(timeout=timeout)
        assert m is not None, "guest exited"
        if m["type"] == kind:
            return m
        if m["type"] == "error":
            errors.append(m["message"])


def settle(p, wait):
    """Let `wait` seconds of real time pass (animations), then return the newest tree."""
    time.sleep(wait)
    send(p, type="environment", appearance="light")
    t = recv(p)
    while True:
        try:
            m = p.q.get(timeout=0.25)
        except queue.Empty:
            return t
        assert m is not None, "guest exited"
        if m["type"] == "tree":
            t = m
        elif m["type"] == "error":
            errors.append(m["message"])


def walk(n):
    yield n
    for c in n.get("children", []):
        yield from walk(c)


def texts(n):
    return [m["props"]["text"] for m in walk(n) if m["component"] == "Text"]


def text(t, prefix):
    return next(s for s in texts(t["root"]) if s.startswith(prefix))


def mods(n):
    return n["props"].get("modifier", [])


def keys(n):
    out = []
    for m in mods(n):
        if "clickable" in m and "key" in m["clickable"]:
            out.append(m["clickable"]["key"])
        if "gesture" in m:
            out.append(m["gesture"]["key"])
    return out


def target(t, prefix):
    """Gesture keys of the innermost node holding text `prefix`, then its ancestors' (innermost first)."""
    path = []

    def find(n, stack):
        if n["component"] == "Text" and n["props"].get("text", "").startswith(prefix):
            path.extend(stack + [n])
            return True
        return any(find(c, stack + [n]) for c in n.get("children", []))

    assert find(t["root"], []), prefix
    out = []
    for n in reversed(path):
        out += list(reversed(keys(n)))
    return out


F = [0, 0, 400, 400]
clock = [1000]


def touch(p, ks, phase, *pts):
    send(p, type="gesture", phase=phase, time=clock[0], points=[list(q) for q in pts], targets=[{"key": k, "frame": F} for k in ks])
    return recv(p)


def tap(p, label):
    t = settle(p, 0.05)
    k = target(t, label)
    clock[0] += 1000
    touch(p, k, "start", (10, 10))
    clock[0] += 48
    touch(p, k, "end", (10, 10))
    return settle(p, 0.2)


def drag(p, label, dy_px, release=True, fling=False):
    """Drag the node holding `label` vertically so that `dy_px` pixels pass the touch slop (negative = finger up).
    Without `fling`, the finger rests 200 ms before lifting (no release velocity). Returns the tree before the lift."""
    t = settle(p, 0.05)
    k = target(t, label)
    clock[0] += 1000
    y0 = 200.0
    total = (dy_px + (SLOP if dy_px > 0 else -SLOP)) / D
    touch(p, k, "start", (100, y0))
    for i in range(1, 9):
        clock[0] += 16
        touch(p, k, "move", (100, y0 + total * i / 8))
    t = settle(p, 0.05)
    if not release:
        return t
    if not fling:
        clock[0] += 200
        touch(p, k, "move", (100, y0 + total))
    clock[0] += 16
    touch(p, k, "end", (100, y0 + total))
    return t


def state(t, which="Top"):
    s = text(t, which + " ").split()[1:]
    return dict(kv.split("=") for kv in s)


def list_pos(t):
    return int(text(t, "List").split()[1])


def bar_height(t, title):
    """The `height` (dp) of the clipped row box holding `title`."""
    for n in walk(t["root"]):
        if n["component"] == "Box" and any("clip" in m for m in mods(n)) and texts(n) == [title]:
            return next(m["height"] for m in mods(n) if "height" in m)
    raise AssertionError("no bar row for " + title)


def bar_color(t, title):
    for n in walk(t["root"]):
        if n["component"] == "Box" and texts(n) and set(texts(n)) == {title}:
            bg = [m["background"]["color"] for m in mods(n) if "background" in m]
            if bg:
                return bg[0]
    raise AssertionError("no bar background")


def alphas(t, title):
    """Alpha of each title block holding `title` (1 when it has no alpha modifier)."""
    out = []
    for n in walk(t["root"]):
        if n["component"] == "Box" and texts(n) == [title] and any("padding" in m for m in mods(n)) and \
                n.get("children", [{}])[0].get("component") == "Text":
            out.append(next((m["alpha"] for m in mods(n) if "alpha" in m), 1))
    return out


def near(a, b, tol=1.5):
    return abs(float(a) - float(b)) <= tol


p = spawn(sys.argv[1])
assert recv(p, "hello")["protocol"] == 1
t = recv(p)

SURFACE = [round(c / 255, 3) for c in (0xFE, 0xF7, 0xFF)] + [1]
SCROLLED = [round(c / 255, 3) for c in (0xF3, 0xED, 0xF7)] + [1]

# --- pinned: the bar never moves; content scroll accumulates and turns the container to surfaceContainer
assert text(t, "Mode") == "Mode pinned"
assert bar_color(t, "Bar") == SURFACE, bar_color(t, "Bar")
drag(p, "Item 1", -100)
t = settle(p, 0.6)
s = state(t)
assert s["h"] == "0" and s["lim"] == "-168" and s["c"] == "-100", s
assert list_pos(t) == 100
assert s["o"] == "0.6", s  # 1 - (-168 + 100) / -168
assert near(bar_height(t, "Bar"), 64, 0.01)
assert bar_color(t, "Bar") == SCROLLED, ("scrolled container color", bar_color(t, "Bar"))
drag(p, "Item 1", 100)
t = settle(p, 0.6)
assert state(t)["c"] == "0" and list_pos(t) == 0, state(t)
assert bar_color(t, "Bar") == SURFACE, "back to the container color at the start"
print("pinned: ok")

# --- enterAlways: the bar takes the scroll first, both ways; snaps to the nearer end on release
t = tap(p, "Mode")
assert text(t, "Mode") == "Mode enter"
t = drag(p, "Item 1", -60, release=False)
s = state(t)
assert s["h"] == "-60" and list_pos(t) == 0, ("pre-scroll collapses the bar first", s)
assert near(bar_height(t, "Bar"), (168 - 60) / D, 0.01), bar_height(t, "Bar")
clock[0] += 200
k = target(t, "Item 1")
touch(p, k, "move", (100, 200 - (60 + SLOP) / D))
clock[0] += 16
touch(p, k, "end", (100, 200 - (60 + SLOP) / D))
t = settle(p, 0.8)
assert state(t)["h"] == "0", ("collapsed fraction 0.36 snaps back open", state(t))
drag(p, "Item 1", -120)
t = settle(p, 0.8)
assert state(t)["h"] == "-168" and state(t)["f"] == "1.0", ("fraction 0.71 snaps closed", state(t))
assert near(bar_height(t, "Bar"), 0, 0.01)
drag(p, "Item 1", -100)
t = settle(p, 0.5)
s = state(t)
assert s["h"] == "-168" and s["c"] == "-100" and list_pos(t) == 100, s
t = drag(p, "Item 1", 50, release=False)
assert state(t)["h"] == "-118" and list_pos(t) == 100, ("scrolling down brings the bar back first", state(t))
clock[0] += 200
touch(p, target(t, "Item 1"), "end", (100, 200))
t = settle(p, 0.8)
assert state(t)["h"] == "-168", state(t)
# A fling: the bar opens first, the list takes the rest and keeps going.
drag(p, "Item 1", 300, fling=True)
t = settle(p, 2.0)
s = state(t)
assert s["h"] == "0" and list_pos(t) == 0 and s["c"] == "0", ("fling down opens the bar and scrolls home", s, list_pos(t))
# Dragging the bar itself resizes it, then it settles.
t = drag(p, "Bar", -40, release=False)
assert state(t)["h"] == "-40", ("draggable bar", state(t))
clock[0] += 200
touch(p, target(t, "Bar"), "end", (100, 200))
t = settle(p, 0.8)
assert state(t)["h"] == "0", state(t)
print("enterAlways: ok")

# --- exitUntilCollapsed: collapses before the content scrolls; expands only once the content is back at its start
t = tap(p, "Mode")
assert text(t, "Mode") == "Mode exit"
drag(p, "Item 1", -100)
t = settle(p, 0.8)
assert state(t)["h"] == "-168" and list_pos(t) == 0, ("0.6 collapsed snaps closed", state(t))
drag(p, "Item 1", -100)
t = settle(p, 0.5)
assert state(t)["h"] == "-168" and list_pos(t) == 100, state(t)
drag(p, "Item 1", 50)
t = settle(p, 0.5)
assert state(t)["h"] == "-168" and list_pos(t) == 50, ("content scrolls back first", state(t), list_pos(t))
t = drag(p, "Item 1", 100, release=False)
assert list_pos(t) == 0 and near(state(t)["h"], -118), ("the bar takes what the content leaves", state(t))
clock[0] += 200
touch(p, target(t, "Item 1"), "end", (100, 200))
t = settle(p, 0.8)
assert state(t)["h"] == "-168", ("fraction 0.7 snaps closed", state(t))
print("exitUntilCollapsed: ok")

# --- medium: the second row (48 dp = 126 px) collapses; the titles crossfade by the collapsed fraction
t = tap(p, "Mode")
assert text(t, "Mode") == "Mode medium"
assert alphas(t, "Medium") == [0, 1], alphas(t, "Medium")
t = drag(p, "Item 1", -50, release=False)
s = state(t)
assert s["h"] == "-50" and s["lim"] == "-126", s
top_a, bottom_a = alphas(t, "Medium")
assert abs(bottom_a - (1 - 50 / 126)) < 0.01, bottom_a
assert 0 < top_a < 0.1, ("TopTitleAlphaEasing(0.4)", top_a)
mid = bar_color(t, "Medium")
assert mid not in (SURFACE, SCROLLED), ("container color follows the collapsed fraction", mid)
clock[0] += 200
touch(p, target(t, "Item 1"), "end", (100, 200))
t = settle(p, 0.8)
assert state(t)["h"] == "0" and alphas(t, "Medium") == [0, 1], state(t)
drag(p, "Item 1", -200)
t = settle(p, 0.8)
assert state(t)["h"] == "-126" and alphas(t, "Medium") == [1, 0], (state(t), alphas(t, "Medium"))
assert bar_color(t, "Medium") == SCROLLED
print("medium: ok")

# --- bottom app bar exitAlways: slides out with the content scroll (80 dp = 210 px), snaps on release
t = tap(p, "Mode")
assert text(t, "Mode") == "Mode bottom"
t = drag(p, "Item 1", -100, release=False)
s = state(t, "Bottom")
assert s["h"] == "-100" and s["c"] == "-100" and list_pos(t) == 100, ("post-scroll moves the bar with the content", s)
clock[0] += 200
touch(p, target(t, "Item 1"), "end", (100, 200))
t = settle(p, 0.8)
s = state(t, "Bottom")
assert s["h"] == "0" and s["lim"] == "-210", ("0.48 snaps back", s)
drag(p, "Item 1", -150)
t = settle(p, 0.8)
assert state(t, "Bottom")["h"] == "-210", state(t, "Bottom")
print("bottomAppBar: ok")

# --- no snap / fling specs: the bar stays where the scroll left it; state round-trips through its Saver
t = tap(p, "Mode")
assert text(t, "Mode") == "Mode nosnap"
drag(p, "Item 1", -60)
t = settle(p, 0.6)
assert state(t)["h"] == "-60", ("no snapAnimationSpec: no snapping", state(t))
t = tap(p, "Save")
assert text(t, "Save") == "Save [-168.0, -60.0, 0.0] -60 0 [-3.4028235E38, 0.0, 0.0] 0", text(t, "Save")
print("nosnap + saver: ok")

send(p, type="shutdown")
assert p.wait(timeout=10) == 0
assert not errors, errors
print("compose app bars: ok")
