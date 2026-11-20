//! In-process twin of `onesim-runtime` for the browser (docs/web.md).
//!
//! Same C ABI (docs/protocol.md section C), but `submit_tree` renders into an RGBA framebuffer
//! instead of writing JSON lines, and `run` returns at once: the browser is the event loop.
//! JS drives it through the `simcore_web_*` exports at the bottom; input goes through
//! `onesim_render::Interaction` (the same state machine as the native host) and its effects
//! call the Swift callbacks directly.
use onesim_protocol::{Appearance, DynamicTypeSize, GuestMessage, HostMessage, IosVersion, KeyboardEvent, PointerEvent, PointerKind, Viewport, ViewNode};
#[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
use onesim_protocol::TouchPhase;
use onesim_render::rn_input::{effect_messages, Out, RnCtx, RnInput};
use onesim_render::display::DisplayList;
use onesim_render::{build_scene, Builder, device, look, render, resolve_label, Animator, Device, DEFAULT_DEVICE, DEVICES, DragPhase, Effect, Fonts, Interaction, Scene, SceneParams};
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;
pub type GestureCb = Option<unsafe extern "C" fn(*mut c_void, i32, f64, *const f64, usize, *const *const c_char, *const f64, usize)>;
pub type LayoutCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;

struct State {
    ctx: usize,
    on_action: ActionCb,
    on_env: EnvCb,
    on_text: TextCb,
    on_gesture: GestureCb,
    /// Section M/N/O layout & tick callback (`geometry` / `scroll_position` / `layout` / `tick`).
    on_layout: LayoutCb,
    /// Guards against infinite re-entry when a layout callback submits a new tree.
    in_layout: bool,
    version: Option<u64>,
    tree: Option<ViewNode>,
    scene: Option<Scene>,
    fonts: Option<Fonts>,
    /// Chrome source (crates/onesim-render/src/device.rs); the viewport is a parameter.
    device: &'static Device,
    viewport: (f64, f64),
    scale: f64,
    dark: bool,
    dynamic_type: DynamicTypeSize,
    ios: IosVersion,
    inter: Interaction,
    anim: Animator,
    clock_ms: f64,
    guest_clock_ms: f64,
    guest_due_ms: Option<f64>,
    last_anim_ver: Option<u64>,
    /// Set before a guest callback; a tree submit clears it by drawing.
    stale: bool,
    initialized: bool,
    tree_json: String,
    fb: Vec<u8>,
    fb_size: (u32, u32),
    /// 0: `draw` rasterizes into `fb` (CPU); 1: it lowers the scene to an encoded display list in `dl` for a GPU executor.
    output: i32,
    /// Persistent so successive display lists only carry new resources; reset whenever the consumer changes.
    builder: Builder,
    dl: Vec<u8>,
    /// The latest display list until JS reads it (`simcore_web_dl_ptr`): a newer frame drawn first carries its
    /// resource changes on, since the builder sends each resource once.
    unread: Option<DisplayList>,
    /// Scratch buffer shared with JS (inputs: label/key text; outputs: inspect JSON, find result).
    buf: Vec<u8>,
    /// React Native mode (`simcore_web_rn_init`): no Swift callbacks; the page relays messages to the guest Worker.
    rn: Option<Rn>,
}

/// React Native mode: input state shared with the native host, and the host messages (docs/protocol.md section H)
/// waiting for the page to post them to the guest (`simcore_web_rn_outbox`).
#[derive(Default)]
struct Rn {
    input: RnInput,
    outbox: Vec<HostMessage>,
}

