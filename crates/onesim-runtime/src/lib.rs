//! C ABI for the OneSim guest wire (docs/protocol.md sections B and C).
use serde_json::{value::RawValue, Value};
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::io::{BufRead, Write};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;
/// (ctx, phase 0 start / 1 move / 2 end / 3 cancel, time ms, points, point count, keys, frames, target count).
pub type GestureCb = Option<unsafe extern "C" fn(*mut c_void, i32, f64, *const f64, usize, *const *const c_char, *const f64, usize)>;
/// (ctx, whole host JSON line for `geometry`, `scroll_position`, `layout`, `tick` or `pasteboard`).
pub type LayoutCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;
/// Guest work finished off the runtime thread (an `AsyncImage` HTTP completion). Same ctx as the other callbacks.
pub type WakeCb = Option<unsafe extern "C" fn(*mut c_void)>;

#[derive(Default)]
struct State {
    out: Option<Box<dyn Write>>,
    last_version: Option<u64>,
    ctx: usize,
    on_action: ActionCb,
    on_env: EnvCb,
    on_text: TextCb,
    on_gesture: GestureCb,
    on_layout: LayoutCb,
    on_wake: WakeCb,
    /// Read end of the guest's wake pipe. `None` keeps the blocking stdin loop.
    wake_fd: Option<i32>,
}

thread_local! {
    // ponytail: thread-local state; protocol says single-threaded.
    static STATE: RefCell<State> = RefCell::new(State::default());
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_err(msg: impl Into<String>) {
    let s = msg.into().replace('\0', " ");
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::new(s).ok());
}

fn send_line(line: &str) -> Result<(), String> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let out = s.out.as_mut().ok_or("runtime not initialized")?;
        out.write_all(line.as_bytes())
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(|e| format!("write to host failed: {e}"))
    })
}

fn init_with(out: Box<dyn Write>) -> Result<(), String> {
    STATE.with(|s| s.borrow_mut().out = Some(out));
    send_line(r#"{"type":"hello","protocol":1}"#)
}

fn submit(json: &[u8], version: u64) -> Result<(), String> {
    let text = std::str::from_utf8(json).map_err(|e| format!("tree is not UTF-8: {e}"))?;
    let raw: &RawValue = serde_json::from_str(text).map_err(|e| format!("tree is not valid JSON: {e}"))?;
    if STATE.with(|s| s.borrow().last_version.is_some_and(|v| version <= v)) {
        return Err("tree version must increase monotonically".into());
    }
    // Wire is line-delimited: re-serialize compactly only if the Swift side emitted newlines.
    let compact;
    let root = if raw.get().contains('\n') {
        compact = serde_json::to_string(&serde_json::from_str::<Value>(text).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        compact.as_str()
    } else {
        raw.get()
    };
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let out = s.out.as_mut().ok_or("runtime not initialized")?;
        write!(out, r#"{{"type":"tree","version":{version},"root":"#)
            .and_then(|_| out.write_all(root.as_bytes()))
            .and_then(|_| out.write_all(b"}\n"))
            .and_then(|_| out.flush())
            .map_err(|e| format!("write to host failed: {e}"))?;
        s.last_version = Some(version);
        Ok(())
    })
}

/// Handle one host line. Returns false when the loop should stop.
fn handle_line(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return true;
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            let _ = send_line(&error_msg(&format!("bad host message: {e}")));
            return true;
        }
    };
    // Copy callbacks out: they re-enter submit_tree, so no borrow may be held.
    let (ctx, on_action, on_env, on_text, last) =
        STATE.with(|s| { let s = s.borrow(); (s.ctx, s.on_action, s.on_env, s.on_text, s.last_version) });
    match v["type"].as_str() {
        Some("shutdown") => return false,
        Some("action") => {
            if let (Some(id), Some(ver), Some(cb)) = (v["id"].as_u64(), v["version"].as_u64(), on_action) {
                if Some(ver) == last {
                    unsafe { cb(ctx as *mut c_void, id) };
                }
            }
        }
        Some("text_input") => {
            if let (Some(field), Some(ver), Some(text), Some(cb)) =
                (v["field"].as_u64(), v["version"].as_u64(), v["text"].as_str(), on_text)
            {
                if Some(ver) == last {
                    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
                    unsafe { cb(ctx as *mut c_void, field, text.as_ptr()) };
                }
            }
        }
        Some("gesture") => {
            if let Some(cb) = STATE.with(|s| s.borrow().on_gesture) {
                deliver_gesture(cb, ctx, &v);
            }
        }
        Some("geometry" | "scroll_position" | "layout" | "tick" | "pasteboard") => {
            if let Some(cb) = STATE.with(|s| s.borrow().on_layout) {
                let owned = CString::new(line).unwrap_or_default();
                unsafe { cb(ctx as *mut c_void, owned.as_ptr()) };
            }
        }
        Some("environment") => {
            let appearance = CString::new(v["appearance"].as_str().unwrap_or("light")).unwrap_or_default();
            let (w, h) = (v["viewport"]["width"].as_f64().unwrap_or(0.0), v["viewport"]["height"].as_f64().unwrap_or(0.0));
            if let Some(cb) = on_env {
                unsafe { cb(ctx as *mut c_void, appearance.as_ptr(), w, h) };
            }
        }
        _ => {}
    }
    true
}

