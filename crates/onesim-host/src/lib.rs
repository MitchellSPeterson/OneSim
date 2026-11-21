//! Session manager and external API for OneSim (docs/protocol.md section A).
//!
//! Test hook: `create_session` skips the Swift build and spawns `app_path` directly when
//! `"prebuilt": true` is passed or `app_path` is an executable file that is not `*.swift`.
//! `onesim-fake-guest` (a bin of this crate) is such an executable.

mod events;
mod process;
mod renderer;
mod rpc;
mod session;
mod settle;
mod v1;
mod ws;

use onesim_protocol::*;
use onesim_render::rn_input::{Out, RnCtx};
use onesim_render::{build_scene_reusing_layout, Fonts, Scene, SceneParams};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use events::{EventLog, HostEvent};
pub use process::onesim_home;
pub use rpc::Server;
pub use session::{device, Device, State, DEVICES};
pub use v1::COMMANDS;
pub use ws::serve_ws;
use session::Session;
use settle::STARTUP_MAX;

pub(crate) fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

fn check_device_platform(dev: &Device, platform: &str) -> Result<(), ApiError> {
    if dev.android != (platform == "android") {
        return Err(err(ErrorCode::Unsupported, format!("device {:?} does not support platform {platform:?}", dev.name)));
    }
    Ok(())
}

