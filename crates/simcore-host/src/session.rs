//! One guest OS process and everything we know about it.

use simcore_protocol::{
    ApiError, Appearance, CreateSessionParams, DynamicTypeSize, ErrorCode, GuestMessage, HostMessage, ScrollPhase, SessionError, Viewport,
    ViewNode,
};
pub use simcore_render::{device, Device, DEFAULT_DEVICE, DEVICES};
use simcore_render::{Effect, Interaction};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(300);
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_ERRORS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Created,
    Starting,
    Running,
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
            State::Stopping => "stopping",
            State::Stopped => "stopped",
            State::Failed => "failed",
            State::Crashed => "crashed",
        }
    }
}

/// State shared with the guest reader threads.
#[derive(Default)]
struct Inner {
    tree: Option<(u64, ViewNode)>,
    /// Number of trees received; lets waiters detect "a new tree arrived".
    generation: u64,
    errors: Vec<SessionError>,
    exited: bool,
    /// When the last tree or guest request arrived (settling).
    last_event: Option<Instant>,
    /// React Native `focus_request` / `scroll_to` messages not yet applied by the host.
    requests: Vec<GuestMessage>,
}

#[derive(Default)]
struct Shared {
    inner: Mutex<Inner>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push_error(&self, source: &str, message: String) {
        let mut g = self.lock();
        if g.errors.len() < MAX_ERRORS {
            g.errors.push(SessionError { message, source: source.into() });
        }
    }
}

pub struct Session {
    pub id: String,
    pub params: CreateSessionParams,
    pub device: Device,
    pub viewport: Viewport,
    pub scale: f64,
    pub appearance: Appearance,
    pub dynamic_type: DynamicTypeSize,
    /// Scroll, focus, shift and press state (shared with the browser backend).
    pub inter: Interaction,
    /// React Native: frames last sent to the guest (`layout` message), to send only changes.
    pub sent_frames: BTreeMap<String, [f64; 4]>,
    /// React Native: tag the current touch started on.
    pub touch: Option<u64>,
    /// When the host last wrote to the guest: its reaction (new trees) is due within the quiet window.
    last_sent: Instant,
    state: State,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    shared: Arc<Shared>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn is_multiline(n: &ViewNode, id: &str) -> bool {
    match &n.kind {
        simcore_protocol::ViewKind::Rn { component, children, .. } => {
            (n.id == id && component == "RCTMultilineTextInputView") || children.iter().any(|c| is_multiline(c, id))
        }
        _ => false,
    }
}

pub fn simcore_home() -> PathBuf {
    std::env::var_os("SIMCORE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
}

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

/// Test hook: a regular executable that is not a `.swift` file is spawned directly.
fn is_prebuilt(p: &CreateSessionParams) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let path = Path::new(&p.app_path);
    p.prebuilt
        || (path.extension().is_none_or(|e| e != "swift")
            && path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

impl Session {
    pub fn new(id: String, p: CreateSessionParams) -> Self {
        let dev = *device(p.device.as_deref().unwrap_or(DEFAULT_DEVICE)).expect("device validated by create_session");
        Self {
            id,
            device: dev,
            viewport: p.viewport.unwrap_or(dev.viewport),
            scale: p.scale.unwrap_or(dev.scale),
            appearance: p.appearance.unwrap_or(Appearance::Light),
            dynamic_type: p.dynamic_type_size.unwrap_or_default(),
            inter: Interaction::default(),
            sent_frames: BTreeMap::new(),
            touch: None,
            last_sent: Instant::now(),
            params: p,
            state: State::Created,
            child: None,
            stdin: None,
            shared: Arc::default(),
        }
    }

    pub fn is_rn(&self) -> bool {
        self.params.framework == "react-native"
    }

    /// Current state; a running session whose guest exited reads as crashed.
    pub fn state(&self) -> State {
        if self.state == State::Running && self.shared.lock().exited {
            State::Crashed
        } else {
            self.state
        }
    }

    pub fn tree(&self) -> Option<(u64, ViewNode)> {
        self.shared.lock().tree.clone()
    }

    /// Record a host-side error once per distinct `(origin, message)`.
    pub fn report_once(&self, origin: &str, message: String) {
        let mut g = self.shared.lock();
        if g.errors.len() < MAX_ERRORS && !g.errors.iter().any(|e| e.source == origin && e.message == message) {
            g.errors.push(SessionError { message, source: origin.into() });
        }
    }

    pub fn errors(&self) -> Vec<SessionError> {
        self.shared.lock().errors.clone()
    }

    pub fn require_running(&self) -> Result<(), ApiError> {
        match self.state() {
            State::Running => Ok(()),
            s => Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, s.as_str()))),
        }
    }

    fn fail(&mut self, e: ApiError) -> ApiError {
        self.shared.push_error("host", e.message.clone());
        self.state = State::Failed;
        self.kill();
        e
    }

