use onesim_protocol::{Appearance, DynamicTypeSize, HostMessage, IosVersion, ScenePhase, Viewport, ViewNode};
#[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
use onesim_protocol::TouchPhase;
use onesim_render::display::DisplayList;
use onesim_render::rn_input::{effect_messages, RnInput};
use onesim_render::{
    build_scene_reusing_layout, device, render_reusing, Animator, Builder, Device, Effect, Fonts,
    Interaction, Scene, SceneParams, SwiftUiLayoutCache, DEFAULT_DEVICE,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::{c_char, c_void, CString};

pub type ActionCb = Option<unsafe extern "C" fn(*mut c_void, u64)>;
pub type TextCb = Option<unsafe extern "C" fn(*mut c_void, u64, *const c_char)>;
pub type EnvCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char, f64, f64)>;
pub type GestureCb = Option<
    unsafe extern "C" fn(
        *mut c_void,
        i32,
        f64,
        *const f64,
        usize,
        *const *const c_char,
        *const f64,
        usize,
    ),
>;
pub type LayoutCb = Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;
pub type WakeCb = Option<unsafe extern "C" fn(*mut c_void)>;

pub(crate) struct State {
    pub(crate) ctx: usize,
    pub(crate) on_action: ActionCb,
    pub(crate) on_env: EnvCb,
    pub(crate) on_text: TextCb,
    pub(crate) on_gesture: GestureCb,
    /// Section M/N/O layout & tick callback (`geometry` / `scroll_position` / `layout` / `tick`).
    pub(crate) on_layout: LayoutCb,
    /// Guards against infinite re-entry when a layout callback submits a new tree.
    pub(crate) in_layout: bool,
    pub(crate) version: Option<u64>,
    pub(crate) tree: Option<ViewNode>,
    pub(crate) layout_cache: SwiftUiLayoutCache,
    pub(crate) scene: Option<Scene>,
    pub(crate) fonts: Option<Fonts>,
    /// Chrome source (crates/onesim-render/src/device.rs); the viewport is a parameter.
    pub(crate) device: &'static Device,
    pub(crate) viewport: (f64, f64),
    pub(crate) scale: f64,
    pub(crate) dark: bool,
    pub(crate) dynamic_type: DynamicTypeSize,
    pub(crate) ios: IosVersion,
    pub(crate) inter: Interaction,
    pub(crate) anim: Animator,
    /// Uninterpolated scene kept while animating so ticks can advance without rebuilding it.
    pub(crate) animation_target: Option<Scene>,
    pub(crate) clock_ms: f64,
    pub(crate) guest_clock_ms: f64,
    pub(crate) guest_due_ms: Option<f64>,
    /// Items the app last put on the pasteboard (section W), until JS takes them (`simcore_web_take_pasteboard`).
    pub(crate) pasteboard_out: Option<Vec<String>>,
    /// SwiftUI scene phase (`simcore_web_set_scene_phase`).
    pub(crate) scene_phase: ScenePhase,
    pub(crate) last_anim_ver: Option<u64>,
    /// React Native: spinners and animated images on the clock, the scene before that pass, and whether one moves.
    pub(crate) live: onesim_render::Live,
    pub(crate) live_base: Option<Scene>,
    pub(crate) live_moving: bool,
    /// Set before a guest callback; a tree submit clears it by updating the scene.
    pub(crate) stale: bool,
    pub(crate) initialized: bool,
    pub(crate) tree_json: String,
    pub(crate) fb: Vec<u8>,
    pub(crate) fb_size: (u32, u32),
    pub(crate) frame_version: u32,
    pub(crate) frame_dirty: bool,
    /// 0: rasterize into `fb` (CPU); 1: lower the scene to an encoded display list in `dl`.
    pub(crate) output: i32,
    /// Persistent so successive display lists only carry new resources; reset whenever the consumer changes.
    pub(crate) builder: Builder,
    pub(crate) dl: Vec<u8>,
    /// The latest display list until JS reads it (`simcore_web_dl_ptr`): a newer frame drawn first carries its
    /// resource changes on, since the builder sends each resource once.
    pub(crate) unread: Option<DisplayList>,
    /// Scratch buffer shared with JS (inputs: label/key text; outputs: inspect JSON, find result).
    pub(crate) buf: Vec<u8>,
    /// React Native mode (`simcore_web_rn_init`): no Swift callbacks; the page relays messages to the guest Worker.
    pub(crate) rn: Option<Rn>,
}

/// React Native mode: input state shared with the native host, and the host messages (docs/protocol.md section H)
/// waiting for the page to post them to the guest (`simcore_web_rn_outbox`).
#[derive(Default)]
pub(crate) struct Rn {
    pub(crate) input: RnInput,
    pub(crate) outbox: Vec<HostMessage>,
}

