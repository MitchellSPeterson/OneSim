//! One guest OS process and everything we know about it.

use onesim_protocol::{
    ApiError, Appearance, CreateSessionParams, DynamicTypeSize, ErrorCode, GeoBox, GuestMessage, HostMessage, IosVersion, SessionError, Viewport,
    ViewNode,
};
pub use onesim_render::{device, Device, DEFAULT_DEVICE, DEVICES};
use crate::events::EventLog;
use serde_json::{json, Value};
use onesim_render::rn_input::{focus_change, Out, RnInput, Wait};
use onesim_render::{Animator, Effect, Interaction};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
#[derive(Default)]
struct Inner {
    tree: Option<(u64, ViewNode)>,
    /// Number of trees received; lets waiters detect "a new tree arrived".
    generation: u64,
    errors: Vec<SessionError>,
    /// Last `MAX_LOG_LINES` guest stderr lines (reported by `get_errors` with source `stderr`).
    logs: VecDeque<SessionError>,
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
        Self { inner: Mutex::new(Inner { ready_pending: true, ..Inner::default() }), cv: Condvar::new(), log }
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
            self.log.emit("runtime.stopped", json!({"reason": reason}));
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
            signal(c, SIGCONT);
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[cfg(unix)]
const SIGSTOP: i32 = libc::SIGSTOP;
#[cfg(unix)]
const SIGCONT: i32 = libc::SIGCONT;
#[cfg(not(unix))]
const SIGSTOP: i32 = 0;
#[cfg(not(unix))]
const SIGCONT: i32 = 1;

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(process: std::os::windows::io::RawHandle) -> i32;
    fn NtResumeProcess(process: std::os::windows::io::RawHandle) -> i32;
}

/// Stop or continue the guest child: kill(2) on unix, NtSuspendProcess/NtResumeProcess on Windows.
// ponytail: Windows suspends the direct child only; a Compose `.cmd` launcher's java keeps running.
fn signal(c: &Child, sig: i32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: plain kill(2) on a pid we spawned and still own (not yet waited).
        unsafe { libc::kill(c.id() as i32, sig) == 0 }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: the handle belongs to a child we spawned and still own (not yet waited).
        unsafe { (if sig == SIGSTOP { NtSuspendProcess } else { NtResumeProcess })(c.as_raw_handle()) >= 0 }
    }
}

pub fn onesim_home() -> PathBuf {
    std::env::var_os("SIMCORE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")))
}

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

/// Test hook: a regular executable that is not a `.swift` file is spawned directly.
fn is_prebuilt(p: &CreateSessionParams) -> bool {
    let path = Path::new(&p.app_path);
    #[cfg(unix)]
    let exe = |m: &std::fs::Metadata| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0;
    #[cfg(not(unix))]
    let exe = |_: &std::fs::Metadata| path.extension().is_some_and(|e| e == "exe");
    p.prebuilt
        || (path.extension().is_none_or(|e| e != "swift" && e != "kt")
            && path.metadata().is_ok_and(|m| m.is_file() && exe(&m)))
}

/// The build scripts are bash. On Windows that is Git Bash, never System32's WSL launcher (`SIMCORE_BASH` overrides).
fn bash() -> Command {
    if let Some(b) = std::env::var_os("SIMCORE_BASH") {
        return Command::new(b);
    }
    #[cfg(windows)]
    for dir in ["ProgramW6432", "ProgramFiles"].iter().filter_map(std::env::var_os) {
        let git = PathBuf::from(dir).join("Git/bin/bash.exe");
        if git.is_file() {
            return Command::new(git);
        }
    }
    Command::new("bash")
}

/// Path argument for a bash script: forward slashes, which Git Bash accepts and backslash-escaping cannot break.
fn sh_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
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
            last_sent: Instant::now(),
            params: p,
            state: State::Created,
            child: None,
            stdin: None,
            shared: Arc::new(Shared::new(log)),
        }
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

    /// Current state; a running session whose guest exited reads as crashed.
    pub fn state(&self) -> State {
        if self.state == State::Running && self.shared.lock().exited {
            State::Crashed
        } else {
            self.state
        }
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

    pub fn tree(&self) -> Option<(u64, ViewNode)> {
        self.shared.lock().tree.clone()
    }

    /// Record a host-side error once per distinct `(origin, message)`.
    pub fn report_once(&self, origin: &str, message: String) {
        let g = self.shared.lock();
        if g.errors.len() < MAX_ERRORS && !g.errors.iter().any(|e| e.source == origin && e.message == message) {
            drop(g);
            self.shared.push_error(origin, message);
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
        self.state = State::Failed;
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
        if self.state != State::Created {
            return Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, self.state.as_str())));
        }
        self.state = State::Starting;
        let cmd = if self.is_rn() && !self.params.prebuilt {
            // No build step: node runs the Expo/Metro bundle (frameworks/react-native/guest/host.js).
            let mut c = Command::new("node");
            c.arg(onesim_home().join("frameworks/react-native/guest/host.js"))
                .env("SIMCORE_METRO_URL", self.params.metro_url.as_deref().unwrap_or("http://localhost:8081"));
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
            g = self.shared.wait(g, left);
        }
        if g.tree.is_none() {
            let tail = g.errors.iter().chain(&g.logs).rev().take(5).map(|e| e.message.as_str()).collect::<Vec<_>>().join("\n");
            drop(g);
            return Err(self.fail(err(ErrorCode::RuntimeError, format!("guest exited before first tree\n{tail}"))));
        }
        drop(g);
        self.state = State::Running;
        Ok(())
    }

    fn build(&mut self) -> Result<PathBuf, ApiError> {
        let home = onesim_home();
        let script = home.join(if self.is_compose() { "frameworks/compose/scripts/build-app.sh" } else { "scripts/build-swift-app.sh" });
        if !script.exists() {
            return Err(err(ErrorCode::BuildFailed, format!("{} not found (set SIMCORE_HOME)", script.display())));
        }
        let dir = home.join("target/onesim-apps").join(format!("{}-{}", std::process::id(), self.id));
        std::fs::create_dir_all(&dir).map_err(|e| err(ErrorCode::BuildFailed, e.to_string()))?;
        // Windows runs the Compose guest through a batch launcher (build-app.sh writes one for a `.cmd` output).
        let out = dir.join(if cfg!(windows) && self.is_compose() { "app.cmd".into() } else { format!("app{}", std::env::consts::EXE_SUFFIX) });
        let mut cmd = bash();
        cmd.arg(sh_path(&script)).arg(sh_path(Path::new(&self.params.app_path))).arg(sh_path(&out));
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
        if self.device.android {
            cmd.env_remove("SIMCORE_OS_BUILD");
        } else {
            cmd.env("SIMCORE_OS_BUILD", self.ios.build());
        }
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
                        g.tree = Some((version, root));
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
    pub fn sync_layout(&mut self, scene: &onesim_render::Scene) -> bool {
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
        self.state = State::Paused;
        self.shared.log.emit("runtime.paused", json!({}));
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), ApiError> {
        if self.state != State::Paused {
            return Err(err(ErrorCode::InvalidState, format!("session {} is {}", self.id, self.state().as_str())));
        }
        if let Some(c) = &self.child {
            signal(c, SIGCONT);
        }
        self.state = State::Running;
        self.shared.log.emit("runtime.resumed", json!({}));
        Ok(())
    }

    pub fn stop(&mut self) {
        if matches!(self.state, State::Stopped) {
            return;
        }
        self.shared.lock().stopping = true;
        if let Some(c) = &self.child {
            signal(c, SIGCONT); // a suspended guest could not read the shutdown message
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
