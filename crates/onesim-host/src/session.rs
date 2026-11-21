//! One guest OS process and everything we know about it.

use crate::events::EventLog;
use crate::process::{build_app, is_prebuilt, onesim_home, signal, SIGCONT, SIGSTOP};
use onesim_protocol::{
    ApiError, Appearance, CreateSessionParams, DynamicTypeSize, ErrorCode, GeoBox, GuestMessage, HostMessage, IosVersion, SessionError, Viewport,
    ViewNode,
};
use onesim_render::rn_input::{focus_change, Out, RnInput, Wait};
pub use onesim_render::{device, Device, DEFAULT_DEVICE, DEVICES};
use onesim_render::{Animator, Effect, Interaction, Scene, SwiftUiLayoutCache};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(300);
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const SETTLE_TAIL: Duration = Duration::from_millis(800);
const MAX_ERRORS: usize = 1000;
/// Guest stderr lines kept (oldest evicted), separate from `errors` so a chatty guest can't starve real errors.
const MAX_LOG_LINES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Created,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed,
    Crashed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Created => "created",
            State::Starting => "starting",
            State::Running => "running",
            State::Paused => "paused",
            State::Stopping => "stopping",
            State::Stopped => "stopped",
            State::Failed => "failed",
            State::Crashed => "crashed",
        }
    }
}

/// State shared with the guest reader threads.
struct Inner {
    tree: Option<(u64, Arc<ViewNode>)>,
    /// Number of trees received; lets waiters detect "a new tree arrived".
    generation: u64,
    errors: Vec<SessionError>,
    /// Deduplication set for `report_once` `(origin, message)` pairs.
    reported: HashSet<(String, String)>,
    /// Last `MAX_LOG_LINES` guest stderr lines (reported by `get_errors` with source `stderr`).
    logs: VecDeque<SessionError>,
    state: State,
    exited: bool,
    /// When the last tree or guest request arrived (settling).
    last_event: Option<Instant>,
    /// React Native `focus_request` / `scroll_to` messages not yet applied by the host.
    requests: Vec<GuestMessage>,
    /// React Native: the guest (re)started (`hello`); the next layout pass resends every frame.
    restarted: bool,
    /// React Native native-module inventory (`modules` message), once the guest has sent it.
    modules: Option<Value>,
    /// The next tree is the first after load/reload: it raises `runtime.ready`.
    ready_pending: bool,
    /// The host is stopping the guest on purpose (its exit is not a crash).
    stopping: bool,
    stopped_emitted: bool,
    /// SwiftUI animation wake-up (section P): `Some(Some(ms))` = request tick in `ms`; `Some(None)` = cancel.
    schedule_update: Option<Option<f64>>,
    /// Bumped on every `schedule` message so `send_tick_and_wait` unblocks even when no tree changes.
    schedule_gen: u64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            tree: None,
            generation: 0,
            errors: Vec::new(),
            reported: HashSet::new(),
            logs: VecDeque::new(),
            state: State::Created,
            exited: false,
            last_event: None,
            requests: Vec::new(),
            restarted: false,
            modules: None,
            ready_pending: true,
            stopping: false,
            stopped_emitted: false,
            schedule_update: None,
            schedule_gen: 0,
        }
    }
}

struct Shared {
    inner: Mutex<Inner>,
    cv: Condvar,
    log: Arc<EventLog>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn new(log: Arc<EventLog>) -> Self {
        Self { inner: Mutex::new(Inner::default()), cv: Condvar::new(), log }
    }

