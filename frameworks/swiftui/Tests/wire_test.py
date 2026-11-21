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
    out = list(n.get("children", [])) + [page["content"] for page in n.get("pages", [])]
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
assert {"foregroundColor": {"semantic": "secondary", "hierarchical": True}} in ver["modifiers"], ver
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

# --- Tab(_:systemImage:value:) and tabBarOnly with an enum selection and text badge
p = spawn(sys.argv[7]); assert recv(p)["type"] == "hello"; t = recv(p)
assert tv(t)["selected"] == 1 and [(x["title"], x.get("badge")) for x in tv(t)["tabs"]] == [("One", None), ("Two", "New"), ("Lifecycle", None)], tv(t)
t = act(p, t, button(t, "two 0")["action"]); assert "two 1" in alltexts(tv(t))
t = tab(t, 0); assert tv(t)["selected"] == 0 and "one 0" in alltexts(tv(t))
t = tab(t, 1); assert "two 1" in alltexts(tv(t))
t = tab(t, 2)
assert tv(t)["kind"] == "tabview" and "visits 0, sidebar false" in alltexts(tv(t))
pages = next(m for m in walk(tv(t)) if m["kind"] == "pagetabview")
t = act(p, t, pages["pages"][1]["action"])
assert tv(t)["kind"] == "presentation" and tv(t)["layers"][0]["title"] == "Second page alert"
assert "visits 1, sidebar false" in alltexts(tv(t))
finish(p)

# --- page selection works in both directions and survives leaving its enclosing tab
p = spawn(sys.argv[15]); assert recv(p)["type"] == "hello"; t = recv(p)
def pager(t): return next(m for m in walk(t["root"]) if m["kind"] == "pagetabview")
assert [item.get("badge") for item in tv(t)["tabs"]] == [None, "New", "12"]
assert pager(t)["selected"] == 0 and pager(t)["indexBackground"] == "always"
assert "Page 1 of 3" in alltexts(tv(t)) and len(pager(t)["pages"]) == 3
t = act(p, t, button(t, "Next")["action"])
assert pager(t)["selected"] == 1 and "Page 2 of 3" in alltexts(tv(t))
t = act(p, t, pager(t)["pages"][2]["action"])
assert pager(t)["selected"] == 2 and "Page 3 of 3" in alltexts(tv(t))
t = tab(t, 1); assert "Inbox" in alltexts(tv(t)) and not any(m["kind"] == "pagetabview" for m in walk(tv(t)))
t = tab(t, 0); assert pager(t)["selected"] == 2 and "Page 3 of 3" in alltexts(tv(t))
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

# --- animation & transitions (protocol section P).
def recv_tree(p):
    """Read until the next `tree` message, returning `(tree, schedules)`."""
    scheds = []
    while True:
        m = recv(p)
        if m["type"] == "tree":
            return m, scheds
        if m["type"] == "schedule":
            scheds.append(m.get("afterMs"))

