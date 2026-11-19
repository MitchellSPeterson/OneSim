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
    /// React Native `focus`/`blur` commands: focus the field with this tag (`null` = blur).
    #[serde(rename = "focus_request")]
    FocusRequest { tag: Option<u64> },
    /// React Native `scrollTo` / `scrollToEnd` commands.
    #[serde(rename = "scroll_to")]
    ScrollTo { tag: u64, x: ScrollTarget, y: ScrollTarget },
    /// Lifecycle event (`bundle.loaded`, `bundle.reloaded`, `bundle.error`, `refresh.applied`).
    Event { name: String, #[serde(default)] detail: serde_json::Value },
    /// React Native native-module inventory, once after boot (same shape as the API's `modules.list`).
    Modules { modules: serde_json::Value },
}

/// A `scroll_to` coordinate: an offset in points or the string `"end"` (max offset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScrollTarget {
    Pos(f64),
    Named(String),
}

impl ScrollTarget {
    /// Offset in points; `"end"` is `max`, any other string 0.
    pub fn resolve(&self, max: f64) -> f64 {
        match self {
            ScrollTarget::Pos(v) => *v,
            ScrollTarget::Named(n) if n == "end" => max,
            ScrollTarget::Named(_) => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollPhase {
    Begin,
    Scroll,
    End,
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

/// Simulated iOS release: picks the look the host draws and the OS version the guest reports.
/// Each is pinned to the Simulator runtime its reference screenshots were captured on
/// (`docs/compatibility/versions.json`); the newest is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum IosVersion {
    #[serde(rename = "26.5", alias = "26")]
    Ios26,
    #[default]
    #[serde(rename = "27.2", alias = "27")]
    Ios27,
}

impl IosVersion {
    pub const ALL: [IosVersion; 2] = [Self::Ios26, Self::Ios27];
    /// Runtime version, e.g. `27.2`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ios26 => "26.5",
            Self::Ios27 => "27.2",
        }
    }
    /// Runtime build, e.g. `24B8117`.
    pub fn build(self) -> &'static str {
        match self {
            Self::Ios26 => "23F77",
            Self::Ios27 => "24B8117",
        }
    }
    /// Accepts `27.2`, `27`, `ios27`, `iOS 27.2`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s
            .trim()
            .trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ' ');
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.name() == s || v.name().split('.').next() == Some(s))
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
    /// React Native: host focus moved to / away from the text input `tag`.
    Focus { tag: u64 },
    Blur { tag: u64 },
    /// React Native: the software keyboard appeared (`height` points, from the screen bottom) or hid (`0`).
    Keyboard { height: f64 },
    /// React Native: return in text input `tag`. Focus follows its `submitBehavior` and multiline setting.
    Submit { tag: u64 },
    /// React Native scroll gesture on `tag`: `x`/`y` content offset, sizes in points.
    Scroll {
        tag: u64,
        phase: ScrollPhase,
        x: f64,
        y: f64,
        #[serde(rename = "contentWidth")]
        content_width: f64,
        #[serde(rename = "contentHeight")]
        content_height: f64,
        width: f64,
        height: f64,
    },
    /// React Native: set simulated platform state of a native module (`modules.simulate`).
    Simulate { module: String, state: serde_json::Value },
    /// React Native: tab bar item tapped (`RNSTabsHostIOS` tag, `screenKey`).
    #[serde(rename = "select_tab")]
    SelectTab { tag: u64, key: String },
    /// React Native: native header back button tapped (`RNSScreenStack` tag).
    #[serde(rename = "native_back")]
    NativeBack { tag: u64 },
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
            serde_json::from_str::<GuestMessage>(r#"{"type":"event","name":"refresh.applied","detail":{"ms":120}}"#).unwrap(),
            GuestMessage::Event { ref name, .. } if name == "refresh.applied"
        ));
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
        for (m, j) in [
            (HostMessage::Focus { tag: 9 }, r#"{"type":"focus","tag":9}"#),
            (HostMessage::Blur { tag: 9 }, r#"{"type":"blur","tag":9}"#),
            (HostMessage::Keyboard { height: 335.0 }, r#"{"type":"keyboard","height":335.0}"#),
            (HostMessage::Submit { tag: 9 }, r#"{"type":"submit","tag":9}"#),
            (HostMessage::SelectTab { tag: 4, key: "explore".into() }, r#"{"type":"select_tab","tag":4,"key":"explore"}"#),
            (HostMessage::NativeBack { tag: 5 }, r#"{"type":"native_back","tag":5}"#),
            (
                HostMessage::Scroll { tag: 3, phase: ScrollPhase::Begin, x: 0.0, y: 5.0, content_width: 393.0, content_height: 2000.0, width: 393.0, height: 800.0 },
                r#"{"type":"scroll","tag":3,"phase":"begin","x":0.0,"y":5.0,"contentWidth":393.0,"contentHeight":2000.0,"width":393.0,"height":800.0}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&m).unwrap(), j);
            assert_eq!(serde_json::from_str::<HostMessage>(j).unwrap(), m);
        }
        let g: GuestMessage = serde_json::from_str(r#"{"type":"focus_request","tag":null}"#).unwrap();
        assert_eq!(g, GuestMessage::FocusRequest { tag: None });
        assert_eq!(serde_json::from_str::<GuestMessage>(r#"{"type":"focus_request","tag":4}"#).unwrap(), GuestMessage::FocusRequest { tag: Some(4) });
        let g: GuestMessage = serde_json::from_str(r#"{"type":"scroll_to","tag":3,"x":0,"y":"end"}"#).unwrap();
        let GuestMessage::ScrollTo { tag: 3, x, y } = g else { panic!() };
        assert_eq!((x.resolve(10.0), y.resolve(10.0)), (0.0, 10.0));
        assert_eq!(
            serde_json::to_string(&HostMessage::TextInput { field: 4, version: 2, text: "hi".into() }).unwrap(),
            r#"{"type":"text_input","field":4,"version":2,"text":"hi"}"#
        );
    }
}