    /// Wait on the condvar for one wakeup or `timeout`.
    fn wait<'a>(&self, g: MutexGuard<'a, Inner>, timeout: Duration) -> MutexGuard<'a, Inner> {
        self.cv.wait_timeout(g, timeout).unwrap_or_else(|e| e.into_inner()).0
    }

    /// Wait until `generation` moves past `before` (a new tree), the guest exits, or `timeout`.
    /// Returns the guard; callers compare `g.generation` to `before`.
    fn wait_generation<'a>(&self, g: MutexGuard<'a, Inner>, before: u64, timeout: Duration) -> MutexGuard<'a, Inner> {
        self.cv.wait_timeout_while(g, timeout, |i| i.generation == before && !i.exited).unwrap_or_else(|e| e.into_inner()).0
    }

    fn push_error(&self, source: &str, message: String) {
        let mut g = self.lock();
        if source == "stderr" {
            if g.logs.len() == MAX_LOG_LINES {
                g.logs.pop_front();
            }
            g.logs.push_back(SessionError { message, source: source.into() });
        } else if g.errors.len() < MAX_ERRORS {
            g.errors.push(SessionError { message: message.clone(), source: source.into() });
            drop(g);
            self.log.emit("runtime.error", json!({"message": message, "source": source}));
        }
    }

    /// `runtime.stopped`, at most once per session.
    fn stopped(&self, reason: &str) {
        let mut g = self.lock();
        if !std::mem::replace(&mut g.stopped_emitted, true) {
            drop(g);
            self.log.emit("runtime.stopped", json!({"reason": reason}));
        }
    }
}

struct CachedScene {
    generation: u64,
    epoch: u64,
    version: u64,
    scene: Arc<Scene>,
}

pub struct Session {
    pub id: String,
    pub params: CreateSessionParams,
    pub device: Device,
    pub viewport: Viewport,
    pub scale: f64,
    pub appearance: Appearance,
    pub dynamic_type: DynamicTypeSize,
    /// Simulated iOS release (look + reported OS version).
    pub ios: IosVersion,
    /// Scroll, focus, shift and press state (shared with the browser backend).
    pub inter: Interaction,
    /// SwiftUI animation / transition state machine (section P).
    pub anim: Animator,
    /// Monotonic animation clock in milliseconds (starts at 0).
    pub clock_ms: f64,
    /// Last `clock_ms` delivered to the SwiftUI guest via `HostMessage::Tick`.
    pub guest_clock_ms: f64,
    /// Next `clock_ms` at which the SwiftUI guest needs a `tick` message.
    pub guest_due_ms: Option<f64>,
    /// Tree generation last ingested by `anim.update`.
    pub last_anim_gen: u64,
    /// React Native: touch in progress and frames the guest already has (shared with the browser backend).
    pub rn: RnInput,
    /// SwiftUI: last `scroll_position` target sent per scroll-node id.
    pub sent_positions: BTreeMap<String, Option<String>>,
    /// SwiftUI: last `geometry` box sent per reader id, so a stable frame is not sent again.
    pub sent_geometry: BTreeMap<String, GeoBox>,
    /// React Native: `false` (live viewers, set per call by the v2 API) skips every wait for the guest:
    /// inputs return once sent and snapshots take one layout pass; later trees come as `render.updated`.
    pub settle: bool,
    /// Reusable SwiftUI layout cache across frames.
    pub layout_cache: SwiftUiLayoutCache,
    /// Tree generation currently bound to `layout_cache`.
    pub layout_cache_gen: u64,
    /// Cached laid-out scene for the current `(generation, layout_epoch)`.
    cached_scene: Option<CachedScene>,
    /// Bumped whenever interaction, animation, viewport, appearance, or device changes.
    layout_epoch: u64,
    /// When the host last wrote to the guest: its reaction (new trees) is due within the quiet window.
    last_sent: Instant,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    shared: Arc<Shared>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            signal(c, SIGCONT);
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

impl Session {
    pub fn new(id: String, p: CreateSessionParams, log: Arc<EventLog>) -> Self {
        let dev = *device(p.device.as_deref().unwrap_or(DEFAULT_DEVICE)).expect("device validated by create_session");
        Self {
            id,
            device: dev,
            viewport: p.viewport.unwrap_or(dev.viewport),
            scale: p.scale.unwrap_or(dev.scale),
            appearance: p.appearance.unwrap_or(Appearance::Light),
            dynamic_type: p.dynamic_type_size.unwrap_or_default(),
            ios: p.os_version.unwrap_or_default(),
            inter: Interaction::default(),
            anim: Animator::new(),
            clock_ms: 0.0,
            guest_clock_ms: 0.0,
            guest_due_ms: None,
            last_anim_gen: 0,
            rn: RnInput::default(),
            sent_positions: BTreeMap::new(),
            sent_geometry: BTreeMap::new(),
            settle: true,
            layout_cache: SwiftUiLayoutCache::default(),
            layout_cache_gen: 0,
            cached_scene: None,
            layout_epoch: 0,
            last_sent: Instant::now(),
            params: p,
            child: None,
            stdin: None,
            shared: Arc::new(Shared::new(log)),
        }
    }

