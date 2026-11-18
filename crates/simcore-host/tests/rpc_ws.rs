//! v2 API (docs/api.md) over a real WebSocket against the fake guest.

use base64::Engine;
use serde_json::{json, Value};
use simcore_host::{serve_ws, Server};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

const GUEST: &str = env!("CARGO_BIN_EXE_simcore-fake-guest");

struct Client {
    ws: WebSocket<TcpStream>,
    next: u64,
    notes: Vec<Value>,
}

impl Client {
    fn connect(addr: &str) -> Self {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let (ws, _) = tungstenite::client(format!("ws://{addr}/"), stream).unwrap();
        Self { ws, next: 1, notes: vec![] }
    }

    fn read(&mut self) -> Value {
        loop {
            if let Message::Text(t) = self.ws.read().expect("socket read") {
                return serde_json::from_str(t.as_str()).unwrap();
            }
        }
    }

    /// Result, or the whole error object. Notifications that arrive meanwhile are kept.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.next;
        self.next += 1;
        self.ws.send(Message::text(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string())).unwrap();
        loop {
            let m = self.read();
            if m["id"] == id {
                return m.get("result").cloned().ok_or(m["error"].clone());
            }
            self.notes.push(m);
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params).unwrap_or_else(|e| panic!("{method}: {e}"))
    }

    fn code(&mut self, method: &str, params: Value) -> String {
        self.call(method, params).unwrap_err()["data"]["code"].as_str().unwrap().to_string()
    }

    /// The next notification named `name` satisfying `pred` (looking at those already received first).
    fn event(&mut self, name: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(i) = self.notes.iter().position(|n| n["params"]["name"] == name && pred(&n["params"]["data"])) {
                return self.notes.remove(i)["params"].take();
            }
            assert!(Instant::now() < end, "no {name} event; got {:?}", self.notes);
            let m = self.read();
            self.notes.push(m);
        }
    }
}

fn start() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || serve_ws(listener, Server::new()));
    addr
}

fn init(c: &mut Client) -> Value {
    c.ok("initialize", json!({"protocolVersion": "2.0", "client": {"name": "test"}}))
}

fn find(n: &Value, text: &str) -> bool {
    n["text"] == text || n["children"].as_array().is_some_and(|k| k.iter().any(|c| find(c, text)))
}

fn button(n: &Value) -> Option<&Value> {
    if n["role"] == "button" {
        return Some(n);
    }
    n["children"].as_array()?.iter().find_map(button)
}