thread_local! {
    // ponytail: thread-local state; protocol says single-threaded.
    pub(crate) static STATE: RefCell<State> = RefCell::new(State {
        ctx: 0, on_action: None, on_env: None, on_text: None, on_gesture: None, on_layout: None, in_layout: false, version: None, tree: None, layout_cache: SwiftUiLayoutCache::default(), scene: None,
        fonts: None, device: device(DEFAULT_DEVICE).unwrap(), viewport: (402.0, 874.0), scale: 3.0, dark: false, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios27, inter: Interaction::default(),
        anim: Animator::new(), animation_target: None, clock_ms: 0.0, guest_clock_ms: 0.0, guest_due_ms: None, pasteboard_out: None, scene_phase: ScenePhase::Active, last_anim_ver: None, live: onesim_render::Live::new(), live_base: None, live_moving: false,
        stale: false, initialized: false, tree_json: String::new(), fb: Vec::new(), fb_size: (0, 0), frame_version: 0, frame_dirty: false,
        output: 0, builder: Builder::new(), dl: Vec::new(), unread: None, buf: Vec::new(), rn: None,
    });
    pub(crate) static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

#[cfg(all(target_family = "wasm", not(feature = "js-guest")))]
extern "C" {
    /// The Swift program's entry (top-level code or `@main`; clang/Swift name `main(argc, argv)` this on Wasm): registers the app, no event loop.
    pub(crate) fn __main_argc_argv(argc: i32, argv: *const *const c_char) -> i32;
}

#[cfg(all(target_family = "wasm", feature = "js-guest"))]
#[link(wasm_import_module = "onesim_guest")]
extern "C" {
    pub(crate) fn on_init(dark: i32, width: f64, height: f64, safe_top: f64, safe_bottom: f64) -> i32;
    pub(crate) fn on_env(dark: i32, width: f64, height: f64, safe_top: f64, safe_bottom: f64) -> i32;
    pub(crate) fn on_action(id: f64);
    pub(crate) fn on_text(field: f64, ptr: *const u8, len: usize);
}

pub(crate) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub(crate) fn set_err(msg: impl Into<String>) {
    let s = msg.into().replace('\0', " ");
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::new(s).ok());
}

pub(crate) fn update_scene(s: &mut State) {
    let Some(root) = &s.tree else { return };
    let fonts = s.fonts.get_or_insert_with(|| Fonts::with_fallback(true)); // browsers: bundled Inter only
    let mut layout = |inter: &Interaction| {
        let params = SceneParams {
            viewport: s.viewport,
            device: s.device,
            scale: s.scale,
            appearance: if s.dark { Appearance::Dark } else { Appearance::Light },
            dynamic_type: s.dynamic_type,
            ios: s.ios,
            scroll: &inter.scroll,
            sheets: &inter.sheets,
            focus: inter.focus.as_deref(),
            overscroll: inter.live(),
            status_bar: true,
            chrome: inter.chrome(),
        };
        build_scene_reusing_layout(root, &params, fonts, &mut s.layout_cache)
    };
    let mut scene = layout(&s.inter);
    if s.rn.is_none() && s.version.is_some_and(|version| s.inter.sync_text_focus(&scene, version)) {
        scene = layout(&s.inter);
    }
    // React Native needs current layout even when multiple updates share a presentation.
    if let Some(rn) = s.rn.as_mut() {
        rn.outbox.extend(rn.input.layout(&scene));
    }
    // SwiftUI animations, and React Native LayoutAnimation (carried by the tree of the commit it configured).
    if s.version != s.last_anim_ver {
        s.anim.update(&scene);
        s.last_anim_ver = s.version;
    }
    if s.anim.is_animating() || s.animation_target.is_some() {
        s.animation_target = Some(scene);
    } else {
        s.scene = Some(scene);
    }
    s.stale = false;
    update_animation_frame(s);
}

pub(crate) fn update_animation_frame(s: &mut State) {
    if let Some(target) = &s.animation_target {
        s.scene = Some(s.anim.render_scene(target));
        if !s.anim.is_animating() {
            s.animation_target = None;
        }
    }
    if s.rn.is_some() {
        if let Some(scene) = s.scene.as_mut() {
            s.live_base = Some(scene.clone());
            s.live_moving = s.live.apply(scene, s.clock_ms);
        }
    }
    invalidate_frame(s);
}

/// React Native: move spinners and animated images on to the current clock (no new layout). True if redrawn.
pub(crate) fn update_live_frame(s: &mut State) -> bool {
    let Some(base) = s.live_base.as_ref().filter(|_| s.live_moving) else { return false };
    let mut scene = base.clone();
    s.live_moving = s.live.apply(&mut scene, s.clock_ms);
    s.scene = Some(scene);
    invalidate_frame(s);
    true
}

pub(crate) fn invalidate_frame(s: &mut State) {
    s.frame_version = s.frame_version.wrapping_add(1);
    s.frame_dirty = true;
}