    pub fn cached_scene(&self, generation: u64) -> Option<(Arc<Scene>, u64)> {
        self.cached_scene
            .as_ref()
            .filter(|c| c.generation == generation && c.epoch == self.layout_epoch)
            .map(|c| (c.scene.clone(), c.version))
    }

    pub fn cache_scene(&mut self, generation: u64, version: u64, scene: Arc<Scene>) {
        self.cached_scene = Some(CachedScene { generation, epoch: self.layout_epoch, version, scene });
    }

    pub fn invalidate_scene(&mut self) {
        self.layout_epoch = self.layout_epoch.wrapping_add(1);
        self.cached_scene = None;
    }

    pub fn is_rn(&self) -> bool {
        self.params.framework == "react-native"
    }

    pub fn is_swiftui(&self) -> bool {
        self.params.framework == "swiftui"
    }

    pub fn os_version(&self) -> &'static str {
        if self.device.android { "17" } else { self.ios.name() }
    }

    /// Jetpack Compose on Android (protocol section K).
    pub fn is_compose(&self) -> bool {
        self.params.framework == "compose"
    }

    /// Current state (single source of truth in `Inner`).
    pub fn state(&self) -> State {
        self.shared.lock().state
    }

    pub fn log(&self) -> Arc<EventLog> {
        self.shared.log.clone()
    }

    pub fn tree_version(&self) -> Option<u64> {
        self.shared.lock().tree.as_ref().map(|t| t.0)
    }

    pub fn modules(&self) -> Option<Value> {
        self.shared.lock().modules.clone()
    }

    pub fn tree(&self) -> Option<(u64, Arc<ViewNode>)> {
        self.shared.lock().tree.clone()
    }

    /// Record a host-side error once per distinct `(origin, message)` atomically.
    pub fn report_once(&self, origin: &str, message: String) {
        let mut g = self.shared.lock();
        if g.errors.len() < MAX_ERRORS && g.reported.insert((origin.to_string(), message.clone())) {
            g.errors.push(SessionError { message: message.clone(), source: origin.into() });
            drop(g);
            self.shared.log.emit("runtime.error", json!({"message": message, "source": origin}));
        }
    }

    pub fn errors(&self) -> Vec<SessionError> {
        let g = self.shared.lock();
        g.errors.iter().chain(&g.logs).cloned().collect()
    }

