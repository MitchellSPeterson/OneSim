import json, subprocess, sys

def spawn(exe):
    return subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

def send(p, **m):
    p.stdin.write(json.dumps(m) + "\n"); p.stdin.flush()

def recv(p):
    return json.loads(p.stdout.readline())  # raises if app output corrupted the wire

def find(n, pred):
    if pred(n): return n
    for c in n.get("children", []) + ([n["label"]] if "label" in n else []):
        r = find(c, pred)
        if r: return r

def texts(n):
    out = [n["text"]] if n.get("kind") == "text" else []
    for c in n.get("children", []) + ([n["label"]] if "label" in n else []):
        out += texts(c)
    return out

def kids(n):
    out = list(n.get("children", []))
    for k in ("label", "child", "content"):
        if k in n: out.append(n[k])
    for sec in n.get("sections", []): out += sec["rows"]
    return out

def walk(n):
    yield n
    for c in kids(n): yield from walk(c)

def alltexts(n): return [m["text"] for m in walk(n) if m.get("kind") == "text"]

def rows(t):
    lst = next(m for m in walk(t["root"]) if m.get("kind") == "list")
    return [r for s in lst["sections"] for r in s["rows"]]

def button(t, label):
    return next(m for m in walk(t["root"]) if m.get("kind") == "button" and label in alltexts(m))

def act(p, t, id_):
    send(p, type="action", id=id_, version=t["version"]); return recv(p)

def title(t): return t["root"]["title"]

def rss(p):
    return int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(p.pid)]).strip())

def finish(p):
    send(p, type="shutdown")
    assert p.wait(timeout=5) == 0, "guest must exit 0 on shutdown"

# --- counter
p = spawn(sys.argv[1])
assert recv(p) == {"type": "hello", "protocol": 1}
t = recv(p); assert t["type"] == "tree" and "Count: 0" in texts(t["root"]), t
N = 1100
for i in range(1, N + 1):
    btn = find(t["root"], lambda n: n.get("kind") == "button")
    send(p, type="action", id=btn["action"], version=t["version"])
    t = recv(p)
    assert t["version"] == i + 1 and f"Count: {i}" in texts(t["root"]), t
    if i == 100: base = rss(p)
# stale version must be ignored: next tree comes only from the valid action below
send(p, type="action", id=btn["action"], version=t["version"] - 1)
send(p, type="action", id=find(t["root"], lambda n: n.get("kind") == "button")["action"], version=t["version"])
t2 = recv(p); assert f"Count: {N + 1}" in texts(t2["root"]), "stale action was not ignored"
send(p, type="environment", appearance="dark", viewport={"width": 100, "height": 200})
assert recv(p)["type"] == "tree"
growth = rss(p) - base
print(f"counter: {N} actions ok, RSS growth after warmup: {growth} KB")
assert growth < 4096, f"RSS grew {growth} KB"
finish(p)

# --- app entry: print() must go to stderr, not the wire
p = spawn(sys.argv[2])
assert recv(p)["type"] == "hello"
t = recv(p)
tog = find(t["root"], lambda n: n.get("kind") == "button" and "Toggle" in texts(n))
send(p, type="action", id=tog["action"], version=t["version"])
t = recv(p); assert "On" in texts(t["root"]), texts(t["root"])
p.stdin.close()  # EOF also exits 0
assert p.wait(timeout=5) == 0
assert "toggled true" in p.stderr.read(), "print() should reach stderr"
print("entry: ok, print() on stderr, EOF exit 0")