pub struct Host {
    pub(crate) sessions: HashMap<String, Session>,
    next_id: u64,
    fonts: Arc<Mutex<Option<Fonts>>>,
    renderer: renderer::RendererSlot,
    pub(crate) greeted: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl Host {
    pub fn new() -> Self {
        Self::with_shared_fonts(Arc::new(Mutex::new(None)))
    }

    pub(crate) fn with_shared_fonts(fonts: Arc<Mutex<Option<Fonts>>>) -> Self {
        Self { sessions: HashMap::new(), next_id: 1, fonts, renderer: renderer::RendererSlot::new(), greeted: false }
    }

    /// Choose the screenshot backend (default: `SIMCORE_RENDERER`, where `auto` means CPU headless).
    /// Here `Auto` tries the GPU and falls back to CPU, recording why in [`Host::renderer_diagnostics`].
    pub fn set_renderer(&mut self, pref: onesim_render::BackendPref) {
        self.renderer.set(pref);
    }

    /// Active backend, adapter, limitations, fallback reason and last-frame timings. Errors on an invalid
    /// `SIMCORE_RENDERER` or an unavailable explicit `gpu`.
    pub fn renderer_diagnostics(&mut self) -> Result<onesim_render::Diagnostics, ApiError> {
        self.renderer.diagnostics().map_err(|e| err(ErrorCode::RuntimeError, e))
    }

    /// React Native live mode for the calls that follow (`Session::settle`); restore `true` afterwards.
    pub fn set_settle(&mut self, id: &str, on: bool) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.settle = on;
        }
    }

    pub(crate) fn get(&mut self, id: &str) -> Result<&mut Session, ApiError> {
        self.sessions.get_mut(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))
    }

    pub fn create_session(&mut self, p: CreateSessionParams) -> Result<String, ApiError> {
        let id = format!("s{}", self.next_id);
        self.next_id += 1;
        self.create_session_as(id, p)
    }

    /// `create_session` with a caller-chosen id (the v2 server owns the id space).
    pub(crate) fn create_session_as(&mut self, id: String, mut p: CreateSessionParams) -> Result<String, ApiError> {
        if p.framework == "expo" {
            p.framework = "react-native".into();
        }
        if matches!(p.framework.as_str(), "jetpack-compose" | "android") {
            p.framework = "compose".into();
        }
        if !matches!(p.framework.as_str(), "swiftui" | "react-native" | "compose") {
            return Err(err(
                ErrorCode::Unsupported,
                format!("framework {:?} (\"swiftui\", \"react-native\", \"compose\")", p.framework),
            ));
        }
        // Compose runs on Android only; `platform` defaults to "ios" on the wire, so it is implied here.
        if p.framework == "compose" {
            p.platform = "android".into();
        }
        match (p.framework.as_str(), p.platform.as_str()) {
            (_, "ios") | ("compose" | "react-native", "android") => {}
            _ => return Err(err(ErrorCode::Unsupported, format!("platform {:?} for {}", p.platform, p.framework))),
        }
        if p.platform == "android" {
            p.device.get_or_insert_with(|| onesim_render::DEFAULT_ANDROID_DEVICE.into());
        }
        if matches!(p.framework.as_str(), "swiftui" | "compose") && !std::path::Path::new(&p.app_path).exists() {
            return Err(err(ErrorCode::BadRequest, format!("app_path {:?} does not exist", p.app_path)));
        }
        if let Some(d) = &p.device {
            if let Some(dev) = device(d) {
                check_device_platform(dev, &p.platform)?;
            } else {
                let known: Vec<&str> = DEVICES.iter().map(|d| d.name).collect();
                return Err(err(ErrorCode::Unsupported, format!("device {d:?} (known: {})", known.join(", "))));
            }
        }
        let log = EventLog::new(id.clone());
        self.sessions.insert(id.clone(), Session::new(id.clone(), p, log));
        Ok(id)
    }

    /// Blocks until the first tree arrives (or the build/launch fails).
    pub fn start_session(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        log::info!("starting session {id}");
        s.start()?;
        if self.get(id)?.is_rn() {
            // First layout pass: FlatList and friends render the rest once they know their frames.
            let _ = self.scene_within(id, STARTUP_MAX);
        }
        Ok(())
    }

    /// The session's event log (v2 API).
    pub fn events(&mut self, id: &str) -> Result<Arc<EventLog>, ApiError> {
        Ok(self.get(id)?.log())
    }

    pub fn stop_session(&mut self, id: &str) -> Result<(), ApiError> {
        self.get(id)?.stop();
        Ok(())
    }

    pub fn state(&mut self, id: &str) -> Result<State, ApiError> {
        Ok(self.get(id)?.state())
    }

    pub fn status(&mut self, id: &str) -> Result<Value, ApiError> {
        let s = self.get(id)?;
        Ok(json!({
            "session_id": s.id, "state": s.state().as_str(), "framework": s.params.framework,
            "platform": s.params.platform, "app_path": s.params.app_path, "viewport": s.viewport,
            "device": s.device.name, "os_version": s.os_version(), "safe_area": {"top": s.device.safe_top, "bottom": s.device.safe_bottom},
            "focus": s.inter.focus, "island": s.device.island.map(|(x, y, w, h)| json!({"x": x, "y": y, "width": w, "height": h})),
            "scale": s.scale, "appearance": s.appearance, "tree_version": s.tree_version(),
        }))
    }

    pub fn set_viewport(&mut self, id: &str, width: f64, height: f64) -> Result<(), ApiError> {
        if !(width > 0.0 && height > 0.0) {
            return Err(err(ErrorCode::BadRequest, "width and height must be positive"));
        }
        let s = self.get(id)?;
        s.viewport = Viewport { width, height };
        s.invalidate_scene();
        Self::push_env(s)
    }

    pub fn set_appearance(&mut self, id: &str, mode: Appearance) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.appearance = mode;
        s.invalidate_scene();
        Self::push_env(s)
    }

    /// SwiftUI: replace the app's pasteboard (protocol section U). It also seeds relaunches. The guest re-renders
    /// only when that changes what it shows, so this does not wait for a tree.
    pub fn set_pasteboard(&mut self, id: &str, items: Vec<String>) -> Result<(), ApiError> {
        let s = self.get(id)?;
        if s.params.framework != "swiftui" {
            return Err(err(ErrorCode::Unsupported, "the pasteboard is SwiftUI-only"));
        }
        s.params.pasteboard = items.first().cloned();
        if s.state() == State::Running {
            s.send(&HostMessage::Pasteboard { items })?;
        }
        Ok(())
    }

    fn push_env(s: &mut Session) -> Result<(), ApiError> {
        if s.state() == State::Running {
            let m = HostMessage::Environment {
                appearance: s.appearance,
                viewport: s.viewport,
                dynamic_type_size: (s.dynamic_type != DynamicTypeSize::Large).then_some(s.dynamic_type),
            };
            s.send_and_wait_tree(&m)?;
        }
        Ok(())
    }

    /// The tree a React Native input is interpreted against (`RnCtx::root`).
    fn rn_root(s: &Session) -> Result<Arc<ViewNode>, ApiError> {
        s.tree().map(|t| t.1).ok_or_else(|| err(ErrorCode::InvalidState, "no tree"))
    }

    pub(crate) fn build(&mut self, id: &str) -> Result<(Arc<Scene>, u64, f64), ApiError> {
        let s = self.sessions.get_mut(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))?;
        let gen = s.generation();
        if let Some((scene, version)) = s.cached_scene(gen) {
            return Ok((scene, version, s.scale));
        }
        let (version, root) = s
            .tree()
            .ok_or_else(|| err(ErrorCode::InvalidState, format!("session {id} is {} and has no tree", s.state().as_str())))?;
        let mut fonts_guard = self.fonts.lock().unwrap_or_else(|e| e.into_inner());
        let fonts = fonts_guard.get_or_insert_with(Fonts::new);
        let mut layout = |inter: &onesim_render::Interaction, cache: &mut onesim_render::SwiftUiLayoutCache| {
            let params = SceneParams {
                viewport: (s.viewport.width, s.viewport.height),
                device: &s.device,
                scale: s.scale,
                appearance: s.appearance,
                dynamic_type: s.dynamic_type,
                ios: s.ios,
                scroll: &inter.scroll,
                sheets: &inter.sheets,
                focus: inter.focus.as_deref(),
                overscroll: inter.live(),
                status_bar: true,
                chrome: inter.chrome(),
            };
            build_scene_reusing_layout(&root, &params, fonts, cache)
        };
        if s.layout_cache_gen != gen {
            s.layout_cache.clear();
            s.layout_cache_gen = gen;
        }
        let mut scene = layout(&s.inter, &mut s.layout_cache);
        if s.is_swiftui() {
            if s.inter.sync_text_focus(&scene, version) {
                scene = layout(&s.inter, &mut s.layout_cache);
            }
            s.sync_schedule();
            if gen != s.last_anim_gen {
                s.anim.update(&scene);
                s.last_anim_gen = gen;
            }
            if s.anim.is_animating() {
                scene = s.anim.render_scene(&scene);
            }
        }
        drop(fonts_guard);
        for name in &scene.unsupported_symbols {
            s.report_once("host", format!("unsupported_symbol: no system symbol {name:?} on this host; drew a placeholder"));
        }
        for note in &scene.notes {
            s.report_once("host", note.clone());
        }
        let scene = Arc::new(scene);
        s.cache_scene(gen, version, scene.clone());
        Ok((scene, version, s.scale))
    }

    /// Keep the SwiftUI guest's monotonic clock aligned with host `clock_ms` before an input
    /// starts new timers, animators, or completion callbacks.
    fn sync_guest_clock(&mut self, id: &str) -> Result<(), ApiError> {
        let needs_sync = {
            let s = self.get(id)?;
            s.is_swiftui() && s.state() == State::Running && (s.clock_ms - s.guest_clock_ms).abs() > 1e-6
        };
        if needs_sync {
            let tree_arrived = self.get(id)?.send_tick_and_wait()?;
            if tree_arrived {
                let _ = self.scene(id)?;
            }
        }
        Ok(())
    }

    /// Returns `(hit node id, action dispatched)`. `click` = down + up. A hit on a text field
    /// focuses it (action = false); any other click unfocuses. `scroll` hits the scroller and
    /// never reaches the guest. The state machine is `onesim_render::Interaction`.
    pub fn pointer(&mut self, id: &str, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        let (scene, version, _) = self.scene(id)?;
        if self.get(id)?.is_rn() {
            if ev.kind == PointerKind::Drag && scene.scroll_target(ev.x, ev.y).is_some_and(|i| scene.nodes[i].tag.is_some()) {
                return self.rn_whole_drag(id, &scene, ev);
            }
            return self.rn_pointer(id, &scene, version, ev);
        }
        let s = self.get(id)?;
        s.inter.version = version;
        let effect = s.inter.pointer(&scene, ev);
        s.invalidate_scene();
        let out = (s.inter.hit.clone(), s.inter.acted);
        s.send_effect(effect)?;
        Ok(out)
    }

    /// A whole `drag` on a React Native scroll view, run as live phases (each settles first): JS gets the touch and
    /// a move inside the slop, so a PanResponder around the list can claim it, before the scroll view takes it at the
    /// slop. Lifted at rest (no fling); the content lags the finger by the slop, as on iOS.
    fn rn_whole_drag(&mut self, id: &str, scene: &Scene, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        use onesim_render::DragPhase;
        let hit = scene.hit_test(ev.x, ev.y).map(|i| scene.nodes[i].id.clone());
        let (x, y, len) = (ev.x, ev.y, ev.dx.hypot(ev.dy));
        let mut t = self.get(id)?.clock_ms;
        self.drag(id, DragPhase::Begin, x, y, t)?;
        // Inside the slop, just past it, then on to the end in 16 ms steps.
        let mut stops = vec![5.0, 11.0];
        stops.extend((1..=8).map(|i| len * f64::from(i) / 8.0));
        for d in stops.into_iter().filter(|d| *d <= len) {
            t += 16.0;
            let f = d / len;
            self.drag(id, DragPhase::Move, x + ev.dx * f, y + ev.dy * f, t)?;
        }
        self.drag(id, DragPhase::End, x + ev.dx, y + ev.dy, t + 1000.0)?;
        let _ = self.scene(id)?;
        Ok((hit, false))
    }

    /// React Native pointer input (`onesim_render::rn_input`), then settle and push the new layout.
    fn rn_pointer(&mut self, id: &str, scene: &Scene, version: u64, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        let s = self.get(id)?;
        s.inter.version = version;
        let root = Self::rn_root(s)?;
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let (hit, acted, out) = s.rn.pointer(&mut s.inter, &cx, ev);
        s.invalidate_scene();
        s.send_out(out)?;
        let _ = self.scene(id);
        Ok((hit, acted))
    }

    /// Live viewers (native window): lay out the current tree and hand the scene, the shared fonts and the tree
    /// version to `f`. Pair React Native sessions with `set_settle(id, false)` so this never waits for the guest.
    pub fn with_frame<R>(&mut self, id: &str, f: impl FnOnce(&Scene, &mut Fonts, u64) -> R) -> Result<R, ApiError> {
        let (scene, version, _) = self.scene(id)?;
        let mut fonts_guard = self.fonts.lock().unwrap_or_else(|e| e.into_inner());
        let fonts = fonts_guard.get_or_insert_with(Fonts::new);
        Ok(f(&scene, fonts, version))
    }

    /// Whether the session has active SwiftUI animations, a scheduled guest tick, or a held gesture.
    pub fn is_animating(&mut self, id: &str) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        s.sync_schedule();
        s.anim.is_animating() || s.guest_due_ms.is_some() || s.inter.holding()
    }

    /// Advance scroll momentum, bounces, SwiftUI animations, and scheduled guest ticks by `dt` seconds.
    /// Returns whether anything is still moving (call again next frame); false means the scene is at rest.
    /// A finger resting on a SwiftUI gesture view is reported again each tick (long presses fire while held), and keeps
    /// the scene live.
    pub fn tick(&mut self, id: &str, dt: f64) -> Result<bool, ApiError> {
        let (scene, ..) = self.scene(id)?;
        let s = self.get(id)?;
        let dt = dt.max(0.0);
        s.clock_ms += dt * 1000.0;
        s.sync_schedule();
        let mut guest_ticked = false;
        if s.is_swiftui() && s.state() == State::Running && s.guest_due_ms.is_some_and(|due| s.clock_ms + 1e-6 >= due) {
            s.guest_due_ms = None;
            guest_ticked = s.send_tick_and_wait()?;
        }
        let was_animating = s.anim.is_animating();
        let anim_moving = s.anim.tick(dt);
        let scroll_moving = s.inter.tick(&scene, dt);
        if s.is_rn() {
            let out = s.rn.scrolled(&s.inter, &scene);
            s.send_out(out)?;
        }
        let holding = s.inter.holding();
        if holding {
            let held = s.inter.hold(dt);
            s.send_effect(held)?;
        }
        if was_animating || anim_moving || scroll_moving || holding || guest_ticked {
            s.invalidate_scene();
        }
        if guest_ticked {
            // Ingest any new tree the guest submitted on tick into `Animator`.
            let _ = self.scene(id)?;
        }
        let s = self.get(id)?;
        s.sync_schedule();
        Ok(anim_moving || scroll_moving || holding || s.anim.is_animating() || s.guest_due_ms.is_some())
    }

    /// Advance the session's animation clock deterministically by `ms` milliseconds in frame steps
    /// (headless CLI `--wait`). Ensures any preceding tree is ingested before stepping.
    pub fn wait(&mut self, id: &str, ms: f64) -> Result<(), ApiError> {
        let _ = self.scene(id)?;
        let mut left = ms.max(0.0);
        const STEP_MS: f64 = 16.0;
        while left > 1e-6 {
            let s = self.get(id)?;
            s.sync_schedule();
            // Step to the next guest due time or 16ms frame, whichever is smaller.
            let mut step = left.min(STEP_MS);
            if let Some(due) = s.guest_due_ms {
                let until = (due - s.clock_ms).max(0.0);
                if until > 1e-6 && until < step {
                    step = until;
                }
            }
            left -= step;
            let active = self.tick(id, step / 1000.0)?;
            if !active && left > 0.0 {
                // Still advance the clock if nothing is scheduled, in case a later step checks time.
                self.get(id)?.clock_ms += left;
                break;
            }
        }
        let _ = self.scene(id)?;
        Ok(())
    }

    /// One phase of a live finger drag (`t_ms` is the event time): the content follows the finger and carries on
    /// with its release velocity (see [`Host::tick`]); sheets follow too. Returns whether the scene changed.
    /// React Native receives each phase on the same Fabric target and supplies its own gesture behavior.
    pub fn drag(&mut self, id: &str, phase: onesim_render::DragPhase, x: f64, y: f64, t_ms: f64) -> Result<bool, ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        s.inter.version = version;
        // React Native: touches go to the touched Fabric view, scrolls to the host; later trees arrive through
        // render.updated, so the live caller never waits for a commit.
        if s.is_rn() {
            let root = Self::rn_root(s)?;
            let cx = RnCtx { scene: &scene, root: &root, keyboard: s.keyboard_height() };
            let out = s.rn.drag(&mut s.inter, &cx, phase, x, y, t_ms);
            let changed = out.iter().any(|o| matches!(o, Out::Effect(e) if *e != onesim_render::Effect::None));
            s.invalidate_scene();
            s.send_out(out)?;
            return Ok(changed);
        }
        let effect = s.inter.drag(&scene, phase, x, y, t_ms);
        let changed = effect != onesim_render::Effect::None;
        if changed {
            s.invalidate_scene();
        }
        s.send_effect(effect)?;
        Ok(changed)
    }

    /// Keyboard input for the focused text field. The host computes the new text and sends it
    /// to the guest as `text_input`.
    pub fn keyboard(&mut self, id: &str, ev: KeyboardEvent) -> Result<(), ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        if let KeyboardEvent::Key { key } = &ev {
            if !matches!(key.as_str(), "backspace" | "enter") {
                return Err(err(ErrorCode::Unsupported, format!("unknown key {key:?} (backspace, enter)")));
            }
        }
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        s.inter.version = version;
        if s.is_rn() {
            let root = Self::rn_root(s)?;
            let cx = RnCtx { scene: &scene, root: &root, keyboard: s.keyboard_height() };
            let out = s.rn.keyboard(&mut s.inter, &cx, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
            s.invalidate_scene();
            s.send_out(out)?;
            self.scene(id)?;
            return Ok(());
        }
        let effect = s.inter.keyboard(&scene, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
        s.invalidate_scene();
        s.send_effect(effect)
    }

    /// PNG bytes plus pixel size.
    pub fn screenshot(&mut self, id: &str) -> Result<(u32, u32, Vec<u8>), ApiError> {
        self.screenshot_sized(id, None, None, None).map(|(w, h, png, _)| (w, h, png))
    }

    /// Screenshot at a different resolution: `scale` = device pixels per point, or a target `width` /
    /// `height` in pixels (aspect kept; both = the largest size that fits). Also returns the tree version.
    pub fn screenshot_sized(
        &mut self,
        id: &str,
        scale: Option<f64>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<(u32, u32, Vec<u8>, u64), ApiError> {
        let (scene, version, dev_scale) = self.scene(id)?;
        let fit = |px: u32, pt: f64| f64::from(px) / pt;
        let scale = match (width, height) {
            (Some(w), Some(h)) => fit(w, scene.size.0).min(fit(h, scene.size.1)),
            (Some(w), None) => fit(w, scene.size.0),
            (None, Some(h)) => fit(h, scene.size.1),
            (None, None) => scale.unwrap_or(dev_scale),
        };
        let px = |v: f64| (v * scale).round().max(1.0) as u32;
        if !scale.is_finite() || scale <= 0.0 || px(scene.size.0).max(px(scene.size.1)) > 8192 {
            return Err(err(ErrorCode::BadRequest, "screenshot size must be between 1 and 8192 pixels"));
        }
        let mut fonts_guard = self.fonts.lock().unwrap_or_else(|e| e.into_inner());
        let fonts = fonts_guard.get_or_insert_with(Fonts::new);
        let png = self
            .renderer
            .capture(&scene, fonts, scale)
            .and_then(|f| f.encode_png())
            .map_err(|e| err(ErrorCode::RuntimeError, e))?;
        Ok((px(scene.size.0), px(scene.size.1), png, version))
    }

    /// Switch the device profile: viewport, scale and chrome follow; the guest gets the new dimensions.
    pub fn set_device(&mut self, id: &str, name: &str) -> Result<(), ApiError> {
        let dev = *device(name).ok_or_else(|| err(ErrorCode::Unsupported, format!("device {name:?}")))?;
        let s = self.get(id)?;
        check_device_platform(&dev, &s.params.platform)?;
        s.device = dev;
        s.viewport = dev.viewport;
        s.scale = dev.scale;
        s.params.device = Some(dev.name.to_string());
        s.invalidate_scene();
        Self::push_env(s)
    }

    /// Hide the software keyboard by blurring the focused field.
    pub fn hide_keyboard(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.require_running()?;
        if s.is_swiftui() {
            self.sync_guest_clock(id)?;
            let (scene, version, _) = self.scene(id)?;
            let s = self.get(id)?;
            s.inter.version = version;
            let effect = s.inter.blur_text(&scene);
            s.invalidate_scene();
            return s.send_effect(effect);
        }
        let before = s.inter.focus.take();
        s.invalidate_scene();
        if s.is_rn() {
            s.sync_focus(before)?;
            self.scene(id)?;
        }
        Ok(())
    }

    /// React Native native-module inventory reported by the guest (empty until it has booted).
    pub fn modules(&mut self, id: &str) -> Result<Value, ApiError> {
        let s = self.get(id)?;
        if !s.is_rn() {
            return Err(err(ErrorCode::Unsupported, "native modules exist only in react-native sessions"));
        }
        Ok(s.modules().unwrap_or_else(|| json!([])))
    }

    pub fn simulate(&mut self, id: &str, module: String, state: Value) -> Result<(), ApiError> {
        let s = self.get(id)?;
        if !s.is_rn() {
            return Err(err(ErrorCode::Unsupported, "native modules exist only in react-native sessions"));
        }
        s.require_running()?;
        s.send(&HostMessage::Simulate { module, state })
    }

    pub fn inspect(&mut self, id: &str) -> Result<(u64, UiNode), ApiError> {
        let (scene, version, _) = self.scene(id)?;
        Ok((version, scene.ui_tree()))
    }

    pub fn errors(&mut self, id: &str) -> Result<Vec<SessionError>, ApiError> {
        Ok(self.get(id)?.errors())
    }

    pub fn shutdown(&mut self) {
        self.sessions.values_mut().for_each(Session::stop);
    }
}