/// Flatten a section L `gesture` message into the callback's arrays (owned here, valid during the call).
fn deliver_gesture(cb: unsafe extern "C" fn(*mut c_void, i32, f64, *const f64, usize, *const *const c_char, *const f64, usize), ctx: usize, v: &Value) {
    let phase = match v["phase"].as_str() {
        Some("start") => 0,
        Some("move") => 1,
        Some("end") => 2,
        Some("cancel") => 3,
        _ => return,
    };
    let empty = vec![];
    let pts: Vec<f64> = v["points"].as_array().unwrap_or(&empty).iter().flat_map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)]).collect();
    let targets = v["targets"].as_array().unwrap_or(&empty);
    let keys: Vec<CString> = targets.iter().map(|t| CString::new(t["key"].as_str().unwrap_or("").replace('\0', "")).unwrap_or_default()).collect();
    let key_ptrs: Vec<*const c_char> = keys.iter().map(|k| k.as_ptr()).collect();
    let frames: Vec<f64> = targets.iter().flat_map(|t| (0..4).map(move |i| t["frame"][i].as_f64().unwrap_or(0.0))).collect();
    unsafe { cb(ctx as *mut c_void, phase, v["time"].as_f64().unwrap_or(0.0), pts.as_ptr(), pts.len() / 2, key_ptrs.as_ptr(), frames.as_ptr(), keys.len()) };
}

fn error_msg(m: &str) -> String {
    serde_json::json!({"type":"error","message":m}).to_string()
}

#[no_mangle]
pub extern "C" fn simcore_runtime_init() -> i32 {
    unsafe {
        let fd = libc::dup(1);
        if fd < 0 || libc::dup2(2, 1) < 0 {
            set_err(format!("dup/dup2 failed: {}", std::io::Error::last_os_error()));
            return 1;
        }
        #[cfg(unix)]
        let file: std::fs::File = std::os::fd::FromRawFd::from_raw_fd(fd);
        // The CRT fd owns a Win32 handle; write to it directly so CRT text mode never turns `\n` into `\r\n`.
        #[cfg(windows)]
        let file: std::fs::File = std::os::windows::io::FromRawHandle::from_raw_handle(libc::get_osfhandle(fd) as _);
        match init_with(Box::new(std::io::BufWriter::new(file))) {
            Ok(()) => 0,
            Err(e) => { set_err(e); 1 }
        }
    }
}

#[no_mangle]
pub extern "C" fn simcore_runtime_set_callbacks(ctx: *mut c_void, on_action: ActionCb, on_env: EnvCb) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.ctx = ctx as usize;
        s.on_action = on_action;
        s.on_env = on_env;
    });
}

/// Optional text_input callback; uses the ctx given to `simcore_runtime_set_callbacks`.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_text_callback(on_text: TextCb) {
    STATE.with(|s| s.borrow_mut().on_text = on_text);
}

/// Optional section L gesture callback; uses the ctx given to `simcore_runtime_set_callbacks`.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_gesture_callback(on_gesture: GestureCb) {
    STATE.with(|s| s.borrow_mut().on_gesture = on_gesture);
}

