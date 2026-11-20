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
    for row in n.get("rows", []) or []:
        if isinstance(row, dict):
            out += row.get("cells", [])
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
assert len(links) == 8 and all(l["kind"] == "button" and l["style"] == "navigationLink" for l in links)
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
# Form: Form = inset list, SecureField, LabeledContent, Picker (menu + segmented), Slider, ProgressView
t = act(p, t, button(t, "Form")["action"]); assert title(t) == "Form"
lst = t["root"]["content"]; assert lst["kind"] == "list" and lst["style"] == "insetGrouped"
assert [s.get("header") for s in lst["sections"]] == ["Account", "Preferences"]
sec = next(m for m in walk(t["root"]) if m.get("kind") == "text_field" and m.get("secure"))
assert sec["text"] == "hunter2" and sec["placeholder"] == "Password", sec
assert ["Version", "1.0"] == alltexts(lst["sections"][0]["rows"][2]), alltexts(lst["sections"][0]["rows"][2])
ver = next(m for m in walk(lst["sections"][0]["rows"][2]) if m.get("text") == "1.0")
assert {"foregroundColor": {"semantic": "secondary"}} in ver["modifiers"], ver
pk = lambda t, style: next(m for m in walk(t["root"]) if m.get("kind") == "picker" and m["style"] == style)
fl = pk(t, "menu")
assert fl["label"]["text"] == "Flavor" and [o["text"] for o in fl["options"]] == ["Vanilla", "Chocolate", "Strawberry"] and fl["selected"] == 0, fl
assert "layers" not in t["root"]
t = act(p, t, fl["open"])
mn = t["root"]["layers"][0]; assert mn["kind"] == "menu" and mn["anchor"] == fl["id"], mn
assert [(i["text"], bool(i.get("checked"))) for i in mn["items"]] == [("Vanilla", True), ("Chocolate", False), ("Strawberry", False)], mn
t = act(p, t, mn["dismiss"]); assert t["root"]["kind"] == "navigation" and pk(t, "menu")["selected"] == 0
t = act(p, t, pk(t, "menu")["open"])
t = act(p, t, t["root"]["layers"][0]["items"][1]["action"])
assert t["root"]["kind"] == "navigation" and pk(t, "menu")["selected"] == 1, "menu item selects and closes"
sg = pk(t, "segmented"); assert [o["text"] for o in sg["options"]] == ["Small", "Medium", "Large"] and sg["selected"] == 1, sg
t = act(p, t, sg["options"][2]["action"]); assert pk(t, "segmented")["selected"] == 2
sl = next(m for m in walk(t["root"]) if m.get("kind") == "slider")
assert (sl["value"], sl["min"], sl["max"], sl["step"]) == (4.0, 0.0, 10.0, 1.0), sl
send(p, type="text_input", field=sl["field"], version=t["version"], text="7.4"); t = recv(p)
sl = next(m for m in walk(t["root"]) if m.get("kind") == "slider"); assert sl["value"] == 7.0, "slider snaps to step"
send(p, type="text_input", field=sl["field"], version=t["version"], text="99"); t = recv(p)
sl = next(m for m in walk(t["root"]) if m.get("kind") == "slider"); assert sl["value"] == 10.0, "slider clamps"
assert "10" in alltexts(t["root"])
pv = [m for m in walk(t["root"]) if m.get("kind") == "progress"]
assert [m["value"] for m in pv] == [1.0, None], pv
t = act(p, t, t["root"]["back"]["action"])
t = act(p, t, button(t, "Form")["action"])
assert pk(t, "menu")["selected"] == 0 and pk(t, "segmented")["selected"] == 1, "fresh state on re-entry"
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
# onChange (all forms, initial, state-dirtying), onTapGesture, @AppStorage, Lazy stacks
t = act(p, t, button(t, "Reset store")["action"])  # @AppStorage persists in UserDefaults across runs: start clean
rx = lambda t: alltexts(t["root"])
for s in ["Count: 0", "Log: c", "Tapped: 0", "Flag: off", "Name: none", "Launches: 0", "Mirror: 0"]: assert s in rx(t), (s, rx(t))
t = act(p, t, button(t, "Bump")["action"])
assert "Count: 1" in rx(t) and "Log: c,c,0>1" in rx(t), rx(t)  # initial fired once; then both forms fire on change
t = act(p, t, button(t, "Bump")["action"])  # the perform form sets count = 20 -> re-eval fires the others again
assert "Count: 20" in rx(t) and "Mirror: 200" in rx(t) and "Log: c,c,0>1,c,1>2,c,2>20" in rx(t), rx(t)
t = act(p, t, button(t, "Bump")["action"]); assert "Count: 21" in rx(t) and "Log: c,c,0>1,c,1>2,c,2>20,c,20>21" in rx(t), rx(t)
tap = button(t, "Tap me"); assert tap["style"] == "plain" and alltexts(tap) == ["Tap me"], tap  # plain button node: host hit testing, no accent tint
t = act(p, t, tap["action"]); assert "Tapped: 1" in rx(t), rx(t)
t = act(p, t, button(t, "Store")["action"]); assert all(s in rx(t) for s in ["Launches: 1", "Flag: on", "Name: set"]), rx(t)
finish(p)
p = spawn(sys.argv[4]); assert recv(p)["type"] == "hello"; t = recv(p)  # UserDefaults.standard survived the restart
assert all(s in rx(t) for s in ["Launches: 1", "Flag: on", "Name: set"]), rx(t)
t = act(p, t, button(t, "Reset store")["action"]); assert all(s in rx(t) for s in ["Launches: 0", "Flag: off", "Name: none"]), rx(t)
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