env = dict(os.environ, SIMCORE_TIME="1700000000")
p = subprocess.Popen([sys.argv[12]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
assert recv(p)["type"] == "hello"
t, scheds = recv_tree(p)
# One-shot keyframeAnimator(repeating: false) runs from launch (16ms schedule); Periodic/Animation/EveryMinute schedules pass.
assert scheds and scheds[-1] <= 20.0, scheds
assert "tl:0" in alltexts(t["root"]) and "ct:numeric" in alltexts(t["root"]) and "sched:ok" in alltexts(t["root"]), alltexts(t["root"])
assert "phase:0" in alltexts(t["root"]) and "kf:0,0" in alltexts(t["root"]), alltexts(t["root"])
oneshot_node = next(n for n in walk(t["root"]) if n.get("text") == "oneshot")
assert any(m.get("offset", {}).get("x") == 0 for m in oneshot_node.get("modifiers", [])), oneshot_node

# @Namespace + matchedGeometryEffect: Src and Dst share the same namespace string.
mgs = [m["matchedGeometry"] for n in walk(t["root"]) for m in n.get("modifiers", []) if "matchedGeometry" in m]
assert len(mgs) == 2 and mgs[0]["namespace"] == mgs[1]["namespace"] and mgs[0]["isSource"] is True and mgs[1]["isSource"] is False and mgs[1]["properties"] == "position", mgs

# withAnimation(.linear(duration: 0.2)) + completion + asymmetric & custom Transition.
send(p, type="action", id=button(t, "Toggle")["action"], version=t["version"])
t, scheds = recv_tree(p)
assert scheds and scheds[-1] <= 205.0, scheds
root_anims = [m["animation"] for m in t["root"].get("modifiers", []) if "animation" in m]
assert len(root_anims) == 1 and root_anims[0]["curve"] == "linear" and root_anims[0]["duration"] == 0.2 and root_anims[0]["token"] >= 1, root_anims
trs = [m["transition"] for n in walk(t["root"]) for m in n.get("modifiers", []) if "transition" in m]
assert len(trs) == 2, trs
assert trs[0]["insertion"]["edge"] == "leading" and trs[0]["insertion"]["opacity"] == 0 and trs[0]["removal"]["scale"] == 0.5, trs[0]
assert trs[1]["insertion"]["opacity"] == 0 and trs[1]["insertion"]["offset"] == [-20, 0] and trs[1]["removal"]["offset"] == [20, 0], trs[1]

# Advance clock to 200 ms -> completion fires ONCE even though two @State props mutated (done:1, multi:1), and one-shot keyframeAnimator finishes at offset.x == 40.
send(p, type="tick", time=200.0)
t, _ = recv_tree(p)
assert any("done:1 multi:1" in s for s in alltexts(t["root"])), alltexts(t["root"])
oneshot_node = next(n for n in walk(t["root"]) if n.get("text") == "oneshot")
assert any(m.get("offset", {}).get("x") == 40 for m in oneshot_node.get("modifiers", [])), oneshot_node

# onChange calling withAnimation consumes its transaction on the same submission without leaking to the next action.
send(p, type="action", id=button(t, "OnChg")["action"], version=t["version"])
t, _ = recv_tree(p)
assert any("oc:1" in s for s in alltexts(t["root"])), alltexts(t["root"])
oc_anims = [m["animation"] for m in t["root"].get("modifiers", []) if "animation" in m]
assert len(oc_anims) == 1 and oc_anims[0]["duration"] == 0.25, oc_anims
send(p, type="action", id=button(t, "Other")["action"], version=t["version"])
t, _ = recv_tree(p)
assert not any("animation" in m for m in t["root"].get("modifiers", [])), "onChange transaction must not leak"

# .animation(_:value:) fires ONLY when `count` changes, NOT when `other` changes; .animation(nil, value:) emits null.
send(p, type="action", id=button(t, "Inc")["action"], version=t["version"])
t, _ = recv_tree(p)
cnt_node = next(n for n in walk(t["root"]) if n.get("text") == "Count:1")
assert any("animation" in m and m["animation"]["curve"] == "easeInOut" for m in cnt_node.get("modifiers", [])), cnt_node
assert any(m.get("contentTransition") == "numericTextUp" for m in cnt_node.get("modifiers", [])), cnt_node
sup_node = next(n for n in walk(t["root"]) if n.get("text") == "Suppressed:1")
assert any("animation" in m and m["animation"] is None for m in sup_node.get("modifiers", [])), sup_node
# One-shot keyframeAnimator must hold its final value (40) on subsequent re-renders instead of snapping back to 0.
oneshot_node = next(n for n in walk(t["root"]) if n.get("text") == "oneshot")
assert any(m.get("offset", {}).get("x") == 40 for m in oneshot_node.get("modifiers", [])), oneshot_node

send(p, type="action", id=button(t, "Other")["action"], version=t["version"])
t, _ = recv_tree(p)
cnt_node = next(n for n in walk(t["root"]) if n.get("text") == "Count:1")
assert not any("animation" in m for m in cnt_node.get("modifiers", [])), "unchanged value must not re-animate"

# CustomAnimation + TransactionKey via withTransaction + .transaction(_:) reading the active transaction.
send(p, type="action", id=button(t, "Tx")["action"], version=t["version"])
t, _ = recv_tree(p)
assert any("tx:7" in s for s in alltexts(t["root"])), alltexts(t["root"])
root_anims = [m["animation"] for m in t["root"].get("modifiers", []) if "animation" in m]
assert len(root_anims) == 1 and "samples" in root_anims[0]["curve"], root_anims
txmod_node = next(n for n in walk(t["root"]) if n.get("text") == "txmod:7")
assert any("animation" in m and m["animation"] is None for m in txmod_node.get("modifiers", [])), txmod_node

# PhaseAnimator + .phaseAnimator modifier: trigger advances 0 -> 1 (pm opacity 0.5) immediately, then 1 -> 2 at +100ms, then 2 -> 0 at +200ms and stops.
send(p, type="action", id=button(t, "Phase")["action"], version=t["version"])
t, _ = recv_tree(p)
assert "phase:1" in alltexts(t["root"]), alltexts(t["root"])
pm_node = next(n for n in walk(t["root"]) if n.get("text") == "pm")
assert any(m.get("opacity") == 0.5 for m in pm_node.get("modifiers", [])), pm_node
send(p, type="tick", time=300.0)
t, _ = recv_tree(p)
assert "phase:2" in alltexts(t["root"]), alltexts(t["root"])
send(p, type="tick", time=400.0)
t, _ = recv_tree(p)
assert "phase:0" in alltexts(t["root"]), alltexts(t["root"])

# KeyframeAnimator: trigger at t=400ms; at t=500ms (halfway through 0.2s) x == 50 and y between 10 and 50; at t=600ms x == 100, y == 50.
send(p, type="action", id=button(t, "Keyframe")["action"], version=t["version"])
t, _ = recv_tree(p)
send(p, type="tick", time=500.0)
t, _ = recv_tree(p)
kf_mid = next(s for s in alltexts(t["root"]) if s.startswith("kf:"))
assert kf_mid.startswith("kf:50,"), kf_mid
send(p, type="tick", time=600.0)
t, _ = recv_tree(p)
assert "kf:100,50" in alltexts(t["root"]), alltexts(t["root"])

# TimelineView(.explicit): advance to 1000ms -> tl:1 (and kf holds 100,50!), advance to 2000ms -> tl:2 and schedule clears (afterMs absent).
send(p, type="tick", time=1000.0)
t, _ = recv_tree(p)
assert "tl:1" in alltexts(t["root"]) and "kf:100,50" in alltexts(t["root"]), alltexts(t["root"])
send(p, type="tick", time=2000.0)
t, _ = recv_tree(p)
assert "tl:2" in alltexts(t["root"]), alltexts(t["root"])
sched = recv(p)
assert sched["type"] == "schedule" and "afterMs" not in sched, f"exhausted timeline should stop scheduling: {sched}"
finish(p)

# --- shapes & paths (section N)
p = spawn(sys.argv[10])
assert recv(p) == {"type": "hello", "protocol": 1}
t = recv(p)
assert alltexts(t["root"]) == ["OK"], f"ShapeLab path checks failed: {alltexts(t['root'])}"
ch = t["root"]["children"]
# 1: Ellipse emits real "ellipse"
assert ch[1]["kind"] == "shape" and ch[1]["shape"] == "ellipse", ch[1]
# 2: UnevenRoundedRectangle chained .fill().strokeBorder() -> stroke on top, fill in background modifier
assert ch[2]["shape"] == "unevenRoundedRectangle" and ch[2]["cornerRadii"] == [20, 8, 16, 4], ch[2]
st = ch[2]["stroke"]
assert st["lineWidth"] == 4 and st["lineCap"] == "round" and st["lineJoin"] == "bevel" and st["dash"] == [8, 4] and st["dashPhase"] == 2 and st["inset"] == 2, st
bg = next(m["background"] for m in ch[2]["modifiers"] if "background" in m)
assert bg["fillStyle"] == {"eoFill": True, "antialiased": False}, bg
# 3: containerShape modifier + ContainerRelativeShape + ConcentricRectangle
assert any("containerShape" in m for m in ch[3]["modifiers"]), ch[3]["modifiers"]
assert ch[3]["children"][0]["shape"] == "containerRelative"
assert ch[3]["children"][1]["shape"] == "concentricRectangle"
# 4: Custom Shape (Diamond) starts with custom=True and no path yet; layout message populates exact path
diamond = ch[4]
assert diamond["custom"] is True and "path" not in diamond, diamond
# 5: FixedBadge custom sizeThatFits emits sizeHint fixed [48, 32]
assert ch[5]["sizeHint"] == {"fixed": [48, 32]}, ch[5]
# 6: AnyShape(Circle()) stays declarative circle
assert ch[6]["shape"] == "circle", ch[6]
# 7, 8: Composed trimmed/rotated & boolean shapes emit custom=True
assert ch[7]["custom"] is True and ch[8]["custom"] is True
# 9: Foreground-styled strokeBorder omits explicit stroke color so host inherits foregroundStyle
assert "color" not in ch[9]["stroke"] and ch[9]["stroke"]["inset"] == 2, ch[9]
# 10: Pre-inset shape + strokeBorder accumulates both insets (4 + 6/2 = 7)
assert ch[10]["custom"] is True, ch[10]
# 11: ShapeView foreground stroke overload omits stroke color
assert "color" not in ch[11]["stroke"] and ch[11]["stroke"]["lineWidth"] == 2, ch[11]
assert ch[12]["sizeHint"] == {"fixed": [100, 100]}, ch[12]
assert ch[13]["sizeHint"] == "square", ch[13]
# Send layout frames for custom shapes and check guest evaluates path(in:) at the exact sizes
send(p, type="layout", frames={diamond["id"]: [0, 0, 80, 60], ch[7]["id"]: [0, 0, 60, 60], ch[8]["id"]: [0, 0, 60, 60], ch[10]["id"]: [0, 0, 40, 40]})
t2 = recv(p)
d2 = t2["root"]["children"][4]
assert d2["pathSize"] == [80, 60], d2
assert d2["path"] == [{"m": [40, 0]}, {"l": [80, 30]}, {"l": [40, 60]}, {"l": [0, 30]}, "z"], d2["path"]
assert len(t2["root"]["children"][7]["path"]) > 2 and len(t2["root"]["children"][8]["path"]) > 4
# Pre-inset Rectangle(inset: 4).strokeBorder(lineWidth: 6) on 40x40 => rect [7..33, 7..33]
assert t2["root"]["children"][10]["path"] == [{"m": [7, 7]}, {"l": [33, 7]}, {"l": [33, 33]}, {"l": [7, 33]}, "z"], t2["root"]["children"][10]["path"]
finish(p)

# --- type (protocol section O)
p = spawn(sys.argv[11]); assert recv(p)["type"] == "hello"; t = recv(p)
hello = next(m for m in walk(t["root"]) if m.get("text") == "Hello")
assert any(m.get("textCase") == "uppercase" for m in hello.get("modifiers", [])), hello
joined = next(m for m in walk(t["root"]) if m.get("text") == "AB")
runs = joined["runs"]
assert runs[0].get("italic") is True and runs[1].get("kerning") == 2, joined
dotted = next(m for m in walk(t["root"]) if m.get("text") == "DotEnd")
assert dotted["runs"][0].get("underline") is True and dotted["runs"][0].get("underlinePattern") == "dot", dotted
fox = next(m for m in walk(t["root"]) if isinstance(m.get("text"), str) and m["text"].startswith("The quick"))
mm = fox.get("modifiers", [])
assert any(m.get("truncationMode") == "middle" for m in mm), mm
assert any(isinstance(m.get("lineLimit"), dict) and m["lineLimit"].get("max") == 1 for m in mm), mm
star = next(m for m in walk(t["root"]) if m.get("kind") == "image" and m.get("systemName") == "star.fill")
assert star.get("symbolRenderingMode") == "hierarchical", star
assert any(m.get("kind") == "empty" for m in walk(t["root"]))
bm = next(m for m in walk(t["root"]) if m.get("kind") == "image" and "bitmap" in m)
assert bm["bitmap"]["path"].endswith("onesim-typelab.png"), bm
assert bm["bitmap"]["width"] == 8 and bm["bitmap"]["height"] == 8, bm
finish(p)

# --- state and environment (#47)
p = spawn(sys.argv[13], env={**os.environ, "SIMCORE_DYNAMIC_TYPE": "accessibility1"})
assert recv(p)["type"] == "hello"; t = recv(p)
tx = alltexts(t["root"])
assert tx[:4] == ["Accent: plain enabled: true", "Accent: loud enabled: true", "Accent: outer enabled: false",
                  "Accent: inner enabled: false"], tx  # nearest .environment wins; .disabled reaches isEnabled
assert "Size: compact/regular" in tx, tx  # portrait iPhone
assert "Badge: ada 0 some" in tx, tx
# body 17 -> 28 at accessibility1, caption 12 -> 22 (Apple's Dynamic Type table)
assert f"Metric: {10 * 28 / 17} 22.0 active" in tx, tx
assert "Metric: 10.0 12.0 active" in tx, tx  # .environment(\.dynamicTypeSize, .large) override
assert button(t, "Inner")["disabled"], button(t, "Inner")
t = act(p, t, [m for m in walk(t["root"]) if m.get("kind") == "button" and "Draft: none" in alltexts(m)][0]["action"])
assert alltexts(t["root"]).count("Draft: saved") == 2, alltexts(t["root"])  # both views share the key
send(p, type="action", id=button(t, "Open")["action"], version=t["version"])
ev = recv(p)  # system open: reported to the host, no state change, so no tree follows
assert ev == {"type": "event", "name": "module.called",
              "detail": {"module": "OpenURLAction", "method": "openURL", "args": ["https://example.com/a"]}}, ev
t = act(p, t, button(t, "Local")["action"])  # handled in-app: no event, completion(true)
assert "Result: true" in alltexts(t["root"]), alltexts(t["root"])
t = act(p, t, button(t, "Like")["action"])
assert "Badge: ada 1 some" in alltexts(t["root"]), alltexts(t["root"])  # @Observable mutation re-renders
fld = next(m for m in walk(t["root"]) if m.get("kind") == "text_field")
send(p, type="text_input", field=fld["field"], version=t["version"], text="grace"); t = recv(p)
assert "Badge: grace 1 some" in alltexts(t["root"]), alltexts(t["root"])  # $bindable.name writes through
send(p, type="environment", appearance="light", viewport={"width": 956, "height": 440}); t = recv(p)
assert "Size: regular/compact" in alltexts(t["root"]), alltexts(t["root"])  # Pro Max landscape
send(p, type="environment", appearance="light", viewport={"width": 874, "height": 402}); t = recv(p)
assert "Size: compact/compact" in alltexts(t["root"]), alltexts(t["root"])  # Pro landscape
send(p, type="environment", appearance="light", viewport={"width": 1032, "height": 1376}); t = recv(p)
assert "Size: regular/regular" in alltexts(t["root"]), alltexts(t["root"])  # iPad
t = act(p, t, button(t, "Push")["action"])
assert "Pushed: day inner" in alltexts(t["root"]), alltexts(t["root"])
t = act(p, t, button(t, "Pushed: day inner")["action"])  # changes the value set above the NavigationStack
assert "Pushed: night inner" in alltexts(t["root"]), alltexts(t["root"])
finish(p)

# --- controls (protocol section Q)
p = spawn(sys.argv[14]); assert recv(p)["type"] == "hello"; t = recv(p)
def tree(t): return t["root"]["content"] if t["root"]["kind"] == "presentation" else t["root"]
def layers(t): return t["root"].get("layers", []) if t["root"]["kind"] == "presentation" else []
def btn(t, label): return next(m for m in walk(tree(t)) if m.get("kind") == "button" and label in alltexts(m))
def has(t, s): return any(s in x for x in alltexts(tree(t)))
def node(t, kind): return [m for m in walk(tree(t)) if m.get("kind") == kind]
# Menu: a button that opens a centred menu layer; items carry icon, role and dividers (Divider and Section).
t = act(p, t, btn(t, "Menu")["action"])
menu = next(l for l in layers(t) if l["kind"] == "menu")
assert menu["centered"] and [i["text"] for i in menu["items"]] == ["Alpha", "Delete", "Inside", "Off", "Flag", "P1", "P2"], menu
off, flag, p1, p2 = menu["items"][3:]
assert off.get("disabled") and not menu["items"][2].get("disabled"), "a disabled button is a dimmed item"
assert not flag.get("checked") and p1.get("checked") and not p2.get("checked") and p1.get("divider"), "toggles and picker options are checked items"
assert off["action"] == 0, "a disabled item has no action"
t = act(p, t, next(l for l in layers(t) if l["kind"] == "menu")["items"][6]["action"]); assert not layers(t)
assert has(t, "mpick 2"), "a picker option in a menu selects it and closes the menu"
t = act(p, t, btn(t, "Menu")["action"]); menu = next(l for l in layers(t) if l["kind"] == "menu")
assert menu["items"][0].get("systemImage") == "star" and not menu["items"][0].get("divider"), menu
assert menu["items"][1].get("destructive") and menu["items"][1].get("divider") and menu["items"][2].get("divider"), menu
t = act(p, t, menu["items"][0]["action"])
assert has(t, "last Alpha") and not layers(t), "an item runs its action and closes the menu"
t = act(p, t, btn(t, "Menu")["action"]); t = act(p, t, next(l for l in layers(t) if l["kind"] == "menu")["dismiss"])
assert not layers(t) and has(t, "last Alpha"), "tap outside only closes"
t = act(p, t, btn(t, "Primary")["action"]); assert has(t, "last Primary") and not layers(t), "primaryAction runs instead of opening"
faded = btn(t, "Faded")
assert {"opacity": 0.5} in faded.get("modifiers", []), "a custom MenuStyle wraps the menu"
t = act(p, t, faded["action"]); t = act(p, t, next(l for l in layers(t) if l["kind"] == "menu")["items"][0]["action"])
assert has(t, "last Beta"), "the styled menu still works"
# Link: runs the environment's openURL; the system action reports the URL on stderr.
t = act(p, t, btn(t, "Docs")["action"]); assert has(t, "opened https://docs.test"), alltexts(tree(t))
def opened_event(url): return {"type": "event", "name": "module.called", "detail": {"module": "OpenURLAction", "method": "openURL", "args": [url]}}
# .discarded opens nothing (no event, no tree); the system action is reported as an event, redirected by systemAction(url).
send(p, type="action", id=btn(t, "Dropped")["action"], version=t["version"])
send(p, type="action", id=btn(t, "Plain")["action"], version=t["version"])
ev = recv(p); assert ev == opened_event("https://plain.test"), ev
send(p, type="action", id=btn(t, "Redirect")["action"], version=t["version"])
ev = recv(p); assert ev == opened_event("https://to.test"), ev
# Toggle styles: .button is a selectable button; a custom ToggleStyle draws its own body.
tbn = btn(t, "TB"); assert tbn["selected"] is False and tbn["style"] == "automatic", tbn
t = act(p, t, tbn["action"]); assert btn(t, "TB")["selected"] is True
chk = btn(t, "Check"); assert any(m.get("systemName") == "square" for m in walk(chk)), chk
t = act(p, t, chk["action"]); assert any(m.get("systemName") == "checkmark.square.fill" for m in walk(btn(t, "Check")))
# Button styles: ButtonStyle styles the label of a plain button; PrimitiveButtonStyle's trigger() runs the action.
sb = btn(t, "Styled"); assert sb["style"] == "plain" and any("background" in x for x in sb["label"].get("modifiers", [])), sb
t = act(p, t, sb["action"]); t = act(p, t, btn(t, "Prim")["action"]); assert has(t, "styled 1 prim 1"), alltexts(tree(t))
# Slider value labels sit beside it; onEditingChanged brackets each change.
row = next(m for m in walk(tree(t)) if m.get("kind") == "hstack" and any(c.get("kind") == "slider" for c in m["children"]))
assert [alltexts(c) for c in row["children"] if c.get("kind") == "text"] == [["Lo"], ["Hi"]], row
sl = next(c for c in row["children"] if c.get("kind") == "slider")
send(p, type="text_input", field=sl["field"], version=t["version"], text="0.5"); t = recv(p)
assert has(t, "edits tf 0.5"), alltexts(tree(t))
# Progress and gauge styles.
assert has(t, "pct 50"), "custom ProgressViewStyle"
assert any(m.get("value") is None for m in node(t, "progress")), ".circular is the spinner"
g = node(t, "gauge"); assert len(g) == 1 and g[0]["value"] == 0.25 and g[0]["style"] == "automatic" and alltexts(g[0]["label"]) == ["Fuel"], g
assert has(t, "G 0.25"), "custom GaugeStyle gets the fraction"
# Pickers: inline rows in a list, navigationLink pushes a list and pops on a choice, wheel options select.
one, two = btn(t, "One"), btn(t, "Two"); assert one.get("value") == "checked" and two.get("value") is None, (one, two)
t = act(p, t, two["action"]); assert has(t, "pick 2") and btn(t, "Two").get("value") == "checked"
sz = btn(t, "Size"); assert sz["style"] == "navigationLink" and "S" in alltexts(sz), sz
t = act(p, t, sz["action"]); nav = t["root"]
assert nav["kind"] == "navigation" and nav["title"] == "Size" and nav["displayMode"] == "inline" and "back" in nav, nav
t = act(p, t, btn(t, "L")["action"]); assert "back" not in t["root"] and has(t, "size L"), "choosing pops back"
# A button-style toggle in a menu is a checked item; a pushed destination keeps the link's openURL.
t = act(p, t, btn(t, "BMenu")["action"]); bf = next(l for l in layers(t) if l["kind"] == "menu")["items"][0]
assert bf["text"] == "BFlag" and bf["checked"] is True, bf
t = act(p, t, bf["action"]); assert not layers(t)
t = act(p, t, btn(t, "BMenu")["action"]); assert next(l for l in layers(t) if l["kind"] == "menu")["items"][0]["checked"] is False
t = act(p, t, next(l for l in layers(t) if l["kind"] == "menu")["dismiss"])
t = act(p, t, btn(t, "Deeper")["action"]); t = act(p, t, btn(t, "Inner")["action"])
t = act(p, t, t["root"]["back"]["action"]); assert has(t, "opened https://inner.test"), alltexts(tree(t))
wh = next(m for m in node(t, "picker") if m["style"] == "wheel"); assert wh["selected"] == 1 and len(wh["options"]) == 4, wh
t = act(p, t, wh["options"][2]["action"]); assert has(t, "hour 2")
fl = next(m for m in node(t, "picker") if m["style"] == "menu"); assert fl["label"]["kind"] == "empty", "labelsHidden"
assert any(m["label"]["kind"] == "empty" for m in node(t, "toggle")), "labelsHidden hides a toggle's label"
finish(p)
# --- Canvas and visual effects (protocol section R)
p = spawn(sys.argv[16])
assert recv(p)["type"] == "hello"
t = recv(p)
canvas = lambda t: next(n for n in walk(t["root"]) if n["kind"] == "canvas")
effect = lambda t: next(n for n in walk(t["root"]) if n["kind"] == "visualEffect")
cid, eid = canvas(t)["id"], effect(t)["id"]
palette = next(m["foregroundPalette"] for n in walk(t["root"]) for m in n.get("modifiers", []) if "foregroundPalette" in m)
assert palette["primary"]["r"] == 1 and palette["secondary"]["b"] == 1
assert palette["tertiary"]["g"] > palette["tertiary"]["r"]
assert canvas(t)["children"] == [], "initial Canvas waits for its layout size"
send(p, type="layout", frames={cid: [3, 4, 44, 28], eid: [7, 11, 80, 20]})
t = recv(p)
assert canvas(t)["proposal"] == [44, 28]
clipped, clean, symbol, masked, resolved = canvas(t)["children"]
assert clipped["path"][2] == {"l": [44, 28]}, clipped
assert [{"effect": {"kind": "affine", "matrix": [1, 0, 0, 1, 3, -2], "anchor": [0, 0]}}] == clipped["modifiers"][:1]
assert clipped["modifiers"][1] == {"effect": {"kind": "opacity", "amount": 0.25}} and "invertedMask" in clipped["modifiers"][2]
assert "modifiers" not in clean and clean["stroke"]["dash"] == [1, 2], "clip changes stay on a context copy"
assert symbol["origin"] == [22, 14] and symbol["child"]["text"] == "星", symbol
assert masked["path"][2] == {"l": [22, 28]}, "clipBoundingRect follows the layer's half-width fill"
assert resolved["child"]["modifiers"][0]["foregroundColor"]["r"] == 1, "resolved shading preserves explicit red"
assert effect(t)["frame"] == [7, 11, 80, 20]
translation, scale, blend = effect(t)["effects"]
assert translation["matrix"] == [1, 0, 0, 1, 87, 0]
assert scale["matrix"] == [-1, 0, 0, 0, 0, 0] and scale["anchor"] == [1, 0]
assert blend == {"kind": "blend", "mode": "multiply"}
assert effect(t)["child"]["fill"]["gradient"]["colorSpace"] == "perceptual"
t = act(p, t, button(t, "Draw more")["action"])
assert len(canvas(t)["children"]) == 304, "a state update redraws every Canvas command"
send(p, type="layout", frames={cid: [0, 0, 0, 0], eid: [-6, 3, 120, 20]})
t = recv(p)
assert canvas(t)["proposal"] == [0, 0]
assert canvas(t)["children"][-3]["origin"] == [0, 0]
assert effect(t)["effects"][0]["matrix"][4] == 114
finish(p)
# --- text input: modifier scopes, Unicode bindings, focus movement, and submission
p = spawn(sys.argv[17])
assert recv(p)["type"] == "hello"
t = recv(p)

def input_field(t, placeholder):
    return next(m for m in walk(t["root"]) if m["kind"] == "text_field" and m["placeholder"] == placeholder)

def input_action(t, placeholder, action):
    return act(p, t, input_field(t, placeholder)[action])

name = input_field(t, "Full name")
assert name["input_style"] == "rounded_border" and name["prompt"]["text"] == "Full name", name
assert name["keyboard_type"] == "emailAddress" and name["autocapitalization"] == "never", name
assert name["submit_label"] == "next" and name["autocorrection_disabled"], name
assert not name["focused"] and not input_field(t, "Other")["focused"]
assert "submit_action" not in input_field(t, "Other")
assert input_field(t, "Other")["input_style"] == "plain"
disabled = input_field(t, "Disabled")
assert disabled["disabled"] and disabled["field"] == 0 and "submit_action" not in disabled
assert disabled["input_style"] == "bordered" and disabled["input_border_shape"] == "rounded_rectangle"

t = act(p, t, button(t, "Focus name")["action"])
assert input_field(t, "Full name")["focused"]
t = act(p, t, button(t, "Focus full name")["action"])
assert input_field(t, "Full name")["focused"]
name = input_field(t, "Full name")
value = "李 García 👩🏽‍💻"
send(p, type="text_input", field=name["field"], version=t["version"], text=value)
t = recv(p)
assert input_field(t, "Full name")["text"] == value
t = input_action(t, "Full name", "submit_action")
assert "outer inner " in alltexts(t["root"])
assert not input_field(t, "Full name")["focused"]
password = input_field(t, "Enter password")
assert password["secure"] and password["focused"] and password["content_type"] == "password"
assert "keyboard_type" not in password
secret = "秘密 🔐"
send(p, type="text_input", field=password["field"], version=t["version"], text=secret)
t = recv(p)
assert input_field(t, "Enter password")["text"] == secret
t = input_action(t, "Enter password", "submit_action")
assert "outer inner outer " in alltexts(t["root"])

t = input_action(t, "Other", "focus_action")
assert input_field(t, "Other")["focused"] and not input_field(t, "Enter password")["focused"]
assert "No enum focus" in alltexts(t["root"])
t = input_action(t, "", "focus_action")
assert input_field(t, "")["focused"] and not input_field(t, "Other")["focused"]
editor = input_field(t, "")
assert editor["editor"] and editor["axis"] == "vertical" and editor["input_style"] == "plain"
assert "submit_action" not in editor
value = ("א👩🏽‍💻\n" * 2048) + "Last line"
send(p, type="text_input", field=editor["field"], version=t["version"], text=value)
t = recv(p)
assert input_field(t, "")["text"] == value
t = input_action(t, "", "blur_action")
assert not input_field(t, "")["focused"] and "No enum focus" in alltexts(t["root"])
editor = input_field(t, "")
send(p, type="text_input", field=editor["field"], version=t["version"], text="")
t = recv(p)
assert input_field(t, "")["text"] == ""

t = input_action(t, "Enter password", "focus_action")
assert input_field(t, "Enter password")["focused"]
t = act(p, t, button(t, "Hide password")["action"])
assert "No enum focus" in alltexts(t["root"])
finish(p)
print("text input: secure and multiline bindings, inherited submission, focus transfer, disabled controls ok")


# --- DatePicker, MultiDatePicker and ColorPicker
p = spawn(sys.argv[18])
assert recv(p)["type"] == "hello"
recv(p)
send(p, type="environment", appearance="light", viewport={"width": 402, "height": 874})
t = recv(p)

def picker_button(root, label):
    return next(n for n in walk(root) if n["kind"] == "button" and label in alltexts(n))

# Compact date and time share a binding with Alarm. Date changes preserve time
# and a time-wheel selection leaves the date unchanged.
t = act(p, t, picker_button(tree(t), "Mar 14, 2025")["action"])
layer, = layers(t)
assert layer["kind"] == "popover"
t = act(p, t, picker_button(layer["content"], "15")["action"])
assert "Mar 15, 2025" in alltexts(tree(t))
if layers(t):
    t = act(p, t, layers(t)[0]["dismiss"])
t = act(p, t, picker_button(tree(t), "2:30\u202fPM")["action"])
layer, = layers(t)
wheel = next(n for n in walk(layer["content"]) if n["kind"] == "wheel")
minute = next(n for n in wheel["columns"][1]["options"] if n["text"] == "31")
t = act(p, t, minute["action"])
assert alltexts(tree(t)).count("2:31\u202fPM") == 2 and "Mar 15, 2025" in alltexts(tree(t))

def picker_drag(t, node, start, end, frame, time):
    key = next(m["gesture"]["key"] for m in node["modifiers"] if "gesture" in m)
    for offset, phase, point in [(0, "start", start), (16, "move", end), (32, "end", end)]:
        send(p, type="gesture", phase=phase, time=time + offset, points=[list(point)],
             targets=[{"key": key, "frame": frame}])
        t = recv(p)
    return t

wheel = next(n for n in walk(layers(t)[0]["content"]) if n["kind"] == "wheel")
t = picker_drag(t, wheel, (100, 150), (100, -42), [0, 0, 218, 172], 100)
assert alltexts(tree(t)).count("2:37\u202fPM") == 2, "a drag can advance beyond the visible options"
t = act(p, t, layers(t)[0]["dismiss"])
assert not layers(t)

# A date outside the graphical picker's upper bound is disabled. Month navigation
# keeps the previous month available and prevents advancing beyond the upper bound.
t = act(p, t, picker_button(tree(t), "Graphical")["action"])
assert picker_button(tree(t), "25")["disabled"]
assert not picker_button(tree(t), "24")["disabled"]
prev = next(n for n in walk(tree(t)) if n["kind"] == "button" and n["id"].endswith("/prev"))
t = act(p, t, prev["action"])
assert "February 2025" in alltexts(tree(t))
t = act(p, t, t["root"]["back"]["action"])

# March's exclusive upper bound prevents April navigation. Repeated toggles
# remove and restore the day, including entries supplied without a calendar.
t = act(p, t, picker_button(tree(t), "Multiple")["action"])
assert "3 days" in alltexts(tree(t))
next_month = next(n for n in walk(tree(t)) if n["kind"] == "button" and n["id"].endswith("/next"))
assert next_month["disabled"]
t = act(p, t, picker_button(tree(t), "14")["action"])
assert "2 days" in alltexts(tree(t))
t = act(p, t, picker_button(tree(t), "14")["action"])
assert "3 days" in alltexts(tree(t))
t = act(p, t, t["root"]["back"]["action"])

# Palette colors and both opacity endpoints update the same binding.
t = act(p, t, picker_button(tree(t), "Tint")["action"])
layer, = layers(t)
assert layer["kind"] == "sheet"
white = next(n for n in walk(layer["content"]) if n["kind"] == "button" and n["id"].endswith("/g/0/0"))
t = act(p, t, white["action"])
assert picker_button(tree(t), "Tint")["value"] == "#FFFFFF"
for label, expected in [("0%", "#FFFFFF00"), ("100%", "#FFFFFF")]:
    action = next(n["action"] for n in walk(layers(t)[0]["content"]) if n.get("value") == label)
    t = act(p, t, action)
    assert picker_button(tree(t), "Tint")["value"] == expected
track = next(n for n in walk(layers(t)[0]["content"]) if n["id"].endswith("/op/track"))
t = picker_drag(t, track, (170, 16), (74, 16), [0, 0, 279, 32.7], 200)
assert picker_button(tree(t), "Tint")["value"] == "#FFFFFF3C"
action = next(n["action"] for n in walk(layers(t)[0]["content"]) if n.get("value") == "100%")
t = act(p, t, action)
t = act(p, t, picker_button(layers(t)[0]["content"], "Sliders")["action"])
red = next(n for n in walk(layers(t)[0]["content"]) if n["kind"] == "slider")
send(p, type="text_input", field=red["field"], version=t["version"], text="0")
t = recv(p)
assert picker_button(tree(t), "Tint")["value"] == "#00FFFF"
def hex_input(t):
    return next(n for n in walk(layers(t)[0]["content"]) if n["kind"] == "text_field")

def edit_hex(t, text, expected_color):
    send(p, type="text_input", field=hex_input(t)["field"], version=t["version"], text=text)
    t = recv(p)
    assert hex_input(t)["text"] == text
    assert picker_button(tree(t), "Tint")["value"] == expected_color
    return t

draft = hex_input(t)["text"]
assert draft == "#00FFFF"
while draft:
    draft = draft[:-1]
    t = edit_hex(t, draft, "#00FFFF")
t = edit_hex(t, "##bad", "#00FFFF")
t = edit_hex(t, "", "#00FFFF")
for char in "a103fd":
    draft += char
    t = edit_hex(t, draft, "#A103FD" if len(draft) == 6 else "#00FFFF")
favorite = next(n for n in walk(layers(t)[0]["content"]) if n.get("value") == "Add favorite")
t = act(p, t, favorite["action"])
assert any(n.get("value") == "#A103FD" for n in walk(layers(t)[0]["content"]))

# An RGB slider change replaces a partial draft with the new selection.
t = edit_hex(t, "a103f", "#A103FD")
red = next(n for n in walk(layers(t)[0]["content"]) if n["kind"] == "slider")
send(p, type="text_input", field=red["field"], version=t["version"], text="0")
t = recv(p)
assert hex_input(t)["text"] == "#0003FD"

t = act(p, t, picker_button(layers(t)[0]["content"], "Spectrum")["action"])
spectrum = next(n for n in walk(layers(t)[0]["content"]) if n["id"].endswith("/spectrum"))
t = picker_drag(t, spectrum, (50, 150), (185, 0), [0, 0, 370, 308], 400)
assert picker_button(tree(t), "Tint")["value"] == "#FFFFFF"
t = act(p, t, layers(t)[0]["dismiss"])
t = act(p, t, picker_button(tree(t), "Paper")["action"])
assert "OPACITY" not in alltexts(layers(t)[0]["content"])
finish(p)
# A custom style can delegate to Apple's corresponding built-in style API.
# For a same-day range, the wheel must disable times as well as dates.
p = spawn(sys.argv[19], {**os.environ, "SIMCORE_TIME": "1746057600"})
assert recv(p)["type"] == "hello"
t = recv(p)
wheel = next(n for n in walk(tree(t)) if n["kind"] == "wheel")
hours, minutes, _ = wheel["columns"]
assert next(n for n in hours["options"] if n["text"] == "3")["disabled"]
assert next(n for n in minutes["options"] if n["text"] == "29")["disabled"]
assert not next(n for n in minutes["options"] if n["text"] == "31")["disabled"]

# An empty selection with a past exclusive midnight bound opens on the last
# available month. An empty calendar in April would have no selectable days.
assert "March 2025" in alltexts(tree(t))
assert not picker_button(tree(t), "31")["disabled"]

# 02:30 is skipped by the Pacific DST transition on March 9, 2025.
t = act(p, t, picker_button(tree(t), "Mar 8, 2025")["action"])
t = act(p, t, picker_button(layers(t)[0]["content"], "9")["action"])
assert "Mar 9, 2025" in alltexts(tree(t)) and "3:30\u202fAM" in alltexts(tree(t))
t = act(p, t, layers(t)[0]["dismiss"])

# The selection starts before the range. Both calendars can reach the first
# allowed month without writing a new date through the binding.
header = picker_button(tree(t), "March 2030")
next_id = header["id"][:-len("/title")] + "/next"
t = act(p, t, next(n["action"] for n in walk(tree(t)) if n["id"] == next_id))
assert "April 2030" in alltexts(tree(t))
t = act(p, t, picker_button(tree(t), "Mar 14, 2025")["action"])
assert "March 2030" in alltexts(layers(t)[0]["content"])
assert picker_button(layers(t)[0]["content"], "4")["disabled"]
assert not picker_button(layers(t)[0]["content"], "5")["disabled"]
t = act(p, t, layers(t)[0]["dismiss"])

# Changing the range leaves the binding alone and clamps both the cached
# graphical month and the compact popover to the new upper bound's month.
t = act(p, t, picker_button(tree(t), "Change range")["action"])
assert "January 2020" in alltexts(tree(t))
t = act(p, t, picker_button(tree(t), "Mar 14, 2025")["action"])
assert "January 2020" in alltexts(layers(t)[0]["content"])
assert picker_button(layers(t)[0]["content"], "21")["disabled"]
t = act(p, t, picker_button(layers(t)[0]["content"], "20")["action"])
assert "Jan 20, 2020" in alltexts(tree(t))
finish(p)
# --- compact navigation, toolbar actions, and search binding/focus
p = spawn(sys.argv[20])
assert recv(p)["type"] == "hello"
t = recv(p)
assert title(t) == "Library" and "back" not in tree(t)
split_id = tree(t)["id"]
item, = tree(t)["chrome"]["items"]
assert item["region"] == "trailing" and "About" in alltexts(item["content"]["label"]["title"])
t = act(p, t, item["content"]["action"])
assert title(t) == "Detail" and tree(t)["displayMode"] == "inline"
assert tree(t)["id"] == split_id and tree(t)["back"]["label"] == "Library"
chrome = tree(t)["chrome"]
item, = chrome["items"]
assert item["region"] == "bottom"
assert chrome["navigationBarBackgroundVisibility"] == chrome["bottomBarBackgroundVisibility"] == "visible"
assert "navigationBarBackground" in chrome and "bottomBarBackground" in chrome
t = act(p, t, item["content"]["action"])
assert "Book marked" in alltexts(tree(t))
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Library"
t = act(p, t, button(t, "Open detail")["action"])
assert "Book marked" in alltexts(tree(t)), "compact columns retain their state when hidden"
t = act(p, t, tree(t)["chrome"]["titleMenu"]["action"])
menu = layers(t)[0]
assert menu["kind"] == "menu"
reset, = menu["items"]
t = act(p, t, reset["action"])
assert "Choose a book" in alltexts(tree(t))
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Library"

t = act(p, t, button(t, "Legacy")["action"])
assert tree(t)["content"]["kind"] == "navigation"
assert tree(t)["content"]["title"] == "Archive"
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Library"

t = act(p, t, button(t, "Search")["action"])
assert title(t) == "Search"
search = lambda t: tree(t)["chrome"]["search"]
assert search(t)["placement"] == "always"
assert "cancel" not in search(t) and not search(t)["field"]["focused"]
assert "Reset search" not in alltexts(tree(t))
t = act(p, t, search(t)["field"]["focus_action"])
assert search(t)["field"]["focused"]
assert "Close" in alltexts(search(t)["cancel"]["label"]["title"])
assert "Reset search" in alltexts(tree(t))
for query in ("書📚 مرحبا", "é" * 1024, ""):
    send(p, type="text_input", field=search(t)["field"]["field"], version=t["version"], text=query)
    t = recv(p)
    assert search(t)["field"]["text"] == query
    assert (query or "All books") in alltexts(tree(t))
send(p, type="text_input", field=search(t)["field"]["field"], version=t["version"], text="Reset me")
t = recv(p)
t = act(p, t, button(t, "Reset search")["action"])
assert search(t)["field"]["text"] == "" and not search(t)["field"]["focused"]
assert "cancel" not in search(t) and "Reset search" not in alltexts(tree(t))
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Library" and "search" not in tree(t)["chrome"]
finish(p)
print("navigation: compact column changes, toolbar actions, and search binding and dismissal ok")

p = spawn(sys.argv[21])
assert recv(p)["type"] == "hello"
t = recv(p)
assert title(t) == "Content" and search(t)["field"]["placeholder"] == "Find a title"
assert search(t)["field"]["text"] == "Prefilled" and not search(t)["field"]["focused"]
t = act(p, t, button(t, "Dismiss search")["action"])
assert "Dismissed: 1" in alltexts(tree(t))
assert search(t)["field"]["text"] == "Prefilled", "inactive dismiss must preserve the query"
t = act(p, t, button(t, "Present search")["action"])
assert search(t)["field"]["focused"]
t = act(p, t, search(t)["field"]["submit_action"])
assert "Submitted: Prefilled" in alltexts(tree(t))
t = act(p, t, search(t)["cancel"]["action"])
assert search(t)["field"]["text"] == "" and not search(t)["field"]["focused"]

t = act(p, t, button(t, "Open detail")["action"])
assert title(t) == "Detail" and tree(t)["back"]["label"] == "Content"
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Content" and "Submitted: Prefilled" in alltexts(tree(t))
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Sidebar" and "back" not in tree(t)
t = act(p, t, button(t, "Open content")["action"])
assert title(t) == "Content"
t = act(p, t, tree(t)["back"]["action"])
t = act(p, t, button(t, "Test toolbars")["action"])

def toolbar_action(nav, label):
    return next(item["content"]["action"] for item in nav["chrome"]["items"]
                if label in alltexts(item["content"]))

assert title(t) == "Outer"
t = act(p, t, toolbar_action(tree(t), "Toolbar count: 0"))
t = act(p, t, toolbar_action(tree(t), "Toolbar count: 1"))
assert toolbar_action(tree(t), "Toolbar count: 2")
# Queue the count action too, so an inert navigation action produces a
# mismatched tree instead of waiting forever for the guest to respond.
send(p, type="action", id=toolbar_action(tree(t), "Open inner"), version=t["version"])
send(p, type="action", id=toolbar_action(tree(t), "Toolbar count: 2"), version=t["version"])
t = recv(p)
inner = tree(t)["content"]
assert inner["title"] == "Inner"
send(p, type="action", id=toolbar_action(inner, "Open inner detail"), version=t["version"])
send(p, type="action", id=toolbar_action(tree(t), "Toolbar count: 2"), version=t["version"])
t = recv(p)
inner = tree(t)["content"]
assert inner["title"] == "Inner detail" and "Inner destination 42" in alltexts(inner)
t = act(p, t, inner["back"]["action"])
assert tree(t)["content"]["title"] == "Inner"
t = act(p, t, tree(t)["back"]["action"])
assert title(t) == "Outer" and toolbar_action(tree(t), "Toolbar count: 2")
finish(p)
print("navigation lab: inactive dismissal, search submission, and three compact columns ok")
# --- forms and containers (section T)
p = spawn(sys.argv[22])
assert recv(p)["type"] == "hello"
recv(p)
send(p, type="environment", appearance="light", viewport={"width": 402, "height": 874})
t = recv(p)

def fwalk(n):
    yield n
    subs = list(n.get("children", []))
    for k in ("label", "child", "content", "title", "icon", "prompt"):
        if isinstance(n.get(k), dict): subs.append(n[k])
    for sec in n.get("sections", []):
        subs += sec["rows"]
        for k in ("headerView", "footerView"):
            if k in sec: subs.append(sec[k])
    for row in n.get("rows", []) or []:
        if isinstance(row, dict): subs += row.get("cells", [])
    for m in n.get("modifiers", []) or []:
        for a in (m.get("listRow", {}).get("swipe") or {}).get("leading", []) + (m.get("listRow", {}).get("swipe") or {}).get("trailing", []):
            subs.append(a["label"])
    for c in subs: yield from fwalk(c)
def ftexts(n): return [m["text"] for m in fwalk(n) if m.get("kind") == "text"]
def fbtn(root, label): return next(m for m in fwalk(root) if m.get("kind") == "button" and label in ftexts(m))
def lists(t): return [m for m in fwalk(tree(t)) if m.get("kind") == "list"]
def lrow(n): return next((m["listRow"] for m in n.get("modifiers", []) if "listRow" in m), {})
def go(t, label):
    return act(p, t, fbtn(tree(t), label)["action"])
def back(t): return act(p, t, tree(t)["back"]["action"])
def gesture(t, key, frame, *steps):
    for time, phase, pt in steps:
        send(p, type="gesture", phase=phase, time=time, points=[list(pt)], targets=[{"key": key, "frame": frame}])
        t = recv(p)
    return t
def row_with(t, text):
    return next(r for l in lists(t) for s in l["sections"] for r in s["rows"] if text in ftexts(r))

# List and Form styles: `.automatic` and Form are inset grouped; each built-in style has its wire name.
t = go(t, "Styles")
styles = [l["style"] for l in lists(t)]
assert styles == ["insetGrouped", "grouped", "plain", "inset", "sidebar", "insetGrouped", "insetGrouped", "insetGrouped"], styles
t = back(t)

# Sections: a view header, a text footer, a collapsible section whose toggle hides its rows, loose rows in their own section.
t = go(t, "Sections")
secs = lists(t)[0]["sections"]
assert "headerView" in secs[0] and "header" not in secs[0] and secs[0]["footer"] == "Plain footer", secs[0].keys()
assert secs[1]["header"] == "Folding" and secs[1]["expanded"] is True and ftexts(secs[1]["rows"][0]) == ["hidden when folded"]
assert len(secs) == 3 and ftexts(secs[2]["rows"][0]) == ["loose row"]
t = act(p, t, secs[1]["toggle"])
secs = lists(t)[0]["sections"]
assert secs[1]["expanded"] is False and secs[1]["rows"] == [], secs[1]
t = act(p, t, secs[1]["toggle"])
assert lists(t)[0]["sections"][1]["expanded"] is True
t = back(t)

# Edit mode: EditButton toggles it; ForEach rows with onDelete/onMove get a delete control and a reorder strip.
t = go(t, "Editing")
assert "Edit" in ftexts(tree(t)) and not any(m.get("id", "").endswith("#minus") for m in fwalk(tree(t)))
a_row = row_with(t, "A")
assert [x["label"]["kind"] for x in lrow(a_row)["swipe"]["trailing"]] == ["label"], "onDelete alone offers swipe-to-delete"
t = act(p, t, fbtn(tree(t), "Edit")["action"])
assert "Done" in ftexts(tree(t))
rows = lists(t)[0]["sections"][0]["rows"]
assert [r["kind"] for r in rows] == ["hstack"] * 4 and all(lrow(r)["reorder"]["axis"] == "vertical" for r in rows)
assert "gesture" not in lrow(rows[0]), "no swipes while editing"
static = row_with(t, "static x")
assert static["kind"] == "text" and not lrow(static), "a ForEach without handlers is untouched"
# The delete control opens the row's Delete action; Delete runs onDelete with that element's offset.
t = act(p, t, next(m for m in fwalk(rows[1]) if m.get("id", "").endswith("#minus"))["action"])
sw = lrow(row_with(t, "B"))["swipe"]
assert sw["open"] == "trailing" and "Delete" in ftexts(sw["trailing"][0]["label"])
t = act(p, t, sw["trailing"][0]["action"])
assert "order ACD" in ftexts(tree(t)) and "log del " in ftexts(tree(t))
# Dragging a handle down by two of its row's heights moves the element after the one two rows down.
r = lrow(row_with(t, "A"))["reorder"]
t = gesture(t, r["key"], [330, 150, 56, 52], (0, "start", (350, 170)), (16, "move", (350, 230)))
assert lrow(row_with(t, "A")).get("lift") == 60, lrow(row_with(t, "A"))
t = gesture(t, r["key"], [330, 150, 56, 52], (32, "move", (350, 274)), (48, "end", (350, 274)))
assert "order CDA" in ftexts(tree(t)) and "log del move " in ftexts(tree(t)), ftexts(tree(t))
assert not lrow(row_with(t, "A")).get("lift")
# Up past the first row clamps to the top.
r = lrow(row_with(t, "A"))["reorder"]
t = gesture(t, r["key"], [330, 250, 56, 52], (100, "start", (350, 270)), (116, "move", (350, -400)), (132, "end", (350, -400)))
assert "order ACD" in ftexts(tree(t))
# Leaving edit mode closes a row its delete control opened.
t = act(p, t, next(m for m in fwalk(row_with(t, "C")) if m.get("id", "").endswith("#minus"))["action"])
assert lrow(row_with(t, "C"))["swipe"]["open"] == "trailing"
t = act(p, t, fbtn(tree(t), "Done")["action"])
assert "Edit" in ftexts(tree(t)) and row_with(t, "A")["kind"] == "text"
assert "open" not in lrow(row_with(t, "C"))["swipe"]
t = back(t)

# Swipe actions: horizontal pans open an edge, an action runs and closes, a full swipe runs the outermost action.
t = go(t, "Swipes")
m1 = row_with(t, "Mail 1")
lr = lrow(m1)
assert lr["gesture"]["axis"] == "horizontal" and lr["gesture"]["pans"]
assert [ftexts(a["label"]) for a in lr["swipe"]["trailing"]] == [["Delete"], ["Flag"]]
assert lr["swipe"]["trailing"][0]["role"] == "destructive" and "tint" in lr["swipe"]["trailing"][1]
assert [ftexts(a["label"]) for a in lr["swipe"]["leading"]] == [["Read"]] and "open" not in lr["swipe"]
key, F = lr["gesture"]["key"], [16, 100, 370, 52]
t = gesture(t, key, F, (0, "start", (300, 120)), (16, "move", (250, 121)))
assert lrow(row_with(t, "Mail 1"))["swipe"]["offset"] == -50, "the row follows the finger"
t = gesture(t, key, F, (32, "move", (220, 121)), (48, "end", (220, 121)))
sw = lrow(row_with(t, "Mail 1"))["swipe"]
assert sw["open"] == "trailing" and "offset" not in sw
t = act(p, t, sw["trailing"][1]["action"])
assert "log flag(Mail 1) " in ftexts(tree(t)) and "open" not in lrow(row_with(t, "Mail 1"))["swipe"], "an action closes the row"
# A short drag springs back; a tap on an open row closes it.
t = gesture(t, key, F, (100, "start", (300, 120)), (116, "move", (285, 120)), (132, "end", (285, 120)))
assert "open" not in lrow(row_with(t, "Mail 1"))["swipe"]
t = gesture(t, key, F, (200, "start", (100, 120)), (216, "move", (180, 120)), (232, "end", (180, 120)))
assert lrow(row_with(t, "Mail 1"))["swipe"]["open"] == "leading"
t = gesture(t, key, F, (300, "start", (300, 120)), (316, "end", (300, 120)))
assert "open" not in lrow(row_with(t, "Mail 1"))["swipe"]
# Opening another row closes the first.
t = gesture(t, key, F, (400, "start", (300, 120)), (416, "move", (220, 120)), (432, "end", (220, 120)))
k2 = lrow(row_with(t, "Mail 2"))["gesture"]["key"]
t = gesture(t, k2, F, (500, "start", (300, 172)), (516, "move", (220, 172)), (532, "end", (220, 172)))
assert "open" not in lrow(row_with(t, "Mail 1"))["swipe"] and lrow(row_with(t, "Mail 2"))["swipe"]["open"] == "trailing"
t = gesture(t, k2, F, (550, "start", (300, 172)), (566, "end", (300, 172)))
# Full swipe trailing runs the outermost action; leading does not allow it.
t = gesture(t, key, F, (600, "start", (360, 120)), (616, "move", (100, 120)))
assert lrow(row_with(t, "Mail 1"))["swipe"]["full"] is True
t = gesture(t, key, F, (632, "move", (40, 120)), (648, "end", (40, 120)))
assert "delete(Mail 1) " in ftexts(tree(t))[0] and "open" not in lrow(row_with(t, "Mail 1"))["swipe"]
t = gesture(t, key, F, (700, "start", (30, 120)), (716, "move", (380, 120)), (732, "end", (380, 120)))
sw = lrow(row_with(t, "Mail 1"))["swipe"]
assert "read" not in ftexts(tree(t))[0] and sw["open"] == "leading" and not sw.get("full")
# onDelete alone: a trailing Delete that runs it.
od = lrow(row_with(t, "Only delete"))["swipe"]["trailing"]
assert len(od) == 1 and od[0]["role"] == "destructive"
t = act(p, t, od[0]["action"])
assert "ondelete" in ftexts(tree(t))[0]
t = back(t)

# Disclosure groups: rows in a list (children indented), a button outside; an outline expands level by level.
t = go(t, "Disclosure")
lst = lists(t)[0]
assert [r["id"].split("/")[-1] for r in lst["sections"][0]["rows"]] and len(lst["sections"][0]["rows"]) == 2
t = act(p, t, fbtn(lists(t)[0], "Internal")["action"])
rows = lists(t)[0]["sections"][0]["rows"]
assert ftexts(rows[1]) == ["internal child"] and any("padding" in m for m in rows[1]["modifiers"])
assert fbtn(rows[0], "Internal")["value"] == "expanded"
t = act(p, t, fbtn(lists(t)[0], "Bound")["action"])
assert "bound open" in ftexts(tree(t)) and len(lists(t)[0]["sections"][0]["rows"]) == 5
outside = fbtn(tree(t), "Outside")
assert outside["style"] == "automatic" and outside["value"] == "collapsed"
t = act(p, t, outside["action"])
assert "outside child" in ftexts(tree(t))
outline = lists(t)[1]
assert [ftexts(r)[0] for r in outline["sections"][0]["rows"]] == ["root", "solo"]
t = act(p, t, fbtn(lists(t)[1], "root")["action"])
assert [ftexts(r)[0] for r in lists(t)[1]["sections"][0]["rows"]] == ["root", "leaf", "branch", "solo"]
t = act(p, t, fbtn(lists(t)[1], "branch")["action"])
assert [ftexts(r)[0] for r in lists(t)[1]["sections"][0]["rows"]] == ["root", "leaf", "branch", "deep", "solo"]
t = back(t)

# Custom styles get their configurations (label, content, isExpanded); wrappers draw the default look.
t = go(t, "Custom")
x = ftexts(tree(t))
for s_ in ["custom box", "Box label", "box content", "Key", "Value", "stacked", "Wrapped", "Default", "[", "One", "Two", "]", "framed form", "in framed"]:
    assert s_ in x, (s_, x)
assert "styled child" not in x
t = act(p, t, fbtn(tree(t), "expand")["action"])
assert "styled child" in ftexts(tree(t)) and "collapse" in ftexts(tree(t))
menu = fbtn(tree(t), "Actions")
t = act(p, t, menu["action"])
assert any(i["text"] == "Menu item" for l in layers(t) if l["kind"] == "menu" for i in l["items"])
t = back(t)

# Columns forms, tables, empty states, nested boxes.
t = go(t, "Misc")
vtf = next(m for m in fwalk(tree(t)) if m.get("kind") == "viewThatFits")
grid = vtf["children"][0]
cells = [[c for c in r["cells"]] for r in grid["rows"]]
assert ftexts(cells[0][1]) == ["Net"] and cells[0][0]["kind"] == "empty"
assert ftexts(cells[1][0]) == ["Wi-Fi"] and cells[1][1]["kind"] == "toggle" and cells[1][1]["label"]["kind"] == "empty"
assert ftexts(cells[2][0]) == ["Count"] and ftexts(cells[2][1]) == ["1,234.5"], cells[2]
assert vtf["children"][1]["kind"] == "scroll", "falls back to scrolling when it does not fit"
table = next(l for l in lists(t) if l["style"] == "plain")
assert [ftexts(r) for r in table["sections"][0]["rows"]] == [["Ann"], ["Bo"]], "compact tables show the first column"
x = ftexts(tree(t))
assert "No Results" in x and "Gone" in x and "Nothing here" in x
boxes = [m for m in fwalk(tree(t)) if m.get("kind") == "vstack" and any("cornerRadius" in q for q in m.get("modifiers", []))]
fills = [next(q["background"]["color"]["r"] for q in b["modifiers"] if "background" in q) for b in boxes]
assert len(fills) == 2 and fills[0] != fills[1], "nested group boxes alternate backgrounds"
t = back(t)

# An `.environment(\.editMode, _)` around a NavigationStack puts its lists in edit mode.
t = go(t, "Override")
assert any(m.get("id", "").endswith("#minus") for m in fwalk(row_with(t, "p")))
finish(p)

# The forms example runs and pushes every screen.
p = spawn(sys.argv[23])
assert recv(p)["type"] == "hello"
t = recv(p)
for screen in ["Grouped", "Plain", "Inset", "Sidebar", "Boxes", "Columns", "Empty", "Outline", "Edit", "Swipe", "Table"]:
    t = act(p, t, fbtn(tree(t), screen)["action"])
    assert title(t) == screen, title(t)
    t = back(t)
finish(p)
print("forms: list styles, sections, edit mode, swipe actions, disclosure, outline, custom styles, columns, table ok")
print("PASS")