/// Optional section M layout callback; uses the ctx given to `simcore_runtime_set_callbacks`.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_layout_callback(on_layout: LayoutCb) {
    STATE.with(|s| s.borrow_mut().on_layout = on_layout);
}

/// # Safety
/// `line` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn simcore_send_line(line: *const c_char) -> i32 {
    if line.is_null() {
        set_err("null line");
        return 1;
    }
    let text = CStr::from_ptr(line).to_string_lossy();
    let text = text.trim_end_matches(['\n', '\r']);
    match send_line(text) {
        Ok(()) => 0,
        Err(e) => {
            set_err(e);
            1
        }
    }
}

/// # Safety
/// `json` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn simcore_submit_tree(json: *const u8, len: usize, version: u64) -> i32 {
    if json.is_null() {
        set_err("null json");
        return 1;
    }
    match submit(std::slice::from_raw_parts(json, len), version) {
        Ok(()) => 0,
        Err(e) => { set_err(e); 1 }
    }
}

/// # Safety
/// `msg` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn simcore_report_error(msg: *const c_char) {
    if msg.is_null() { return; }
    let m = CStr::from_ptr(msg).to_string_lossy();
    let _ = send_line(&error_msg(&m));
}

#[no_mangle]
pub extern "C" fn simcore_runtime_set_wake(fd: i32, on_wake: WakeCb) {
    #[cfg(unix)]
    if fd >= 0 {
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags >= 0 {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.wake_fd = (fd >= 0).then_some(fd);
        s.on_wake = on_wake;
    });
}

#[no_mangle]
pub extern "C" fn simcore_runtime_run() -> i32 {
    #[cfg(unix)]
    if STATE.with(|s| s.borrow().wake_fd.is_some()) {
        return run_with_wake(libc::STDIN_FILENO);
    }
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => return 0,
            Ok(_) => if !handle_line(&line) { return 0 },
            Err(e) => { set_err(format!("stdin read failed: {e}")); return 1 }
        }
    }
}

/// Stdin plus the wake pipe. The callback runs on this thread, with no `STATE` borrow held.
#[cfg(unix)]
fn run_with_wake(stdin_fd: i32) -> i32 {
    let mut carry = Vec::<u8>::new();
    loop {
        let wake_fd = STATE.with(|s| s.borrow().wake_fd);
        let mut fds = [
            libc::pollfd { fd: stdin_fd, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: wake_fd.unwrap_or(-1), events: libc::POLLIN, revents: 0 },
        ];
        let nfd: libc::nfds_t = if wake_fd.is_some() { 2 } else { 1 };
        loop {
            let rc = unsafe { libc::poll(fds.as_mut_ptr(), nfd, -1) };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                set_err(format!("poll failed: {err}"));
                return 1;
            }
            break;
        }
        let hot = libc::POLLIN | libc::POLLHUP | libc::POLLERR;
        if fds[0].revents & hot != 0 {
            match read_some(stdin_fd, &mut carry) {
                Ok(None) => return 0,
                Ok(Some(lines)) => {
                    for line in lines {
                        if !handle_line(&line) {
                            return 0;
                        }
                    }
                }
                Err(e) => {
                    set_err(format!("stdin read failed: {e}"));
                    return 1;
                }
            }
        }
        if nfd == 2 && fds[1].revents & hot != 0 {
            if let Some(fd) = wake_fd {
                drain_fd(fd);
            }
            let (ctx, cb) = STATE.with(|s| {
                let s = s.borrow();
                (s.ctx, s.on_wake)
            });
            if let Some(cb) = cb {
                unsafe { cb(ctx as *mut c_void) };
            }
        }
    }
}

#[cfg(unix)]
fn read_some(fd: i32, carry: &mut Vec<u8>) -> std::io::Result<Option<Vec<String>>> {
    let mut buf = [0u8; 8192];
    let n = loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        break n;
    };
    if n == 0 {
        return Ok(None);
    }
    carry.extend_from_slice(&buf[..n as usize]);
    let mut lines = Vec::new();
    let mut start = 0;
    while let Some(rel) = carry[start..].iter().position(|&b| b == b'\n') {
        let end = start + rel + 1;
        lines.push(String::from_utf8_lossy(&carry[start..end]).into_owned());
        start = end;
    }
    if start > 0 {
        carry.drain(..start);
    }
    Ok(Some(lines))
}