# --- tabs: only the selected tab is sent; each tab's state survives switching; NavigationStack inside a tab
p = spawn(sys.argv[6]); assert recv(p)["type"] == "hello"; t = recv(p)
tv = lambda t: t["root"]
def tab(t, i): return act(p, t, tv(t)["tabs"][i]["action"])
assert tv(t)["kind"] == "tabview" and tv(t)["selected"] == 0, tv(t)
assert [(x["title"], x.get("systemImage"), x.get("badge")) for x in tv(t)["tabs"]] == [("Home", "house", None), ("Counter", "star", None), ("Settings", "gearshape", "3")], tv(t)["tabs"]
assert tv(t)["content"]["kind"] == "navigation" and "Count: 0" not in alltexts(tv(t)), "only the selected tab is evaluated"
t = act(p, t, button(t, "Apple")["action"]); assert "You picked Apple" in alltexts(tv(t)), alltexts(tv(t))
t = tab(t, 1); assert tv(t)["selected"] == 1 and "Count: 0" in alltexts(tv(t))
t = act(p, t, button(t, "Increment")["action"]); t = act(p, t, button(t, "Increment")["action"])
assert "Count: 2" in alltexts(tv(t))
t = tab(t, 2); assert tv(t)["selected"] == 2 and tv(t)["content"]["title"] == "Settings"
t = tab(t, 1); assert "Count: 2" in alltexts(tv(t)), "tab @State survives a switch"
t = tab(t, 0); assert "You picked Apple" in alltexts(tv(t)) and tv(t)["content"]["back"], "pushed screen survives a switch"
finish(p)

# --- Tab(_:systemImage:value:) with an enum selection, string badge
p = spawn(sys.argv[7]); assert recv(p)["type"] == "hello"; t = recv(p)
assert tv(t)["selected"] == 1 and [(x["title"], x.get("badge")) for x in tv(t)["tabs"]] == [("One", None), ("Two", "New")], tv(t)
t = act(p, t, button(t, "two 0")["action"]); assert "two 1" in alltexts(tv(t))
t = tab(t, 0); assert tv(t)["selected"] == 0 and "one 0" in alltexts(tv(t))
t = tab(t, 1); assert "two 1" in alltexts(tv(t))
finish(p)

# --- gestures (protocol section L): touches at gesture keys drive recognizers, composition and arbitration
p = spawn(sys.argv[8]); assert recv(p)["type"] == "hello"; t = recv(p)
F = [100, 200, 300, 40]  # every target's frame: local = point - (100, 200)
def gview(t, prefix):
    return next(m for m in walk(t["root"]) if any("gesture" in x for x in m.get("modifiers", [])) and any(s.startswith(prefix) for s in alltexts(m)))
def gkeys(t, prefix): return [x["gesture"]["key"] for x in gview(t, prefix)["modifiers"] if "gesture" in x]
def touch(keys, phase, time, *pts):
    send(p, type="gesture", phase=phase, time=time, points=[list(q) for q in pts], targets=[{"key": k, "frame": F} for k in keys])
    return recv(p)  # every touch event is answered with a tree