# --- showcase: navigation, observable objects, controls, text input
p = spawn(sys.argv[3])
assert recv(p)["type"] == "hello"
t = recv(p); r = t["root"]
assert r["kind"] == "navigation" and r["title"] == "Showcase" and "back" not in r, r
lst = r["content"]; assert lst["kind"] == "list"
assert [s.get("header") for s in lst["sections"]] == ["Screens", "Summary"]
links = lst["sections"][0]["rows"]
assert len(links) == 5 and all(l["kind"] == "button" and l["style"] == "navigationLink" for l in links)
assert "1" in alltexts(lst["sections"][1]["rows"][0])
t = act(p, t, button(t, "Todos")["action"])
assert title(t) == "Todos" and t["root"]["back"]["label"] == "Showcase", t["root"]
items = lambda t: next(s for s in next(m for m in walk(t["root"]) if m.get("kind") == "list")["sections"] if s.get("header") == "Items")["rows"]
assert len(items(t)) == 3
ship = items(t)[2]
t = act(p, t, ship["action"])
ship = items(t)[2]
assert any(m.get("modifiers") and {"strikethrough": True} in m["modifiers"] for m in walk(ship) if m.get("kind") == "text"), ship
t = act(p, t, t["root"]["back"]["action"])
assert title(t) == "Showcase" and "0" in alltexts(rows(t)[-1]), "ObservableObject state must persist across navigation"
t = act(p, t, button(t, "Todos")["action"])
assert items(t)[2] and len(items(t)) == 3
ship = items(t)[2]
assert any({"strikethrough": True} in (m.get("modifiers") or []) for m in walk(ship) if m.get("kind") == "text"), "done persisted"
add = button(t, "Add"); assert add["disabled"] is True
fld = next(m for m in walk(t["root"]) if m.get("kind") == "text_field")
send(p, type="text_input", field=fld["field"], version=t["version"], text="Buy milk")
t = recv(p)
fld = next(m for m in walk(t["root"]) if m.get("kind") == "text_field"); assert fld["text"] == "Buy milk"
add = button(t, "Add"); assert add["disabled"] is False
t = act(p, t, add["action"])
assert len(items(t)) == 4 and "Buy milk" in alltexts(items(t)[3])
assert next(m for m in walk(t["root"]) if m.get("kind") == "text_field")["text"] == ""
t = act(p, t, t["root"]["back"]["action"])
assert title(t) == "Showcase" and "1" in alltexts(rows(t)[-1]), alltexts(t["root"])  # only "Buy milk" is open
# Controls: toggle, stepper, fresh state on re-entry
def controls(t): return act(p, t, button(t, "Controls")["action"])
t = controls(t); assert title(t) == "Controls"
assert "You will be notified" in alltexts(t["root"])
tog = next(m for m in walk(t["root"]) if m.get("kind") == "toggle"); assert tog["isOn"] is True
t = act(p, t, tog["action"])
assert not next(m for m in walk(t["root"]) if m.get("kind") == "toggle")["isOn"] and "You will be notified" not in alltexts(t["root"])
stp = next(m for m in walk(t["root"]) if m.get("kind") == "stepper"); assert "Volume: 3" in alltexts(stp)
t = act(p, t, stp["increment"])
assert "Volume: 4" in alltexts(t["root"])
t = act(p, t, t["root"]["back"]["action"])
t = controls(t)
assert "You will be notified" in alltexts(t["root"]) and "Volume: 3" in alltexts(t["root"]), "popped screen state must be discarded"
t = act(p, t, t["root"]["back"]["action"])
# Typography + environment
t = act(p, t, button(t, "Typography")["action"])
assert "Light appearance" in alltexts(t["root"])
assert t["root"]["displayMode"] == "inline"
send(p, type="environment", appearance="dark", viewport={"width": 393, "height": 852}); t = recv(p)
assert "Dark appearance" in alltexts(t["root"]), alltexts(t["root"])
t = act(p, t, t["root"]["back"]["action"])
# Symbols: Image / Label / imageScale / aspect ratio / Font.weight all reach the wire
t = act(p, t, button(t, "Symbols")["action"]); assert title(t) == "Symbols"
secs = next(m for m in walk(t["root"]) if m.get("kind") == "list")["sections"]
labs = secs[0]["rows"]
assert [l["kind"] for l in labs] == ["label"] * 3, labs
assert labs[0]["title"]["text"] == "Favorites" and labs[0]["icon"] == {"kind": "image", "id": labs[0]["icon"]["id"], "systemName": "star.fill", "resizable": False}, labs[0]
assert any("foregroundColor" in m for m in labs[2]["modifiers"]), labs[2]
imgs = [m for m in walk(secs[1]["rows"][0]) if m.get("kind") == "image"]
assert [i["systemName"] for i in imgs] == ["heart.fill", "bell", "checkmark.circle.fill", "paperplane", "cloud.sun.fill"], imgs
assert {"imageScale": "large"} in imgs[2]["modifiers"], imgs[2]
assert imgs[4]["resizable"] is True and imgs[4]["contentMode"] == "fit", imgs[4]
assert {"font": {"size": 28.0, "weight": "bold"}} in imgs[3]["modifiers"], imgs[3]
chev = next(m for m in walk(secs[2]["rows"][0]) if m.get("kind") == "image" and m["systemName"] == "chevron.right")
assert {"font": {"style": "footnote", "weight": "semibold"}} in chev["modifiers"], chev
t = act(p, t, t["root"]["back"]["action"])
# push/pop cycles: memory stays bounded
for i in range(300):
    t = act(p, t, button(t, "Shapes")["action"]); assert title(t) == "Shapes"
    t = act(p, t, t["root"]["back"]["action"]); assert title(t) == "Showcase"
    if i == 50: base = rss(p)
growth = rss(p) - base
print(f"showcase: ok, RSS growth over 250 push/pop cycles: {growth} KB")
assert growth < 4096, f"RSS grew {growth} KB"
finish(p)
print("PASS")
