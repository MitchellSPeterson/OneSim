//! OneSim API v2: JSON-RPC 2.0 dispatcher over `Host` (docs/api.md).
//!
//! Concurrency: the server owns a registry of sessions, each with its **own** `Host` behind its own
//! mutex (sharing a single `Arc<Mutex<Option<Fonts>>>` across all sessions) plus an `EventLog`. A
//! call locks the registry only to look the session up, then blocks on that session's host alone,
//! so a 30 s `runtime.load` never delays another session. Calls on the *same* session serialize.
//! Events bypass the host lock entirely.

use crate::events::EventLog;
use crate::settle::STARTUP_MAX;
use crate::{Host, DEVICES};
use base64::Engine;
use onesim_protocol::{
    ApiError, Appearance, CreateSessionParams, DynamicTypeSize, ErrorCode, IosVersion, KeyboardEvent, PointerEvent, PointerKind, UiNode, Viewport,
};
use onesim_render::Fonts;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::Duration;

pub const API_VERSION: &str = "2.0";

pub const METHODS: &[&str] = &[
    "initialize", "session.create", "session.list", "session.get", "session.destroy",
    "runtime.load", "runtime.reload", "runtime.pause", "runtime.resume", "runtime.stop", "runtime.status",
    "input.tap", "input.press", "input.type", "input.key", "input.scroll", "input.swipe",
    "render.screenshot", "render.frame", "render.backend", "render.tree", "render.inspect",
    "device.list", "device.set", "device.rotate", "device.appearance", "device.keyboard",
    "modules.list", "modules.simulate", "events.subscribe", "events.unsubscribe",
];

pub const EVENTS: &[&str] = &[
    "runtime.ready", "runtime.error", "runtime.stopped", "runtime.paused", "runtime.resumed", "bundle.loaded",
    "bundle.reloaded", "bundle.error", "refresh.applied", "render.updated", "navigation.changed", "module.unsupported",
    "module.called",
];

/// A JSON-RPC error: `code` is the JSON-RPC code, `kind` the OneSim `data.code`.
#[derive(Debug)]
pub struct RpcError {
    code: i64,
    kind: String,
    message: String,
    session: Option<String>,
}

type R = Result<Value, RpcError>;

fn e(code: i64, kind: &str, message: impl Into<String>) -> RpcError {
    RpcError { code, kind: kind.into(), message: message.into(), session: None }
}

fn bad(message: impl Into<String>) -> RpcError {
    e(-32602, "bad_request", message)
}

impl From<ApiError> for RpcError {
    fn from(a: ApiError) -> Self {
        let kind = serde_json::to_value(a.code).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        e(if a.code == ErrorCode::BadRequest { -32602 } else { -32000 }, &kind, a.message)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn args<T: serde::de::DeserializeOwned>(p: &Value) -> Result<T, RpcError> {
    serde_json::from_value(p.clone()).map_err(|x| bad(format!("invalid params: {x}")))
}

/// Restores `Session::settle = true` on drop so panics can never leave `settle == false`.
struct SettleGuard<'a> {
    host: &'a mut Host,
    sid: &'a str,
}

impl<'a> SettleGuard<'a> {
    fn new(host: &'a mut Host, sid: &'a str, settle: bool) -> Self {
        host.set_settle(sid, settle);
        Self { host, sid }
    }
}

impl Drop for SettleGuard<'_> {
    fn drop(&mut self) {
        self.host.set_settle(self.sid, true);
    }
}

