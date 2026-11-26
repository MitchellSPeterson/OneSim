"""Compose guest wire test: navigation-compose and Android system back (protocol section X).

Drives NavLab.kt over stdin/stdout: taps go to clickables' gesture keys, back arrives as `back` messages (the back key
is a lone `commit`, a swipe is `start`, `progress`..., then `commit` or `cancel`). Usage: nav_wire_test.py <NavLab launcher>
"""
import json, os, queue, subprocess, sys, threading, time

ENV = dict(os.environ, SIMCORE_SCALE="2.625", SIMCORE_VIEWPORT="411x923")
p = subprocess.Popen([sys.argv[1]], env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
q = queue.Queue()
threading.Thread(target=lambda: ([q.put(json.loads(l)) for l in p.stdout], q.put(None)), daemon=True).start()
errors, events, clock = [], [], [1000]
last = [None]


def send(**m):
    p.stdin.write(json.dumps(m) + "\n")
    p.stdin.flush()


def settle(wait=0.9):
    """Let `wait` s pass (the 700 ms NavHost fades), then return the newest tree.

    Past the deadline it only takes what has already arrived (a running animation sends a tree every frame, so waiting
    for quiet would return its end state), unless no tree came yet: then it waits up to 0.25 s for the guest's reply.
    """
    end = time.time() + wait
    got = False
    while True:
        now = time.time()
        try:
            m = q.get(timeout=end - now) if now < end else (q.get_nowait() if got else q.get(timeout=0.25))
        except queue.Empty:
            if time.time() >= end:
                return last[0]
            continue
        assert m is not None, "guest exited"
        if m["type"] == "tree":
            last[0] = m
            got = True
        elif m["type"] == "error":
            errors.append(m["message"])
        elif m["type"] == "event":
            events.append(m["name"])


def walk(n):
    yield n
    for c in n.get("children", []):
        yield from walk(c)


def texts(n=None):
    n = n or last[0]["root"]
    return [m["props"]["text"] for m in walk(n) if m["component"] == "Text"]


def text(prefix):
    found = [s for s in texts() if s.startswith(prefix)]
    assert found, f"no text {prefix!r} in {texts()}"
    return found[0]


def tap(label, wait=0.9):
    key = None
    for n in walk(last[0]["root"]):
        for m in n["props"].get("modifier", []):
            own = texts(n)
            if "clickable" in m and (label in own or (label.endswith("=") and any(s.startswith(label) for s in own))):
                key = m["clickable"]["key"]
    assert key, f"no clickable {label!r} in {texts()}"
    for ph in ("start", "end"):
        send(type="gesture", phase=ph, time=clock[0], points=[[5, 5]], targets=[{"key": key, "frame": [0, 0, 200, 20]}])
        clock[0] += 48
    clock[0] += 400
    return settle(wait)


def back(wait=0.9):
    send(type="back", phase="commit", progress=0, edge="none", x=205, y=460, time=clock[0])
    clock[0] += 100
    return settle(wait)


def swipe(to, commit, wait=0.9, edge="left"):
    send(type="back", phase="start", progress=0, edge=edge, x=2, y=400, time=clock[0])
    for i in range(1, 9):
        send(type="back", phase="progress", progress=to * i / 8, edge=edge, x=2 + 411 * to * i / 8, y=400, time=clock[0] + 16 * i)
    clock[0] += 200
    mid = settle(0.3)
    send(type="back", phase="commit" if commit else "cancel", progress=to, edge=edge, x=2 + 411 * to, y=400, time=clock[0])
    clock[0] += 100
    settle(wait)
    return mid


def check(cond, what):
    assert cond, f"{what}: {texts()}"


assert q.get(timeout=20)["type"] == "hello"
settle(0.5)
check(text("route=") == "route=home" and text("stack=") == "stack=home", "start destination")

# Route arguments: path Int, query default and a percent-encoded query value.
tap("act:detail")
check(text("detail") == "detail id=7 tab=main", "path argument + default")
check(text("stack=") == "stack=home,detail/{id}?tab={tab}", "pushed")
check(not any(s.startswith("home n=") for s in texts()), "push transition finished: home no longer shown")
tap("act:detailq")
check(text("detail") == "detail id=8 tab=x y", "query argument decoded")
# launchSingleTop on the same destination replaces the arguments instead of pushing.
tap("act:single")
check(text("detail") == "detail id=9 tab=main" and text("stack=").count("detail") == 2, "single top")

# The back key pops; the saved `rememberSaveable` state of home comes back.
back(); back()
check(text("route=") == "route=home", "back key popped to home")
tap("home n=")
tap("home n=")
check(text("home n=") == "home n=2 result=null", "home state")
tap("act:detail")
back()
check(text("home n=") == "home n=2 result=null", "home state kept across a push and pop")

# navigateUp on the start destination does nothing (the activity would finish).
tap("act:up")
check(text("log=") == "log=up=false" and text("route=") == "route=home", "navigateUp at the root")

# A predictive back swipe: the pop transition follows the finger (both destinations shown), cancel restores.
tap("act:detail")
mid = swipe(0.4, commit=False)
mid_texts = texts(mid["root"])
check(any(s.startswith("home n=") for s in mid_texts) and any(s.startswith("detail") for s in mid_texts), "both shown mid-swipe")
check(text("route=").startswith("route=detail") and not any(s.startswith("home n=") for s in texts()), "cancelled swipe stays")
swipe(0.4, commit=True)
check(text("route=") == "route=home" and not any(s.startswith("detail") for s in texts()), "committed swipe popped")

# Bottom navigation pattern: popUpTo(start) { saveState } + restoreState keeps each tab's state.
tap("act:tabB")
tap("b n=")
check(text("b n=") == "b n=1" and text("stack=") == "stack=home,b", "tab b")
tap("act:tabA")
check(text("stack=") == "stack=home" and text("home n=") == "home n=2 result=null", "back to tab a")
tap("act:tabB")
check(text("b n=") == "b n=1", "tab b state restored")
tap("act:tabA")

# popBackStack(route) and its result; a missing route returns false.
tap("act:detail"); tap("act:detailq")
tap("act:popHome")
check(text("log=") == "log=pop=true" and text("stack=") == "stack=home", "popBackStack(route)")
tap("act:popMissing")
check(text("log=") == "log=pop=false", "popBackStack to a missing route")

# Unknown routes throw IllegalArgumentException.
tap("act:bad")
check(text("log=") == "log=bad", "unknown route")

# Results through the previous entry's savedStateHandle.
tap("act:detail")
tap("act:result")
check(text("home n=") == "home n=2 result=done", "result delivered")

# Nested graphs: navigating to the graph shows its start destination; popping the graph inclusive leaves it.
tap("act:nested")
check(text("route=") == "route=flow/one" and text("stack=") == "stack=home,flow,flow/one", "nested graph start")
tap("flow one")
tap("flow two")
check(text("stack=") == "stack=home", "graph popped inclusive")
# Leaving a nested graph and coming back into it pushes a new graph entry above the other destination.
tap("act:nested"); tap("act:detail"); tap("act:flowTwo")
check(text("stack=") == "stack=home,flow,flow/one,detail/{id}?tab={tab},flow,flow/two", "graph entry pushed again")
tap("act:popHome")

# Dialog destinations show over the content; back dismisses them.
tap("act:dialog", 0.4)
check("about dialog" in texts() and any(s.startswith("home n=") for s in texts()), "dialog over home")
back(0.4)
check("about dialog" not in texts(), "back dismissed the dialog")

# BackHandler composed after the NavHost's... still: the most recently added enabled handler wins over NavHost.
tap("act:detail")
tap("act:handler")
back()
check(text("log=") == "log=handler" and text("route=").startswith("route=detail"), "BackHandler takes back")
tap("act:handler")
back()
check(text("route=") == "route=home", "disabled BackHandler lets NavHost pop")

# PredictiveBackHandler: progress events, cancel, commit and the back key (an empty flow).
tap("act:predictive")
swipe(0.5, commit=False, edge="right")
check(text("log=") == "log=cancelled at 0.5", "predictive cancel")
mid = swipe(0.25, commit=True)
check(any(s == "log=progress=0.25 edge=0" for s in texts(mid["root"])), "progress with the left edge")
check(text("log=") == "log=committed after 0.25", "predictive commit")
back()
check(text("log=") == "log=committed after -1.0", "back key: the flow completes empty")
tap("act:predictive")

# Tabs: a selection slides the indicator (from, progress) and then rests on the new tab; each selection of a
# scrollable row is a new scroll position the host centres.
def indicators():
    return [m["tabIndicator"] for n in walk(last[0]["root"]) for m in n["props"].get("modifier", []) if "tabIndicator" in m]

def scrolls():
    return [n["props"]["scroll"] for n in walk(last[0]["root"]) if n["component"] == "TabRow" and "scroll" in n["props"]]

check([i["index"] for i in indicators()] == [0, 0] and all("from" not in i for i in indicators()), "indicators at rest")
check(scrolls() == [{"seq": 0}], f"scroll {scrolls()}")
tap("T2", 0.05)
mid = [i for i in indicators() if i["index"] == 2]
check(mid and mid[0]["match"] == "content" and mid[0]["from"][:2] == [0, 0] and 0 <= mid[0]["progress"] < 1, f"sliding {indicators()}")
settle(0.8)
check([i["index"] for i in indicators()][0] == 2 and all("from" not in i for i in indicators()), f"rested {indicators()}")
tap("S5", 0.05)
check(scrolls()[0]["seq"] == 1 and scrolls()[0].get("from") == 0, f"scroll animates from tab 0: {scrolls()}")
settle(0.8)
check(scrolls() == [{"seq": 1}], f"scroll rested {scrolls()}")

# Popping the start destination empties the back stack (the NavHost shows nothing), as in AndroidX.
tap("act:popAll")
check(text("log=") == "log=pop=true current=null" and not any(s.startswith("home n=") for s in texts()), "empty back stack")

# Back with nothing to handle it finishes the activity: the host hears of it.
back()
check("activity.finished" in events, f"activity finished event, events={events}")

assert not errors, errors
send(type="shutdown")
assert p.wait(timeout=10) == 0
print("compose navigation: ok")
