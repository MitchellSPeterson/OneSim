//! Legacy v1 JSON-lines API (docs/protocol.md section A).

use crate::{err, Host, DEVICES};
use base64::Engine;
use onesim_protocol::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

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
    "set_scene_phase",
    "open_url",
    "send_pointer_event",
    "send_drag_phase",
    "send_keyboard_event",
    "tick",
    "wait",
    "capture_screenshot",
    "inspect_ui_tree",
    "get_errors",
    "shutdown",
];

impl Host {
    /// Dispatch one JSON request. The bool is true after `shutdown`.
    pub fn handle(&mut self, req: Request) -> (Response, bool) {
        let shutdown = req.method == "shutdown" && self.greeted;
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
                          "scroll", "list", "navigation", "toggle", "stepper", "text_field", "picker", "slider", "progress", "image", "label", "empty",
                          "canvas", "canvasItem", "visualEffect"],
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
            "set_scene_phase" => {
                let a: SetScenePhaseParams = p(params)?;
                self.set_scene_phase(&a.session_id, a.phase)?;
                ok(json!({"scene_phase": self.scene_phase(&a.session_id)?.as_str()}))
            }
            "open_url" => {
                let a: OpenUrlParams = p(params)?;
                self.open_url(&a.session_id, &a.url)?;
                ok(json!({"scene_phase": self.scene_phase(&a.session_id)?.as_str()}))
            }
            "send_pointer_event" => {
                let a: PointerParams = p(params)?;
                let (hit, action) = self.pointer(&a.session_id, a.event)?;
                ok(json!({"hit": hit, "action": action}))
            }
            "send_drag_phase" => {
                let a: DragPhaseParams = p(params)?;
                let phase = match a.phase {
                    DragPhaseKind::Begin => onesim_render::DragPhase::Begin,
                    DragPhaseKind::Move => onesim_render::DragPhase::Move,
                    DragPhaseKind::End => onesim_render::DragPhase::End,
                };
                let t_ms = match a.time_ms {
                    Some(t) if t.is_finite() => t,
                    Some(_) => return Err(err(ErrorCode::BadRequest, "time_ms must be finite")),
                    None => self.clock_ms(&a.session_id)?,
                };
                let changed = self.drag(&a.session_id, phase, a.x, a.y, t_ms)?;
                ok(json!({"changed": changed}))
            }
            "send_keyboard_event" => {
                let a: KeyboardParams = p(params)?;
                self.keyboard(&a.session_id, a.event)?;
                ok(json!({}))
            }
            "tick" => {
                let a: TickParams = p(params)?;
                if !a.dt_ms.is_finite() || a.dt_ms < 0.0 {
                    return Err(err(ErrorCode::BadRequest, "dt_ms must be a non-negative finite number"));
                }
                let animating = self.tick(&a.session_id, a.dt_ms / 1000.0)?;
                let clock_ms = self.clock_ms(&a.session_id)?;
                ok(json!({"clock_ms": clock_ms, "animating": animating}))
            }
            "wait" => {
                let a: WaitParams = p(params)?;
                if !a.ms.is_finite() || a.ms < 0.0 {
                    return Err(err(ErrorCode::BadRequest, "ms must be a non-negative finite number"));
                }
                self.wait(&a.session_id, a.ms)?;
                let clock_ms = self.clock_ms(&a.session_id)?;
                let animating = self.is_animating(&a.session_id);
                ok(json!({"clock_ms": clock_ms, "animating": animating}))
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