pub(crate) fn render_frame(s: &mut State) {
    if !s.frame_dirty {
        return;
    }
    let (Some(scene), Some(fonts)) = (&s.scene, &mut s.fonts) else { return };
    s.frame_dirty = false;
    if s.output == 1 {
        let mut list = s.builder.build(scene, fonts, s.scale);
        if let Some(old) = s.unread.take() {
            // The executor frees `dropped`, then adds `added`: keep what the old list added unless this one drops it.
            let added_ids: HashSet<u64> = old.added.iter().map(|r| r.id).collect();
            let dropped_ids: HashSet<u64> = list.dropped.iter().copied().collect();
            let mut added: Vec<_> = old.added.into_iter().filter(|r| !dropped_ids.contains(&r.id)).collect();
            list.dropped.retain(|id| !added_ids.contains(id));
            list.dropped.extend(old.dropped);
            added.append(&mut list.added);
            list.added = added;
        }
        s.fb_size = (list.width, list.height);
        list.encode_into(&mut s.dl);
        s.unread = Some(list);
    } else {
        let pm = render_reusing(scene, fonts, s.scale, std::mem::take(&mut s.fb));
        s.fb_size = (pm.width(), pm.height());
        s.fb = pm.take(); // opaque full-screen scene: premultiplied == straight RGBA
    }
}

pub(crate) fn with_frame<T>(f: impl FnOnce(&mut State) -> T) -> T {
    with(|s| {
        render_frame(s);
        f(s)
    })
}

pub(crate) fn submit(json: &[u8], version: u64) -> Result<(), String> {
    let text = std::str::from_utf8(json).map_err(|e| format!("tree is not UTF-8: {e}"))?;
    let root: ViewNode = serde_json::from_str(text).map_err(|e| format!("tree is not valid ViewNode JSON: {e}"))?;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.version.is_some_and(|v| version <= v) {
            return Err("tree version must increase monotonically".to_string());
        }
        s.version = Some(version);
        s.inter.version = version;
        s.layout_cache.clear();
        s.tree = Some(root);
        s.tree_json.clear();
        update_scene(&mut s);
        Ok(())
    })?;
    send_layout();
    Ok(())
}

/// Supply size-dependent drawing and effect closures after guest and host layout changes.
pub(crate) fn send_layout() {
    let pending = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.in_layout {
            return None;
        }
        let cb = s.on_layout?;
        let frames = s.scene.as_ref()?.pending_shape_frames();
        if frames.is_empty() {
            return None;
        }
        s.in_layout = true;
        Some((cb, s.ctx, frames))
    });
    if let Some((cb, ctx, frames)) = pending {
        let line = CString::new(serde_json::json!({"type": "layout", "frames": frames}).to_string()).unwrap_or_default();
        unsafe { cb(ctx as *mut c_void, line.as_ptr()) };
        STATE.with(|s| s.borrow_mut().in_layout = false);
    }
}

/// Replace the guest's pasteboard (`{"type":"pasteboard","items"}` through the layout callback); the guest
/// re-submits its tree when that changes what it shows. 0 ok, 1 no SwiftUI guest.
pub(crate) fn send_pasteboard(items: &[String]) -> i32 {
    let Some((cb, ctx)) = STATE.with(|s| { let s = s.borrow(); s.on_layout.map(|cb| (cb, s.ctx)) }) else { return 1 };
    let line = CString::new(serde_json::json!({"type": "pasteboard", "items": items}).to_string()).unwrap_or_default();
    unsafe { cb(ctx as *mut c_void, line.as_ptr()) };
    0
}

/// Push the environment to the guest (as the native host does); the guest re-submits its tree.
pub(crate) fn push_environment() -> i32 {
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
        with(update_scene); // the viewport is the host's: lay out now, the guest re-renders after
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
                update_scene(s);
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

/// Apply an interaction effect: guest callbacks for actions/text, a redraw otherwise.
/// Returns 1 if anything happened.
pub(crate) fn dispatch(e: Effect) -> i32 {
    #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
    let (ctx, on_action, on_text) = with(|s| (s.ctx, s.on_action, s.on_text));
    if !matches!(e, Effect::None | Effect::Redraw) && with(|s| s.rn.is_some()) {
        return with(|s| {
            if let Some(rn) = s.rn.as_mut() {
                rn.outbox.extend(effect_messages(e));
            }
            update_scene(s); // focus/press state may have changed; the guest's answer arrives as a new tree
            1
        });
    }
    match e {
        Effect::None => return 0,
        Effect::Redraw => with(update_scene),
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
    // The callback re-submits only if the guest state changed; focus/shift may have.
    with(|s| {
        if s.stale {
            update_scene(s)
        }
    });
    send_layout();
    1
}

/// Synchronize the SwiftUI guest's clock with `clock_ms` before an input starts new timers.
pub(crate) fn sync_guest_clock() {
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

pub(crate) fn ensure_tree_json(s: &mut State) {
    if s.tree_json.is_empty() {
        if let Some(root) = &s.tree {
            s.tree_json = serde_json::to_string(root).unwrap_or_default();
        }
    }
}
