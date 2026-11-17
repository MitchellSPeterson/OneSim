//! In-process twin of `simcore-runtime` for the browser (docs/web.md).
//!
//! Same C ABI (docs/protocol.md section C), but `submit_tree` renders into an RGBA framebuffer
//! instead of writing JSON lines, and `run` returns at once: the browser is the event loop.
//! JS drives it through the `simcore_web_*` exports at the bottom; input goes through
//! `simcore_render::Interaction` (the same state machine as the native host) and its effects
//! call the Swift callbacks directly.
use simcore_protocol::{Appearance, KeyboardEvent, PointerEvent, PointerKind, ViewNode};
use simcore_render::{build_scene, render, resolve_label, Effect, Fonts, Interaction, Scene, SceneParams};
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;

// iphone-17-pro chrome (crates/simcore-host/src/session.rs DEVICES); the viewport is a parameter.
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
    viewport: (f64, f64),
    scale: f64,
    dark: bool,
    inter: Interaction,
    /// Set before a guest callback; a tree submit clears it by drawing.
    stale: bool,
    tree_json: String,
    fb: Vec<u8>,
    fb_size: (u32, u32),
    /// Scratch buffer shared with JS (inputs: label/key text; outputs: inspect JSON, find result).
    buf: Vec<u8>,
}

thread_local! {
    // ponytail: thread-local state; protocol says single-threaded.
    static STATE: RefCell<State> = RefCell::new(State {
        ctx: 0, on_action: None, on_env: None, on_text: None, version: None, tree: None, scene: None,
        fonts: None, viewport: (402.0, 874.0), scale: 3.0, dark: false, inter: Interaction::default(),
        stale: false, tree_json: String::new(), fb: Vec::new(), fb_size: (0, 0), buf: Vec::new(),
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
        viewport: s.viewport,
        safe_top: SAFE.0,
        safe_bottom: SAFE.1,
        scale: s.scale,
        appearance: if s.dark { Appearance::Dark } else { Appearance::Light },
        scroll: &s.inter.scroll,
        focus: s.inter.focus.as_deref(),
        status_bar: true,
        chrome: s.inter.chrome(Some(ISLAND)),
    };
    let scene = build_scene(root, &params, fonts);
    let pm = render(&scene, fonts, s.scale);
    s.fb_size = (pm.width(), pm.height());
    s.fb = pm.take(); // opaque full-screen scene: premultiplied == straight RGBA
    s.scene = Some(scene);
    s.stale = false;
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
        s.inter.version = version;
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

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Evaluate the app and render the first frame. Call once, after `_initialize`.
/// `width` x `height` = viewport in points; `scale` = device pixels per point.
#[no_mangle]
pub extern "C" fn simcore_web_init(width: f64, height: f64, scale: f64, dark: i32) -> i32 {
    with(|s| {
        s.viewport = (width.max(1.0), height.max(1.0));
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
    });
    #[cfg(target_family = "wasm")]
    unsafe {
        __main_argc_argv(0, std::ptr::null());
    }
    push_environment()
}

/// Push the environment to the guest (as the native host does); the guest re-submits its tree.
fn push_environment() -> i32 {
    let (ctx, cb, dark, vp) = with(|s| (s.ctx, s.on_env, s.dark, s.viewport));
    let Some(cb) = cb else {
        set_err("app did not register callbacks");
        return 1;
    };
    let a = CString::new(if dark { "dark" } else { "light" }).unwrap_or_default();
    unsafe { cb(ctx as *mut c_void, a.as_ptr(), vp.0, vp.1) };
    0
}

#[no_mangle]
pub extern "C" fn simcore_web_set_dark(dark: i32) -> i32 {
    with(|s| s.dark = dark != 0);
    push_environment()
}

#[no_mangle]
pub extern "C" fn simcore_web_set_viewport(width: f64, height: f64) -> i32 {
    with(|s| s.viewport = (width.max(1.0), height.max(1.0)));
    push_environment()
}

/// Re-render at another device pixel scale (e.g. the window resized); no guest call.
#[no_mangle]
pub extern "C" fn simcore_web_set_scale(scale: f64) {
    with(|s| {
        s.scale = scale.clamp(1.0, 4.0);
        draw(s);
    });
}

/// Apply an interaction effect: guest callbacks for actions/text, a redraw otherwise.
/// Returns 1 if anything happened.
fn dispatch(e: Effect) -> i32 {
    let (ctx, on_action, on_text) = with(|s| (s.ctx, s.on_action, s.on_text));
    match e {
        Effect::None => return 0,
        Effect::Redraw => with(draw),
        Effect::Action { id, .. } => {
            with(|s| s.stale = true);
            if let Some(cb) = on_action {
                unsafe { cb(ctx as *mut c_void, id) };
            }
        }
        Effect::TextInput { field, text, .. } => {
            with(|s| s.stale = true);
            if let (Some(cb), Ok(t)) = (on_text, CString::new(text.replace('\0', ""))) {
                unsafe { cb(ctx as *mut c_void, field, t.as_ptr()) };
            }
        }
    }
    // The callback re-submits (and draws) only if the guest state changed; focus/shift may have.
    with(|s| {
        if s.stale {
            draw(s)
        }
    });
    1
}

/// kind: 0 down, 1 up, 2 click. x, y in points. Returns 1 if the interaction changed anything.
#[no_mangle]
pub extern "C" fn simcore_web_pointer(kind: i32, x: f64, y: f64) -> i32 {
    let kind = match kind {
        0 => PointerKind::Down,
        1 => PointerKind::Up,
        _ => PointerKind::Click,
    };
    let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, PointerEvent::new(kind, x, y))));
    e.map_or(0, dispatch)
}