thread_local! {
    // ponytail: thread-local state; protocol says single-threaded.
    static STATE: RefCell<State> = RefCell::new(State {
        ctx: 0, on_action: None, on_env: None, on_text: None, on_gesture: None, on_layout: None, in_layout: false, version: None, tree: None, scene: None,
        fonts: None, device: device(DEFAULT_DEVICE).unwrap(), viewport: (402.0, 874.0), scale: 3.0, dark: false, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios27, inter: Interaction::default(),
        anim: Animator::new(), clock_ms: 0.0, guest_clock_ms: 0.0, guest_due_ms: None, last_anim_ver: None,
        stale: false, initialized: false, tree_json: String::new(), fb: Vec::new(), fb_size: (0, 0), output: 0, builder: Builder::new(), dl: Vec::new(), unread: None, buf: Vec::new(), rn: None,
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
        device: s.device,
        scale: s.scale,
        appearance: if s.dark { Appearance::Dark } else { Appearance::Light },
        dynamic_type: s.dynamic_type,
        ios: s.ios,
        scroll: &s.inter.scroll,
        sheets: &s.inter.sheets,
        focus: s.inter.focus.as_deref(),
        overscroll: s.inter.live(),
        status_bar: true,
        chrome: s.inter.chrome(),
    };
    let mut scene = build_scene(root, &params, fonts);
    if s.rn.is_none() {
        if s.version != s.last_anim_ver {
            s.anim.update(&scene);
            s.last_anim_ver = s.version;
        }
        if s.anim.is_animating() {
            scene = s.anim.render_scene(&scene);
        }
    }
    if s.output == 1 {
        let mut list = s.builder.build(&scene, fonts, s.scale);
        if let Some(old) = s.unread.take() {
            // The executor frees `dropped`, then adds `added`: keep what the old list added unless this one drops it.
            let added_ids: Vec<u64> = old.added.iter().map(|r| r.id).collect();
            let mut added: Vec<_> = old.added.into_iter().filter(|r| !list.dropped.contains(&r.id)).collect();
            list.dropped.retain(|id| !added_ids.contains(id));
            list.dropped.extend(old.dropped);
            added.append(&mut list.added);
            list.added = added;
        }
        s.fb_size = (list.width, list.height);
        s.dl = list.encode();
        s.unread = Some(list);
    } else {
        let pm = render(&scene, fonts, s.scale);
        s.fb_size = (pm.width(), pm.height());
        s.fb = pm.take(); // opaque full-screen scene: premultiplied == straight RGBA
    }
    // React Native lays out on the host: the guest learns every frame change (`layout`), as from the native host.
    if let Some(rn) = s.rn.as_mut() {
        rn.outbox.extend(rn.input.layout(&scene));
    }
    s.scene = Some(scene);
    s.stale = false;
}

fn submit(json: &[u8], version: u64) -> Result<(), String> {
    let text = std::str::from_utf8(json).map_err(|e| format!("tree is not UTF-8: {e}"))?;
    let root: ViewNode = serde_json::from_str(text).map_err(|e| format!("tree is not valid ViewNode JSON: {e}"))?;
    let pending = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.version.is_some_and(|v| version <= v) {
            return Err("tree version must increase monotonically".to_string());
        }
        s.version = Some(version);
        s.inter.version = version;
        s.tree = Some(root);
        s.tree_json = text.to_string();
        draw(&mut s);
        if s.in_layout {
            return Ok(None);
        }
        let frames = s.scene.as_ref().map(|sc| sc.pending_shape_frames()).unwrap_or_default();
        if !frames.is_empty() {
            if let Some(cb) = s.on_layout {
                s.in_layout = true;
                return Ok(Some((cb, s.ctx, frames)));
            }
        }
        Ok(None)
    })?;
    if let Some((cb, ctx, frames)) = pending {
        let line = CString::new(serde_json::json!({"type": "layout", "frames": frames}).to_string()).unwrap_or_default();
        unsafe { cb(ctx as *mut c_void, line.as_ptr()) };
        STATE.with(|s| s.borrow_mut().in_layout = false);
    }
    Ok(())
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

/// Guest JSON line (`scroll_to_id` dropped; `schedule` updates the animation tick deadline).
///
/// # Safety
/// `line` must be NUL-terminated or null.
#[no_mangle]
pub unsafe extern "C" fn simcore_send_line(line: *const c_char) -> i32 {
    if !line.is_null() {
        if let Ok(text) = CStr::from_ptr(line).to_str() {
            if let Ok(GuestMessage::Schedule { after_ms }) = serde_json::from_str::<GuestMessage>(text.trim()) {
                with(|s| {
                    s.guest_due_ms = after_ms.map(|ms| s.clock_ms + ms.max(0.0));
                });
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

// ---- JS-facing exports -------------------------------------------------------------------

#[cfg(all(target_family = "wasm", not(feature = "js-guest")))]
extern "C" {
    /// The Swift program's entry (top-level code or `@main`; clang/Swift name `main(argc, argv)` this on Wasm): registers the app, no event loop.
    fn __main_argc_argv(argc: i32, argv: *const *const c_char) -> i32;
}

#[cfg(all(target_family = "wasm", feature = "js-guest"))]
#[link(wasm_import_module = "onesim_guest")]
extern "C" {
    fn on_init(dark: i32, width: f64, height: f64, safe_top: f64, safe_bottom: f64) -> i32;
    fn on_env(dark: i32, width: f64, height: f64, safe_top: f64, safe_bottom: f64) -> i32;
    fn on_action(id: f64);
    fn on_text(field: f64, ptr: *const u8, len: usize);
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Device list as JSON `[{name,width,height,scale,safeTop,safeBottom,android}]` in the scratch buffer (usable before init).
#[no_mangle]
pub extern "C" fn simcore_web_devices() -> i32 {
    let list: Vec<_> = DEVICES
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "width": d.viewport.width,
                "height": d.viewport.height,
                "scale": d.scale,
                "safeTop": d.safe_top,
                "safeBottom": d.safe_bottom,
                "android": d.android,
            })
        })
        .collect();
    with(|s| s.buf = serde_json::to_vec(&list).unwrap_or_default());
    0
}

/// Select the device whose name is the `len` UTF-8 bytes in the scratch buffer (chrome only; the
/// viewport and scale come from `simcore_web_init`). Call before init. 0 ok, 1 unknown name.
#[no_mangle]
pub extern "C" fn simcore_web_set_device(len: usize) -> i32 {
    with(|s| match device(&String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())])) {
        Some(d) => {
            s.device = d;
            0
        }
        None => 1,
    })
}

