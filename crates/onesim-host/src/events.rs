//! Per-session event log for the v2 API (docs/api.md "Events"): seq from 1, ring buffer of the last
//! 1000, pushed to subscribers as pre-serialized JSON-RPC notifications in seq order.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BUFFER: usize = 1000;
const RENDER_GAP: Duration = Duration::from_millis(16);

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
    last_render: Option<Instant>,
    /// Tree version of a `render.updated` held back by coalescing (delivered by a trailing timer).
    pending_render: Option<u64>,
    timer_armed: bool,
}

pub struct EventLog {
    session_id: String,
    inner: Mutex<Inner>,
}

impl EventLog {
    pub fn new(session_id: impl Into<String>) -> Arc<Self> {
        Arc::new(Self { session_id: session_id.into(), inner: Mutex::default() })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn notification(&self, e: &Event) -> String {
        json!({"jsonrpc": "2.0", "method": "event", "params": {
            "sessionId": self.session_id, "seq": e.seq, "name": e.name, "time": e.time, "data": e.data}})
        .to_string()
    }

    pub fn emit(&self, name: &str, data: Value) {
        emit_locked(self, &mut self.lock(), name, data);
    }

    /// `render.updated`, at most one per 16 ms; a skipped one is delivered once the gap has passed.
    pub fn render_updated(self: &Arc<Self>, version: u64) {
        let mut g = self.lock();
        let due = g.last_render.is_none_or(|t| t.elapsed() >= RENDER_GAP);
        if due && !g.timer_armed {
            g.last_render = Some(Instant::now());
            emit_locked(self, &mut g, "render.updated", json!({"treeVersion": version}));
            return;
        }
        g.pending_render = Some(version);
        if g.timer_armed {
            return;
        }
        g.timer_armed = true;
        let wait = RENDER_GAP.saturating_sub(g.last_render.map_or(RENDER_GAP, |t| t.elapsed()));
        let log = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            let mut g = log.lock();
            g.timer_armed = false;
            if let Some(v) = g.pending_render.take() {
                g.last_render = Some(Instant::now());
                emit_locked(&log, &mut g, "render.updated", json!({"treeVersion": v}));
            }
        });
    }

    /// Replay buffered events with `seq > since` (matching `names`), then register `tx` for new ones.
    /// Atomic with respect to `emit`, so nothing is lost, duplicated or reordered. Returns the last seq.
    pub fn subscribe(&self, conn: u64, tx: Sender<String>, names: Option<Vec<String>>, since: Option<u64>) -> u64 {
        let mut g = self.lock();
        let wanted = |n: &str| names.as_ref().is_none_or(|l| l.iter().any(|x| x == n));
        if let Some(since) = since {
            for e in g.buf.iter().filter(|e| e.seq > since && wanted(&e.name)) {
                let _ = tx.send(self.notification(e));
            }
        }
        g.subs.retain(|s| s.conn != conn);
        g.subs.push(Sub { conn, names, tx });
        g.seq
    }

    pub fn unsubscribe(&self, conn: u64) {
        self.lock().subs.retain(|s| s.conn != conn);
    }
}

fn emit_locked(log: &EventLog, g: &mut Inner, name: &str, data: Value) {
    g.seq += 1;
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let e = Event { seq: g.seq, name: name.into(), time, data };
    let text = log.notification(&e);
    g.subs.retain(|s| !s.names.as_ref().is_none_or(|l| l.iter().any(|x| x == name)) || s.tx.send(text.clone()).is_ok());
    if g.buf.len() == BUFFER {
        g.buf.pop_front();
    }
    g.buf.push_back(e);
}
