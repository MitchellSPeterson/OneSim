use crate::state::{set_err, submit, with, ActionCb, EnvCb, GestureCb, LayoutCb, TextCb, WakeCb, LAST_ERROR, STATE};
use onesim_protocol::GuestMessage;
use std::ffi::{c_char, c_void, CStr};

#[no_mangle]
pub extern "C" fn simcore_runtime_init() -> i32 {
    0
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

#[no_mangle]
pub extern "C" fn simcore_runtime_set_text_callback(on_text: TextCb) {
    STATE.with(|s| s.borrow_mut().on_text = on_text);
}

/// Optional section L gesture callback (same ctx as the action callback); `dispatch` calls it for touch effects.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_gesture_callback(on_gesture: GestureCb) {
    STATE.with(|s| s.borrow_mut().on_gesture = on_gesture);
}

/// Optional section N layout callback (same ctx as the action callback); called when custom SwiftUI shapes need `path(in:)`.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_layout_callback(on_layout: LayoutCb) {
    STATE.with(|s| s.borrow_mut().on_layout = on_layout);
}

/// Stored for ABI parity. The browser is the event loop; WASI has no HTTP `AsyncImage`.
#[no_mangle]
pub extern "C" fn simcore_runtime_set_wake(_fd: i32, _on_wake: WakeCb) {}

/// Guest JSON line (`scroll_to_id` dropped; `schedule` updates the animation tick deadline; `pasteboard` is kept
/// for `simcore_web_take_pasteboard`).
///
/// # Safety
/// `line` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn simcore_send_line(line: *const c_char) -> i32 {
    if !line.is_null() {
        if let Ok(text) = CStr::from_ptr(line).to_str() {
            match serde_json::from_str::<GuestMessage>(text.trim()) {
                Ok(GuestMessage::Schedule { after_ms }) => with(|s| {
                    s.guest_due_ms = after_ms.map(|ms| s.clock_ms + ms.max(0.0));
                }),
                Ok(GuestMessage::Pasteboard { items }) => with(|s| s.pasteboard_out = Some(items)),
                _ => {}
            }
        }
    }
    0
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
        Err(e) => {
            set_err(e);
            1
        }
    }
}

/// # Safety
/// `msg` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn simcore_report_error(msg: *const c_char) {
    if !msg.is_null() {
        eprintln!("onesim error: {}", CStr::from_ptr(msg).to_string_lossy());
    }
}

/// Returns at once; the browser delivers events via the `simcore_web_*` exports.
#[no_mangle]
pub extern "C" fn simcore_runtime_run() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn simcore_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(std::ptr::null(), |c| c.as_ptr()))
}