def txt(t, prefix): return next(s for s in alltexts(t["root"]) if s.startswith(prefix))
def taps(keys, time, at=(130, 215)):
    touch(keys, "start", time, at); return touch(keys, "end", time + 50, at)

k = gkeys(t, "Drag ")
assert [x["gesture"]["pans"] for x in gview(t, "Drag ")["modifiers"] if "gesture" in x] == [True]
assert [x["gesture"]["pans"] for x in gview(t, "Double ")["modifiers"] if "gesture" in x] == [False]
t = touch(k, "start", 0, (110, 210)); assert txt(t, "Drag ") == "Drag 0 total 0 global 110", txt(t, "Drag ")
t = touch(k, "move", 16, (115, 210)); assert txt(t, "Drag ") == "Drag 0 total 0 global 115", "under minimumDistance: no drag yet"
t = touch(k, "move", 32, (140, 210)); assert txt(t, "Drag ") == "Drag 30 total 0 global 140", txt(t, "Drag ")
t = touch(k, "end", 48, (150, 210)); assert txt(t, "Drag ") == "Drag 0 total 40 global 140", "@GestureState resets, onEnded adds: " + txt(t, "Drag ")
touch(k, "start", 100, (110, 210)); t = touch(k, "move", 116, (150, 210)); assert txt(t, "Drag ").startswith("Drag 40 total 40")
t = touch(k, "cancel", 132, (150, 210)); assert txt(t, "Drag ").startswith("Drag 0 total 40"), "a cancelled drag never ends: " + txt(t, "Drag ")

k = gkeys(t, "Long "); assert len(k) == 2, k
t = touch(k, "start", 1000, (120, 210)); assert txt(t, "Long ") == "Long 0 pressing yes held yes", txt(t, "Long ")
t = touch(k, "move", 1300, (122, 211)); assert txt(t, "Long ") == "Long 0 pressing yes held yes"
t = touch(k, "move", 1600, (122, 211)); assert txt(t, "Long ") == "Long 1 pressing no held yes", "fires while held: " + txt(t, "Long ")
t = touch(k, "end", 1700, (122, 211)); assert txt(t, "Long ") == "Long 1 pressing no held no", txt(t, "Long ")
touch(k, "start", 3000, (120, 210)); t = touch(k, "end", 3100, (120, 210)); assert txt(t, "Long ") == "Long 1 pressing no held no", "too short"
touch(k, "start", 4000, (120, 210)); t = touch(k, "move", 4100, (140, 210)); assert txt(t, "Long ") == "Long 1 pressing no held no", "moved away"
touch(k, "end", 4700, (140, 210))

k = gkeys(t, "Double ")
t = taps(k, 5000); assert txt(t, "Double ") == "Double 0"
t = taps(k, 5150); assert txt(t, "Double ") == "Double 1", txt(t, "Double ")
taps(k, 6000); t = taps(k, 7000); assert txt(t, "Double ") == "Double 1", "taps too far apart"

k = gkeys(t, "Tap at"); t = taps(k, 8000); assert txt(t, "Tap at") == "Tap at 30,15", txt(t, "Tap at")

k = gkeys(t, "Pinch ")
t = touch(k, "start", 9000, (150, 220), (250, 220)); assert txt(t, "Pinch ").startswith("Pinch 100 turn 0")
t = touch(k, "move", 9016, (100, 220), (300, 220)); assert txt(t, "Pinch ").startswith("Pinch 200 turn 0"), txt(t, "Pinch ")
t = touch(k, "move", 9032, (200, 170), (200, 270)); assert txt(t, "Pinch ").startswith("Pinch 100 turn 90"), txt(t, "Pinch ")
t = touch(k, "end", 9048, (200, 170), (200, 270)); assert txt(t, "Pinch ") == "Pinch 100 turn 90 done", txt(t, "Pinch ")

import math
k = gkeys(t, "Pinch ")  # past 180 degrees the rotation keeps counting
touch(k, "start", 9100, (150, 220), (250, 220))
for i in range(1, 7):
    a = math.radians(45 * i); t = touch(k, "move", 9100 + 16 * i, (200 - 50 * math.cos(a), 220 - 50 * math.sin(a)), (200 + 50 * math.cos(a), 220 + 50 * math.sin(a)))
