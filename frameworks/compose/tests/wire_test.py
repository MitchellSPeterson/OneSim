"""Compose guest wire test: pointer input and interaction (protocol sections K and L) driven over stdin/stdout.

Touches go straight to gesture keys with explicit event times and frames, so timing rules (long press, double tap,
tap timeout, touch slop) are checked exactly. Usage: wire_test.py <GestureLab launcher> <counter launcher>
"""
import json, os, queue, subprocess, sys, threading, time

ENV = dict(os.environ, SIMCORE_SCALE="2.625")
D = 2.625


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
    """Let `wait` seconds of real time pass (timers, animations), then return the newest tree."""
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


def innermost(t, prefix):
    """The deepest Box whose texts start with `prefix`."""
    return [n for n in walk(t["root"]) if n["component"] == "Box" and any(s.startswith(prefix) for s in texts(n))][-1]


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
        out += list(reversed(keys(n)))  # inner modifiers are innermost
    return out


F = [0, 0, 200, 40]


def touch(p, ks, phase, t, *pts, frame=F):
    send(p, type="gesture", phase=phase, time=t, points=[list(q) for q in pts], targets=[{"key": k, "frame": frame} for k in ks])
    return recv(p)


p = spawn(sys.argv[1])
assert recv(p, "hello")["protocol"] == 1
t = recv(p)

# --- every pointer modifier is a gesture layer; disabled clickables, hoverable and selectableGroup put nothing on the wire
plain = innermost(t, "Plain")
assert not any("gesture" in m or "clickable" in m for m in mods(plain)), mods(plain)
dis = innermost(t, "Disabled 0")
assert [{k: v for k, v in m["clickable"].items() if k != "action"} for m in mods(dis) if "clickable" in m] == [{"enabled": False}], mods(dis)
assert all(m.get("gesture", {}).get("pans", True) for n in walk(t["root"]) for m in mods(n) if "gesture" in m)
drag_node = innermost(t, "Drag ")
assert [m["gesture"].get("axis") for m in mods(drag_node) if "gesture" in m] == ["horizontal"], "draggable is axis-locked"

# --- nested clickables: the child consumes the tap
k = target(t, "Nest")
assert len(k) == 2, k
touch(p, k, "start", 1000, (10, 10))
t = touch(p, k, "end", 1048, (10, 10))
t = settle(p, 0.1)
assert text(t, "Nest") == "Nest 1 0", text(t, "Nest")
# Leaving the bounds cancels the click.
touch(p, k, "start", 2000, (10, 10))
touch(p, k, "move", 2016, (10, 90))
touch(p, k, "end", 2032, (10, 90))
t = settle(p, 0.1)
assert text(t, "Nest") == "Nest 1 0", "moved out of bounds: no click"
# Touches on the parent alone click the parent.
touch(p, k[1:], "start", 2100, (150, 10))
touch(p, k[1:], "end", 2148, (150, 10))
t = settle(p, 0.1)
assert text(t, "Nest") == "Nest 1 1", text(t, "Nest")

# --- pass order: Initial outer -> inner, Main inner -> outer, Final outer -> inner
k = target(t, "Log")
t = touch(p, k, "start", 3000, (10, 10))
assert text(t, "Log").strip() == "Log oI iI iM oM oF iF", text(t, "Log")

# --- long press by event time (400 ms), double tap (second down within 300 ms), single tap after the timeout
k = target(t, "Combo")
touch(p, k, "start", 4000, (10, 10))
touch(p, k, "move", 4300, (10, 10))
t = touch(p, k, "move", 4401, (10, 10))
assert text(t, "Combo") == "Combo 0 1 0", ("long press at 400 ms", text(t, "Combo"))
touch(p, k, "end", 4600, (10, 10))
for (a, b) in [(5000, 5048), (5148, 5196)]:
    touch(p, k, "start", a, (10, 10))
    t = touch(p, k, "end", b, (10, 10))
