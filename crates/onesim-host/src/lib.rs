//! Session manager and external API for OneSim (docs/protocol.md section A).
//!
//! Test hook: `create_session` skips the Swift build and spawns `app_path` directly when
//! `"prebuilt": true` is passed or `app_path` is an executable file that is not `*.swift`.
//! `onesim-fake-guest` (a bin of this crate) is such an executable.

mod events;
mod renderer;
mod rpc;
mod session;
mod ws;

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use onesim_protocol::*;
use onesim_render::rn_input::RnCtx;
use onesim_render::{build_scene, Fonts, Scene, SceneParams};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

pub use events::EventLog;
pub use rpc::Server;
pub use session::{device, onesim_home, Device, State, DEVICES};
pub use ws::serve_ws;
use session::Session;

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

fn check_device_platform(dev: &Device, platform: &str) -> Result<(), ApiError> {
    if dev.android != (platform == "android") {
        return Err(err(ErrorCode::Unsupported, format!("device {:?} does not support platform {platform:?}", dev.name)));
    }
    Ok(())
}

/// React Native: how long the guest must be quiet before a snapshot, and the most a snapshot waits.
const QUIET: Duration = Duration::from_millis(150);
const SETTLE_MAX: Duration = Duration::from_millis(2500);
/// React Native: when trees keep arriving but the layout no longer changes (an endless animation, e.g. a
/// rotating logo), a snapshot stops waiting after this long; startup waits `STARTUP_MAX` (splash, first layout).
const ANIM_MAX: Duration = Duration::from_millis(500);
const STARTUP_MAX: Duration = Duration::from_millis(2500);

