import json, os, subprocess, sys

def spawn(exe, env=None):
    return subprocess.Popen([exe], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

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
assert len(links) == 7 and all(l["kind"] == "button" and l["style"] == "navigationLink" for l in links)
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
# Presentation layers: alert, sheet (+detents, drag indicator, swipe dismiss, nesting), dialog, cover; Text runs; ButtonRole
layers = lambda t: t["root"]["layers"]
under = lambda t: t["root"]["content"]
btn_in = lambda n, label: next(m for m in walk(n) if m.get("kind") == "button" and label in alltexts(m))
assert t["root"]["kind"] == "navigation"  # nothing presented: no presentation wrapper
t = act(p, t, button(t, "Show alert")["action"])
assert t["root"]["kind"] == "presentation" and [l["kind"] for l in layers(t)] == ["alert"], t["root"]["kind"]
al = layers(t)[0]
assert al["title"] == "Reset volume?" and alltexts(al["message"]) == ["Volume goes back to 0."], al
assert [(alltexts(b)[0], b.get("role")) for b in al["actions"]] == [("Cancel", "cancel"), ("Reset", "destructive")], al["actions"]
t = act(p, t, al["actions"][0]["action"])  # Cancel dismisses, volume unchanged
assert t["root"]["kind"] == "navigation" and "Volume: 3" in alltexts(t["root"])
t = act(p, t, button(t, "Show alert")["action"])
t = act(p, t, layers(t)[0]["actions"][1]["action"])  # Reset runs its action, then dismisses
assert t["root"]["kind"] == "navigation" and "Volume: 0" in alltexts(t["root"]), alltexts(t["root"])
# plain sheet: large detent, no indicator, Text runs, Done = @Environment(\.dismiss)
t = act(p, t, button(t, "Show sheet")["action"])
sl = layers(t)[0]
assert [l["kind"] for l in layers(t)] == ["sheet"] and sl["detents"] == [] and sl["dragIndicator"] is False and sl.get("dismiss"), sl
txt = next(m for m in walk(sl["content"]) if m.get("kind") == "text" and m.get("runs"))
assert txt["text"] == "Volume is 0 of 10", txt
assert [(r["text"], r.get("fontWeight"), r.get("foregroundColor")) for r in txt["runs"]] == [
    ("Volume is ", None, None), ("0", "bold", None), (" of 10", None, {"semantic": "secondary"})], txt["runs"]
assert find(under(t), lambda n: n.get("kind") == "navigation")  # presenting screen stays underneath
t = act(p, t, btn_in(sl["content"], "Done")["action"])
assert t["root"]["kind"] == "navigation" and title(t) == "Controls", t["root"]["kind"]
# swipe-to-dismiss action clears the binding
t = act(p, t, button(t, "Show sheet")["action"])
t = act(p, t, layers(t)[0]["dismiss"]); assert t["root"]["kind"] == "navigation"
# half sheet: detents + drag indicator + italic/underline runs, nested sheet, state inside the sheet
t = act(p, t, button(t, "Show half sheet")["action"])
hs = layers(t)[0]
assert sorted(d["kind"] for d in hs["detents"]) == ["large", "medium"] and hs["dragIndicator"] is True and hs.get("dismiss"), hs
assert not any("presentationDetents" in m or "presentationDragIndicator" in m for n in walk(hs["content"]) for m in (n.get("modifiers") or []))
runs = next(m for m in walk(hs["content"]) if m.get("runs"))["runs"]
assert [(r["text"], bool(r.get("italic")), bool(r.get("underline"))) for r in runs] == [
    ("Drag ", False, False), ("down", True, False), (" to ", False, False), ("dismiss", False, True)], runs
t = act(p, t, btn_in(hs["content"], "Open another")["action"])
assert [l["kind"] for l in layers(t)] == ["sheet", "sheet"], layers(t)
assert "Volume is 7 of 10" in alltexts(layers(t)[1]["content"])
t = act(p, t, btn_in(layers(t)[1]["content"], "Done")["action"])  # inner dismiss: only the nested sheet goes
assert [l["kind"] for l in layers(t)] == ["sheet"], layers(t)
t = act(p, t, layers(t)[0]["dismiss"]); assert t["root"]["kind"] == "navigation"
# confirmation dialog: popover anchored to its source button; automatic Cancel; outside tap = the cancel action
src = button(t, "Show dialog")
t = act(p, t, src["action"])
dg = layers(t)[0]
assert dg["kind"] == "dialog" and dg["title"] == "Volume preset" and dg["titleVisible"] is True and alltexts(dg["message"]) == ["Pick a preset level."], dg
assert [(alltexts(b)[0], b.get("role")) for b in dg["actions"]] == [("Mute", "destructive"), ("Max", None), ("Cancel", "cancel")], dg["actions"]
assert dg["anchor"] == src["id"] and dg["dismiss"] == dg["actions"][2]["action"], (dg["anchor"], src["id"])
t = act(p, t, dg["dismiss"]); assert t["root"]["kind"] == "navigation" and "Volume: 0" in alltexts(t["root"])
t = act(p, t, button(t, "Show dialog")["action"])
t = act(p, t, layers(t)[0]["actions"][1]["action"])  # Max
assert t["root"]["kind"] == "navigation" and "Volume: 10" in alltexts(t["root"]), alltexts(t["root"])
# full screen cover
t = act(p, t, button(t, "Show cover")["action"])
cv = layers(t)[0]; assert cv["kind"] == "cover" and "Full screen" in alltexts(cv["content"])
t = act(p, t, btn_in(cv["content"], "Close")["action"]); assert t["root"]["kind"] == "navigation"
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
# Glass: .glassEffect(...) variants/shapes/tint and glass button styles reach the wire (protocol F)
t = act(p, t, button(t, "Glass")["action"]); assert title(t) == "Glass"
gl = [(m["modifiers"][-1]["glass"], m) for m in walk(t["root"]) if any("glass" in x for x in (m.get("modifiers") or []))]
assert [g["shape"] for g, _ in gl] == [{"shape": "capsule"}, {"shape": "circle"}, {"shape": "circle"}, {"shape": "circle"}, {"shape": "rect", "cornerRadius": 20.0}], gl
assert [g["variant"] for g, _ in gl] == ["regular", "regular", "regular", "clear", "regular"], gl
tint = gl[2][0]["tint"]; assert tint["r"] == 1.0 and tint["g"] < 0.3, tint
assert "tint" not in gl[1][0] and "interactive" not in gl[1][0]
assert [b["style"] for b in walk(t["root"]) if b.get("kind") == "button" and b["style"].startswith("glass")] == ["glass", "glassProminent"]
t = act(p, t, button(t, "Glass 0")["action"]); assert "Glass 1" in alltexts(t["root"])
t = act(p, t, t["root"]["back"]["action"]); assert title(t) == "Showcase"
# push/pop cycles: memory stays bounded
for i in range(300):
    t = act(p, t, button(t, "Shapes")["action"]); assert title(t) == "Shapes"
    t = act(p, t, t["root"]["back"]["action"]); assert title(t) == "Showcase"
    if i == 50: base = rss(p)
growth = rss(p) - base
print(f"showcase: ok, RSS growth over 250 push/pop cycles: {growth} KB")
assert growth < 4096, f"RSS grew {growth} KB"
finish(p)

# --- dataflow: value navigation, environment objects, lifecycle, dismiss, Group, List(data)
p = spawn(sys.argv[4])
assert recv(p)["type"] == "hello"
t = recv(p)
navs = lambda t: [m for m in walk(t["root"]) if m.get("kind") == "navigation"]
home = lambda t: navs(t)[0]
tx = lambda t: alltexts(home(t))
assert home(t)["title"] == "Dataflow" and "back" not in home(t)
for s in ["User: ada", "Appeared: 1", "Disappeared: 0", "Path: 0", "Type size: large", "Group A", "Group B"]:
    assert s in tx(t), (s, tx(t))  # onAppear fired exactly once (first pass + one re-eval)
links = home(t)["content"]["sections"][1]["rows"]
assert len(links) == 7 and [alltexts(l)[0] for l in links[:5]] == ["Alpha", "Number 2", "Lists", "Plain", "Orphan"], links  # Group flattened into the list
# a modifier on a multi-child Group lands on each child, not on a wrapping vstack
assert [(alltexts(l)[0], l["kind"], l["modifiers"]) for l in links[5:]] == [
    ("Group A", "text", [{"opacity": 0.5}]), ("Group B", "text", [{"opacity": 0.5}])], links[5:]
# value link -> destination(for:), env object on the pushed screen, root onDisappear
t = act(p, t, button(t, "Alpha")["action"])
assert home(t)["title"] == "Item Alpha" and home(t)["back"]["label"] == "Dataflow", home(t)
for s in ["Item: Alpha", "User: ada", "Counts: 1/1", "Depth: 1"]: assert s in tx(t), (s, tx(t))
t = act(p, t, button(t, "Rename")["action"])
assert "User: grace" in tx(t) and "Counts: 1/1" in tx(t), tx(t)  # mutation re-renders; no spurious lifecycle
t = act(p, t, button(t, "Dismiss")["action"])  # @Environment(\.dismiss)
for s in ["User: grace", "Appeared: 2", "Disappeared: 1", "Path: 0"]: assert s in tx(t), (s, tx(t))
# Int value, then a nested push, then back pops one at a time
t = act(p, t, button(t, "Number 2")["action"])
assert home(t)["title"] == "Number 2" and "Depth: 1" in tx(t), tx(t)
# destination link on top of a path-driven screen: pushes above it, back pops it first
t = act(p, t, button(t, "Detail")["action"])
assert home(t)["title"] == "Detail" and "Detail depth 1" in tx(t) and home(t)["back"]["label"] == "Number 2", tx(t)
t = act(p, t, home(t)["back"]["action"]); assert home(t)["title"] == "Number 2" and "Depth: 1" in tx(t)
t = act(p, t, button(t, "Nested")["action"])
assert home(t)["title"] == "Item Nested" and "Depth: 2" in tx(t) and home(t)["back"]["label"] == "Number 2", tx(t)
t = act(p, t, home(t)["back"]["action"]); assert home(t)["title"] == "Number 2" and "Depth: 1" in tx(t)
t = act(p, t, home(t)["back"]["action"])
for s in ["Path: 0", "Appeared: 3", "Disappeared: 2"]: assert s in tx(t), (s, tx(t))
# Enum value + List(data) in all three forms
t = act(p, t, button(t, "Lists")["action"]); assert home(t)["title"] == "Lists"
lists = [m for m in walk(home(t)) if m.get("kind") == "list"]
assert [[alltexts(r)[0] for s in l["sections"] for r in s["rows"]] for l in lists] == [
    ["Person Ada", "Person Grace"], ["Key x", "Key y"], ["Range 0", "Range 1", "Range 2"]], lists
t = act(p, t, home(t)["back"]["action"])
# value with no navigationDestination pushes nothing (SwiftUI only logs)
send(p, type="action", id=button(t, "Orphan")["action"], version=t["version"])  # nothing changes, so no new tree
# destination-style link in a path-bound stack pushes (one level only: the orphan left nothing behind)
t = act(p, t, button(t, "Plain")["action"])
assert home(t)["title"] == "Plain" and "Plain pushed" in tx(t) and home(t)["back"]["label"] == "Dataflow", home(t)
t = act(p, t, home(t)["back"]["action"]); assert home(t)["title"] == "Dataflow" and "back" not in home(t) and "Path: 0" in tx(t)
# [T] path binding
typed = lambda t: navs(t)[1]
assert typed(t)["title"] == "Typed" and "Typed depth: 0" in alltexts(typed(t))
t = act(p, t, button(t, "Seven")["action"])
assert typed(t)["title"] == "Typed 7" and "Typed 7 depth 1" in alltexts(typed(t)) and typed(t)["back"]["label"] == "Typed"
assert home(t)["title"] == "Dataflow"  # the other stack is untouched
t = act(p, t, typed(t)["back"]["action"])
assert typed(t)["title"] == "Typed" and "Typed depth: 0" in alltexts(typed(t))
finish(p)
# dynamicTypeSize follows the launch environment
p = spawn(sys.argv[4], {**os.environ, "SIMCORE_DYNAMIC_TYPE": "accessibility2"})
assert recv(p)["type"] == "hello"
assert "Type size: accessibility2" in tx(recv(p))
finish(p)
# --- presentation: item / presenting variants, onDismiss, locked sheets, detents
p = spawn(sys.argv[5])
assert recv(p)["type"] == "hello"
t = recv(p)
L = lambda t: t["root"].get("layers", [])
base = lambda t: t["root"]["content"] if t["root"]["kind"] == "presentation" else t["root"]
def tap(t, label): return act(p, t, next(m for m in walk(base(t)) if m.get("kind") == "button" and label in alltexts(m))["action"])
assert t["root"]["kind"] != "presentation" and "Dismissed: 0" in alltexts(t["root"])
t = tap(t, "Plain"); assert [l["kind"] for l in L(t)] == ["sheet"] and L(t)[0].get("dismiss")
t = tap(t, "Clear plain")  # app code clears the binding: onDismiss still fires
assert not L(t) and "Dismissed: 1" in alltexts(t["root"]), alltexts(t["root"])
t = tap(t, "Plain"); t = act(p, t, L(t)[0]["dismiss"])  # the swipe path
assert not L(t) and "Dismissed: 2" in alltexts(t["root"]), alltexts(t["root"])
t = tap(t, "Locked"); assert "dismiss" not in L(t)[0], L(t)[0]  # interactiveDismissDisabled: no swipe action
t = tap(t, "Tall")  # a second sheet from the same presenter stacks on top
assert [l["kind"] for l in L(t)] == ["sheet", "sheet"], L(t)
assert sorted((d["kind"], d.get("value")) for d in L(t)[1]["detents"]) == [("fraction", 0.4), ("height", 300.0), ("large", None)], L(t)[1]["detents"]
t = act(p, t, L(t)[1]["dismiss"]); t = tap(t, "Item")  # item: the sheet gets the item
assert [l["kind"] for l in L(t)] == ["sheet", "sheet"] and "Item 3" in alltexts(L(t)[1]["content"]), L(t)
t = act(p, t, L(t)[1]["dismiss"])  # swiping it away clears the item binding
assert [l["kind"] for l in L(t)] == ["sheet"], L(t)
t = tap(t, "Cover item"); assert L(t)[-1]["kind"] == "cover" and "Cover 5" in alltexts(L(t)[-1]["content"])
t = tap(t, "Alert")  # alert with presenting data, above the cover
assert [l["kind"] for l in L(t)][-2:] == ["cover", "alert"], L(t)
al = L(t)[-1]; assert alltexts(al["message"]) == ["Greeting Ada"] and [alltexts(b)[0] for b in al["actions"]] == ["Hi Ada"], al
t = act(p, t, al["actions"][0]["action"])
t = tap(t, "Dialog"); dg = L(t)[-1]
assert dg["kind"] == "dialog" and [alltexts(b)[0] for b in dg["actions"]] == ["Use 9", "Cancel"] and dg["titleVisible"] is False, dg
finish(p)
print("PASS")