t = settle(p, 0.1)
assert text(t, "Combo") == "Combo 0 1 1", ("double tap", text(t, "Combo"))
touch(p, k, "start", 6000, (10, 10))
t = touch(p, k, "end", 6048, (10, 10))
assert text(t, "Combo") == "Combo 0 1 1", "a single tap waits for the double tap timeout"
t = settle(p, 0.6)
assert text(t, "Combo") == "Combo 1 1 1", ("single tap after 300 ms (real time)", text(t, "Combo"))
# A second down sooner than 40 ms after the first up is not a double tap.
touch(p, k, "start", 7000, (10, 10)); touch(p, k, "end", 7048, (10, 10))
touch(p, k, "start", 7060, (10, 10)); touch(p, k, "end", 7100, (10, 10))
t = settle(p, 0.8)
assert text(t, "Combo").split()[3] == "1", ("too fast for a double tap", text(t, "Combo"))

# --- draggable: touch slop (8 dp = 21 px) is not part of the drag; release velocity; drag interactions
k = target(t, "Drag")
touch(p, k, "start", 8000, (10, 10))
t = touch(p, k, "move", 8016, (17, 10))
assert text(t, "Drag").startswith("Drag 0 - -"), "inside the slop"
for i in range(1, 6):
    t = touch(p, k, "move", 8016 + 16 * i, (17 + 10 * i, 10))
t = settle(p, 0.05)
assert text(t, "Drag").endswith("true"), ("collectIsDraggedAsState while dragging", text(t, "Drag"))
touch(p, k, "end", 8112, (67, 10))
t = settle(p, 0.1)
# 57 dp moved = 149.6 px, minus 21 px of slop.
assert text(t, "Drag").split()[1] == str(round(57 * D - 21)), text(t, "Drag")
assert text(t, "Drag").split()[3] == "+" and text(t, "Drag").endswith("false"), text(t, "Drag")
# Vertical movement does not start a horizontal drag.
touch(p, k, "start", 9000, (100, 5))
for i in range(1, 5):
    touch(p, k, "move", 9000 + 16 * i, (100, 5 + 10 * i))
touch(p, k, "end", 9100, (100, 45))
t = settle(p, 0.1)
assert text(t, "Drag").split()[1] == str(round(57 * D - 21)), "cross-axis moves are ignored"

# --- pointerInput restarts when its key changes (a new handler starts at the next touch)
k = target(t, "Starts")
touch(p, k, "start", 10000, (10, 10)); t = touch(p, k, "end", 10048, (10, 10))
assert text(t, "Starts") == "Starts 1", text(t, "Starts")
touch(p, k, "start", 10100, (10, 10)); t = touch(p, k, "end", 10148, (10, 10))
assert text(t, "Starts") == "Starts 1", "same key: the handler keeps running"
kk = target(t, "Rekey")
touch(p, kk, "start", 10200, (10, 10)); touch(p, kk, "end", 10248, (10, 10))
t = settle(p, 0.1)
touch(p, k, "start", 10300, (10, 10)); t = touch(p, k, "end", 10348, (10, 10))
t = settle(p, 0.1)
assert text(t, "Starts") == "Starts 2", text(t, "Starts")

# --- two-finger zoom (detectTransformGestures)
k = target(t, "Zoom")
touch(p, k, "start", 11000, (80, 20), (120, 20))
for i in range(1, 9):
    touch(p, k, "move", 11000 + 16 * i, (80 - 2.5 * i, 20), (120 + 2.5 * i, 20))
t = touch(p, k, "end", 11200, (60, 20), (140, 20))
# Past the slop (8 dp of zoom motion) at the third step, which is reported: 40 dp / 25 dp of spread = 1.6.
assert text(t, "Zoom") == "Zoom 16", text(t, "Zoom")

# --- drag after a long press
k = target(t, "AfterLong")
touch(p, k, "start", 12000, (10, 10))
t = touch(p, k, "move", 12450, (10, 10))
assert text(t, "AfterLong") == "AfterLong start", text(t, "AfterLong")
touch(p, k, "move", 12466, (30, 10))
t = touch(p, k, "end", 12482, (30, 10))
t = settle(p, 0.1)
assert text(t, "AfterLong") == "AfterLong start move end", text(t, "AfterLong")