assert txt(t, "Pinch ").startswith("Pinch 100 turn 270"), txt(t, "Pinch ")
touch(k, "end", 9300, (200, 270), (200, 170))
k = gkeys(t, "Seq ")
t = touch(k, "start", 10000, (120, 210)); assert txt(t, "Seq ") == "Seq pressing", txt(t, "Seq ")
t = touch(k, "move", 10400, (120, 210)); assert txt(t, "Seq ") == "Seq dragging 0", txt(t, "Seq ")
t = touch(k, "move", 10450, (145, 210)); assert txt(t, "Seq ") == "Seq dragging 25", txt(t, "Seq ")
t = touch(k, "end", 10500, (145, 210)); assert txt(t, "Seq ") == "Seq done", txt(t, "Seq ")

k = gkeys(t, "Excl ")
touch(k, "start", 11000, (120, 210)); touch(k, "move", 11016, (150, 210)); t = touch(k, "end", 11032, (150, 210))
assert txt(t, "Excl ") == "Excl drag", txt(t, "Excl ")
taps(k, 12000); t = taps(k, 12100); assert txt(t, "Excl ") == "Excl double", txt(t, "Excl ")

k = gkeys(t, "Nested "); assert len(k) == 4, k  # child tap, parent tap, high-priority drag, simultaneous tap
t = taps(k, 13000); assert txt(t, "Nested ") == "Nested cs", "child beats parent, simultaneous joins: " + txt(t, "Nested ")
touch(k, "start", 14000, (120, 210)); touch(k, "move", 14016, (150, 210)); t = touch(k, "end", 14032, (150, 210))
assert txt(t, "Nested ") == "Nested csH", "high priority wins once it commits: " + txt(t, "Nested ")

k = gkeys(t, "Custom "); taps(k, 15000); t = taps(k, 15100); assert txt(t, "Custom ") == "Custom 1", txt(t, "Custom ")

k = gkeys(t, "Masked "); assert len(k) == 1, "including: .subviews drops the parent's own gesture"
t = taps(k, 16000); assert txt(t, "Masked ") == "Masked c", txt(t, "Masked ")
k = gkeys(t, "Combo "); assert len(k) == 2, k  # inner drag leads, outer long press
t = touch(k, "start", 18000, (120, 210)); assert txt(t, "Combo ") == "Combo +", "pressing shows while the drag leads: " + txt(t, "Combo ")
t = touch(k, "move", 18400, (121, 210)); assert txt(t, "Combo ") == "Combo +L", "a long press under a still drag fires while held: " + txt(t, "Combo ")
touch(k, "end", 18500, (121, 210))
touch(k, "start", 19000, (120, 210)); touch(k, "move", 19016, (160, 210)); t = touch(k, "end", 19032, (160, 210))
assert txt(t, "Combo ") == "Combo +L+d", txt(t, "Combo ")
only = gview(t, "Only ")
assert len(gkeys(t, "Only ")) == 1 and not any(m.get("kind") == "button" for m in walk(only)), "including: .gesture drops subview gestures and taps"
t = taps(gkeys(t, "Only "), 20000); assert txt(t, "Only ") == "Only p", txt(t, "Only ")
assert not any("gesture" in x for m in walk(t["root"]) if "Hidden 0" in alltexts(m) for x in m.get("modifiers", [])), "isEnabled: false"
t = taps(gkeys(t, "Either "), 21000); assert txt(t, "Either ") == "Either 1", "only the chosen branch runs: " + txt(t, "Either ")
assert not any("gesture" in x for m in walk(t["root"]) if "Off 0" in alltexts(m) for x in m.get("modifiers", [])), "disabled views take no gestures"
t = touch(["stale#g0"], "start", 17000, (1, 1)); t = touch(["stale#g0"], "end", 17050, (1, 1))  # unknown keys: no-op
finish(p)

# --- layout (protocol section M). The layout example app is not required here.
p = spawn(sys.argv[9]); assert recv(p)["type"] == "hello"; t = recv(p)
kinds = {m.get("kind") for m in walk(t["root"])}
assert {"grid", "lazyGrid", "geometry", "viewThatFits"} <= kinds, kinds
geo = next(m for m in walk(t["root"]) if m.get("kind") == "geometry")
prop = geo.get("proposal")
assert isinstance(prop, list) and len(prop) == 2 and all(isinstance(x, (int, float)) and not isinstance(x, bool) for x in prop), geo
send(p, type="action", id=button(t, "Jump")["action"], version=t["version"])
line = recv(p)
assert line["type"] == "scroll_to_id" and isinstance(line.get("scroll"), str) and line["scroll"] and line["target"] == "there", line
finish(p)
print("PASS")
