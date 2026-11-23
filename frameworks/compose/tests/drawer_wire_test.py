"""Compose guest wire test: Material 3 navigation drawers (DrawerState, drag settling, scrim, system back).

Drives DrawerLab over stdin/stdout like wire_test.py. Usage: drawer_wire_test.py <DrawerLab launcher>
Build: frameworks/compose/scripts/build-app.sh frameworks/compose/tests/DrawerLab.kt <out>/app DrawerLab
"""
import json, os, queue, re, subprocess, sys, threading, time

ENV = dict(os.environ, SIMCORE_SCALE="2.625")
D = 2.625
W, H = 411.42856, 923.4286
CLOSED = -round(360 * D)  # closed anchor in px: -sheet width (360 dp)
SCREEN = [0, 0, W, H]

failures = []
errors = []


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


def recv(p, kind="tree", timeout=10):
    while True:
        m = p.q.get(timeout=timeout)
        assert m is not None, "guest exited"
        if m["type"] == kind:
            return m
        if m["type"] == "error":
            errors.append(m["message"])


def settle(p, wait=0.8):
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


def idle(p, t, wait=1.0):
    """The newest tree the guest sends on its own within `wait` seconds (no message pokes it)."""
    end = time.time() + wait
    while time.time() < end:
        try:
            m = p.q.get(timeout=0.05)
        except queue.Empty:
            continue
        assert m is not None, "guest exited"
        if m["type"] == "tree":
            t = m
    return t


def walk(n):
    yield n
    for c in n.get("children", []):
        yield from walk(c)


def texts(n):
    return [m["props"]["text"] for m in walk(n) if m["component"] == "Text"]


def state(t, name="Modal"):
    line = next(s for s in texts(t["root"]) if s.startswith(name + " cur="))
    return dict(kv.split("=") for kv in line.split()[1:])


def mods(n):
    return n["props"].get("modifier", [])


def action(p, t, label):
    """Click the button labelled `label` through its action id (works under the scrim too)."""
    for n in walk(t["root"]):
        for m in mods(n):
            if "clickable" in m and label in texts(n) and len(texts(n)) == 1:
                send(p, type="action", id=m["clickable"]["action"], version=t["version"])
                return recv(p)
    raise AssertionError("no button " + label)


def drawer_key(t):
    """The drawer's horizontal draggable (the outermost horizontal gesture layer)."""
    for n in walk(t["root"]):
        for m in mods(n):
            if "gesture" in m and m["gesture"].get("axis") == "horizontal":
                return m["gesture"]["key"]
    raise AssertionError("no draggable")


def scrim_key(t):
    """The scrim: a full-screen Box with a background and a (non-axis) gesture layer."""
    for n in walk(t["root"]):
        ms = mods(n)
        if any("gesture" in m and "axis" not in m["gesture"] for m in ms) and any("background" in m for m in ms) and not n.get("children"):
            return next(m["gesture"]["key"] for m in ms if "gesture" in m)
    return None


def check(cond, what, detail=""):
    if not cond:
        failures.append(f"{what}: {detail}")
        print("FAIL", what, detail)
    else:
        print("ok  ", what)


def expect(t, what, name="Modal", **kv):
    s = state(t, name)
    check(all(s.get(k) == v for k, v in kv.items()), what, s)


T = [10000]


def gesture(p, keys, phase, *pt):
    send(p, type="gesture", phase=phase, time=T[0], points=[list(pt)], targets=[{"key": k, "frame": SCREEN} for k in keys])
    return recv(p)