# --- press inside a scrollable shows after the 100 ms tap timeout; a quick drag flings past the finger
k = target(t, "Scroll")
assert len(k) == 2, k
t = touch(p, k, "start", 13000, (10, 10))
assert text(t, "Scroll").endswith("false"), "press delayed inside a scroller"
t = settle(p, 0.3)
assert text(t, "Scroll").endswith("true"), ("press shown after the tap timeout", text(t, "Scroll"))
touch(p, k, "end", 13400, (10, 10))
t = settle(p, 0.1)
assert text(t, "Scroll").endswith("false")
touch(p, k, "start", 14000, (10, 10))
for i in range(1, 9):
    touch(p, k, "move", 14000 + 16 * i, (10 + 30 * i, 10))
touch(p, k, "end", 14144, (250, 10))
dragged = round(240 * D - 21)
t = settle(p, 2.5)
flung = int(text(t, "Scroll").split()[1])
# The lift (resting 16 ms) slows the least-squares velocity; the spline fling still carries it 300+ px further.
assert flung > dragged + 300, ("fling continues after release", dragged, flung)

# --- host cancel: the press is cancelled (pointer input cancellation)
k = target(t, "Cancels")
touch(p, k, "start", 15000, (10, 10))
touch(p, k, "cancel", 15016, (10, 10))
t = settle(p, 0.1)
assert text(t, "Cancels") == "Cancels 1", text(t, "Cancels")

# --- a shared interaction source drives an unbounded ripple on another node
k = target(t, "Shared")
t = touch(p, k, "start", 16000, (10, 10))
t = settle(p, 0.4)
shared = innermost(t, "Shared")
r = next(m["ripple"] for m in mods(shared) if "ripple" in m)
assert r["bounded"] is False and r["radius"] == 20 and r["color"] == [1, 0, 0, 1], r
assert r["ripples"][0]["origin"] is None and r["ripples"][0]["radius"] == 1 and abs(r["ripples"][0]["alpha"] - 0.1) < 1e-6, r
touch(p, k, "end", 16400, (10, 10))
t = settle(p, 0.5)
assert not any("ripple" in m for n in walk(t["root"]) for m in mods(n) if "Shared" in texts(n)), "ripple faded out"

# --- a throwing handler is reported and the app keeps going
k = target(t, "Boom")
touch(p, k, "start", 17000, (10, 10))
t = settle(p, 0.1)
assert any("boom" in e for e in errors), errors
assert text(t, "Boom") == "Boom 1"

# --- a user drag interrupts a programmatic drag of lower priority
k = target(t, "Prog")
touch(p, k, "start", 18000, (10, 10))
for i in range(1, 5):
    touch(p, k, "move", 18000 + 16 * i, (10 + 10 * i, 10))
touch(p, k, "end", 18100, (50, 10))
t = settle(p, 0.1)
assert text(t, "Prog") == "Prog interrupted", text(t, "Prog")

send(p, type="shutdown")
assert p.wait(timeout=10) == 0
print("compose gestures: ok")

# --- the material counter: a click is press + release on the button's gesture key, with a ripple while pressed
p = spawn(sys.argv[2])
recv(p, "hello")
t = recv(p)
k = target(t, "Increment")
assert len(k) == 1
t = touch(p, k, "start", 1000, (20, 20), frame=[0, 0, 120, 40])
t = settle(p, 0.4)
btn = next(n for n in walk(t["root"]) if any("ripple" in m for m in mods(n)))
r = next(m["ripple"] for m in mods(btn) if "ripple" in m)
assert r["bounded"] and r["color"] == [1, 1, 1, 1] and r["ripples"][0]["origin"] == [20, 20], r
assert r["ripples"][0]["radius"] == 1 and r["ripples"][0]["center"] == 1, "fully grown after 225 ms"
touch(p, k, "end", 1500, (20, 20), frame=[0, 0, 120, 40])
t = settle(p, 0.5)
assert "Count: 1" in texts(t["root"]) and not any("ripple" in m for n in walk(t["root"]) for m in mods(n))
send(p, type="shutdown")
assert p.wait(timeout=10) == 0
print("compose counter: ok")
