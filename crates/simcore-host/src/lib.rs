//! Session manager and external API for SimCore (docs/protocol.md section A).
//!
//! Test hook: `create_session` skips the Swift build and spawns `app_path` directly when
//! `"prebuilt": true` is passed or `app_path` is an executable file that is not `*.swift`.
//! `simcore-fake-guest` (a bin of this crate) is such an executable.

mod session;

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use simcore_protocol::*;
use simcore_render::{build_scene, render_png, Fonts, Scene, SceneParams};
use std::collections::HashMap;
use std::io::{BufRead, Write};

pub use session::{device, simcore_home, Device, State, DEVICES};
use session::Session;

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

pub struct Host {
    sessions: HashMap<String, Session>,
    next_id: u64,
    fonts: Option<Fonts>,
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
        Self { sessions: HashMap::new(), next_id: 1, fonts: None, greeted: false }
    }

    fn get(&mut self, id: &str) -> Result<&mut Session, ApiError> {
        self.sessions.get_mut(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))
    }

    pub fn create_session(&mut self, p: CreateSessionParams) -> Result<String, ApiError> {
        if p.framework != "swiftui" {
            return Err(err(ErrorCode::Unsupported, format!("framework {:?} (only \"swiftui\")", p.framework)));
        }
        if p.platform != "ios" {
            return Err(err(ErrorCode::Unsupported, format!("platform {:?} (only \"ios\")", p.platform)));
        }
        if !std::path::Path::new(&p.app_path).exists() {
            return Err(err(ErrorCode::BadRequest, format!("app_path {:?} does not exist", p.app_path)));
        }
        if let Some(d) = &p.device {
            if device(d).is_none() {
                let known: Vec<&str> = DEVICES.iter().map(|d| d.name).collect();
                return Err(err(ErrorCode::Unsupported, format!("device {d:?} (known: {})", known.join(", "))));
            }
        }
        let id = format!("s{}", self.next_id);
        self.next_id += 1;
        self.sessions.insert(id.clone(), Session::new(id.clone(), p));
        Ok(id)
    }

    /// Blocks until the first tree arrives (or the build/launch fails).
    pub fn start_session(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        log::info!("starting session {id}");
        s.start()
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
            "device": s.device.name, "safe_area": {"top": s.device.safe_top, "bottom": s.device.safe_bottom},
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

    fn scene(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        let s = self.sessions.get(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))?;
        let (version, root) = s
            .tree()
            .ok_or_else(|| err(ErrorCode::InvalidState, format!("session {id} is {} and has no tree", s.state().as_str())))?;
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        let params = SceneParams {
            viewport: (s.viewport.width, s.viewport.height),
            safe_top: s.device.safe_top,
            safe_bottom: s.device.safe_bottom,
            scale: s.scale,
            appearance: s.appearance,
            dynamic_type: s.dynamic_type,
            scroll: &s.inter.scroll,
            focus: s.inter.focus.as_deref(),
            status_bar: true,
            chrome: s.inter.chrome(s.device.island),
        };
        let scene = build_scene(&root, &params, fonts);
        for name in &scene.unsupported_symbols {
            s.report_once("host", format!("unsupported_symbol: no system symbol {name:?} on this host; drew a placeholder"));
        }
        Ok((scene, version, s.scale))
    }

    /// Returns `(hit node id, action dispatched)`. `click` = down + up. A hit on a text field
    /// focuses it (action = false); any other click unfocuses. `scroll` hits the scroller and
    /// never reaches the guest. The state machine is `simcore_render::Interaction`.
    pub fn pointer(&mut self, id: &str, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        self.get(id)?.require_running()?;
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        s.inter.version = version;
        let effect = s.inter.pointer(&scene, ev);
        let out = (s.inter.hit.clone(), s.inter.acted);
        s.send_effect(effect)?;
        Ok(out)
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
        let effect = s.inter.keyboard(&scene, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
        s.send_effect(effect)
    }

    /// PNG bytes plus pixel size.
    pub fn screenshot(&mut self, id: &str) -> Result<(u32, u32, Vec<u8>), ApiError> {
        let (scene, _, scale) = self.scene(id)?;
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        let png = render_png(&scene, fonts, scale).map_err(|e| err(ErrorCode::RuntimeError, e))?;
        let px = |v: f64| (v * scale).round().max(1.0) as u32;
        Ok((px(scene.size.0), px(scene.size.1), png))
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
            return ok(json!({"protocol_version": PROTOCOL_VERSION, "server": "simcore", "version": env!("CARGO_PKG_VERSION")}));
        }
        if !self.greeted {
            return Err(err(ErrorCode::BadRequest, "first request must be hello"));
        }
        match method {
            "get_capabilities" => ok(json!({
                "frameworks": ["swiftui"], "platforms": ["ios"], "commands": COMMANDS, "headless": true,
                "devices": DEVICES.iter().map(|d| d.name).collect::<Vec<_>>(),
                "pointer_events": ["down", "up", "click", "scroll"],
                "keyboard": {"events": ["text", "key"], "keys": ["backspace", "enter"]},
                "views": ["text", "vstack", "hstack", "zstack", "spacer", "button", "color", "rectangle", "shape", "divider",
                          "scroll", "list", "navigation", "toggle", "stepper", "text_field", "image", "label", "empty"],
                "symbols": simcore_render::symbols_capability(),
                "fonts": simcore_render::fonts_capability(),
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