def drag(p, key, x, y, dx, steps=8, step_ms=16, hold_ms=0):
    """A one-finger drag by dx dp; `hold_ms` rests the finger before lifting (release velocity ~0)."""
    T[0] += 1000
    gesture(p, [key], "start", x, y)
    for i in range(1, steps + 1):
        T[0] += step_ms
        gesture(p, [key], "move", x + dx * i / steps, y)
    if hold_ms:
        for _ in range(hold_ms // 50):
            T[0] += 50
            gesture(p, [key], "move", x + dx, y)
    T[0] += 16
    gesture(p, [key], "end", x + dx, y)


def back(p, phase, progress=0.0, edge="left"):
    T[0] += 16
    send(p, type="back", phase=phase, progress=progress, edge=edge, x=0 if edge == "left" else W, y=400, time=T[0])
    return recv(p)


def sheet_padding(t):
    """Vertical padding the predictive back puts around the modal sheet (0 without)."""
    for n in walk(t["root"]):
        ms = mods(n)
        if "Pick" in texts(n) and any("widthIn" in m for m in ms):
            pads = [m["padding"] for m in ms if "padding" in m and m["padding"][1] > 0 and m["padding"][0] == 0]
            return pads[0][1] if pads else 0
    return 0


p = spawn(sys.argv[1])
assert recv(p, "hello")["protocol"] == 1
t = settle(p, 0.3)

# --- initial state: closed, anchors from the 360 dp sheet; no scrim
expect(t, "initial closed", cur="Closed", target="Closed", open="false", closed="true", anim="false", off=str(CLOSED))
check(scrim_key(t) is None, "no scrim while closed")

# --- open(): animates (isAnimationRunning, targetValue Open while currentValue is still Closed), then settles
t = action(p, t, "Open")  # the answer to the action: the animation has started
expect(t, "opening animates", cur="Closed", target="Open", anim="true")
t = idle(p, t)
expect(t, "open() settles open (the guest sends the settled tree by itself)", cur="Open", target="Open", open="true", anim="false", off="0")
check(scrim_key(t) is not None, "scrim while open")

# --- the back key (a lone commit) closes the drawer
t = back(p, "commit", edge="none")
t = settle(p)
expect(t, "back key closes", cur="Closed", off=str(CLOSED))

# --- back without an open drawer is not handled by the drawer (the handler is disabled)
t = back(p, "commit", edge="none")
t = settle(p, 0.3)
expect(t, "back while closed leaves it closed", cur="Closed")

# --- predictive back: progress shrinks the sheet, cancel restores it open
t = action(p, t, "SnapOpen")
t = settle(p, 0.2)
expect(t, "snapTo(Open)", cur="Open", off="0", anim="false")
back(p, "start", 0.0)
back(p, "progress", 0.5)
t = back(p, "progress", 1.0)
pad = sheet_padding(t)
check(abs(pad - 24) < 0.5, "predictive back shrinks the sheet by 48 dp at full progress", pad)
expect(t, "drawer stays open during the back gesture", cur="Open", off="0")
t = back(p, "cancel")
t = settle(p, 0.4)
expect(t, "cancelled back keeps it open", cur="Open", off="0")
check(sheet_padding(t) == 0, "cancel clears the predictive back shrink", sheet_padding(t))

# --- predictive back committed closes it
back(p, "start", 0.0, edge="right")
back(p, "progress", 0.3, edge="right")
t = back(p, "commit", edge="right")
t = settle(p)
expect(t, "committed back gesture closes", cur="Closed", off=str(CLOSED))

# --- swipe to open from the content: slow drags settle by the 50% positional threshold
k = drawer_key(t)
drag(p, k, 100, 500, 120, hold_ms=300)  # 120 dp - slop < 180 dp: back to closed
t = settle(p)
expect(t, "short slow drag settles closed", cur="Closed", off=str(CLOSED))
drag(p, k, 60, 500, 260, hold_ms=300)  # past half the sheet
t = settle(p)
expect(t, "long slow drag settles open", cur="Open", off="0")

# --- drag the sheet closed; a fast flick beats the positional threshold (velocity > 400 dp/s)
drag(p, k, 300, 500, -100, steps=8)  # 100 dp in 128 ms: under the 180 dp positional threshold
t = settle(p)
expect(t, "fling closes", cur="Closed", off=str(CLOSED))
drag(p, k, 100, 500, 100, steps=8)
t = settle(p)
expect(t, "fling opens", cur="Open", off="0")

# --- items in the sheet are clickable
t = action(p, t, "Pick")
t = settle(p, 0.2)
check(next(s for s in texts(t["root"]) if s.startswith("Allow")).endswith("picks 1"), "drawer item click")

# --- confirmStateChange vetoes closing: drag bounces back open, the scrim does nothing
t = action(p, t, "Veto")
t = settle(p, 0.2)
drag(p, k, 300, 500, -250, hold_ms=300)
t = settle(p)
expect(t, "vetoed drag returns open", cur="Open", off="0")
sk = scrim_key(t)
T[0] += 1000
gesture(p, [sk], "start", 390, 500); T[0] += 48; gesture(p, [sk], "end", 390, 500)
t = settle(p)
expect(t, "vetoed scrim tap keeps it open", cur="Open", off="0")

# --- the scrim tap closes when allowed
t = action(p, t, "Veto")
t = settle(p, 0.2)
sk = scrim_key(t)
T[0] += 1000
gesture(p, [sk], "start", 390, 500); T[0] += 48; gesture(p, [sk], "end", 390, 500)
t = settle(p)
expect(t, "scrim tap closes", cur="Closed", off=str(CLOSED))

# --- close() and snapTo(Closed)
t = action(p, t, "SnapOpen"); t = settle(p, 0.2)
t = action(p, t, "Close"); t = settle(p)
expect(t, "close()", cur="Closed", off=str(CLOSED))
t = action(p, t, "SnapOpen"); t = settle(p, 0.2)
t = action(p, t, "SnapClosed"); t = settle(p, 0.2)
expect(t, "snapTo(Closed)", cur="Closed", off=str(CLOSED), anim="false")

# --- dismissible: the content moves with the drawer (offset = sheet width + drawer offset)
t = action(p, t, "Mode"); t = settle(p, 0.3)
expect(t, "dismissible starts closed", name="Dis", cur="Closed", off=str(CLOSED))


def content_offset(t):
    for n in walk(t["root"]):
        ts = texts(n)
        if any(s.startswith("Dis ") for s in ts) and not any(s == "Dis sheet" for s in ts):
            o = [m["offset"] for m in mods(n) if "offset" in m]
            if o:
                return o[0][0]
    return None


check(abs(content_offset(t) or 0) < 0.5, "dismissible content at 0 while closed", content_offset(t))
t = action(p, t, "DisOpen"); t = settle(p)
expect(t, "dismissible opens", name="Dis", cur="Open", off="0")
check(abs((content_offset(t) or 0) - 360) < 0.5, "dismissible content moved by the sheet width", content_offset(t))
t = back(p, "commit", edge="none"); t = settle(p)
expect(t, "dismissible sheet closes on back", name="Dis", cur="Closed")

send(p, type="shutdown")
check(not errors, "no guest errors", errors)
if failures:
    print(f"{len(failures)} failure(s)")
    sys.exit(1)
print("drawer wire test passed")