pub struct Host {
    sessions: HashMap<String, Session>,
    next_id: u64,
    fonts: Option<Fonts>,
    renderer: renderer::RendererSlot,
    greeted: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

pub const COMMANDS: &[&str] = &[
    "hello",
    "get_capabilities",
    "create_session",
    "start_session",
    "stop_session",
    "reload_session",
    "get_session_status",
    "set_viewport",
    "set_appearance",
    "send_pointer_event",
    "send_keyboard_event",
    "capture_screenshot",
    "inspect_ui_tree",
    "get_errors",
    "shutdown",
];

impl Host {
    pub fn new() -> Self {
        Self { sessions: HashMap::new(), next_id: 1, fonts: None, renderer: renderer::RendererSlot::new(), greeted: false }
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
    pub fn events(&mut self, id: &str) -> Result<std::sync::Arc<EventLog>, ApiError> {
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
            "scale": s.scale, "appearance": s.appearance, "tree_version": s.tree().map(|t| t.0),
        }))
    }

    pub fn set_viewport(&mut self, id: &str, width: f64, height: f64) -> Result<(), ApiError> {
        if !(width > 0.0 && height > 0.0) {
            return Err(err(ErrorCode::BadRequest, "width and height must be positive"));
        }
        let s = self.get(id)?;
        s.viewport = Viewport { width, height };
        Self::push_env(s)
    }

    pub fn set_appearance(&mut self, id: &str, mode: Appearance) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.appearance = mode;
        Self::push_env(s)
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

    /// Scene of the current tree. React Native sessions first settle: the guest's trees, layout
    /// frames, focus requests and `scroll_to` replies are exchanged until nothing changes.
    fn scene(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        self.scene_within(id, ANIM_MAX)
    }

    /// `scene`, where an animation that keeps trees coming (layout unchanged) is waited on for `anim` at most.
    fn scene_within(&mut self, id: &str, anim: Duration) -> Result<(Scene, u64, f64), ApiError> {
        // A paused guest cannot answer: serve the last frame as is.
        if !self.get(id)?.is_rn() || self.get(id)?.state() == State::Paused {
            return self.build(id);
        }
        let settle = self.get(id)?.settle;
        let (start, deadline) = (Instant::now(), Instant::now() + SETTLE_MAX.max(anim));
        loop {
            if settle {
                self.get(id)?.wait_quiet(QUIET, anim);
            }
            let generation = self.get(id)?.generation();
            let out = self.build(id)?;
            let acted = self.rn_apply_requests(id, &out.0);
            let s = self.get(id)?;
            let sent = s.sync_layout(&out.0);
            let animating = !acted && !sent && start.elapsed() >= anim;
            let done = !acted && !sent && s.generation() == generation;
            // Live: no waiting, but a host-side change (`scroll_to`, focus) still needs one rebuild,
            // since the guest may never send a tree for it.
            let live_done = !settle && !acted;
            if done || animating || live_done || Instant::now() > deadline {
                return Ok(out);
            }
        }
    }

    /// Apply the guest's `focus_request` / `scroll_to` messages and drop focus on a vanished field.
    /// Returns whether the interaction state changed (the scene must be rebuilt).
    fn rn_apply_requests(&mut self, id: &str, scene: &Scene) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        let Some((_, root)) = s.tree() else { return false };
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let requests = s.take_requests();
        let (changed, out) = s.rn.apply_requests(&mut s.inter, &cx, requests);
        let _ = s.send_out(out);
        changed
    }

    /// The tree a React Native input is interpreted against (`RnCtx::root`).
    fn rn_root(s: &Session) -> Result<ViewNode, ApiError> {
        s.tree().map(|t| t.1).ok_or_else(|| err(ErrorCode::InvalidState, "no tree"))
    }

    fn build(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        let s = self.sessions.get(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))?;
        let (version, root) = s
            .tree()
            .ok_or_else(|| err(ErrorCode::InvalidState, format!("session {id} is {} and has no tree", s.state().as_str())))?;
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        let params = SceneParams {
            viewport: (s.viewport.width, s.viewport.height),
            device: &s.device,
            scale: s.scale,
            appearance: s.appearance,
            dynamic_type: s.dynamic_type,
            ios: s.ios,
            scroll: &s.inter.scroll,
            sheets: &s.inter.sheets,
            focus: s.inter.focus.as_deref(),
            overscroll: s.inter.live(),
            status_bar: true,
            chrome: s.inter.chrome(),
        };
        let scene = build_scene(&root, &params, fonts);
        for name in &scene.unsupported_symbols {
            s.report_once("host", format!("unsupported_symbol: no system symbol {name:?} on this host; drew a placeholder"));
        }
        for note in &scene.notes {
            s.report_once("host", note.clone());
        }
        Ok((scene, version, s.scale))
    }

    /// Returns `(hit node id, action dispatched)`. `click` = down + up. A hit on a text field
    /// focuses it (action = false); any other click unfocuses. `scroll` hits the scroller and
    /// never reaches the guest. The state machine is `onesim_render::Interaction`.
    pub fn pointer(&mut self, id: &str, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        self.get(id)?.require_running()?;
        let (scene, version, _) = self.scene(id)?;
        if self.get(id)?.is_rn() {
            return self.rn_pointer(id, &scene, version, ev);
        }
        let s = self.get(id)?;
        s.inter.version = version;
        let effect = s.inter.pointer(&scene, ev);
        let out = (s.inter.hit.clone(), s.inter.acted);
        s.send_effect(effect)?;
        Ok(out)
    }

    /// React Native pointer input (`onesim_render::rn_input`), then settle and push the new layout.
    fn rn_pointer(&mut self, id: &str, scene: &Scene, version: u64, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        let s = self.get(id)?;
        s.inter.version = version;
        let root = Self::rn_root(s)?;
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let (hit, acted, out) = s.rn.pointer(&mut s.inter, &cx, ev);
        s.send_out(out)?;
        let _ = self.scene(id);
        Ok((hit, acted))
    }

    /// Live viewers (native window): lay out the current tree and hand the scene, the shared fonts and the tree
    /// version to `f`. Pair React Native sessions with `set_settle(id, false)` so this never waits for the guest.
    pub fn with_frame<R>(&mut self, id: &str, f: impl FnOnce(&Scene, &mut Fonts, u64) -> R) -> Result<R, ApiError> {
        let (scene, version, _) = self.scene(id)?;
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        Ok(f(&scene, fonts, version))
    }

    /// Advance scroll momentum and bounces by `dt` seconds. Returns whether anything is still moving (call
    /// again next frame); false means the scene is at rest.
    /// A finger resting on a SwiftUI gesture view is reported again each tick (long presses fire while held), and keeps
    /// the scene live.
    pub fn tick(&mut self, id: &str, dt: f64) -> Result<bool, ApiError> {
        let (scene, ..) = self.scene(id)?;
        let s = self.get(id)?;
        let moving = s.inter.tick(&scene, dt);
        if !s.inter.holding() {
            return Ok(moving);
        }
        let held = s.inter.hold(dt);
        s.send_effect(held)?;
        Ok(true)
    }

    /// One phase of a live finger drag (`t_ms` is the event time): the content follows the finger and carries on
    /// with its release velocity (see [`Host::tick`]); sheets follow too. Returns whether the scene changed.
    /// React Native receives each phase on the same Fabric target and supplies its own gesture behavior.
    pub fn drag(&mut self, id: &str, phase: onesim_render::DragPhase, x: f64, y: f64, t_ms: f64) -> Result<bool, ApiError> {
        self.get(id)?.require_running()?;
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        // React Native receives each phase on the touched Fabric view; later trees arrive through render.updated,
        // so the live caller never waits for a commit.
        if s.is_rn() {
            if let Some(out) = s.rn.drag(&s.inter, &scene, phase, x, y, t_ms) {
                s.send_out(out)?;
                return Ok(false);
            }
        }
        s.inter.version = version;
        let effect = s.inter.drag(&scene, phase, x, y, t_ms);
        let changed = effect != onesim_render::Effect::None;
        s.send_effect(effect)?;
        Ok(changed)
    }

    /// Keyboard input for the focused text field. The host computes the new text and sends it
    /// to the guest as `text_input`.
    pub fn keyboard(&mut self, id: &str, ev: KeyboardEvent) -> Result<(), ApiError> {
        self.get(id)?.require_running()?;
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
            s.send_out(out)?;
            self.scene(id)?;
            return Ok(());
        }
        let effect = s.inter.keyboard(&scene, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
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
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
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
        Self::push_env(s)
    }

    /// Hide the software keyboard by blurring the focused field.
    pub fn hide_keyboard(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.require_running()?;
        let before = s.inter.focus.take();
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

    /// Dispatch one JSON request. The bool is true after `shutdown`.
    pub fn handle(&mut self, req: Request) -> (Response, bool) {
        let shutdown = req.method == "shutdown" && (self.greeted);
        let r = self.dispatch(&req.method, req.params);
        if shutdown && r.is_ok() {
            self.shutdown();
        }
        (Response::from_result(req.id, r), shutdown)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        fn p<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, ApiError> {
            serde_json::from_value(v).map_err(|e| err(ErrorCode::BadRequest, format!("bad params: {e}")))
        }
        fn ok<T: Serialize>(v: T) -> Result<Value, ApiError> {
            Ok(serde_json::to_value(v).expect("serialize"))
        }
        if method == "hello" {
            let h: HelloParams = p(params)?;
            if h.protocol_version != PROTOCOL_VERSION {
                return Err(err(
                    ErrorCode::ProtocolVersion,
                    format!("server speaks protocol {PROTOCOL_VERSION}, client sent {}", h.protocol_version),
                ));
            }
            self.greeted = true;
            return ok(json!({"protocol_version": PROTOCOL_VERSION, "server": "onesim", "version": env!("CARGO_PKG_VERSION")}));
        }
        if !self.greeted {
            return Err(err(ErrorCode::BadRequest, "first request must be hello"));
        }
        match method {
            "get_capabilities" => ok(json!({
                "frameworks": ["swiftui", "react-native", "compose"], "platforms": ["ios", "android"], "commands": COMMANDS, "headless": true,
                "devices": DEVICES.iter().map(|d| d.name).collect::<Vec<_>>(),
                "pointer_events": ["down", "up", "click", "scroll"],
                "keyboard": {"events": ["text", "key"], "keys": ["backspace", "enter"]},
                "views": ["text", "vstack", "hstack", "zstack", "spacer", "button", "color", "rectangle", "shape", "divider",
                          "scroll", "list", "navigation", "toggle", "stepper", "text_field", "picker", "slider", "progress", "image", "label", "empty"],
                "symbols": onesim_render::symbols_capability(),
                "fonts": onesim_render::fonts_capability(),
            })),
            "create_session" => ok(json!({"session_id": self.create_session(p(params)?)?})),
            "start_session" => {
                let a: SessionIdParams = p(params)?;
                self.start_session(&a.session_id)?;
                ok(json!({"state": self.state(&a.session_id)?.as_str()}))
            }
            "stop_session" => {
                let a: SessionIdParams = p(params)?;
                self.stop_session(&a.session_id)?;
                ok(json!({"state": "stopped"}))
            }
            "reload_session" => {
                let a: SessionIdParams = p(params)?;
                self.get(&a.session_id)?.reload()?;
                ok(json!({"state": self.state(&a.session_id)?.as_str()}))
            }
            "get_session_status" => self.status(&p::<SessionIdParams>(params)?.session_id),
            "set_viewport" => {
                let a: SetViewportParams = p(params)?;
                self.set_viewport(&a.session_id, a.width, a.height)?;
                ok(json!({}))
            }
            "set_appearance" => {
                let a: SetAppearanceParams = p(params)?;
                self.set_appearance(&a.session_id, a.mode)?;
                ok(json!({}))
            }
            "send_pointer_event" => {
                let a: PointerParams = p(params)?;
                let (hit, action) = self.pointer(&a.session_id, a.event)?;
                ok(json!({"hit": hit, "action": action}))
            }
            "send_keyboard_event" => {
                let a: KeyboardParams = p(params)?;
                self.keyboard(&a.session_id, a.event)?;
                ok(json!({}))
            }
            "capture_screenshot" => {
                let a: ScreenshotParams = p(params)?;
                let (width, height, png) = self.screenshot(&a.session_id)?;
                match a.path {
                    Some(path) => {
                        std::fs::write(&path, png).map_err(|e| err(ErrorCode::RuntimeError, format!("write {path}: {e}")))?;
                        ok(json!({"width": width, "height": height, "path": path}))
                    }
                    None => ok(json!({"width": width, "height": height,
                        "png_base64": base64::engine::general_purpose::STANDARD.encode(png)})),
                }
            }
            "inspect_ui_tree" => {
                let (tree_version, root) = self.inspect(&p::<SessionIdParams>(params)?.session_id)?;
                ok(json!({"tree_version": tree_version, "root": root}))
            }
            "get_errors" => ok(json!({"errors": self.errors(&p::<SessionIdParams>(params)?.session_id)?})),
            "shutdown" => ok(json!({})),
            m => Err(err(ErrorCode::BadRequest, format!("unknown method {m:?}"))),
        }
    }

    /// JSON-lines server loop: one request per line in, one response per line out.
    pub fn serve(&mut self, input: impl BufRead, mut out: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let (resp, done) = match serde_json::from_str::<Request>(&line) {
                Ok(req) => self.handle(req),
                Err(e) => (Response::from_result(0, Err(err(ErrorCode::BadRequest, format!("invalid request: {e}")))), false),
            };
            writeln!(out, "{}", serde_json::to_string(&resp).expect("serialize"))?;
            out.flush()?;
            if done {
                break;
            }
        }
        self.shutdown();
        Ok(())
    }
}
