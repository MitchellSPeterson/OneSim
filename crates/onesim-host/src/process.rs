//! Guest process spawning, signaling, and build-script invocation.

use onesim_protocol::{ApiError, CreateSessionParams, ErrorCode};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
pub(crate) const SIGSTOP: i32 = libc::SIGSTOP;
#[cfg(unix)]
pub(crate) const SIGCONT: i32 = libc::SIGCONT;
#[cfg(not(unix))]
pub(crate) const SIGSTOP: i32 = 0;
#[cfg(not(unix))]
pub(crate) const SIGCONT: i32 = 1;

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(process: std::os::windows::io::RawHandle) -> i32;
    fn NtResumeProcess(process: std::os::windows::io::RawHandle) -> i32;
}

/// Stop or continue the guest child: kill(2) on unix, NtSuspendProcess/NtResumeProcess on Windows.
// ponytail: Windows suspends the direct child only; a Compose `.cmd` launcher's java keeps running.
pub(crate) fn signal(c: &Child, sig: i32) -> bool {
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

/// Test hook: a regular executable that is not a `.swift` or `.kt` file is spawned directly.
pub(crate) fn is_prebuilt(p: &CreateSessionParams) -> bool {
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
pub(crate) fn bash() -> Command {
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
pub(crate) fn sh_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

/// Invoke the SwiftUI or Compose build script and wait up to `timeout` for the output binary.
pub(crate) fn build_app(
    id: &str,
    params: &CreateSessionParams,
    is_compose: bool,
    timeout: Duration,
    on_log: impl FnOnce(String),
) -> Result<PathBuf, ApiError> {
    let home = onesim_home();
    let script = home.join(if is_compose { "frameworks/compose/scripts/build-app.sh" } else { "scripts/build-swift-app.sh" });
    if !script.exists() {
        return Err(err(ErrorCode::BuildFailed, format!("{} not found (set SIMCORE_HOME)", script.display())));
    }
    let dir = home.join("target/onesim-apps").join(format!("{}-{}", std::process::id(), id));
    std::fs::create_dir_all(&dir).map_err(|e| err(ErrorCode::BuildFailed, e.to_string()))?;
    // Windows runs the Compose guest through a batch launcher (build-app.sh writes one for a `.cmd` output).
    let out = dir.join(if cfg!(windows) && is_compose { "app.cmd".into() } else { format!("app{}", std::env::consts::EXE_SUFFIX) });
    let mut cmd = bash();
    cmd.arg(sh_path(&script)).arg(sh_path(Path::new(&params.app_path))).arg(sh_path(&out));
    if let Some(r) = &params.root_view {
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
    let deadline = Instant::now() + timeout;
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
        on_log(log.clone());
    }
    let status = status?;
    if !status.success() {
        return Err(err(ErrorCode::BuildFailed, format!("build exited with {status}\n{log}")));
    }
    Ok(out)
}
