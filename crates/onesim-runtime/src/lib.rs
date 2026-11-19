//! C ABI for the OneSim guest wire (docs/protocol.md sections B and C).
use serde_json::{value::RawValue, Value};
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::io::{BufRead, Write};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;

#[derive(Default)]
struct State {
    out: Option<Box<dyn Write>>,
    last_version: Option<u64>,
    ctx: usize,
    on_action: ActionCb,
    on_env: EnvCb,
    on_text: TextCb,
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
    send_line(&format!(r#"{{"type":"tree","version":{version},"root":{root}}}"#))?;
    STATE.with(|s| s.borrow_mut().last_version = Some(version));
    Ok(())
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
        match init_with(Box::new(file)) {
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
pub extern "C" fn simcore_runtime_run() -> i32 {
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
    unsafe extern "C" fn on_action(_: *mut c_void, id: u64) { SEEN.with(|s| s.borrow_mut().push(format!("a{id}"))) }
    unsafe extern "C" fn on_env(_: *mut c_void, a: *const c_char, w: f64, h: f64) {
        SEEN.with(|s| s.borrow_mut().push(format!("e{}{w}x{h}", CStr::from_ptr(a).to_str().unwrap())))
    }

    unsafe extern "C" fn on_text(_: *mut c_void, f: u64, t: *const c_char) {
        SEEN.with(|s| s.borrow_mut().push(format!("t{f}={}", CStr::from_ptr(t).to_str().unwrap())))
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
}