#[cfg(unix)]
fn drain_fd(fd: i32) {
    let mut buf = [0u8; 256];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if n == 0 {
            break;
        }
    }
}

#[no_mangle]
pub extern "C" fn simcore_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(std::ptr::null(), |c| c.as_ptr()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Buf(Rc<RefCell<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { self.0.borrow_mut().extend_from_slice(b); Ok(b.len()) }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    thread_local!(static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) });
    thread_local!(static STOP_FD: RefCell<i32> = const { RefCell::new(-1) });
    unsafe extern "C" fn on_action(_: *mut c_void, id: u64) { SEEN.with(|s| s.borrow_mut().push(format!("a{id}"))) }
    unsafe extern "C" fn on_env(_: *mut c_void, a: *const c_char, w: f64, h: f64) {
        SEEN.with(|s| s.borrow_mut().push(format!("e{}{w}x{h}", CStr::from_ptr(a).to_str().unwrap())))
    }

    unsafe extern "C" fn on_text(_: *mut c_void, f: u64, t: *const c_char) {
        SEEN.with(|s| s.borrow_mut().push(format!("t{f}={}", CStr::from_ptr(t).to_str().unwrap())))
    }

    unsafe extern "C" fn on_gesture(_: *mut c_void, phase: i32, time: f64, pts: *const f64, np: usize, keys: *const *const c_char, frames: *const f64, nt: usize) {
        let pts = std::slice::from_raw_parts(pts, np * 2);
        let keys: Vec<_> = std::slice::from_raw_parts(keys, nt).iter().map(|k| CStr::from_ptr(*k).to_str().unwrap().to_string()).collect();
        let frames = std::slice::from_raw_parts(frames, nt * 4);
        SEEN.with(|s| s.borrow_mut().push(format!("g{phase}@{time} {pts:?} {keys:?} {frames:?}")))
    }

    unsafe extern "C" fn on_layout(_: *mut c_void, line: *const c_char) {
        SEEN.with(|s| s.borrow_mut().push(format!("l{}", CStr::from_ptr(line).to_str().unwrap())))
    }

    #[test]
    fn geometry_line_reaches_the_layout_callback() {
        let b = setup();
        simcore_runtime_set_layout_callback(Some(on_layout));
        assert!(handle_line(r#"{"type":"geometry","frames":{}}"#));
        assert!(handle_line(r#"{"type":"scroll_position","scroll":"0","target":"row"}"#));
        assert!(handle_line(r#"{"type":"tick","time":120.0}"#));
        assert!(handle_line(r#"{"type":"pasteboard","items":["hi"]}"#));
        SEEN.with(|s| {
            assert_eq!(
                s.borrow().as_slice(),
                [
                    r#"l{"type":"geometry","frames":{}}"#,
                    r#"l{"type":"scroll_position","scroll":"0","target":"row"}"#,
                    r#"l{"type":"tick","time":120.0}"#,
                    r#"l{"type":"pasteboard","items":["hi"]}"#,
                ]
            )
        });
        let msg = CString::new(r#"{"type":"scroll_to_id","scroll":"0","target":"row"}"#).unwrap();
        assert_eq!(unsafe { simcore_send_line(msg.as_ptr()) }, 0);
        assert!(lines(&b).iter().any(|l| l.contains("scroll_to_id")));
    }

    #[test]
    fn gesture_touches_reach_the_callback() {
        setup();
        simcore_runtime_set_gesture_callback(Some(on_gesture));
        submit(b"{}", 5).unwrap();
        // Not version-gated: a drag keeps going while its moves produce new trees.
        assert!(handle_line(r#"{"type":"gesture","phase":"start","time":10.5,"points":[[1,2],[3,4]],"targets":[{"key":"0/1","frame":[0,0,10,20]},{"key":"0","frame":[0,0,30,40]}]}"#));
        assert!(handle_line(r#"{"type":"gesture","phase":"cancel","time":12,"points":[[5,6]],"targets":[]}"#));
        SEEN.with(|s| assert_eq!(*s.borrow(), vec![
            r#"g0@10.5 [1.0, 2.0, 3.0, 4.0] ["0/1", "0"] [0.0, 0.0, 10.0, 20.0, 0.0, 0.0, 30.0, 40.0]"#,
            "g3@12 [5.0, 6.0] [] []",
        ]));
    }

    #[test]
    fn layout_frames_reach_the_callback() {
        setup();
        simcore_runtime_set_layout_callback(Some(on_layout));
        submit(b"{}", 1).unwrap();
        assert!(handle_line(r#"{"type":"layout","frames":{"0/2":[10,20,120,80]}}"#));
        SEEN.with(|s| assert_eq!(*s.borrow(), vec![r#"l{"type":"layout","frames":{"0/2":[10,20,120,80]}}"#]));
    }

    fn setup() -> Buf {
        STATE.with(|s| *s.borrow_mut() = State::default());
        SEEN.with(|s| s.borrow_mut().clear());
        let b = Buf::default();
        init_with(Box::new(b.clone())).unwrap();
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), Some(on_env));
        simcore_runtime_set_text_callback(Some(on_text));
        b
    }
    fn lines(b: &Buf) -> Vec<String> {
        String::from_utf8(b.0.borrow().clone()).unwrap().lines().map(String::from).collect()
    }

    #[test]
    fn hello_tree_and_version_gate() {
        let b = setup();
        submit(b"{\"kind\":\"empty\",\n\"id\":\"0\"}", 1).unwrap();
        let l = lines(&b);
        assert_eq!(l[0], r#"{"type":"hello","protocol":1}"#);
        assert_eq!(l[1], r#"{"type":"tree","version":1,"root":{"id":"0","kind":"empty"}}"#);
        assert!(submit(b"{}", 1).is_err(), "non-increasing version rejected");
        assert!(submit(b"{nope", 2).is_err(), "invalid json rejected");
        assert_eq!(lines(&b).len(), 2);
    }

    #[test]
    fn actions_env_shutdown() {
        let b = setup();
        submit(b"{}", 3).unwrap();
        assert!(handle_line(r#"{"type":"action","id":7,"version":3}"#));
        assert!(handle_line(r#"{"type":"action","id":8,"version":2}"#)); // stale
        assert!(handle_line(r#"{"type":"environment","appearance":"dark","viewport":{"width":10,"height":20}}"#));
        assert!(handle_line(r#"{"type":"text_input","field":4,"version":3,"text":"hi \u00e9"}"#));
        assert!(handle_line(r#"{"type":"text_input","field":4,"version":2,"text":"stale"}"#));
        assert!(handle_line("garbage"));
        assert!(!handle_line(r#"{"type":"shutdown"}"#));
        SEEN.with(|s| assert_eq!(*s.borrow(), vec!["a7", "edark10x20", "t4=hi é"]));
        assert!(lines(&b).last().unwrap().contains(r#""type":"error""#));
    }

    #[cfg(unix)]
    #[test]
    fn wake_callback_runs_from_the_event_loop() {
        unsafe extern "C" fn on_wake(_: *mut c_void) {
            SEEN.with(|s| s.borrow_mut().push("wake".into()));
            let fd = STOP_FD.with(|s| *s.borrow());
            let msg = b"{\"type\":\"shutdown\"}\n";
            unsafe { libc::write(fd, msg.as_ptr() as *const libc::c_void, msg.len()); }
        }
        let mut stdin_fds = [0i32; 2];
        let mut wake_fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(stdin_fds.as_mut_ptr()) }, 0);
        assert_eq!(unsafe { libc::pipe(wake_fds.as_mut_ptr()) }, 0);
        STATE.with(|s| *s.borrow_mut() = State::default());
        SEEN.with(|s| s.borrow_mut().clear());
        STOP_FD.with(|s| *s.borrow_mut() = stdin_fds[1]);
        simcore_runtime_set_wake(wake_fds[0], Some(on_wake));
        let byte = 1u8;
        assert_eq!(unsafe { libc::write(wake_fds[1], &byte as *const u8 as *const libc::c_void, 1) }, 1);
        assert_eq!(run_with_wake(stdin_fds[0]), 0);
        SEEN.with(|s| assert_eq!(s.borrow().as_slice(), ["wake".to_string()]));
        STATE.with(|s| *s.borrow_mut() = State::default());
        for fd in [stdin_fds[0], stdin_fds[1], wake_fds[0], wake_fds[1]] {
            unsafe { libc::close(fd) };
        }
    }
}