    pub fn require_running(&self) -> Result<(), ApiError> {
        match self.state() {
            State::Running => Ok(()),
            s => Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, s.as_str()))),
        }
    }

    fn fail(&mut self, e: ApiError) -> ApiError {
        self.shared.push_error("host", e.message.clone());
        self.shared.lock().state = State::Failed;
        self.kill();
        self.shared.stopped("failed");
        e
    }

    fn kill(&mut self) {
        self.shared.lock().stopping = true;
        self.stdin = None;
        if let Some(mut c) = self.child.take() {
            signal(&c, SIGCONT);
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Build (unless prebuilt), spawn the guest and block until its first tree.
    pub fn start(&mut self) -> Result<(), ApiError> {
        {
            let mut g = self.shared.lock();
            if g.state != State::Created {
                return Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, g.state.as_str())));
            }
            g.state = State::Starting;
        }
        let cmd = if self.is_rn() && !self.params.prebuilt {
            // No build step: node runs the Expo/Metro bundle (frameworks/react-native/guest/host.js).
            let mut c = Command::new("node");
            c.arg(onesim_home().join("frameworks/react-native/guest/host.js"))
                .env("SIMCORE_METRO_URL", self.params.metro_url.as_deref().unwrap_or("http://localhost:8081"));
            c
        } else if is_prebuilt(&self.params) {
            Command::new(&self.params.app_path)
        } else {
            let sh = self.shared.clone();
            let bin = build_app(&self.id, &self.params, self.is_compose(), BUILD_TIMEOUT, |log| {
                sh.push_error("build", log);
            })
            .map_err(|e| self.fail(e))?;
            Command::new(bin)
        };
        self.spawn(cmd).map_err(|e| self.fail(e))?;
        let deadline = Instant::now() + LAUNCH_TIMEOUT;
        let mut g = self.shared.lock();
        while g.tree.is_none() && !g.exited {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                drop(g);
                return Err(self.fail(err(ErrorCode::Timeout, "guest produced no tree within 30s")));
            }
            g = self.shared.wait(g, left);
        }
        if g.tree.is_none() {
            let tail = g.errors.iter().chain(&g.logs).rev().take(5).map(|e| e.message.as_str()).collect::<Vec<_>>().join("\n");
            drop(g);
            return Err(self.fail(err(ErrorCode::RuntimeError, format!("guest exited before first tree\n{tail}"))));
        }
        g.state = if g.exited { State::Crashed } else { State::Running };
        drop(g);
        Ok(())
    }

    fn spawn(&mut self, mut cmd: Command) -> Result<(), ApiError> {
        let what = cmd.get_program().to_string_lossy().into_owned();
        if self.device.android {
            cmd.env_remove("SIMCORE_OS_BUILD");
        } else {
            cmd.env("SIMCORE_OS_BUILD", self.ios.build());
        }
        match &self.params.pasteboard {
            Some(pb) => cmd.env("SIMCORE_PASTEBOARD", pb),
            None => cmd.env_remove("SIMCORE_PASTEBOARD"),
        };
        let mut child = cmd
            .env("SIMCORE_SESSION_ID", &self.id)
            .env("SIMCORE_VIEWPORT", format!("{}x{}", self.viewport.width, self.viewport.height))
            .env("SIMCORE_DYNAMIC_TYPE", self.dynamic_type.name())
            .env("SIMCORE_OS_VERSION", self.os_version())
            .env("SIMCORE_DEVICE", self.device.name)
            .env("SIMCORE_APPEARANCE", if self.appearance == Appearance::Dark { "dark" } else { "light" })
            .env("SIMCORE_PLATFORM", &self.params.platform)
            .env("SIMCORE_SCALE", self.scale.to_string())
            .env("SIMCORE_INSETS", format!("{},{}", self.device.safe_top, self.device.safe_bottom))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| err(ErrorCode::RuntimeError, format!("cannot launch {what}: {e}")))?;
        self.stdin = child.stdin.take();
        let (stdout, stderr) = (child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        self.child = Some(child);

        let sh = self.shared.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<GuestMessage>(&line) {
                    Ok(GuestMessage::Tree { version, root }) => {
                        let mut g = sh.lock();
                        g.tree = Some((version, Arc::new(*root)));
                        g.generation += 1;
                        g.last_event = Some(Instant::now());
                        let first = std::mem::replace(&mut g.ready_pending, false);
                        sh.cv.notify_all();
                        drop(g);
                        if first {
                            sh.log.emit("runtime.ready", json!({"treeVersion": version}));
                        }
                        sh.log.render_updated(version);
                    }
                    Ok(m @ (GuestMessage::FocusRequest { .. } | GuestMessage::ScrollTo { .. } | GuestMessage::ScrollToId { .. } | GuestMessage::JsResponder { .. } | GuestMessage::TouchAnswer { .. })) => {
                        let mut g = sh.lock();
                        g.requests.push(m);
                        g.last_event = Some(Instant::now());
                        sh.cv.notify_all();
                    }
                    Ok(GuestMessage::Schedule { after_ms }) => {
                        let mut g = sh.lock();
                        g.schedule_update = Some(after_ms);
                        g.schedule_gen += 1;
                        g.last_event = Some(Instant::now());
                        sh.cv.notify_all();
                    }
                    Ok(GuestMessage::Error { message }) => sh.push_error("guest", message),
                    Ok(GuestMessage::Hello { protocol }) if protocol != onesim_protocol::PROTOCOL_VERSION => {
                        sh.push_error("guest", format!("unexpected guest protocol version {protocol}"))
                    }
                    Ok(GuestMessage::Hello { .. }) => sh.lock().restarted = true,
                    Ok(GuestMessage::Modules { modules }) => sh.lock().modules = Some(modules),
                    Ok(GuestMessage::Pasteboard { items }) => sh.log.emit("pasteboard.changed", json!({"items": items})),
                    Ok(GuestMessage::Event { name, detail }) => match name.as_str() {
                        "bundle.loaded" | "bundle.reloaded" | "bundle.error" | "refresh.applied" | "navigation.changed"
                        | "module.unsupported" | "module.called" => sh.log.emit(&name, detail),
                        _ => log::debug!("guest event {name}: {detail}"),
                    },
                    Err(e) => sh.push_error("host", format!("bad guest message ({e}): {line}")),
                }
            }
            let mut g = sh.lock();
            g.exited = true;
            let crashed = !g.stopping && g.tree.is_some(); // no tree yet: `start` reports it as failed
            if crashed && g.state == State::Running {
                g.state = State::Crashed;
            }
            sh.cv.notify_all();
            drop(g);
            if crashed {
                sh.stopped("crashed");
            }
        });
        let sh = self.shared.clone();
        let id = self.id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log::debug!("[{id}] guest stderr: {line}");
                sh.push_error("stderr", line);
            }
        });
        Ok(())
    }

    pub fn send(&mut self, msg: &HostMessage) -> Result<(), ApiError> {
        self.last_sent = Instant::now();
        let mut line = serde_json::to_string(msg).expect("serialize");
        line.push('\n');
        let w = self.stdin.as_mut().ok_or_else(|| err(ErrorCode::InvalidState, "guest not running"))?;
        w.write_all(line.as_bytes())
            .and_then(|_| w.flush())
            .map_err(|e| err(ErrorCode::RuntimeError, format!("guest pipe closed: {e}")))
    }

    /// React Native: send the frames that changed since the last `layout` message.
    /// Returns whether a `layout` message went out.
    pub fn sync_layout(&mut self, scene: &Scene) -> bool {
        if self.state() != State::Running {
            return false;
        }
        if std::mem::take(&mut self.shared.lock().restarted) {
            self.rn = RnInput::default();
        }
        self.rn.layout(scene).is_some_and(|m| self.send(&m).is_ok())
    }

    pub fn generation(&self) -> u64 {
        self.shared.lock().generation
    }

    /// Guest `focus_request` / `scroll_to` messages since the last call.
    pub fn take_requests(&self) -> Vec<GuestMessage> {
        std::mem::take(&mut self.shared.lock().requests)
    }

    /// Consume any pending `schedule` message from the guest and update `guest_due_ms`.
    pub fn sync_schedule(&mut self) {
        if let Some(after_ms) = self.shared.lock().schedule_update.take() {
            self.guest_due_ms = after_ms.map(|ms| self.clock_ms + ms.max(0.0));
        }
    }

    /// Send a `tick` at `clock_ms` and wait until the guest finishes handling it (signalled by a `schedule` line) or exits.
    pub fn send_tick_and_wait(&mut self) -> Result<bool, ApiError> {
        let (gen_before, sched_before) = {
            let g = self.shared.lock();
            (g.generation, g.schedule_gen)
        };
        let msg = HostMessage::Tick { time: self.clock_ms };
        self.send(&msg)?;
        self.guest_clock_ms = self.clock_ms;
        let g = self
            .shared
            .cv
            .wait_timeout_while(self.shared.lock(), ACTION_TIMEOUT, |i| i.schedule_gen == sched_before && !i.exited)
            .unwrap_or_else(|e| e.into_inner())
            .0;
        let tree_arrived = g.generation != gen_before;
        drop(g);
        self.sync_schedule();
        Ok(tree_arrived)
    }

    /// Block until nothing has arrived from (or been sent to) the guest for `quiet`, at most `max`.
    /// React Native commits UI in several trees (state updates, FlatList batches, onLayout reactions),
    /// so a snapshot is only meaningful once they stop.
    pub fn wait_quiet(&self, quiet: Duration, max: Duration) {
        let limit = Instant::now() + max;
        let mut g = self.shared.lock();
        loop {
            let last = g.last_event.map_or(self.last_sent, |t| t.max(self.last_sent));
            let now = Instant::now();
            let left = (last + quiet).saturating_duration_since(now).min(limit.saturating_duration_since(now));
            if left.is_zero() || g.exited {
                return;
            }
            g = self.shared.wait(g, left);
        }
    }

    /// React Native: tell the guest the focused text input changed from `before` to the current focus.
    pub fn sync_focus(&mut self, before: Option<String>) -> Result<(), ApiError> {
        let out = focus_change(&self.inter, before, self.keyboard_height());
        self.send_out(out)
    }

    /// React Native: height of the software keyboard while shown.
    pub fn keyboard_height(&self) -> f64 {
        onesim_render::look::keyboard_height(&self.device, self.ios)
    }

    /// React Native: deliver messages from `onesim_render::rn_input` in order, waiting as each says.
    pub fn send_out(&mut self, out: Vec<Out>) -> Result<(), ApiError> {
        for o in out {
            match o {
                Out::Send(m, Wait::None) => self.send(&m)?,
                Out::Send(m, Wait::Settle) => drop(self.send_and_settle(&m)?),
                Out::Effect(e) => self.send_effect(e)?,
            }
        }
        Ok(())
    }

    /// Deliver an interaction effect to the guest (no-op for `None`/`Redraw`).
    pub fn send_effect(&mut self, e: Effect) -> Result<(), ApiError> {
        match e {
            Effect::Action { id, version } => {
                let m = HostMessage::Action { id, version };
                if self.is_rn() && !self.settle { self.send(&m) } else { self.send_and_wait_tree(&m).map(drop) }
            }
            Effect::TextInput { field, version, text } => {
                let m = HostMessage::TextInput { field, version, text };
                if self.is_rn() { self.send_and_settle(&m).map(drop) } else { self.send_and_wait_tree(&m).map(drop) }
            }
            // The guest answers every touch event with a tree. A whole synthesized gesture waits for the trees to stop.
            Effect::Gesture(touches) => {
                let n = touches.len();
                for (i, t) in touches.into_iter().enumerate() {
                    let m = HostMessage::Gesture(t);
                    if i + 1 < n {
                        self.send(&m)?;
                    } else if n > 1 {
                        self.send_and_settle(&m)?;
                    } else {
                        self.send_and_wait_tree(&m)?;
                    }
                }
                Ok(())
            }
            Effect::None | Effect::Redraw => Ok(()),
        }
    }

    /// Send `msg`, then wait up to `ACTION_TIMEOUT` for the guest's next tree.
    /// Returns whether a new tree arrived.
    pub fn send_and_wait_tree(&mut self, msg: &HostMessage) -> Result<bool, ApiError> {
        let before = self.shared.lock().generation;
        self.send(msg)?;
        let g = self.shared.wait_generation(self.shared.lock(), before, ACTION_TIMEOUT);
        Ok(g.generation != before)
    }

    /// Like `send_and_wait_tree`, then keeps waiting while trees keep coming (quiet for 200 ms, capped
    /// at `ACTION_TIMEOUT`). React Native commits a press in several trees: the state update, then
    /// Pressability's delayed `pressOut` (130 ms minimum press duration), so the first tree is not final.
    pub fn send_and_settle(&mut self, msg: &HostMessage) -> Result<bool, ApiError> {
        if !self.settle {
            self.send(msg)?;
            return Ok(false);
        }
        let changed = self.send_and_wait_tree(msg)?;
        // Trees may keep coming (an endless animation): the tail after the first tree is bounded, long
        // enough for a 350 ms navigation transition and Pressability's delayed `pressOut`.
        let deadline = Instant::now() + SETTLE_TAIL;
        let mut g = self.shared.lock();
        loop {
            let seen = g.generation;
            let left = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(200));
            if left.is_zero() || g.exited {
                break;
            }
            g = self.shared.wait_generation(g, seen, left);
            if g.generation == seen {
                break;
            }
        }
        Ok(changed)
    }

    /// Full JS reload (React Native): returns once the reloaded app submits its first tree.
    pub fn reload(&mut self) -> Result<(), ApiError> {
        self.require_running()?;
        let before = {
            let mut g = self.shared.lock();
            g.ready_pending = true;
            g.generation
        };
        self.send(&HostMessage::Reload)?;
        let g = self.shared.wait_generation(self.shared.lock(), before, LAUNCH_TIMEOUT);
        if g.generation == before {
            return Err(err(ErrorCode::Timeout, "no tree after reload"));
        }
        Ok(())
    }

    /// Suspend the guest process (SIGSTOP, or NtSuspendProcess on Windows). Input and reload are refused until `resume`.
    pub fn pause(&mut self) -> Result<(), ApiError> {
        self.require_running()?;
        let c = self.child.as_ref().ok_or_else(|| err(ErrorCode::InvalidState, "guest not running"))?;
        if !signal(c, SIGSTOP) {
            return Err(err(ErrorCode::RuntimeError, "cannot suspend the guest process"));
        }
        self.shared.lock().state = State::Paused;
        self.shared.log.emit("runtime.paused", json!({}));
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), ApiError> {
        let state = self.state();
        if state != State::Paused {
            return Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, state.as_str())));
        }
        if let Some(c) = &self.child {
            signal(c, SIGCONT);
        }
        self.shared.lock().state = State::Running;
        self.shared.log.emit("runtime.resumed", json!({}));
        Ok(())
    }

    pub fn stop(&mut self) {
        {
            let mut g = self.shared.lock();
            if g.state == State::Stopped {
                return;
            }
            g.stopping = true;
            g.state = State::Stopping;
        }
        if let Some(c) = &self.child {
            signal(c, SIGCONT); // a suspended guest could not read the shutdown message
        }
        let _ = self.send(&HostMessage::Shutdown);
        self.stdin = None; // EOF also makes the guest exit
        if let Some(mut c) = self.child.take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while matches!(c.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if matches!(c.try_wait(), Ok(None)) {
                let _ = c.kill();
            }
            let _ = c.wait();
        }
        self.shared.lock().state = State::Stopped;
        self.shared.stopped("stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_flood_does_not_evict_errors() {
        let sh = Shared::new(EventLog::new("t"));
        sh.push_error("guest", "real".into());
        for i in 0..MAX_ERRORS + 10 {
            sh.push_error("stderr", format!("log {i}"));
        }
        let g = sh.lock();
        assert_eq!(g.errors.len(), 1);
        assert_eq!(g.logs.len(), MAX_LOG_LINES);
        assert_eq!(g.logs.back().unwrap().message, format!("log {}", MAX_ERRORS + 9));
    }
}
