//! In-process twin of `simcore-runtime` for the browser (docs/web.md).
//!
//! Same C ABI (docs/protocol.md section C), but `submit_tree` renders into an RGBA framebuffer
//! instead of writing JSON lines, and `run` returns at once: the browser is the event loop.
//! JS drives it through the `simcore_web_*` exports at the bottom.
use simcore_protocol::{Appearance, ViewNode};
use simcore_render::{build_scene, render, ChromeParams, Fonts, Scene, SceneParams};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;

// iphone-17-pro (crates/simcore-host/src/session.rs DEVICES).
const VIEWPORT: (f64, f64) = (402.0, 874.0);
const SAFE: (f64, f64) = (62.0, 34.0);
const ISLAND: (f64, f64, f64, f64) = (138.4, 13.92, 125.2, 36.83);

struct State {
    ctx: usize,
    on_action: ActionCb,
    on_env: EnvCb,
    on_text: TextCb,
    version: Option<u64>,
    tree: Option<ViewNode>,
    scene: Option<Scene>,
    fonts: Option<Fonts>,
    scale: f64,
    dark: bool,
    pressed: Option<String>,
    tree_json: String,
    fb: Vec<u8>,
    fb_size: (u32, u32),
    scroll: HashMap<String, (f64, f64)>,
}

thread_local! {
    // ponytail: thread-local state; protocol says single-threaded.
    static STATE: RefCell<State> = RefCell::new(State {
        ctx: 0, on_action: None, on_env: None, on_text: None, version: None, tree: None, scene: None,
        fonts: None, scale: 3.0, dark: false, pressed: None, tree_json: String::new(), fb: Vec::new(),
        fb_size: (0, 0), scroll: HashMap::new(),
    });
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_err(msg: impl Into<String>) {
    let s = msg.into().replace('\0', " ");
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::new(s).ok());
}

fn draw(s: &mut State) {
    let Some(root) = &s.tree else { return };
    let fonts = s.fonts.get_or_insert_with(|| Fonts::with_fallback(true)); // browsers: bundled Inter only
    let params = SceneParams {
        viewport: VIEWPORT,
        safe_top: SAFE.0,
        safe_bottom: SAFE.1,
        scale: s.scale,
        appearance: if s.dark { Appearance::Dark } else { Appearance::Light },
        scroll: &s.scroll,
        focus: None,
        status_bar: true,
        chrome: ChromeParams { keyboard: false, shift: false, island: Some(ISLAND) },
    };
    let scene = build_scene(root, &params, fonts);
    let pm = render(&scene, fonts, s.scale);
    s.fb_size = (pm.width(), pm.height());
    s.fb = pm.take(); // opaque full-screen scene: premultiplied == straight RGBA
    s.scene = Some(scene);
}

fn submit(json: &[u8], version: u64) -> Result<(), String> {
    let text = std::str::from_utf8(json).map_err(|e| format!("tree is not UTF-8: {e}"))?;
    let root: ViewNode = serde_json::from_str(text).map_err(|e| format!("tree is not valid ViewNode JSON: {e}"))?;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.version.is_some_and(|v| version <= v) {
            return Err("tree version must increase monotonically".to_string());
        }
        s.version = Some(version);
        s.tree = Some(root);
        s.tree_json = text.to_string();
        draw(&mut s);
        Ok(())
    })
}

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
        eprintln!("simcore error: {}", CStr::from_ptr(msg).to_string_lossy());
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

// ---- JS-facing exports -------------------------------------------------------------------

extern "C" {
    /// The Swift program's entry (top-level code or `@main`; clang/Swift name `main(argc, argv)` this on Wasm): registers the app, no event loop.
    #[cfg(target_family = "wasm")]
    fn __main_argc_argv(argc: i32, argv: *const *const c_char) -> i32;
}

/// Evaluate the app and render the first frame. Call once, after `_initialize`.
/// `scale` = device pixels per point (framebuffer is 402*scale x 874*scale).
#[no_mangle]
pub extern "C" fn simcore_web_init(scale: f64, dark: i32) -> i32 {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
    });
    #[cfg(target_family = "wasm")]
    unsafe {
        __main_argc_argv(0, std::ptr::null());
    }
    set_environment(dark != 0)
}