/// Submit a ViewNode JSON tree written into the scratch buffer (`simcore_web_buf`), for JS-hosted guests.
/// Returns 0 on success, 1 on error.
#[no_mangle]
pub extern "C" fn simcore_web_submit_tree(len: usize, version: f64) -> i32 {
    let bytes = with(|s| s.buf[..len.min(s.buf.len())].to_vec());
    match submit(&bytes, version.max(0.0) as u64) {
        Ok(()) => 0,
        Err(e) => {
            set_err(e);
            1
        }
    }
}

/// Evaluate the app and render the first frame. Call once, after `_initialize`.
/// `width` x `height` = viewport in points; `scale` = device pixels per point.
#[no_mangle]
pub extern "C" fn simcore_web_init(width: f64, height: f64, scale: f64, dark: i32) -> i32 {
    let (w, h, safe_top, safe_bottom) = with(|s| {
        let w = width.max(1.0);
        let h = height.max(1.0);
        s.viewport = (w, h);
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
        s.initialized = true;
        (w, h, s.device.safe_top, s.device.safe_bottom)
    });
    #[cfg(all(target_family = "wasm", feature = "js-guest"))]
    {
        unsafe { on_init(if dark != 0 { 1 } else { 0 }, w, h, safe_top, safe_bottom) }
    }
    #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
    {
        let _ = (w, h, safe_top, safe_bottom);
        #[cfg(target_family = "wasm")]
        unsafe {
            __main_argc_argv(0, std::ptr::null());
        }
        push_environment()
    }
}

/// Push the environment to the guest (as the native host does); the guest re-submits its tree.
fn push_environment() -> i32 {
    let rn = with(|s| {
        let (dark, vp, dt) = (s.dark, s.viewport, s.dynamic_type);
        let rn = s.rn.as_mut()?;
        rn.outbox.push(HostMessage::Environment {
            appearance: if dark { Appearance::Dark } else { Appearance::Light },
            viewport: Viewport { width: vp.0, height: vp.1 },
            dynamic_type_size: (dt != DynamicTypeSize::Large).then_some(dt),
        });
        Some(())
    });
    if rn.is_some() {
        with(draw); // the viewport is the host's: lay out now, the guest re-renders after
        return 0;
    }
    #[cfg(all(target_family = "wasm", feature = "js-guest"))]
    {
        let (dark, vp, safe_top, safe_bottom) = with(|s| {
            s.stale = true;
            (s.dark, s.viewport, s.device.safe_top, s.device.safe_bottom)
        });
        let rc = unsafe { on_env(dark as i32, vp.0, vp.1, safe_top, safe_bottom) };
        with(|s| {
            if s.stale {
                draw(s);
            }
        });
        rc
    }
    #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
    {
        let (ctx, cb, dark, vp) = with(|s| (s.ctx, s.on_env, s.dark, s.viewport));
        let Some(cb) = cb else {
            set_err("app did not register callbacks");
            return 1;
        };
        let a = CString::new(if dark { "dark" } else { "light" }).unwrap_or_default();
        unsafe { cb(ctx as *mut c_void, a.as_ptr(), vp.0, vp.1) };
        0
    }
}

#[no_mangle]
pub extern "C" fn simcore_web_set_dark(dark: i32) -> i32 {
    with(|s| s.dark = dark != 0);
    push_environment()
}

/// Dynamic Type size as an index into `DynamicTypeSize::ALL` (0 = xSmall, 3 = large, 11 = accessibility5).
#[no_mangle]
pub extern "C" fn simcore_web_set_dynamic_type(index: i32) -> i32 {
    with(|s| s.dynamic_type = DynamicTypeSize::from_index(index.max(0) as usize));
    // Before `init` there is no guest yet; the size is picked up by the first render.
    if with(|s| s.on_env.is_some() || s.initialized || s.rn.is_some()) { push_environment() } else { 0 }
}

