//! External API (client <-> host) request/response types.

use crate::wire::{Appearance, DynamicTypeSize, IosVersion, Viewport};
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
    /// React Native targets iOS (default) or Android; Compose implies Android.
    #[serde(default = "default_platform")]
    pub platform: String,
    /// Swift app path; unused by `react-native` sessions.
    #[serde(default)]
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
    /// Device profile name (defaults to `iphone-18-pro` on iOS and `pixel-10a` on Android).
    #[serde(default)]
    pub device: Option<String>,
    /// Simulated iOS release (`"26.5"` or `"27.2"`, default the newest).
    #[serde(default, alias = "osVersion")]
    pub os_version: Option<IosVersion>,
    /// Test hook: skip the Swift build and spawn `app_path` directly.
    #[serde(default)]
    pub prebuilt: bool,
    /// `react-native` only: Metro dev server (default `http://localhost:8081`).
    #[serde(default, alias = "metroUrl")]
    pub metro_url: Option<String>,
    /// `swiftui` only: the app is launched to open this URL (`onOpenURL` gets it after the first frame).
    #[serde(default, alias = "launchUrl", alias = "launchURL")]
    pub launch_url: Option<String>,
}

/// A URL the system can hand to an app: a scheme (`letter (letter | digit | + - .)*`) then `:`, no surrounding
/// whitespace, no control characters.
pub fn is_openable_url(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else { return false };
    let mut cs = scheme.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic())
        && cs.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !url.chars().any(char::is_control)
        && url.trim() == url
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

#[derive(Debug, Clone, Deserialize)]
pub struct SetScenePhaseParams {
    pub session_id: String,
    pub phase: crate::ScenePhase,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenUrlParams {
    pub session_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerKind {
    Down,
    Up,
    Click,
    Scroll,
    /// A finger drag from (x, y) by (dx, dy) points, as one gesture (touch scrolling, sheet swipes; React Native: touch start, moves, end).
    Drag,
    /// A finger held still at (x, y) for `duration` seconds (default 1), then lifted.
    #[serde(rename = "long_press")]
    LongPress,
    /// Two fingers centred on (x, y), 100 pt apart horizontally, spread (or pinched) to `scale` times that distance.
    Pinch,
    /// Two fingers centred on (x, y), 100 pt apart horizontally, turned by `degrees` (positive = clockwise).
    Rotate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f64,
    pub y: f64,
    /// Scroll deltas in points; positive scrolls content forward (reveals later content). For `drag`, how far the finger moves.
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    /// `long_press`: how long the finger stays down (seconds, default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// `pinch`: final finger distance over the initial one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// `rotate`: how far the fingers turn, clockwise degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degrees: Option<f64>,
}

impl PointerEvent {
    pub fn new(kind: PointerKind, x: f64, y: f64) -> Self {
        Self { kind, x, y, dx: 0.0, dy: 0.0, duration: None, scale: None, degrees: None }
    }

    /// `scroll` / `drag` by (dx, dy) points.
    pub fn moved(kind: PointerKind, x: f64, y: f64, dx: f64, dy: f64) -> Self {
        Self { dx, dy, ..Self::new(kind, x, y) }
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
        let p: PointerEvent = serde_json::from_str(r#"{"kind":"pinch","x":1,"y":2,"scale":1.5}"#).unwrap();
        assert_eq!((p.kind, p.scale), (PointerKind::Pinch, Some(1.5)));
        let r: PointerEvent = serde_json::from_str(r#"{"kind":"rotate","x":1,"y":2,"degrees":30}"#).unwrap();
        assert_eq!((r.kind, r.degrees), (PointerKind::Rotate, Some(30.0)));
        let l: PointerEvent = serde_json::from_str(r#"{"kind":"long_press","x":1,"y":2,"duration":0.8}"#).unwrap();
        assert_eq!((l.kind, l.duration), (PointerKind::LongPress, Some(0.8)));
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