    fn kill(&mut self) {
        self.stdin = None;
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Build (unless prebuilt), spawn the guest and block until its first tree.
    pub fn start(&mut self) -> Result<(), ApiError> {
        if self.state != State::Created {
            return Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, self.state.as_str())));
        }
        self.state = State::Starting;
        let cmd = if self.is_rn() && !self.params.prebuilt {
            // No build step: node runs the Expo/Metro bundle (frameworks/react-native/guest/host.js).
            let mut c = Command::new("node");
            c.arg(simcore_home().join("frameworks/react-native/guest/host.js"))
                .env("SIMCORE_METRO_URL", self.params.metro_url.as_deref().unwrap_or("http://localhost:8081"))
                .env("SIMCORE_SCALE", self.scale.to_string());
            c
        } else if is_prebuilt(&self.params) {
            Command::new(&self.params.app_path)
        } else {
            Command::new(self.build().map_err(|e| self.fail(e))?)
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
            g = self.shared.cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        if g.tree.is_none() {
            let tail = g.errors.iter().rev().take(5).map(|e| e.message.as_str()).collect::<Vec<_>>().join("\n");
            drop(g);
            return Err(self.fail(err(ErrorCode::RuntimeError, format!("guest exited before first tree\n{tail}"))));
        }
        drop(g);
        self.state = State::Running;
        Ok(())
    }

    fn build(&mut self) -> Result<PathBuf, ApiError> {
        let home = simcore_home();
        let script = home.join("scripts/build-swift-app.sh");
        if !script.exists() {
            return Err(err(ErrorCode::BuildFailed, format!("{} not found (set SIMCORE_HOME)", script.display())));
        }
        let dir = home.join("target/simcore-apps").join(format!("{}-{}", std::process::id(), self.id));
        std::fs::create_dir_all(&dir).map_err(|e| err(ErrorCode::BuildFailed, e.to_string()))?;
        let out = dir.join("app");
        let mut cmd = Command::new(&script);
        cmd.arg(&self.params.app_path).arg(&out);
        if let Some(r) = &self.params.root_view {
            cmd.arg(r);
        }
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| err(ErrorCode::BuildFailed, format!("cannot run build script: {e}")))?;
        let stderr = child.stderr.take().expect("piped");
        let log = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut s);
            s
        });
        let deadline = Instant::now() + BUILD_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Ok(s),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(err(ErrorCode::Timeout, "build exceeded 300s"));
                }
                Err(e) => break Err(err(ErrorCode::BuildFailed, e.to_string())),
            }
        };
        let log = log.join().unwrap_or_default();
        if !log.is_empty() {
            self.shared.push_error("build", log.clone());
        }
        let status = status?;
        if !status.success() {
            return Err(err(ErrorCode::BuildFailed, format!("build exited with {status}\n{log}")));
        }
        Ok(out)
    }

    fn spawn(&mut self, mut cmd: Command) -> Result<(), ApiError> {
        let what = cmd.get_program().to_string_lossy().into_owned();
        let mut child = cmd
            .env("SIMCORE_SESSION_ID", &self.id)
            .env("SIMCORE_VIEWPORT", format!("{}x{}", self.viewport.width, self.viewport.height))
            .env("SIMCORE_DYNAMIC_TYPE", self.dynamic_type.name())
            .env("SIMCORE_APPEARANCE", if self.appearance == Appearance::Dark { "dark" } else { "light" })
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
                        g.tree = Some((version, root));
                        g.generation += 1;
                        g.last_event = Some(Instant::now());
                        sh.cv.notify_all();
                    }
                    Ok(m @ (GuestMessage::FocusRequest { .. } | GuestMessage::ScrollTo { .. })) => {
                        let mut g = sh.lock();
                        g.requests.push(m);
                        g.last_event = Some(Instant::now());
                        sh.cv.notify_all();
                    }
                    Ok(GuestMessage::Error { message }) => sh.push_error("guest", message),
                    Ok(GuestMessage::Hello { protocol }) if protocol != simcore_protocol::PROTOCOL_VERSION => {
                        sh.push_error("guest", format!("unexpected guest protocol version {protocol}"))
                    }
                    Ok(GuestMessage::Hello { .. }) => {}
                    Err(e) => sh.push_error("host", format!("bad guest message ({e}): {line}")),
                }
            }
            sh.lock().exited = true;
            sh.cv.notify_all();
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
    pub fn sync_layout(&mut self, scene: &simcore_render::Scene) -> bool {
        let frames: BTreeMap<String, [f64; 4]> = scene
            .nodes
            .iter()
            .filter(|n| n.action.is_some())
            .map(|n| (n.id.clone(), [n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h]))
            .collect();
        let changed: BTreeMap<_, _> = frames.iter().filter(|(k, v)| self.sent_frames.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), *v)).collect();
        if !changed.is_empty() && self.state() == State::Running && self.send(&HostMessage::Layout { frames: changed }).is_ok() {
            self.sent_frames = frames;
            return true;
        }
        false
    }

    pub fn generation(&self) -> u64 {
        self.shared.lock().generation
    }

    /// Guest `focus_request` / `scroll_to` messages since the last call.
    pub fn take_requests(&self) -> Vec<GuestMessage> {
        std::mem::take(&mut self.shared.lock().requests)
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
            g = self.shared.cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    /// React Native: tell the guest the focused text input changed from `before` to the current focus.
    pub fn sync_focus(&mut self, before: Option<String>) -> Result<(), ApiError> {
        let tag = |f: &Option<String>| f.as_ref().and_then(|f| f.parse::<u64>().ok());
        let after = self.inter.focus.clone();
        if before == after {
            return Ok(());
        }
        if let Some(tag) = tag(&before) {
            self.send(&HostMessage::Blur { tag })?;
        }
        if let Some(tag) = tag(&after) {
            self.send(&HostMessage::Focus { tag })?;
        }
        Ok(())
    }

    /// React Native: return pressed while `before` was focused (the interaction state already
    /// dropped the focus). Sends `submit`, then blur unless the input is multiline.
    pub fn submit(&mut self, before: &str) -> Result<(), ApiError> {
        let Ok(tag) = before.parse::<u64>() else { return Ok(()) };
        self.send(&HostMessage::Submit { tag })?;
        let multiline = self.tree().is_some_and(|(_, t)| is_multiline(&t, before));
        if multiline {
            self.inter.focus = Some(before.to_string());
            Ok(())
        } else {
            self.send(&HostMessage::Blur { tag })
        }
    }

    /// React Native: send a `scroll` event for the scroller `n` at `offset`.
    pub fn send_scroll(&mut self, n: &simcore_render::Node, phase: ScrollPhase, offset: (f64, f64)) -> Result<(), ApiError> {
        let Some(info) = n.scroll else { return Ok(()) };
        let Ok(tag) = n.id.parse::<u64>() else { return Ok(()) };
        let (content_width, content_height) = info.content.unwrap_or(if info.vertical { (n.bounds.w, n.bounds.h + info.max.1) } else { (n.bounds.w + info.max.0, n.bounds.h) });
        self.send(&HostMessage::Scroll {
            tag,
            phase,
            x: offset.0,
            y: offset.1,
            content_width,
            content_height,
            width: n.bounds.w,
            height: n.bounds.h,
        })
    }

    /// Deliver an interaction effect to the guest (no-op for `None`/`Redraw`).
    pub fn send_effect(&mut self, e: Effect) -> Result<(), ApiError> {
        match e {
            Effect::Action { id, version } => self.send_and_wait_tree(&HostMessage::Action { id, version }).map(drop),
            Effect::TextInput { field, version, text } => {
                let m = HostMessage::TextInput { field, version, text };
                if self.is_rn() { self.send_and_settle(&m).map(drop) } else { self.send_and_wait_tree(&m).map(drop) }
            }
            Effect::None | Effect::Redraw => Ok(()),
        }
    }

    /// Send `msg`, then wait up to `ACTION_TIMEOUT` for the guest's next tree.
    /// Returns whether a new tree arrived.
    pub fn send_and_wait_tree(&mut self, msg: &HostMessage) -> Result<bool, ApiError> {
        let before = self.shared.lock().generation;
        self.send(msg)?;
        let g = self.shared.lock();
        let (g, _) = self
            .shared
            .cv
            .wait_timeout_while(g, ACTION_TIMEOUT, |i| i.generation == before && !i.exited)
            .unwrap_or_else(|e| e.into_inner());
        Ok(g.generation != before)
    }

    /// Like `send_and_wait_tree`, then keeps waiting while trees keep coming (quiet for 200 ms, capped
    /// at `ACTION_TIMEOUT`). React Native commits a press in several trees: the state update, then
    /// Pressability's delayed `pressOut` (130 ms minimum press duration), so the first tree is not final.
    pub fn send_and_settle(&mut self, msg: &HostMessage) -> Result<bool, ApiError> {
        let deadline = Instant::now() + ACTION_TIMEOUT;
        let changed = self.send_and_wait_tree(msg)?;
        let mut g = self.shared.lock();
        loop {
            let seen = g.generation;
            let left = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(200));
            if left.is_zero() || g.exited {
                break;
            }
            g = self.shared.cv.wait_timeout_while(g, left, |i| i.generation == seen && !i.exited).unwrap_or_else(|e| e.into_inner()).0;
            if g.generation == seen {
                break;
            }
        }
        Ok(changed)
    }

    pub fn stop(&mut self) {
        if matches!(self.state, State::Stopped) {
            return;
        }
        self.state = State::Stopping;
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
        self.state = State::Stopped;
    }
}