/// Push the environment to the guest (as the native host does); the guest re-submits its tree.
fn set_environment(dark: bool) -> i32 {
    let (ctx, cb) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.dark = dark;
        (s.ctx, s.on_env)
    });
    let Some(cb) = cb else {
        set_err("app did not register callbacks");
        return 1;
    };
    let a = CString::new(if dark { "dark" } else { "light" }).unwrap_or_default();
    unsafe { cb(ctx as *mut c_void, a.as_ptr(), VIEWPORT.0, VIEWPORT.1) };
    0
}

#[no_mangle]
pub extern "C" fn simcore_web_set_dark(dark: i32) -> i32 {
    set_environment(dark != 0)
}

/// kind: 0 down, 1 up, 2 click. x, y in points. Returns 1 if an action was dispatched to the guest.
#[no_mangle]
pub extern "C" fn simcore_web_pointer(kind: i32, x: f64, y: f64) -> i32 {
    let (fire, ctx, cb) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(scene) = &s.scene else { return (None, 0, None) };
        let hit = scene.hit_test(x, y).map(|i| &scene.nodes[i]);
        let (hit_id, action) = (hit.map(|n| n.id.clone()), hit.and_then(|n| n.action));
        let released = match kind {
            0 => {
                s.pressed = hit_id;
                false
            }
            1 => s.pressed.take().is_some_and(|p| Some(&p) == hit_id.as_ref()),
            _ => true,
        };
        (action.filter(|_| released).zip(s.version), s.ctx, s.on_action)
    });
    // Same stale-version rule as the native loop: actions are tied to the tree they came from.
    match (fire, cb) {
        (Some((id, _)), Some(cb)) => {
            unsafe { cb(ctx as *mut c_void, id) };
            1
        }
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_ptr() -> *const u8 {
    STATE.with(|s| s.borrow().fb.as_ptr())
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_len() -> usize {
    STATE.with(|s| s.borrow().fb.len())
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_width() -> u32 {
    STATE.with(|s| s.borrow().fb_size.0)
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_height() -> u32 {
    STATE.with(|s| s.borrow().fb_size.1)
}
/// Latest submitted tree JSON (debug hook); valid until the next submit.
#[no_mangle]
pub extern "C" fn simcore_web_tree_ptr() -> *const u8 {
    STATE.with(|s| s.borrow().tree_json.as_ptr())
}
#[no_mangle]
pub extern "C" fn simcore_web_tree_len() -> usize {
    STATE.with(|s| s.borrow().tree_json.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = r#"{"kind":"vstack","id":"0","spacing":16.0,"alignment":"center","children":[
        {"kind":"text","id":"0.0","text":"Count: 0"},
        {"kind":"button","id":"0.1","action":1,"label":{"kind":"text","id":"0.1.l","text":"Increment"}}]}"#;

    thread_local!(static HITS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) });
    unsafe extern "C" fn on_action(_: *mut c_void, id: u64) {
        HITS.with(|h| h.borrow_mut().push(id));
    }

    #[test]
    fn submit_renders_and_click_dispatches() {
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 1, "version must increase");
        assert_eq!((simcore_web_fb_width(), simcore_web_fb_height()), (1206, 2622));
        assert_eq!(simcore_web_fb_len(), 1206 * 2622 * 4);
        assert_eq!(simcore_web_pointer(2, 1.0, 1.0), 0, "miss");
        let (x, y) = STATE.with(|s| {
            let s = s.borrow();
            let b = s.scene.as_ref().unwrap().nodes.iter().find(|n| n.action == Some(1)).unwrap().bounds;
            (b.x + b.w / 2.0, b.y + b.h / 2.0)
        });
        assert_eq!(simcore_web_pointer(2, x, y), 1);
        assert_eq!(HITS.with(|h| h.borrow().clone()), vec![1]);
    }
}