// --- typed parameter structs ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SessionIdP {
    #[serde(default)]
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsSubscribeP {
    #[serde(default)]
    session_id: String,
    names: Option<Vec<String>>,
    since: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeLoadP {
    #[serde(default)]
    session_id: String,
    metro_url: Option<String>,
    app_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetP {
    #[serde(default)]
    session_id: String,
    x: Option<f64>,
    y: Option<f64>,
    element: Option<String>,
    label: Option<String>,
    duration_ms: Option<u64>,
    settle: Option<bool>,
    dx: Option<f64>,
    dy: Option<f64>,
}

#[derive(Deserialize)]
struct Pt {
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwipeP {
    #[serde(default)]
    session_id: String,
    from: Option<Pt>,
    to: Option<Pt>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeP {
    #[serde(default)]
    session_id: String,
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyP {
    #[serde(default)]
    session_id: String,
    key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderP {
    #[serde(default)]
    session_id: String,
    format: Option<String>,
    scale: Option<f64>,
    width: Option<u32>,
    height: Option<u32>,
    settle: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendP {
    #[serde(default)]
    session_id: String,
    renderer: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectP {
    #[serde(default)]
    session_id: String,
    element: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSetP {
    #[serde(default)]
    session_id: String,
    device: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceRotateP {
    #[serde(default)]
    session_id: String,
    orientation: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppearanceP {
    #[serde(default)]
    session_id: String,
    mode: Option<Appearance>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyboardP {
    #[serde(default)]
    session_id: String,
    visible: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleSimulateP {
    #[serde(default)]
    session_id: String,
    module: Option<String>,
    state: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateP {
    framework: String,
    platform: Option<String>,
    device: Option<String>,
    appearance: Option<Appearance>,
    dynamic_type_size: Option<DynamicTypeSize>,
    os_version: Option<IosVersion>,
    metro_url: Option<String>,
    app_path: Option<String>,
    root_view: Option<String>,
    viewport: Option<Viewport>,
    scale: Option<f64>,
    #[serde(default)]
    load: bool,
    /// Test hook, like v1: spawn `appPath` directly without building.
    #[serde(default)]
    prebuilt: bool,
    #[serde(default)]
    pasteboard: Option<String>,
}

/// `iphone-17-pro` -> `iPhone 17 Pro`.
fn display_name(slug: &str) -> String {
    slug.split('-')
        .map(|w| match w {
            "iphone" => "iPhone".into(),
            "se" => "SE".into(),
            w if w.starts_with(|c: char| c.is_ascii_digit()) => w.to_string(),
            w => w[..1].to_uppercase() + &w[1..],
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Accepts the display name (any case) or the slug.
fn resolve_device(name: &str) -> Result<&'static str, RpcError> {
    let slug = name.trim().to_lowercase().replace(' ', "-");
    DEVICES.iter().find(|d| d.name == slug).map(|d| d.name).ok_or_else(|| {
        let known: Vec<String> = DEVICES.iter().map(|d| display_name(d.name)).collect();
        e(-32000, "unsupported", format!("device {name:?} (known: {})", known.join(", ")))
    })
}

struct Entry {
    id: String,
    host: Mutex<Host>,
    log: Arc<EventLog>,
    /// Last `SessionInfo`, for `session.get/list` while a long call holds the host.
    info: Mutex<Value>,
}

impl Entry {
    fn info(&self, h: &mut Host) -> Result<Value, ApiError> {
        let s = h.get(&self.id)?;
        let v = json!({
            "sessionId": self.id, "framework": s.params.framework, "platform": s.params.platform,
            "device": display_name(s.device.name), "osVersion": s.os_version(), "viewport": s.viewport, "scale": s.scale,
            "appearance": s.appearance, "status": s.state().as_str(), "treeVersion": s.tree_version().unwrap_or(0),
            "metroUrl": s.params.metro_url, "appPath": (!s.params.app_path.is_empty()).then_some(&s.params.app_path),
        });
        let v = Value::Object(v.as_object().expect("object").iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect());
        *lock(&self.info) = v.clone();
        Ok(v)
    }

    /// Run `f` on the session's host, then refresh the cached info.
    fn with<T>(&self, f: impl FnOnce(&mut Host) -> Result<T, ApiError>) -> Result<T, RpcError> {
        let mut h = lock(&self.host);
        let r = f(&mut h);
        let _ = self.info(&mut h);
        r.map_err(|x| RpcError { session: Some(self.id.clone()), ..RpcError::from(x) })
    }

    /// Run `f` on the session's host without rebuilding `info` JSON (for read-only / high-frequency input calls).
    fn with_ro<T>(&self, f: impl FnOnce(&mut Host) -> Result<T, ApiError>) -> Result<T, RpcError> {
        let mut h = lock(&self.host);
        f(&mut h).map_err(|x| RpcError { session: Some(self.id.clone()), ..RpcError::from(x) })
    }

    fn set_status(&self, status: &str) {
        if let Some(o) = lock(&self.info).as_object_mut() {
            o.insert("status".into(), status.into());
        }
    }

    fn snapshot(&self) -> Value {
        match self.host.try_lock() {
            Ok(mut h) => self.info(&mut h).unwrap_or_else(|_| lock(&self.info).clone()),
            Err(TryLockError::Poisoned(p)) => self.info(&mut p.into_inner()).unwrap_or_default(),
            Err(TryLockError::WouldBlock) => lock(&self.info).clone(),
        }
    }
}

#[derive(Default)]
struct Registry {
    map: BTreeMap<u64, Arc<Entry>>,
    next: u64,
}

pub struct Server {
    reg: Mutex<Registry>,
    conns: AtomicU64,
    fonts: Arc<Mutex<Option<Fonts>>>,
}

impl Server {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            reg: Mutex::new(Registry { next: 1, ..Registry::default() }),
            conns: AtomicU64::new(1),
            fonts: Arc::new(Mutex::new(None)),
        })
    }

    /// A new connection; notifications for its subscriptions are queued on `tx` as JSON text.
    pub fn connect(self: &Arc<Self>, tx: Sender<String>) -> Conn {
        Conn { server: self.clone(), id: self.conns.fetch_add(1, Ordering::Relaxed), initialized: false, tx }
    }

    fn entry(&self, sid: &str) -> Result<Arc<Entry>, RpcError> {
        let n = sid.strip_prefix('s').and_then(|n| n.parse::<u64>().ok());
        n.and_then(|n| lock(&self.reg).map.get(&n).cloned())
            .ok_or_else(|| RpcError { session: Some(sid.into()), ..e(-32000, "not_found", format!("unknown session {sid:?}")) })
    }

    fn entries(&self) -> Vec<Arc<Entry>> {
        lock(&self.reg).map.values().cloned().collect()
    }

    fn create(&self, p: CreateP) -> R {
        let params = CreateSessionParams {
            framework: p.framework,
            platform: p.platform.unwrap_or_else(|| "ios".into()),
            app_path: p.app_path.unwrap_or_default(),
            root_view: p.root_view,
            viewport: p.viewport,
            scale: p.scale,
            appearance: p.appearance,
            dynamic_type_size: p.dynamic_type_size,
            os_version: p.os_version,
            device: p.device.as_deref().map(resolve_device).transpose()?.map(String::from),
            prebuilt: p.prebuilt,
            metro_url: p.metro_url,
            pasteboard: p.pasteboard,
        };
        let entry = {
            let mut reg = lock(&self.reg);
            let id = format!("s{}", reg.next);
            let mut host = Host::with_shared_fonts(self.fonts.clone());
            host.create_session_as(id.clone(), params)?;
            let entry = Arc::new(Entry { log: host.events(&id)?, id, host: Mutex::new(host), info: Mutex::new(Value::Null) });
            let n = reg.next;
            reg.map.insert(n, entry.clone());
            reg.next += 1;
            entry
        };
        entry.with(|_| Ok(()))?;
        if p.load {
            self.load(&entry, None, None)?;
        }
        Ok(json!({"sessionId": entry.id, "status": entry.snapshot()["status"]}))
    }

    fn load(&self, entry: &Entry, metro_url: Option<String>, app_path: Option<String>) -> Result<(), RpcError> {
        entry.set_status("starting");
        entry.with(|h| {
            let s = h.get(&entry.id)?;
            if let Some(m) = metro_url {
                s.params.metro_url = Some(m);
            }
            if let Some(a) = app_path {
                if !s.is_rn() && !std::path::Path::new(&a).exists() {
                    return Err(ApiError::new(ErrorCode::BadRequest, format!("appPath {a:?} does not exist")));
                }
                s.params.app_path = a;
            }
            h.start_session(&entry.id)
        })
    }

    fn destroy(&self, sid: &str) -> R {
        let entry = self.entry(sid)?;
        lock(&self.reg).map.retain(|_, e| !Arc::ptr_eq(e, &entry));
        entry.with(|h| h.stop_session(&entry.id))?;
        Ok(json!({}))
    }
}

pub struct Conn {
    server: Arc<Server>,
    id: u64,
    initialized: bool,
    tx: Sender<String>,
}

fn reply(id: Value, r: R) -> String {
    match r {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(x) => {
            let mut data = json!({"code": x.kind});
            if let Some(s) = x.session {
                data["sessionId"] = s.into();
            }
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": x.code, "message": x.message, "data": data}})
        }
    }
    .to_string()
}

impl Conn {
    /// Handle one text frame; the reply to send back (none for notifications).
    pub fn handle(&mut self, text: &str) -> Option<String> {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return Some(reply(Value::Null, Err(e(-32700, "bad_request", "parse error"))));
        };
        let id = v.get("id").cloned();
        let rid = id.clone().unwrap_or(Value::Null);
        let (Some(method), true) = (v.get("method").and_then(Value::as_str), v.get("jsonrpc") == Some(&json!("2.0"))) else {
            return Some(reply(rid, Err(e(-32600, "bad_request", "invalid request (a single JSON-RPC 2.0 object with a method; no batches)"))));
        };
        let params = match v.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(p) if p.is_object() => p.clone(),
            Some(_) => return Some(reply(rid, Err(bad("params must be an object")))),
        };
        let r = std::panic::catch_unwind(AssertUnwindSafe(|| self.call(method, &params)))
            .unwrap_or_else(|_| Err(e(-32603, "runtime_error", "internal error")));
        id.map(|id| reply(id, r))
    }

    /// The connection closed: drop its subscriptions.
    pub fn close(&self) {
        for en in self.server.entries() {
            en.log.unsubscribe(self.id);
        }
    }

    fn call(&mut self, method: &str, p: &Value) -> R {
        if method == "initialize" {
            return self.initialize(p);
        }
        if !self.initialized {
            return Err(e(-32000, "invalid_state", "first call on a connection must be initialize"));
        }
        let srv = self.server.clone();
        match method {
            "session.create" => srv.create(args(p)?),
            "session.list" => Ok(json!({"sessions": srv.entries().iter().map(|en| en.snapshot()).collect::<Vec<_>>()})),
            "session.get" => Ok(srv.entry(&args::<SessionIdP>(p)?.session_id)?.snapshot()),
            "session.destroy" => srv.destroy(&args::<SessionIdP>(p)?.session_id),
            "device.list" => Ok(json!({"devices": DEVICES.iter().map(|d| json!({
                "name": d.name, "displayName": display_name(d.name), "platform": if d.android { "android" } else { "ios" }, "viewport": d.viewport, "scale": d.scale,
                "safeArea": {"top": d.safe_top, "bottom": d.safe_bottom}})).collect::<Vec<_>>()})),
            "events.subscribe" => {
                let a: EventsSubscribeP = args(p)?;
                let last = srv.entry(&a.session_id)?.log.subscribe(self.id, self.tx.clone(), a.names, a.since);
                Ok(json!({"subscribed": true, "lastSeq": last}))
            }
            "events.unsubscribe" => {
                srv.entry(&args::<SessionIdP>(p)?.session_id)?.log.unsubscribe(self.id);
                Ok(json!({}))
            }
            m if METHODS.contains(&m) => session_call(&srv, m, p),
            m => Err(e(-32601, "method_not_found", format!("unknown method {m:?}"))),
        }
    }

    fn initialize(&mut self, p: &Value) -> R {
        let v = p.get("protocolVersion").and_then(Value::as_str).ok_or_else(|| bad("protocolVersion (string) required"))?;
        if v.split('.').next() != Some("2") {
            return Err(e(-32000, "protocol_version", format!("server speaks protocol {API_VERSION} (2.x), client sent {v}")));
        }
        self.initialized = true;
        Ok(json!({
            "protocolVersion": API_VERSION,
            "server": {"name": "onesim", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {
                "frameworks": ["swiftui", "react-native", "compose"], "platforms": ["ios", "android"], "methods": METHODS, "events": EVENTS,
                "devices": DEVICES.iter().map(|d| d.name).collect::<Vec<_>>(), "screenshotFormats": ["png"],
                "fonts": onesim_render::fonts_capability(), "symbols": onesim_render::symbols_capability(),
            }
        }))
    }
}

fn find_chain<'a>(n: &'a UiNode, pred: &dyn Fn(&UiNode) -> bool, chain: &mut Vec<&'a UiNode>) -> bool {
    chain.push(n);
    if pred(n) || n.children.iter().any(|c| find_chain(c, pred, chain)) {
        return true;
    }
    chain.pop();
    false
}

/// Root-to-node chain ending at the deepest (topmost painted) node containing the point.
fn deepest<'a>(n: &'a UiNode, x: f64, y: f64, chain: &mut Vec<&'a UiNode>) -> bool {
    let f = n.frame;
    if !(x >= f.x && x < f.x + f.width && y >= f.y && y < f.y + f.height) {
        return false;
    }
    chain.push(n);
    let _ = n.children.iter().rev().any(|c| deepest(c, x, y, chain));
    true
}

fn center(n: &UiNode) -> (f64, f64) {
    (n.frame.x + n.frame.width / 2.0, n.frame.y + n.frame.height / 2.0)
}

/// Where an input lands: `element` (UiNode centre), `label` (CLI `--click` syntax) or a point.
fn target(h: &mut Host, sid: &str, a: &TargetP) -> Result<(f64, f64), ApiError> {
    if let Some(el) = &a.element {
        let (_, root) = h.inspect(sid)?;
        let mut chain = vec![];
        return find_chain(&root, &|n| &n.id == el, &mut chain)
            .then(|| center(chain.last().expect("non-empty")))
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no element {el:?}")));
    }
    if let Some(l) = &a.label {
        return onesim_render::resolve_label(&h.inspect(sid)?.1, l).map_err(|m| ApiError::new(ErrorCode::NotFound, m));
    }
    match (a.x, a.y) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(ApiError::new(ErrorCode::BadRequest, "give a target: x and y, element or label")),
    }
}

fn session_call(srv: &Server, method: &str, p: &Value) -> R {
    let b64 = |png: Vec<u8>| base64::engine::general_purpose::STANDARD.encode(png);
    match method {
        "runtime.load" => {
            let a: RuntimeLoadP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            srv.load(&en, a.metro_url, a.app_path)?;
            Ok(json!({"status": "running"}))
        }
        "runtime.reload" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            let sid = en.id.as_str();
            en.with(|h| {
                let s = h.get(sid)?;
                if !s.is_rn() {
                    return Err(ApiError::new(ErrorCode::Unsupported, "reload exists only for react-native sessions"));
                }
                s.reload()?;
                let _ = h.scene_within(sid, STARTUP_MAX);
                Ok(json!({"status": "running"}))
            })
        }
        "runtime.pause" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            en.with(|h| h.get(&en.id)?.pause()).map(|_| json!({"status": "paused"}))
        }
        "runtime.resume" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            en.with(|h| h.get(&en.id)?.resume()).map(|_| json!({"status": "running"}))
        }
        "runtime.stop" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            en.with(|h| h.stop_session(&en.id)).map(|_| json!({"status": "stopped"}))
        }
        "runtime.status" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            let sid = en.id.as_str();
            en.with_ro(|h| {
                let errors = h.errors(sid)?;
                let s = h.get(sid)?;
                Ok(json!({"status": s.state().as_str(), "treeVersion": s.tree_version().unwrap_or(0), "errors": errors}))
            })
        }
        "input.tap" | "input.scroll" => {
            let a: TargetP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let sid = en.id.as_str();
            en.with_ro(|h| {
                let g = SettleGuard::new(h, sid, a.settle != Some(false));
                g.host.get(sid)?.require_running()?;
                let (x, y) = target(g.host, sid, &a)?;
                if method == "input.tap" {
                    let hit = g.host.pointer(sid, PointerEvent::new(PointerKind::Click, x, y))?.0;
                    Ok(json!({"hit": hit}))
                } else {
                    let ev = PointerEvent::moved(PointerKind::Scroll, x, y, a.dx.unwrap_or(0.0), a.dy.unwrap_or(0.0));
                    g.host.pointer(sid, ev)?;
                    Ok(json!({}))
                }
            })
        }
        "input.press" => {
            let a: TargetP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let sid = en.id.as_str();
            let settle = a.settle != Some(false);
            let (x, y) = en.with_ro(|h| {
                let g = SettleGuard::new(h, sid, settle);
                g.host.get(sid)?.require_running()?;
                let (x, y) = target(g.host, sid, &a)?;
                g.host.pointer(sid, PointerEvent::new(PointerKind::Down, x, y))?;
                Ok((x, y))
            })?;
            // Release `en.host` mutex while sleeping so status/other reads aren't blocked.
            std::thread::sleep(Duration::from_millis(a.duration_ms.unwrap_or(500).min(60_000)));
            en.with_ro(|h| {
                let g = SettleGuard::new(h, sid, settle);
                let hit = g.host.pointer(sid, PointerEvent::new(PointerKind::Up, x, y))?.0;
                Ok(json!({"hit": hit}))
            })
        }
        "input.swipe" => {
            let a: SwipeP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let (Some(f), Some(t)) = (&a.from, &a.to) else { return Err(bad("from and to ({x,y}) required")) };
            let ev = PointerEvent::moved(PointerKind::Scroll, f.x, f.y, f.x - t.x, f.y - t.y);
            en.with_ro(|h| h.pointer(&en.id, ev)).map(|_| json!({}))
        }
        "input.type" => {
            let a: TypeP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let text = a.text.ok_or_else(|| bad("text required"))?;
            en.with_ro(|h| h.keyboard(&en.id, KeyboardEvent::Text { text })).map(|_| json!({}))
        }
        "input.key" => {
            let a: KeyP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let key = a.key.filter(|k| matches!(k.as_str(), "enter" | "backspace")).ok_or_else(|| bad("key must be \"enter\" or \"backspace\""))?;
            en.with_ro(|h| h.keyboard(&en.id, KeyboardEvent::Key { key })).map(|_| json!({}))
        }
        "render.screenshot" | "render.frame" => {
            let a: RenderP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let sid = en.id.as_str();
            if a.format.as_deref().is_some_and(|f| f != "png") {
                return Err(e(-32000, "unsupported", "screenshot format must be \"png\""));
            }
            let frame = method == "render.frame";
            let (scale, width, height) = if frame { (None, None, None) } else { (a.scale, a.width, a.height) };
            let (w, h_, png, v) = en.with_ro(|h| {
                let g = SettleGuard::new(h, sid, a.settle != Some(false));
                g.host.screenshot_sized(sid, scale, width, height)
            })?;
            Ok(if frame {
                json!({"treeVersion": v, "width": w, "height": h_, "data": b64(png)})
            } else {
                json!({"format": "png", "width": w, "height": h_, "data": b64(png)})
            })
        }
        "render.backend" => {
            let a: BackendP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let pref = a.renderer.as_deref().map(|r| r.parse::<onesim_render::BackendPref>().map_err(bad)).transpose()?;
            let d = en.with_ro(|h| {
                if let Some(p) = pref {
                    h.set_renderer(p);
                }
                h.renderer_diagnostics()
            })?;
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            Ok(json!({
                "backend": d.backend, "adapter": d.adapter, "limitations": d.limitations,
                "fallbackReason": d.fallback_reason, "frames": d.frames,
                "lastFrameMs": {"encode": ms(d.last_frame.encode), "gpu": ms(d.last_frame.gpu), "readback": ms(d.last_frame.readback)},
            }))
        }
        "render.tree" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            en.with_ro(|h| h.inspect(&en.id)).map(|(v, root)| json!({"treeVersion": v, "root": root}))
        }
        "render.inspect" => {
            let a: InspectP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let (_, root) = en.with_ro(|h| h.inspect(&en.id))?;
            let mut chain = vec![];
            let found = match (&a.element, a.x, a.y) {
                (Some(el), ..) => find_chain(&root, &|n| &n.id == el, &mut chain),
                (None, Some(x), Some(y)) => deepest(&root, x, y, &mut chain),
                _ => return Err(bad("give element or x and y")),
            };
            match chain.last().filter(|_| found) {
                Some(n) => Ok(json!({"node": n, "path": chain.iter().map(|n| &n.id).collect::<Vec<_>>()})),
                None => Err(RpcError { session: Some(en.id.clone()), ..e(-32000, "not_found", "nothing there") }),
            }
        }
        "device.set" => {
            let a: DeviceSetP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let d = resolve_device(a.device.as_deref().ok_or_else(|| bad("device required"))?)?;
            en.with(|h| h.set_device(&en.id, d))?;
            Ok(en.snapshot())
        }
        "device.rotate" => {
            let a: DeviceRotateP = args(p)?;
            let _ = srv.entry(&a.session_id)?;
            match a.orientation.as_deref() {
                Some("portrait") => Ok(json!({})),
                Some("landscape") => Err(e(-32000, "unsupported", "landscape is not supported in v0.2")),
                _ => Err(bad("orientation must be \"portrait\" or \"landscape\"")),
            }
        }
        "device.appearance" => {
            let a: AppearanceP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let m = a.mode.ok_or_else(|| bad("mode must be \"light\" or \"dark\""))?;
            en.with(|h| h.set_appearance(&en.id, m)).map(|_| json!({}))
        }
        "device.keyboard" => {
            let a: KeyboardP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let visible = a.visible.ok_or_else(|| bad("visible (bool) required"))?;
            en.with_ro(|h| {
                if visible {
                    let s = h.get(&en.id)?;
                    s.require_running()?;
                    if s.inter.focus.is_none() {
                        return Err(ApiError::new(ErrorCode::InvalidState, "no focused text field to show the keyboard for"));
                    }
                    return Ok(());
                }
                h.hide_keyboard(&en.id)
            })
            .map(|_| json!({}))
        }
        "modules.list" => {
            let en = srv.entry(&args::<SessionIdP>(p)?.session_id)?;
            en.with_ro(|h| h.modules(&en.id)).map(|m| json!({"modules": m}))
        }
        "modules.simulate" => {
            let a: ModuleSimulateP = args(p)?;
            let en = srv.entry(&a.session_id)?;
            let module = a.module.ok_or_else(|| bad("module required"))?;
            let state = a.state.filter(Value::is_object).ok_or_else(|| bad("state (object) required"))?;
            en.with_ro(|h| h.simulate(&en.id, module, state)).map(|_| json!({}))
        }
        m => Err(e(-32601, "method_not_found", format!("unknown method {m:?}"))),
    }
}