#[test]
fn session_lifecycle_over_websocket() {
    let mut c = Client::connect(&start());
    assert_eq!(c.code("session.list", json!({})), "invalid_state"); // initialize first
    assert_eq!(c.code("initialize", json!({"protocolVersion": "3.0"})), "protocol_version");
    let caps = init(&mut c);
    assert_eq!(caps["protocolVersion"], "2.0");
    assert!(caps["capabilities"]["methods"].as_array().unwrap().contains(&json!("input.tap")));
    assert_eq!(c.code("nope.nothing", json!({})), "method_not_found");

    let r = c.ok("session.create", json!({"framework": "swiftui", "device": "iPhone 17 Pro", "appPath": GUEST, "load": true}));
    assert_eq!((r["sessionId"].as_str(), r["status"].as_str()), (Some("s1"), Some("running")));
    assert_eq!(c.ok("session.get", json!({"sessionId": "s1"}))["device"], "iPhone 17 Pro");
    assert_eq!(c.code("session.create", json!({"framework": "swiftui", "device": "Nokia 3310", "appPath": GUEST})), "unsupported");

    let sub = c.ok("events.subscribe", json!({"sessionId": "s1", "since": 0}));
    assert!(sub["subscribed"].as_bool().unwrap());
    c.event("runtime.ready", |_| true);
    c.event("bundle.loaded", |d| d["url"] == "http://fake");
    let v1 = c.ok("render.tree", json!({"sessionId": "s1"}))["treeVersion"].as_u64().unwrap();

    let tap = c.ok("input.tap", json!({"sessionId": "s1", "label": "Increment"}));
    assert!(!tap["hit"].is_null());
    c.event("render.updated", |d| d["treeVersion"].as_u64().unwrap() > v1);
    let tree = c.ok("render.tree", json!({"sessionId": "s1"}));
    assert!(tree["treeVersion"].as_u64().unwrap() > v1);
    assert!(find(&tree["root"], "Count: 1"));
    let btn = button(&tree["root"]).expect("a button");
    let (x, y) = (btn["frame"]["x"].as_f64().unwrap() + 1.0, btn["frame"]["y"].as_f64().unwrap() + 1.0);
    let inspected = c.ok("render.inspect", json!({"sessionId": "s1", "x": x, "y": y}));
    assert!(inspected["path"].as_array().unwrap().len() >= 2);

    let shot = c.ok("render.screenshot", json!({"sessionId": "s1", "width": 201}));
    let png = base64::engine::general_purpose::STANDARD.decode(shot["data"].as_str().unwrap()).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(shot["width"], 201);

    assert_eq!(c.ok("runtime.pause", json!({"sessionId": "s1"}))["status"], "paused");
    c.event("runtime.paused", |_| true);
    assert_eq!(c.code("input.tap", json!({"sessionId": "s1", "label": "Increment"})), "invalid_state");
    assert_eq!(c.code("runtime.reload", json!({"sessionId": "s1"})), "unsupported"); // swiftui
    assert!(c.ok("render.screenshot", json!({"sessionId": "s1"}))["data"].is_string()); // last frame
    assert_eq!(c.ok("runtime.resume", json!({"sessionId": "s1"}))["status"], "running");
    assert!(!c.ok("input.tap", json!({"sessionId": "s1", "element": btn["id"]}))["hit"].is_null());
    assert_eq!(c.code("modules.list", json!({"sessionId": "s1"})), "unsupported");

    c.ok("session.destroy", json!({"sessionId": "s1"}));
    assert_eq!(c.event("runtime.stopped", |_| true)["data"]["reason"], "stopped");
    assert_eq!(c.code("session.get", json!({"sessionId": "s1"})), "not_found");
    assert!(c.ok("session.list", json!({}))["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn reconnect_replays_since_and_modules() {
    let addr = start();
    let rn = std::env::temp_dir().join(format!("simcore-rn-ws-{}", std::process::id()));
    let _ = std::fs::remove_file(&rn);
    std::os::unix::fs::symlink(GUEST, &rn).unwrap(); // the guest serves a React Native tree when named "*rn*"

    let mut a = Client::connect(&addr);
    init(&mut a);
    a.ok("session.create", json!({"framework": "expo", "appPath": rn, "prebuilt": true, "load": true}));
    a.ok("events.subscribe", json!({"sessionId": "s1", "since": 0}));
    a.event("runtime.ready", |_| true);
    let seen = a.event("bundle.loaded", |_| true)["seq"].as_u64().unwrap();
    let mods = a.ok("modules.list", json!({"sessionId": "s1"}));
    assert_eq!(mods["modules"][0]["name"], "clipboard");
    a.ok("modules.simulate", json!({"sessionId": "s1", "module": "clipboard", "state": {"text": "hi"}}));
    a.ok("input.tap", json!({"sessionId": "s1", "x": 200, "y": 300}));
    drop(a); // connection lost

    let mut b = Client::connect(&addr);
    init(&mut b);
    let sub = b.ok("events.subscribe", json!({"sessionId": "s1", "since": seen, "names": ["render.updated"]}));
    assert!(sub["lastSeq"].as_u64().unwrap() >= seen);
    let first = b.event("render.updated", |_| true);
    assert!(first["seq"].as_u64().unwrap() > seen); // nothing at or before `since` is replayed again
    assert!(b.notes.iter().all(|n| n["params"]["name"] == "render.updated"));
    b.ok("session.destroy", json!({"sessionId": "s1"}));
    let _ = std::fs::remove_file(rn);
}
