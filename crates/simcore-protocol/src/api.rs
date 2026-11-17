//! External API (client <-> host) request/response types.

use crate::wire::{Appearance, DynamicTypeSize, Viewport};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    NotFound,
    InvalidState,
    Unsupported,
    BuildFailed,
    RuntimeError,
    Timeout,
    ProtocolVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

impl Response {
    pub fn from_result(id: u64, r: Result<Value, ApiError>) -> Self {
        match r {
            Ok(v) => Self { id, ok: true, result: Some(v), error: None },
            Err(e) => Self { id, ok: false, result: None, error: Some(e) },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HelloParams {
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionParams {
    pub framework: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    pub app_path: String,
    #[serde(default)]
    pub root_view: Option<String>,
    #[serde(default)]
    pub viewport: Option<Viewport>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub appearance: Option<Appearance>,
    /// Dynamic Type size (default `large`).
    #[serde(default, alias = "dynamicTypeSize")]
    pub dynamic_type_size: Option<DynamicTypeSize>,
    /// Device profile name (default `iphone-17-pro`).
    #[serde(default)]
    pub device: Option<String>,
    /// Test hook: skip the Swift build and spawn `app_path` directly.
    #[serde(default)]
    pub prebuilt: bool,
}

fn default_platform() -> String {
    "ios".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionIdParams {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetViewportParams {
    pub session_id: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetAppearanceParams {
    pub session_id: String,
    pub mode: Appearance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerKind {
    Down,
    Up,
    Click,
    Scroll,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f64,
    pub y: f64,
    /// Scroll deltas in points; positive scrolls content forward (reveals later content).
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
}

impl PointerEvent {
    pub fn new(kind: PointerKind, x: f64, y: f64) -> Self {
        Self { kind, x, y, dx: 0.0, dy: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KeyboardEvent {
    Text { text: String },
    Key { key: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyboardParams {
    pub session_id: String,
    pub event: KeyboardEvent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PointerParams {
    pub session_id: String,
    pub event: PointerEvent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotParams {
    pub session_id: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    pub id: String,
    pub kind: String,
    pub frame: UiFrame,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<u64>,
    /// Current value: toggle "on"/"off", text field contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    pub children: Vec<UiNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionError {
    pub message: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_response() {
        let r: Request = serde_json::from_str(
            r#"{"id":1,"method":"send_pointer_event","params":{"session_id":"s1","event":{"kind":"click","x":1,"y":2.5}}}"#,
        )
        .unwrap();
        let p: PointerParams = serde_json::from_value(r.params).unwrap();
        assert_eq!(p.event.kind, PointerKind::Click);
        let e = Response::from_result(2, Err(ApiError::new(ErrorCode::NotFound, "x")));
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"id":2,"ok":false,"error":{"code":"not_found","message":"x"}}"#
        );
        // params may be omitted
        let r: Request = serde_json::from_str(r#"{"id":3,"method":"shutdown"}"#).unwrap();
        assert!(r.params.is_null());
    }

    #[test]
    fn scroll_and_keyboard_events() {
        let e: PointerEvent = serde_json::from_str(r#"{"kind":"scroll","x":1,"y":2,"dx":0,"dy":-30.5}"#).unwrap();
        assert_eq!((e.kind, e.dy), (PointerKind::Scroll, -30.5));
        let c: PointerEvent = serde_json::from_str(r#"{"kind":"click","x":1,"y":2}"#).unwrap();
        assert_eq!((c.dx, c.dy), (0.0, 0.0));
        let k: KeyboardEvent = serde_json::from_str(r#"{"kind":"text","text":"abc"}"#).unwrap();
        assert_eq!(k, KeyboardEvent::Text { text: "abc".into() });
        let k: KeyboardEvent = serde_json::from_str(r#"{"kind":"key","key":"backspace"}"#).unwrap();
        assert_eq!(k, KeyboardEvent::Key { key: "backspace".into() });
    }

    #[test]
    fn create_defaults() {
        let p: CreateSessionParams =
            serde_json::from_str(r#"{"framework":"swiftui","app_path":"a.swift","appearance":"dark"}"#).unwrap();
        assert_eq!(p.platform, "ios");
        assert_eq!(p.appearance, Some(Appearance::Dark));
    }
}