/// Simulated iOS release by index into `IosVersion::ALL` (0 = 26.5, 1 = 27.2); out of range = newest.
#[no_mangle]
pub extern "C" fn simcore_web_set_ios(index: i32) -> i32 {
    with(|s| {
        s.ios = IosVersion::ALL
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or_default();
        draw(s);
    });
    0
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
    #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
    let (ctx, on_action, on_text) = with(|s| (s.ctx, s.on_action, s.on_text));
    if !matches!(e, Effect::None | Effect::Redraw) && with(|s| s.rn.is_some()) {
        return with(|s| {
            if let Some(rn) = s.rn.as_mut() {
                rn.outbox.extend(effect_messages(e));
            }
            draw(s); // focus/press state may have changed; the guest's answer arrives as a new tree
            1
        });
    }
    match e {
        Effect::None => return 0,
        Effect::Redraw => with(draw),
        Effect::Action { id, .. } => {
            with(|s| s.stale = true);
            #[cfg(all(target_family = "wasm", feature = "js-guest"))]
            unsafe {
                on_action(id as f64);
            }
            #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
            if let Some(cb) = on_action {
                unsafe { cb(ctx as *mut c_void, id) };
            }
        }
        Effect::TextInput { field, text, .. } => {
            with(|s| s.stale = true);
            #[cfg(all(target_family = "wasm", feature = "js-guest"))]
            unsafe {
                on_text(field as f64, text.as_ptr(), text.len());
            }
            #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
            if let (Some(cb), Ok(t)) = (on_text, CString::new(text.replace('\0', ""))) {
                unsafe { cb(ctx as *mut c_void, field, t.as_ptr()) };
            }
        }
        Effect::Gesture(touches) => {
            with(|s| s.stale = true);
            #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
            if let Some(cb) = with(|s| s.on_gesture) {
                for t in touches {
                    let phase = match t.phase {
                        TouchPhase::Start => 0,
                        TouchPhase::Move => 1,
                        TouchPhase::End => 2,
                        TouchPhase::Cancel => 3,
                    };
                    let pts: Vec<f64> = t.points.iter().flatten().copied().collect();
                    let keys: Vec<CString> = t.targets.iter().map(|g| CString::new(g.key.replace('\0', "")).unwrap_or_default()).collect();
                    let ptrs: Vec<*const c_char> = keys.iter().map(|k| k.as_ptr()).collect();
                    let frames: Vec<f64> = t.targets.iter().flat_map(|g| g.frame).collect();
                    unsafe { cb(ctx as *mut c_void, phase, t.time, pts.as_ptr(), t.points.len(), ptrs.as_ptr(), frames.as_ptr(), keys.len()) };
                }
            }
            #[cfg(all(target_family = "wasm", feature = "js-guest"))]
            let _ = touches;
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

/// Synchronize the SwiftUI guest's clock with `clock_ms` before an input starts new timers.
fn sync_guest_clock() {
    let sync = with(|s| {
        if s.rn.is_none() && (s.clock_ms - s.guest_clock_ms).abs() > 1e-6 {
            s.guest_clock_ms = s.clock_ms;
            s.on_layout.map(|cb| (s.ctx, cb, s.clock_ms))
        } else {
            None
        }
    });
    if let Some((ctx, cb, time)) = sync {
        if let Ok(msg) = CString::new(format!(r#"{{"type":"tick","time":{time}}}"#)) {
            unsafe { cb(ctx as *mut c_void, msg.as_ptr()) };
        }
    }
}

/// kind: 0 down, 1 up, 2 click. x, y in points. Returns 1 if the interaction changed anything.
#[no_mangle]
pub extern "C" fn simcore_web_pointer(kind: i32, x: f64, y: f64) -> i32 {
    let kind = match kind {
        0 => PointerKind::Down,
        1 => PointerKind::Up,
        _ => PointerKind::Click,
    };
    rn_pointer(PointerEvent::new(kind, x, y)).unwrap_or_else(|| {
        sync_guest_clock();
        let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, PointerEvent::new(kind, x, y))));
        e.map_or(0, dispatch)
    })
}

/// React Native mode: deliver `out` (messages to the outbox, native-control effects through `dispatch`) and redraw.
fn deliver(out: Vec<Out>) -> i32 {
    let any = !out.is_empty();
    for o in out {
        match o {
            Out::Send(m, _) => with(|s| s.rn.as_mut().map_or((), |rn| rn.outbox.push(m))),
            Out::Effect(e) => drop(dispatch(e)),
        }
    }
    with(draw); // scroll offsets, focus and the keyboard are host state
    i32::from(any)
}

/// Run `f` against the React Native input state, the interaction and the current scene and tree.
/// `None` outside React Native mode or before the first tree.
fn with_rn<R>(f: impl FnOnce(&mut RnInput, &mut Interaction, &RnCtx) -> R) -> Option<R> {
    with(|s| {
        let rn = s.rn.as_mut()?;
        let (scene, root) = (s.scene.as_ref()?, s.tree.as_ref()?);
        let cx = RnCtx { scene, root, keyboard: look::keyboard_height(s.device, s.ios) };
        Some(f(&mut rn.input, &mut s.inter, &cx))
    })
}

fn rn_pointer(ev: PointerEvent) -> Option<i32> {
    if with(|s| s.rn.is_none()) {
        return None;
    }
    let out = with_rn(|rn, inter, cx| rn.pointer(inter, cx, ev).2)?;
    Some(deliver(out))
}

/// Scroll the scroller under (x, y) by (dx, dy) points; positive dy reveals later content.
#[no_mangle]
pub extern "C" fn simcore_web_scroll(x: f64, y: f64, dx: f64, dy: f64) -> i32 {
    let ev = PointerEvent::moved(PointerKind::Scroll, x, y, dx, dy);
    if let Some(r) = rn_pointer(ev) {
        return r;
    }
    let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, ev)));
    e.map_or(0, dispatch)
}

