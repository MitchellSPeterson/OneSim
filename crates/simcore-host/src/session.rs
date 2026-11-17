//! One guest OS process and everything we know about it.

use simcore_protocol::{
    ApiError, Appearance, CreateSessionParams, ErrorCode, GuestMessage, HostMessage, SessionError, Viewport, ViewNode,
};
use std::collections::HashMap;
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

/// Screen geometry of a simulated device (points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Device {
    pub name: &'static str,
    pub viewport: Viewport,
    pub scale: f64,
    pub safe_top: f64,
    pub safe_bottom: f64,
    /// Dynamic Island `(x, y, w, h)`.
    pub island: Option<(f64, f64, f64, f64)>,
}

pub const DEFAULT_DEVICE: &str = "iphone-17-pro";
pub const DEVICES: &[Device] = &[Device {
    name: DEFAULT_DEVICE,
    viewport: Viewport { width: 402.0, height: 874.0 },
    scale: 3.0,
    safe_top: 62.0,
    safe_bottom: 34.0,
    // Measured from simulator screenshots: x 138.3..263.7, y 14..50.7.
    island: Some((138.4, 13.92, 125.2, 36.83)),
}];

pub fn device(name: &str) -> Option<&'static Device> {
    DEVICES.iter().find(|d| d.name == name)
}

pub struct Session {
    pub id: String,
    pub params: CreateSessionParams,
    pub device: Device,
    pub viewport: Viewport,
    pub scale: f64,
    pub appearance: Appearance,
    /// Scroll offsets by node id; survive across trees.
    pub scroll: HashMap<String, (f64, f64)>,
    /// Node id of the focused text field.
    pub focus: Option<String>,
    /// Software keyboard shift state. Key taps and focus update it; `send_keyboard_event` does not.
    pub shift: bool,
    state: State,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    shared: Arc<Shared>,
    /// Node id under the pointer between `down` and `up`.
    pub pressed: Option<String>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
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
            scroll: HashMap::new(),
            focus: None,
            shift: false,
            params: p,
            state: State::Created,
            child: None,
            stdin: None,
            shared: Arc::default(),
            pressed: None,
        }
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
        let exe = if is_prebuilt(&self.params) {
            PathBuf::from(&self.params.app_path)
        } else {
            self.build().map_err(|e| self.fail(e))?
        };
        self.spawn(&exe).map_err(|e| self.fail(e))?;
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

    fn spawn(&mut self, exe: &Path) -> Result<(), ApiError> {
        let mut child = Command::new(exe)
            .env("SIMCORE_SESSION_ID", &self.id)
            .env("SIMCORE_VIEWPORT", format!("{}x{}", self.viewport.width, self.viewport.height))
            .env("SIMCORE_APPEARANCE", if self.appearance == Appearance::Dark { "dark" } else { "light" })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| err(ErrorCode::RuntimeError, format!("cannot launch {}: {e}", exe.display())))?;
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
        let mut line = serde_json::to_string(msg).expect("serialize");
        line.push('\n');
        let w = self.stdin.as_mut().ok_or_else(|| err(ErrorCode::InvalidState, "guest not running"))?;
        w.write_all(line.as_bytes())
            .and_then(|_| w.flush())
            .map_err(|e| err(ErrorCode::RuntimeError, format!("guest pipe closed: {e}")))
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
