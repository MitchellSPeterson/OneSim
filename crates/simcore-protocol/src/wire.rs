//! Guest wire messages (host <-> app process), JSON lines.

use crate::view::ViewNode;
#[cfg(test)]
use crate::view::ViewKind;
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

/// iOS Dynamic Type size (`UIContentSizeCategory`); `Large` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DynamicTypeSize {
    #[serde(rename = "xSmall")]
    XSmall,
    #[serde(rename = "small")]
    Small,
    #[serde(rename = "medium")]
    Medium,
    #[default]
    #[serde(rename = "large")]
    Large,
    #[serde(rename = "xLarge")]
    XLarge,
    #[serde(rename = "xxLarge")]
    XxLarge,
    #[serde(rename = "xxxLarge")]
    XxxLarge,
    #[serde(rename = "accessibility1")]
    Accessibility1,
    #[serde(rename = "accessibility2")]
    Accessibility2,
    #[serde(rename = "accessibility3")]
    Accessibility3,
    #[serde(rename = "accessibility4")]
    Accessibility4,
    #[serde(rename = "accessibility5")]
    Accessibility5,
}

impl DynamicTypeSize {
    pub const ALL: [DynamicTypeSize; 12] = [
        Self::XSmall, Self::Small, Self::Medium, Self::Large, Self::XLarge, Self::XxLarge, Self::XxxLarge,
        Self::Accessibility1, Self::Accessibility2, Self::Accessibility3, Self::Accessibility4, Self::Accessibility5,
    ];
    /// Column in Apple's Dynamic Type size table (0 = xSmall ... 11 = accessibility5).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(3)
    }
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or_default()
    }
    /// Wire/CLI spelling, e.g. `accessibility3`.
    pub fn name(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|d| d.name().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchPhase {
    Start,
    Move,
    End,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HostMessage {
    Action { id: u64, version: u64 },
    #[serde(rename = "text_input")]
    TextInput { field: u64, version: u64, text: String },
    Environment {
        appearance: Appearance,
        viewport: Viewport,
        /// Omitted at the default size (`large`).
        #[serde(default, rename = "dynamicTypeSize", skip_serializing_if = "Option::is_none")]
        dynamic_type_size: Option<DynamicTypeSize>,
    },
    /// React Native touch: `x`/`y` page points, `lx`/`ly` relative to the target node (`tag`).
    Touch { phase: TouchPhase, tag: u64, x: f64, y: f64, lx: f64, ly: f64, timestamp: f64 },
    /// React Native layout frames `tag -> [x, y, w, h]` in page points.
    Layout { frames: std::collections::BTreeMap<String, [f64; 4]> },
    Reload,
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_type_sizes() {
        use crate::view::TextStyle;
        let d = DynamicTypeSize::parse("accessibility5").unwrap();
        assert_eq!(d.index(), 11);
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"accessibility5\"");
        assert_eq!(DynamicTypeSize::parse("xxLarge"), Some(DynamicTypeSize::XxLarge));
        assert_eq!(DynamicTypeSize::default(), DynamicTypeSize::Large);
        assert_eq!((TextStyle::Body.size_at(DynamicTypeSize::Large), TextStyle::LargeTitle.size_at(DynamicTypeSize::Large)), (17.0, 34.0));
        assert_eq!((TextStyle::Body.size_at(d), TextStyle::LargeTitle.size_at(d), TextStyle::Caption.size_at(d)), (53.0, 60.0, 43.0));
    }

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
            dynamic_type_size: None,
        };
        assert_eq!(
            serde_json::to_string(&h).unwrap(),
            r#"{"type":"environment","appearance":"dark","viewport":{"width":393.0,"height":852.0}}"#
        );
        assert_eq!(
            serde_json::to_string(&HostMessage::Action { id: 1, version: 2 }).unwrap(),
            r#"{"type":"action","id":1,"version":2}"#
        );
        let t = HostMessage::Touch { phase: TouchPhase::Start, tag: 7, x: 1.0, y: 2.0, lx: 0.5, ly: 1.5, timestamp: 10.0 };
        let j = serde_json::to_string(&t).unwrap();
        assert_eq!(j, r#"{"type":"touch","phase":"start","tag":7,"x":1.0,"y":2.0,"lx":0.5,"ly":1.5,"timestamp":10.0}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&j).unwrap(), t);
        let l = HostMessage::Layout { frames: [("7".to_string(), [1.0, 2.0, 3.0, 4.0])].into() };
        let j = serde_json::to_string(&l).unwrap();
        assert_eq!(j, r#"{"type":"layout","frames":{"7":[1.0,2.0,3.0,4.0]}}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&j).unwrap(), l);
        assert_eq!(serde_json::to_string(&HostMessage::Reload).unwrap(), r#"{"type":"reload"}"#);
        let rn: GuestMessage = serde_json::from_str(
            r#"{"type":"tree","version":1,"root":{"kind":"rn","id":"1","component":"RootView","props":{"flex":1},"children":[{"kind":"rn","id":"2","component":"RCTRawText","props":{"text":"hi"},"children":[]}]}}"#,
        )
        .unwrap();
        let GuestMessage::Tree { root, .. } = rn else { panic!() };
        let ViewKind::Rn { component, props, children } = &root.kind else { panic!() };
        assert_eq!((component.as_str(), props["flex"].as_i64(), children.len()), ("RootView", Some(1), 1));
        assert_eq!(serde_json::from_str::<ViewNode>(&serde_json::to_string(&root).unwrap()).unwrap(), root);
        assert_eq!(serde_json::to_string(&HostMessage::Shutdown).unwrap(), r#"{"type":"shutdown"}"#);
        assert_eq!(
            serde_json::to_string(&HostMessage::TextInput { field: 4, version: 2, text: "hi".into() }).unwrap(),
            r#"{"type":"text_input","field":4,"version":2,"text":"hi"}"#
        );
    }
}
