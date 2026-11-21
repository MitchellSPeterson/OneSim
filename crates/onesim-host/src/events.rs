//! Per-session event log for the v2 API (docs/api.md "Events"): seq from 1, ring buffer of the last
//! 1000, pushed to subscribers as pre-serialized JSON-RPC notifications in seq order.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BUFFER: usize = 1000;
const RENDER_GAP: Duration = Duration::from_millis(16);

/// Typed in-process event for native listeners (avoids JSON serialize + re-parse).
#[derive(Debug, Clone)]
pub enum HostEvent {
    RenderUpdated { tree_version: u64 },
    RuntimeStopped { reason: String },
    Other { name: String, data: Value },
}

#[derive(Clone)]
struct Event {
    seq: u64,
    name: String,
    time: u64,
    data: Value,
}

struct Sub {
    conn: u64,
    names: Option<Vec<String>>,
    tx: Sender<String>,
}

#[derive(Default)]
struct Inner {
    seq: u64,
    buf: VecDeque<Event>,
    subs: Vec<Sub>,
    typed_subs: Vec<Sender<HostEvent>>,
    last_render: Option<Instant>,
    /// Tree version of a `render.updated` held back by coalescing (delivered by the timer thread).
    pending_render: Option<u64>,
    timer_started: bool,
    closed: bool,
}

struct State {
    session_id: String,
    inner: Mutex<Inner>,
    cv: Condvar,
}

impl State {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct EventLog {
    state: Arc<State>,
}

impl Drop for EventLog {
    fn drop(&mut self) {
        let mut g = self.state.lock();
        g.closed = true;
        self.state.cv.notify_all();
    }
}

impl EventLog {
    pub fn new(session_id: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(State {
                session_id: session_id.into(),
                inner: Mutex::default(),
                cv: Condvar::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.state.lock()
    }

    pub fn emit(&self, name: &str, data: Value) {
        emit_locked(&self.state.session_id, &mut self.lock(), name, data);
    }

    /// `render.updated`, at most one per 16 ms; a skipped one is delivered once the gap has passed.
    pub fn render_updated(self: &Arc<Self>, version: u64) {
        let mut g = self.lock();
        let due = g.last_render.is_none_or(|t| t.elapsed() >= RENDER_GAP);
        if due && g.pending_render.is_none() {
            g.last_render = Some(Instant::now());
            emit_locked(&self.state.session_id, &mut g, "render.updated", json!({"treeVersion": version}));
            return;
        }
        g.pending_render = Some(version);
        if !g.timer_started {
            g.timer_started = true;
            let state = self.state.clone();
            std::thread::spawn(move || timer_loop(state));
        }
        self.state.cv.notify_one();
    }

    /// Replay buffered events with `seq > since` (matching `names`), then register `tx` for new ones.
    /// Atomic with respect to `emit`, so nothing is lost, duplicated or reordered. Returns the last seq.
    pub fn subscribe(&self, conn: u64, tx: Sender<String>, names: Option<Vec<String>>, since: Option<u64>) -> u64 {
        let mut g = self.lock();
        let wanted = |n: &str| names.as_ref().is_none_or(|l| l.iter().any(|x| x == n));
        if let Some(since) = since {
            for e in g.buf.iter().filter(|e| e.seq > since && wanted(&e.name)) {
                let _ = tx.send(notification(&self.state.session_id, e));
            }
        }
        g.subs.retain(|s| s.conn != conn);
        g.subs.push(Sub { conn, names, tx });
        g.seq
    }

    /// Register an in-process typed listener (no JSON serialization overhead).
    pub fn subscribe_typed(&self, tx: Sender<HostEvent>) {
        self.lock().typed_subs.push(tx);
    }

    pub fn unsubscribe(&self, conn: u64) {
        self.lock().subs.retain(|s| s.conn != conn);
    }
}

fn timer_loop(state: Arc<State>) {
    let mut g = state.lock();
    loop {
        while g.pending_render.is_none() && !g.closed {
            g = state.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        if g.closed {
            break;
        }
        let wait = RENDER_GAP.saturating_sub(g.last_render.map_or(RENDER_GAP, |t| t.elapsed()));
        if !wait.is_zero() {
            g = state.cv.wait_timeout(g, wait).unwrap_or_else(|e| e.into_inner()).0;
            if g.closed {
                break;
            }
            if g.last_render.is_some_and(|t| t.elapsed() < RENDER_GAP) {
                continue;
            }
        }
        if let Some(v) = g.pending_render.take() {
            g.last_render = Some(Instant::now());
            emit_locked(&state.session_id, &mut g, "render.updated", json!({"treeVersion": v}));
        }
    }
}

fn notification(session_id: &str, e: &Event) -> String {
    json!({"jsonrpc": "2.0", "method": "event", "params": {
        "sessionId": session_id, "seq": e.seq, "name": e.name, "time": e.time, "data": e.data}})
    .to_string()
}

fn to_host_event(name: &str, data: &Value) -> HostEvent {
    match name {
        "render.updated" => HostEvent::RenderUpdated {
            tree_version: data["treeVersion"].as_u64().unwrap_or(u64::MAX),
        },
        "runtime.stopped" => HostEvent::RuntimeStopped {
            reason: data["reason"].as_str().unwrap_or("stopped").to_string(),
        },
        _ => HostEvent::Other { name: name.to_string(), data: data.clone() },
    }
}

fn emit_locked(session_id: &str, g: &mut Inner, name: &str, data: Value) {
    g.seq += 1;
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    if !g.typed_subs.is_empty() {
        let ev = to_host_event(name, &data);
        g.typed_subs.retain(|tx| tx.send(ev.clone()).is_ok());
    }
    let e = Event { seq: g.seq, name: name.into(), time, data };
    if !g.subs.is_empty() {
        let text = notification(session_id, &e);
        g.subs.retain(|s| !s.names.as_ref().is_none_or(|l| l.iter().any(|x| x == name)) || s.tx.send(text.clone()).is_ok());
    }
    if g.buf.len() == BUFFER {
        g.buf.pop_front();
    }
    g.buf.push_back(e);
}
