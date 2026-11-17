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
print("PASS")