/// Scroll the scroller under (x, y) by (dx, dy) points; positive dy reveals later content.
#[no_mangle]
pub extern "C" fn simcore_web_scroll(x: f64, y: f64, dx: f64, dy: f64) -> i32 {
    let ev = PointerEvent { kind: PointerKind::Scroll, x, y, dx, dy };
    let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, ev)));
    e.map_or(0, dispatch)
}

/// Keyboard input for the focused field; the payload is `len` UTF-8 bytes in the scratch buffer.
/// kind 0: text to append (programmatic, like native `--type`: shift untouched); kind 1: key name
/// (`backspace` or `enter`); kind 2: text typed by a person on a physical keyboard (shift then
/// follows iOS auto-capitalization, as when tapping keys). Returns 1 if applied,
/// 0 if ignored, -1 if no field is focused.
#[no_mangle]
pub extern "C" fn simcore_web_key(kind: i32, len: usize) -> i32 {
    let e = with(|s| {
        let payload = String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())]).into_owned();
        let ev = match (kind, payload.as_str()) {
            (0 | 2, _) => KeyboardEvent::Text { text: payload },
            (1, "backspace" | "enter") => KeyboardEvent::Key { key: payload },
            _ => return Ok(Effect::None),
        };
        let sc = s.scene.as_ref().ok_or("no scene")?;
        let r = s.inter.keyboard(sc, ev);
        if let (2, Ok(Effect::TextInput { text, .. })) = (kind, &r) {
            s.inter.shift = simcore_render::autocap(text);
        }
        r
    });
    match e {
        Ok(e) => dispatch(e),
        Err(_) => -1,
    }
}

/// 1 while a text field is focused (the software keyboard is showing).
#[no_mangle]
pub extern "C" fn simcore_web_focused() -> i32 {
    with(|s| s.inter.focus.is_some() as i32)
}

/// Scratch buffer shared with JS: resized to `len`, returns its address.
#[no_mangle]
pub extern "C" fn simcore_web_buf(len: usize) -> *mut u8 {
    with(|s| {
        s.buf.resize(len, 0);
        s.buf.as_mut_ptr()
    })
}
#[no_mangle]
pub extern "C" fn simcore_web_buf_len() -> usize {
    with(|s| s.buf.len())
}

/// Inspection tree (protocol `UiNode`, as `inspect_ui_tree`) as JSON in the scratch buffer.
#[no_mangle]
pub extern "C" fn simcore_web_inspect() -> i32 {
    with(|s| {
        let Some(sc) = &s.scene else { return 1 };
        s.buf = serde_json::to_vec(&sc.ui_tree()).unwrap_or_default();
        0
    })
}

/// Resolve the label in the scratch buffer (`simcore_render::resolve_label` syntax, the CLI's
/// `--click LABEL`). 0: two little-endian f64 (x, y in points) are in the buffer; 1: error text.
#[no_mangle]
pub extern "C" fn simcore_web_find(len: usize) -> i32 {
    with(|s| {
        let label = String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())]).into_owned();
        let found = match &s.scene {
            Some(sc) => resolve_label(&sc.ui_tree(), &label),
            None => Err("no scene".to_string()),
        };
        match found {
            Ok((x, y)) => {
                s.buf = [x.to_le_bytes(), y.to_le_bytes()].concat();
                0
            }
            Err(m) => {
                s.buf = m.into_bytes();
                1
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_ptr() -> *const u8 {
    with(|s| s.fb.as_ptr())
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_len() -> usize {
    with(|s| s.fb.len())
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_width() -> u32 {
    with(|s| s.fb_size.0)
}
#[no_mangle]
pub extern "C" fn simcore_web_fb_height() -> u32 {
    with(|s| s.fb_size.1)
}
/// Latest submitted tree JSON (debug hook); valid until the next submit.
#[no_mangle]
pub extern "C" fn simcore_web_tree_ptr() -> *const u8 {
    with(|s| s.tree_json.as_ptr())
}
#[no_mangle]
pub extern "C" fn simcore_web_tree_len() -> usize {
    with(|s| s.tree_json.len())
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

    #[test]
    fn find_inspect_and_scroll() {
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let buf = |t: &str| unsafe { std::slice::from_raw_parts_mut(simcore_web_buf(t.len()), t.len()) }.copy_from_slice(t.as_bytes());
        buf("Increment");
        assert_eq!(simcore_web_find(9), 0);
        let (x, y) = with(|s| (f64::from_le_bytes(s.buf[..8].try_into().unwrap()), f64::from_le_bytes(s.buf[8..16].try_into().unwrap())));
        assert_eq!(simcore_web_pointer(2, x, y), 1);
        buf("Nope");
        assert_eq!(simcore_web_find(4), 1);
        assert_eq!(simcore_web_inspect(), 0);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("Increment")));
        assert_eq!(simcore_web_scroll(200.0, 400.0, 0.0, 50.0), 0, "nothing scrollable");
        buf("hi");
        assert_eq!(simcore_web_key(0, 2), -1, "no focus");
    }
}