/// Touch drag (finger scrolling). phase: 0 begin (at the touch-down point), 1 move, 2 end (lift).
/// x, y in points; `t` is the event time in ms. Returns 1 if the frame changed. After an end,
/// call `simcore_web_tick` every frame while it returns 1 (momentum, bounce).
#[no_mangle]
pub extern "C" fn simcore_web_drag(phase: i32, x: f64, y: f64, t: f64) -> i32 {
    let phase = match phase {
        0 => DragPhase::Begin,
        1 => DragPhase::Move,
        _ => DragPhase::End,
    };
    if let Some(Some(out)) = with_rn(|rn, inter, cx| rn.drag(inter, cx.scene, phase, x, y, t)) {
        return deliver(out);
    }
    sync_guest_clock();
    let e = with(|s| s.scene.as_ref().map(|sc| s.inter.drag(sc, phase, x, y, t)));
    e.map_or(0, dispatch)
}

/// Advance scroll animations, SwiftUI animations, and scheduled guest ticks by `dt_ms`.
/// Returns 1 while anything is still active (redrawn); 0 = at rest.
#[no_mangle]
pub extern "C" fn simcore_web_tick(dt_ms: f64) -> i32 {
    let dt_ms = dt_ms.max(0.0);
    let dt = dt_ms / 1000.0;
    let tick_guest = with(|s| {
        s.clock_ms += dt_ms;
        if s.guest_due_ms.is_some_and(|due| s.clock_ms + 1e-6 >= due) {
            s.guest_due_ms = None;
            s.guest_clock_ms = s.clock_ms;
            Some((s.ctx, s.on_layout, s.clock_ms))
        } else {
            None
        }
    });
    if let Some((ctx, Some(cb), time)) = tick_guest {
        if let Ok(msg) = CString::new(format!(r#"{{"type":"tick","time":{time}}}"#)) {
            unsafe { cb(ctx as *mut c_void, msg.as_ptr()) };
        }
    }
    let (scroll_moved, was_animating, anim_moved) = with(|s| {
        let sm = s.scene.as_ref().is_some_and(|sc| s.inter.tick(sc, dt));
        let was = s.anim.is_animating();
        let am = s.anim.tick(dt);
        (sm, was, am)
    });
    let redrew = scroll_moved || was_animating || anim_moved || tick_guest.is_some();
    if redrew {
        with(draw);
    }
    // A finger resting on a gesture view keeps reporting (long presses fire while held).
    let held = with(|s| s.inter.holding());
    if held {
        dispatch(with(|s| s.inter.hold(dt)));
    }
    let more = with(|s| s.anim.is_animating() || s.guest_due_ms.is_some());
    // Return 1 if we redrew (including the final settling frame) so the browser presents it; the next tick returns 0.
    (redrew || held || more) as i32
}

/// Keyboard input for the focused field; the payload is `len` UTF-8 bytes in the scratch buffer.
/// kind 0: text to append (programmatic, like native `--type`: shift untouched); kind 1: key name
/// (`backspace` or `enter`); kind 2: text typed by a person on a physical keyboard (shift then
/// follows iOS auto-capitalization, as when tapping keys). Returns 1 if applied,
/// 0 if ignored, -1 if no field is focused.
#[no_mangle]
pub extern "C" fn simcore_web_key(kind: i32, len: usize) -> i32 {
    let payload = with(|s| String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())]).into_owned());
    let ev = match (kind, payload.as_str()) {
        (0 | 2, _) => KeyboardEvent::Text { text: payload },
        (1, "backspace" | "enter") => KeyboardEvent::Key { key: payload },
        _ => return 0,
    };
    if with(|s| s.rn.is_some()) {
        return match with_rn(|rn, inter, cx| rn.keyboard(inter, cx, ev)) {
            Some(Ok(out)) => deliver(out),
            _ => -1,
        };
    }
    sync_guest_clock();
    let e = with(|s| {
        let sc = s.scene.as_ref().ok_or("no scene")?;
        let r = s.inter.keyboard(sc, ev);
        if let (2, Ok(Effect::TextInput { text, .. })) = (kind, &r) {
            s.inter.shift = onesim_render::autocap(text);
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

/// Resolve the label in the scratch buffer (`onesim_render::resolve_label` syntax, the CLI's
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

/// Choose what `draw` produces: 0 = CPU pixels (`simcore_web_fb_*`, the default), 1 = an encoded display list
/// (`simcore_web_dl_*`) for the GPU executor. Resets the display-list builder (the consumer starts with no
/// resources: call this again after a device loss or re-init) and redraws the current scene in the new mode.
/// Returns 0, or 1 for an unknown mode.
#[no_mangle]
pub extern "C" fn simcore_web_set_output(mode: i32) -> i32 {
    if !(0..=1).contains(&mode) {
        return 1;
    }
    with(|s| {
        s.output = mode;
        s.builder.reset();
        s.unread = None;
        if mode == 1 {
            s.fb = Vec::new();
        } else {
            s.dl = Vec::new();
        }
        draw(s);
    });
    0
}
/// Encoded display list of the latest frame (output mode 1); valid until the next draw.
#[no_mangle]
pub extern "C" fn simcore_web_dl_ptr() -> *const u8 {
    with(|s| {
        s.unread = None; // the consumer has it now
        s.dl.as_ptr()
    })
}
#[no_mangle]
pub extern "C" fn simcore_web_dl_len() -> usize {
    with(|s| s.dl.len())
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

// ---- React Native (the guest runs in a browser Worker; docs/web.md) ----------------------------------------------

/// Switch to React Native mode and set the screen: `width` x `height` points, `scale`, `dark`, `android` (1 = the
/// default Android device unless `simcore_web_set_device` chose one). Nothing draws until the guest's first tree
/// (`simcore_web_rn_receive`). Call once instead of `simcore_web_init`.
#[no_mangle]
pub extern "C" fn simcore_web_rn_init(width: f64, height: f64, scale: f64, dark: i32, android: i32) -> i32 {
    with(|s| {
        s.viewport = (width.max(1.0), height.max(1.0));
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
        if (android != 0) != s.device.android {
            s.device = device(if android != 0 { onesim_render::DEFAULT_ANDROID_DEVICE } else { DEFAULT_DEVICE }).unwrap();
        }
        s.rn = Some(Rn::default());
    });
    0
}

/// One guest message (docs/protocol.md section H, JSON, `len` bytes in the scratch buffer). A `tree` is drawn;
/// `focus_request` / `scroll_to` are applied. Returns 1 if the frame changed, 0 if not (other messages are the
/// page's), -1 on a bad message or a stale tree version (the reason is in the scratch buffer).
#[no_mangle]
pub extern "C" fn simcore_web_rn_receive(len: usize) -> i32 {
    let msg = with(|s| serde_json::from_slice::<GuestMessage>(&s.buf[..len.min(s.buf.len())]));
    let fail = |m: String| {
        with(|s| s.buf = m.into_bytes());
        -1
    };
    match msg {
        Err(e) => fail(format!("bad guest message: {e}")),
        Ok(GuestMessage::Tree { version, root }) => {
            let stale = with(|s| s.version.is_some_and(|v| version <= v));
            if stale {
                return fail("tree version must increase monotonically".into());
            }
            with(|s| {
                s.version = Some(version);
                s.inter.version = version;
                s.tree_json = serde_json::to_string(&root).unwrap_or_default();
                s.tree = Some(root);
                draw(s);
            });
            rn_requests(vec![]); // drop the focus of a field that is gone
            1
        }
        Ok(m @ (GuestMessage::FocusRequest { .. } | GuestMessage::ScrollTo { .. })) => rn_requests(vec![m]),
        // A new guest instance (first start or reload): it knows no frames yet and has no touch in progress.
        Ok(GuestMessage::Hello { .. }) => with(|s| {
            if let Some(rn) = s.rn.as_mut() {
                rn.input = RnInput::default();
            }
            0
        }),
        Ok(_) => 0,
    }
}

fn rn_requests(requests: Vec<GuestMessage>) -> i32 {
    match with_rn(|rn, inter, cx| rn.apply_requests(inter, cx, requests)) {
        Some((true, out)) => {
            deliver(out);
            1
        }
        Some((false, out)) => deliver(out),
        None => 0,
    }
}

/// An image file the guest wrote (React Native `Image` sources are cached as files): the scratch buffer holds the path,
/// a NUL byte, then the PNG or JPEG bytes (`len` in all). Redraws so the image shows. 1 if it decoded, else 0.
#[no_mangle]
pub extern "C" fn simcore_web_put_image(len: usize) -> i32 {
    let ok = with(|s| {
        let b = &s.buf[..len.min(s.buf.len())];
        let Some(nul) = b.iter().position(|&c| c == 0) else { return false };
        onesim_render::put_image(&String::from_utf8_lossy(&b[..nul]), &b[nul + 1..])
    });
    if ok {
        with(draw);
    }
    i32::from(ok)
}

/// Move the host messages waiting for the guest into the scratch buffer as a JSON array; returns how many.
/// The page posts each one to the guest Worker.
#[no_mangle]
pub extern "C" fn simcore_web_rn_outbox() -> i32 {
    with(|s| {
        let out = s.rn.as_mut().map(|rn| std::mem::take(&mut rn.outbox)).unwrap_or_default();
        s.buf = serde_json::to_vec(&out).unwrap_or_default();
        out.len() as i32
    })
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
    fn display_list_mode() {
        use onesim_render::display::DisplayList;
        let dl = || {
            let b = unsafe { std::slice::from_raw_parts(simcore_web_dl_ptr(), simcore_web_dl_len()) };
            DisplayList::decode(b).expect("decodes")
        };
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let cpu = with(|s| s.fb.clone());
        assert!(!cpu.is_empty());
        assert_eq!(simcore_web_set_output(2), 1);
        assert_eq!(simcore_web_set_output(1), 0);
        assert_eq!(simcore_web_fb_len(), 0, "no pixels in display-list mode");
        let first = dl();
        assert_eq!((first.width, first.height), (1206, 2622));
        assert!(!first.ops.is_empty() && !first.added.is_empty(), "text sprites are sent");
        // A redraw (scale change back and forth) reuses the cached resources.
        simcore_web_set_scale(3.0);
        let again = dl();
        assert!(again.added.is_empty(), "resources are not re-sent: {}", again.added.len());
        // Frames drawn but never read (several guest updates between presents) pass their resources on.
        let _ = dl(); // the consumer has the 3x resources
        simcore_web_set_scale(2.0); // drawn, never read: the 2x sprites are first sent here
        simcore_web_set_scale(2.0); // the same frame again: nothing new by itself
        assert!(!dl().added.is_empty(), "the unread frame's resources are carried over");
        simcore_web_set_scale(3.0);
        let _ = dl();
        // Reset (new consumer): everything used is sent again.
        assert_eq!(simcore_web_set_output(1), 0);
        let reset = dl();
        assert_eq!(reset.added.len(), first.added.len());
        // Back to CPU: byte-identical pixels to before, and the list is gone.
        assert_eq!(simcore_web_set_output(0), 0);
        assert_eq!(simcore_web_dl_len(), 0);
        assert!(with(|s| s.fb == cpu), "CPU output unchanged");
    }

    /// Fixtures for the browser presenter test (`web/test/presenter.mjs`): for a few showcase trees, the same frame as
    /// a display list (`NAME.dl`) and as CPU pixels (`NAME.rgba`), plus `manifest.json`. Only runs when
    /// `SIMCORE_WEB_FIXTURE_DIR` is set (the test script does that).
    #[test]
    fn write_browser_fixtures() {
        let Ok(dir) = std::env::var("SIMCORE_WEB_FIXTURE_DIR") else { return };
        std::fs::create_dir_all(&dir).unwrap();
        const SHOWCASE: [(&str, &str); 2] = [
            ("home", include_str!("../../onesim-render/tests/data/home.json")),
            ("controls", include_str!("../../onesim-render/tests/data/controls.json")),
        ];
        let tabs = r#"[{"title":"Home","systemImage":"house","action":1},{"title":"Counter","action":2},{"title":"Settings","systemImage":"gearshape","badge":"3","action":3}]"#;
        let mut trees: Vec<(String, String)> = SHOWCASE.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
        trees.push(("home-tabbar".into(), format!(r#"{{"kind":"tabview","id":"tabs","selected":0,"tabs":{tabs},"content":{}}}"#, SHOWCASE[0].1)));
        trees.push(("home-compact".into(), SHOWCASE[0].1.into()));
        let mut manifest = vec![];
        for (version, (name, tree)) in trees.iter().enumerate() {
            if name == "home-compact" {
                simcore_web_set_viewport(320.0, 568.0);
                simcore_web_set_scale(2.0);
            }
            let v = version as u64 + 1;
            for out in [0, 1] {
                with(|s| s.version = Some(v - 1));
                assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), v) }, 0, "{name}");
                // Switching resets the builder and redraws: the list carries every resource it uses.
                assert_eq!(simcore_web_set_output(out), 0);
                let (ptr, len) = if out == 1 { (simcore_web_dl_ptr(), simcore_web_dl_len()) } else { (simcore_web_fb_ptr(), simcore_web_fb_len()) };
                let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
                std::fs::write(format!("{dir}/{name}.{}", if out == 1 { "dl" } else { "rgba" }), bytes).unwrap();
            }
            manifest.push(serde_json::json!({"name": name, "width": simcore_web_fb_width(), "height": simcore_web_fb_height()}));
        }
        std::fs::write(format!("{dir}/manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
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

    #[test]
    fn react_native_mode_relays_trees_and_touches() {
        let buf = |t: &str| unsafe { std::slice::from_raw_parts_mut(simcore_web_buf(t.len()), t.len()) }.copy_from_slice(t.as_bytes());
        let outbox = || {
            let n = simcore_web_rn_outbox();
            let v: Vec<serde_json::Value> = with(|s| serde_json::from_slice(&s.buf).unwrap());
            assert_eq!(v.len() as i32, n);
            v
        };
        assert_eq!(simcore_web_rn_init(402.0, 874.0, 1.0, 0, 0), 0);
        let tree = r#"{"type":"tree","version":7,"root":{"kind":"rn","id":"1","component":"RootView","props":{},"children":[
            {"kind":"rn","id":"2","component":"RCTView","props":{"flex":1,"paddingTop":100},"children":[
              {"kind":"rn","id":"3","component":"RCTView","props":{"height":60,"backgroundColor":4278222079},"children":[]}]}]}}"#;
        buf(tree);
        assert_eq!(simcore_web_rn_receive(tree.len()), 1, "a tree draws");
        assert_eq!(simcore_web_fb_width(), 402);
        let first = outbox();
        assert_eq!(first.len(), 1);
        assert_eq!((first[0]["type"].as_str(), first[0]["frames"]["3"][3].as_f64()), (Some("layout"), Some(60.0)));
        assert!(outbox().is_empty(), "drained");
        // A tap on the tagged view is a touch start + end for the guest, not a Swift callback.
        assert_eq!(simcore_web_pointer(2, 200.0, 130.0), 1);
        let touches: Vec<_> = outbox().iter().map(|m| (m["type"].clone(), m["phase"].clone(), m["tag"].clone())).collect();
        assert_eq!(touches, vec![("touch".into(), "start".into(), 3.into()), ("touch".into(), "end".into(), 3.into())]);
        // The same tree version again is stale; a bad message is reported.
        buf(tree);
        assert_eq!(simcore_web_rn_receive(tree.len()), -1);
        buf("{}");
        assert_eq!(simcore_web_rn_receive(2), -1);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("bad guest message")));
        // A restarted guest (hello) gets every frame again with its first tree.
        buf(r#"{"type":"hello","protocol":1}"#);
        assert_eq!(simcore_web_rn_receive(29), 0);
        let again = tree.replace(r#""version":7"#, r#""version":8"#);
        buf(&again);
        assert_eq!(simcore_web_rn_receive(again.len()), 1);
        assert_eq!(outbox()[0]["frames"]["3"][3].as_f64(), Some(60.0), "layout resent after a restart");
        // Environment changes go to the guest.
        assert_eq!(simcore_web_set_dark(1), 0);
        assert_eq!(outbox()[0]["type"], "environment");
    }

    #[test]
    fn changing_ios_redraws_the_loaded_scene() {
        assert_eq!(simcore_web_set_ios(0), 0);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let rendered_ios = || with(|s| s.scene.as_ref().unwrap().ios);
        assert_eq!(rendered_ios(), IosVersion::Ios26);
        let before = with(|s| s.fb.clone());

        assert_eq!(simcore_web_set_ios(1), 0);
        assert_eq!(rendered_ios(), IosVersion::Ios27);
        assert!(with(|s| s.fb != before), "framebuffer should redraw with the new iOS look");
    }

    #[test]
    fn scratch_buffer_submit_and_android_device_metadata() {
        assert_eq!(simcore_web_devices(), 0);
        let devs: serde_json::Value = with(|s| serde_json::from_slice(&s.buf).unwrap());
        let pixel = devs.as_array().unwrap().iter().find(|d| d["name"] == "pixel-10a").unwrap();
        assert_eq!(pixel["android"], true);
        assert!(pixel["safeTop"].as_f64().unwrap() > 50.0);
        assert!(pixel["safeBottom"].as_f64().unwrap() > 20.0);

        let put = |t: &str| {
            let p = simcore_web_buf(t.len());
            unsafe { std::slice::from_raw_parts_mut(p, t.len()) }.copy_from_slice(t.as_bytes());
            t.len()
        };
        let name_len = put("pixel-10a");
        assert_eq!(simcore_web_set_device(name_len), 0);
        let tree = r#"{"kind":"compose","id":"root","component":"Box","props":{},"children":[{"kind":"compose","id":"root/0","component":"Text","props":{"text":"Hello Compose"}}]}"#;
        let len = put(tree);
        assert_eq!(simcore_web_submit_tree(len, 1.0), 0);
        assert_eq!(simcore_web_inspect(), 0);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("Hello Compose")));

        // Vertical scroll on a Compose scroll container updates the framebuffer.
        let scroll_tree = r#"{"kind":"compose","id":"root","component":"Column","props":{"modifier":[{"fillMaxWidth":1.0},{"fillMaxHeight":1.0},{"scroll":true}]},"children":[
            {"kind":"compose","id":"root/0","component":"Box","props":{"modifier":[{"fillMaxWidth":1.0},{"height":600},{"background":{"color":[1,0,0,1]}}]}},
            {"kind":"compose","id":"root/1","component":"Box","props":{"modifier":[{"fillMaxWidth":1.0},{"height":600},{"background":{"color":[0,0,1,1]}}]}}]}"#;
        let slen = put(scroll_tree);
        assert_eq!(simcore_web_submit_tree(slen, 2.0), 0);
        let before = with(|s| s.fb.clone());
        assert_eq!(simcore_web_scroll(200.0, 400.0, 0.0, 200.0), 1);
        assert!(with(|s| s.fb != before), "compose scroll should redraw scrolled pixels");
    }

    #[test]
    fn web_tick_redraws_final_settling_frame() {
        let _ = simcore_web_init(200.0, 200.0, 1.0, 0);
        let t1 = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":40.0,"height":40.0}}]}"#;
        let t2 = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":160.0,"height":40.0}},{"animation":{"curve":"linear","duration":0.2,"token":1}}]}"#;
        assert_eq!(unsafe { simcore_submit_tree(t1.as_ptr(), t1.len(), 1) }, 0);
        assert_eq!(unsafe { simcore_submit_tree(t2.as_ptr(), t2.len(), 2) }, 0);
        // Mid-flight at 100ms: moving, returns 1.
        assert_eq!(simcore_web_tick(100.0), 1);
        let fb_mid = with(|s| s.fb.clone());
        // Final settling tick at +100ms (total 200ms): animation finishes, MUST redraw settled frame and return 1.
        assert_eq!(simcore_web_tick(100.0), 1);
        let fb_end = with(|s| s.fb.clone());
        assert!(fb_end != fb_mid, "final settling tick must redraw framebuffer to settled state");
        // Next tick at rest: nothing moves, returns 0.
        assert_eq!(simcore_web_tick(16.0), 0);
    }
}
