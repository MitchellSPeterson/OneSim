//! Guest wire messages (host <-> app process), JSON lines.

use crate::view::ViewNode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GuestMessage {
    Hello { protocol: u32 },
    Tree { version: u64, root: ViewNode },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HostMessage {
    Action { id: u64, version: u64 },
    Environment { appearance: Appearance, viewport: Viewport },
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let g: GuestMessage = serde_json::from_str(
            r#"{"type":"tree","version":3,"root":{"kind":"text","id":"0","text":"hi"}}"#,
        )
        .unwrap();
        assert!(matches!(g, GuestMessage::Tree { version: 3, .. }));
        assert!(matches!(
            serde_json::from_str::<GuestMessage>(r#"{"type":"hello","protocol":1}"#).unwrap(),
            GuestMessage::Hello { protocol: 1 }
        ));
        let h = HostMessage::Environment {
            appearance: Appearance::Dark,
            viewport: Viewport { width: 393.0, height: 852.0 },
        };
        assert_eq!(
            serde_json::to_string(&h).unwrap(),
            r#"{"type":"environment","appearance":"dark","viewport":{"width":393.0,"height":852.0}}"#
        );
        assert_eq!(
            serde_json::to_string(&HostMessage::Action { id: 1, version: 2 }).unwrap(),
            r#"{"type":"action","id":1,"version":2}"#
        );
        assert_eq!(serde_json::to_string(&HostMessage::Shutdown).unwrap(), r#"{"type":"shutdown"}"#);
    }
}
