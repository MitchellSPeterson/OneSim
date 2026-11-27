//! Guest wire messages (host <-> app process), JSON lines.

use crate::view::ViewNode;
#[cfg(test)]
use crate::view::ViewKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GuestMessage {
    Hello { protocol: u32 },
    Tree { version: u64, root: Box<ViewNode> },
    Error { message: String },
    /// React Native `focus`/`blur` commands: focus the field with this tag (`null` = blur).
    #[serde(rename = "focus_request")]
    FocusRequest { tag: Option<u64> },
    /// React Native's responder system: the view `tag` became the JS responder and blocks native ones (a
    /// PanResponder's grant), or `null` when no such responder remains. Scroll views inside it stop scrolling.
    #[serde(rename = "js_responder")]
    JsResponder { tag: Option<u64> },
    /// Answer to the host's `touch_ask`, sent after the touch messages before it were dispatched: whether a gesture
    /// recognizer (react-native-gesture-handler) captured the touch.
    #[serde(rename = "touch_answer")]
    TouchAnswer { ask: u64, captured: bool },
    /// React Native `scrollTo` / `scrollToEnd` commands.
    #[serde(rename = "scroll_to")]
    ScrollTo { tag: u64, x: ScrollTarget, y: ScrollTarget },
    /// The app replaced the session pasteboard (section W: the share sheet's Copy).
    Pasteboard { items: Vec<String> },
    /// SwiftUI `ScrollViewProxy.scrollTo` (section M). `anchor` is a unit point; absent means top-leading.
    #[serde(rename = "scroll_to_id")]
    ScrollToId {
        scroll: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<[f64; 2]>,
    },
    /// SwiftUI animation/timeline wake-up request (section P): ask host for a `tick` after `afterMs` ms,
    /// or cancel pending guest ticks when `None`.
    Schedule {
        #[serde(default, rename = "afterMs", skip_serializing_if = "Option::is_none")]
        after_ms: Option<f64>,
    },
    /// Lifecycle event (`bundle.loaded`, `bundle.reloaded`, `bundle.error`, `refresh.applied`).
    Event { name: String, #[serde(default)] detail: serde_json::Value },
    /// React Native native-module inventory, once after boot (same shape as the API's `modules.list`).
    Modules { modules: serde_json::Value },
    /// Work for the host (React Native, section H): `method` `image.blurhash` / `image.thumbhash` / `text.render`.
    /// The host answers with `HostMessage::Response` carrying the same `id`.
    Request { id: u64, method: String, #[serde(default)] params: serde_json::Value },
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
    /// The finger lifted and the content carries on (a fling or a bounce); `momentum_end` follows when it stops.
    Release,
    #[serde(rename = "momentum_end")]
    MomentumEnd,
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

/// One `GeometryReader` box the host reports back (section M), in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoBox {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    #[serde(rename = "safeTop", default)]
    pub safe_top: f64,
    #[serde(rename = "safeBottom", default)]
    pub safe_bottom: f64,
    #[serde(rename = "safeLeading", default)]
    pub safe_leading: f64,
    #[serde(rename = "safeTrailing", default)]
    pub safe_trailing: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchPhase {
    Start,
    Move,
    End,
    Cancel,
}

/// A view carrying SwiftUI gestures under a touch (protocol section L): its gesture `key` and frame `[x, y, w, h]` in points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureTarget {
    pub key: String,
    pub frame: [f64; 4],
}

/// One SwiftUI touch event (protocol section L). `points` are the fingers in screen points, `time` in ms;
/// `targets` run innermost first and stay the same for the whole touch sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureTouch {
    pub phase: TouchPhase,
    pub time: f64,
    pub points: Vec<[f64; 2]>,
    pub targets: Vec<GestureTarget>,
}

/// Phase of an Android system back (protocol section X): a predictive back gesture is `start`, `progress`..., then
/// `commit` or `cancel`; the back key (or a button press) is a lone `commit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackPhase {
    Start,
    Progress,
    Cancel,
    Commit,
}

/// Screen edge a back swipe started from (`BackEventCompat.swipeEdge`); `none` for the back key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwipeEdge {
    Left,
    Right,
    None,
}

/// One Android system back event (protocol section X). `progress` is 0...1, `x`/`y` the finger in screen points,
/// `time` in ms.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BackEvent {
    pub phase: BackPhase,
    pub progress: f64,
    pub edge: SwipeEdge,
    pub x: f64,
    pub y: f64,
    pub time: f64,
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
    /// SwiftUI: replace the app's pasteboard (section W).
    Pasteboard { items: Vec<String> },
    /// React Native touch: `x`/`y` page points, `lx`/`ly` relative to the target node (`tag`).
    Touch { phase: TouchPhase, tag: u64, x: f64, y: f64, lx: f64, ly: f64, timestamp: f64 },
    /// React Native: a finger on a scroll view passed the pan slop; the guest answers `touch_answer` with this `ask`
    /// once it has dispatched the touches so far (a responder grant or gesture activation may claim the touch).
    #[serde(rename = "touch_ask")]
    TouchAsk { ask: u64 },
    /// SwiftUI touch on views with gestures (section L). Not version-gated: gesture keys are structural paths.
    Gesture(GestureTouch),
    /// Android system back (section X): the back key or a predictive back swipe from a screen edge. Not version-gated.
    Back(BackEvent),
    /// React Native layout frames `tag -> [x, y, w, h]` in page points.
    Layout { frames: std::collections::BTreeMap<String, [f64; 4]> },
    /// SwiftUI `GeometryReader` correction (section M). One frame per geometry node id.
    Geometry { frames: std::collections::BTreeMap<String, GeoBox> },
    /// SwiftUI `.scrollPosition` writeback: the id of the topmost visible anchored view, or null.
    #[serde(rename = "scroll_position")]
    ScrollPosition { scroll: String, target: Option<String> },
    /// SwiftUI guest clock tick (section P): session animation time in ms for `TimelineView`,
    /// `PhaseAnimator`, `KeyframeAnimator`, and animation completion callbacks.
    Tick { time: f64 },
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
    /// SwiftUI scene phase (`ScenePhase`): the app moved to this phase. The host steps through `inactive` between
    /// `active` and `background`, one message per step, as iOS does; the guest answers each with a tree.
    #[serde(rename = "scene_phase")]
    ScenePhase { phase: ScenePhase },
    /// SwiftUI: the system opened this URL in the app (`onOpenURL`). The guest answers with a tree.
    /// React Native (iOS): Linking's `url` event.
    #[serde(rename = "open_url")]
    OpenUrl { url: String },
    /// The host's answer to `GuestMessage::Request` `id`: `result`, or `error` (the message the guest rejects with).
    Response {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Reload,
    Shutdown,
}

/// An app's scene phase (SwiftUI `ScenePhase`, UIKit `UIApplication.State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenePhase {
    #[default]
    Active,
    Inactive,
    Background,
}

impl ScenePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ScenePhase::Active => "active",
            ScenePhase::Inactive => "inactive",
            ScenePhase::Background => "background",
        }
    }

    pub const ALL: [ScenePhase; 3] = [ScenePhase::Active, ScenePhase::Inactive, ScenePhase::Background];

    /// The phases an app passes through from `self` to `to`, ending at `to` (empty when equal): iOS always goes
    /// through `inactive` between `active` and `background`.
    pub fn steps_to(self, to: ScenePhase) -> Vec<ScenePhase> {
        use ScenePhase::*;
        match (self, to) {
            (a, b) if a == b => vec![],
            (Active, Background) | (Background, Active) => vec![Inactive, to],
            _ => vec![to],
        }
    }
}

impl std::str::FromStr for ScenePhase {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        ScenePhase::ALL.into_iter().find(|p| p.as_str() == s).ok_or_else(|| format!("unknown scene phase {s:?} (active, inactive, background)"))
    }
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
        assert_eq!(serde_json::from_str::<ViewNode>(&serde_json::to_string(&root).unwrap()).unwrap(), *root);
        assert_eq!(serde_json::to_string(&HostMessage::Shutdown).unwrap(), r#"{"type":"shutdown"}"#);
        for (m, j) in [
            (
                HostMessage::Gesture(GestureTouch {
                    phase: TouchPhase::Move,
                    time: 16.0,
                    points: vec![[1.0, 2.0]],
                    targets: vec![GestureTarget { key: "0/1".into(), frame: [0.0, 0.0, 10.0, 20.0] }],
                }),
                r#"{"type":"gesture","phase":"move","time":16.0,"points":[[1.0,2.0]],"targets":[{"key":"0/1","frame":[0.0,0.0,10.0,20.0]}]}"#,
            ),
            (
                HostMessage::Back(BackEvent { phase: BackPhase::Progress, progress: 0.25, edge: SwipeEdge::Left, x: 100.0, y: 300.0, time: 32.0 }),
                r#"{"type":"back","phase":"progress","progress":0.25,"edge":"left","x":100.0,"y":300.0,"time":32.0}"#,
            ),
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
        // Scroll-view touch arbitration (React Native).
        let g = |j: &str| serde_json::from_str::<GuestMessage>(j).unwrap();
        assert_eq!(g(r#"{"type":"js_responder","tag":9}"#), GuestMessage::JsResponder { tag: Some(9) });
        assert_eq!(g(r#"{"type":"js_responder","tag":null}"#), GuestMessage::JsResponder { tag: None });
        assert_eq!(g(r#"{"type":"touch_answer","ask":3,"captured":true}"#), GuestMessage::TouchAnswer { ask: 3, captured: true });
        assert_eq!(serde_json::to_string(&HostMessage::TouchAsk { ask: 3 }).unwrap(), r#"{"type":"touch_ask","ask":3}"#);
        for (phase, name) in [(ScrollPhase::Release, "release"), (ScrollPhase::MomentumEnd, "momentum_end")] {
            assert_eq!(serde_json::to_value(phase).unwrap(), name);
        }
        let g: GuestMessage = serde_json::from_str(r#"{"type":"scroll_to","tag":3,"x":0,"y":"end"}"#).unwrap();
        let GuestMessage::ScrollTo { tag: 3, x, y } = g else { panic!() };
        assert_eq!((x.resolve(10.0), y.resolve(10.0)), (0.0, 10.0));
        let pb: GuestMessage = serde_json::from_str(r#"{"type":"pasteboard","items":["a"]}"#).unwrap();
        assert_eq!(pb, GuestMessage::Pasteboard { items: vec!["a".into()] });
        assert_eq!(serde_json::to_string(&HostMessage::Pasteboard { items: vec!["a".into()] }).unwrap(), r#"{"type":"pasteboard","items":["a"]}"#);
        let sid = GuestMessage::ScrollToId { scroll: "0".into(), target: "row".into(), anchor: Some([0.5, 0.0]) };
        let sj = serde_json::to_string(&sid).unwrap();
        assert_eq!(sj, r#"{"type":"scroll_to_id","scroll":"0","target":"row","anchor":[0.5,0.0]}"#);
        assert_eq!(serde_json::from_str::<GuestMessage>(&sj).unwrap(), sid);
        let geo = HostMessage::Geometry {
            frames: [("g".into(), GeoBox { width: 100.0, height: 80.0, x: 16.0, y: 62.0, safe_top: 62.0, safe_bottom: 34.0, safe_leading: 0.0, safe_trailing: 0.0 })].into(),
        };
        let gj = serde_json::to_string(&geo).unwrap();
        assert_eq!(gj, r#"{"type":"geometry","frames":{"g":{"width":100.0,"height":80.0,"x":16.0,"y":62.0,"safeTop":62.0,"safeBottom":34.0,"safeLeading":0.0,"safeTrailing":0.0}}}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&gj).unwrap(), geo);
        let sp = HostMessage::ScrollPosition { scroll: "0".into(), target: Some("row".into()) };
        let pj = serde_json::to_string(&sp).unwrap();
        assert_eq!(pj, r#"{"type":"scroll_position","scroll":"0","target":"row"}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&pj).unwrap(), sp);
        assert_eq!(
            serde_json::from_str::<HostMessage>(r#"{"type":"scroll_position","scroll":"0","target":null}"#).unwrap(),
            HostMessage::ScrollPosition { scroll: "0".into(), target: None }
        );
        assert_eq!(
            serde_json::to_string(&HostMessage::TextInput { field: 4, version: 2, text: "hi".into() }).unwrap(),
            r#"{"type":"text_input","field":4,"version":2,"text":"hi"}"#
        );
        let sch = GuestMessage::Schedule { after_ms: Some(16.0) };
        let schj = serde_json::to_string(&sch).unwrap();
        assert_eq!(schj, r#"{"type":"schedule","afterMs":16.0}"#);
        assert_eq!(serde_json::from_str::<GuestMessage>(&schj).unwrap(), sch);
        assert_eq!(serde_json::from_str::<GuestMessage>(r#"{"type":"schedule"}"#).unwrap(), GuestMessage::Schedule { after_ms: None });
        let tk = HostMessage::Tick { time: 250.5 };
        let tkj = serde_json::to_string(&tk).unwrap();
        assert_eq!(tkj, r#"{"type":"tick","time":250.5}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&tkj).unwrap(), tk);
    }

    #[test]
    fn scene_phase_and_open_url_messages() {
        let m = HostMessage::ScenePhase { phase: ScenePhase::Background };
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(j, r#"{"type":"scene_phase","phase":"background"}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&j).unwrap(), m);
        let u = HostMessage::OpenUrl { url: "app://item/7?q=ü".into() };
        let uj = serde_json::to_string(&u).unwrap();
        assert_eq!(uj, r#"{"type":"open_url","url":"app://item/7?q=ü"}"#);
        assert_eq!(serde_json::from_str::<HostMessage>(&uj).unwrap(), u);
        assert!(serde_json::from_str::<HostMessage>(r#"{"type":"scene_phase","phase":"asleep"}"#).is_err());
    }

    #[test]
    fn scene_phase_steps_pass_through_inactive() {
        use ScenePhase::*;
        assert_eq!(Active.steps_to(Background), vec![Inactive, Background]);
        assert_eq!(Background.steps_to(Active), vec![Inactive, Active]);
        assert_eq!(Active.steps_to(Inactive), vec![Inactive]);
        assert_eq!(Inactive.steps_to(Background), vec![Background]);
        assert_eq!(Inactive.steps_to(Active), vec![Active]);
        assert_eq!(Background.steps_to(Inactive), vec![Inactive]);
        for p in [Active, Inactive, Background] {
            assert!(p.steps_to(p).is_empty());
        }
        assert_eq!(ScenePhase::default(), Active);
        assert_eq!(Background.as_str(), "background");
        for p in ScenePhase::ALL {
            assert_eq!(p.as_str().parse::<ScenePhase>(), Ok(p));
            assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{}\"", p.as_str()));
        }
        assert!("Active".parse::<ScenePhase>().is_err() && "".parse::<ScenePhase>().is_err());
    }

    #[test]
    fn request_and_response_messages() {
        let r: GuestMessage = serde_json::from_str(r#"{"type":"request","id":7,"method":"image.blurhash","params":{"path":"/x.png","x":4,"y":3}}"#).unwrap();
        assert!(matches!(&r, GuestMessage::Request { id: 7, method, params } if method == "image.blurhash" && params["x"] == 4));
        let ok = serde_json::to_value(HostMessage::Response { id: 7, result: Some(serde_json::json!("LEHV6n")), error: None }).unwrap();
        assert_eq!(ok, serde_json::json!({"type": "response", "id": 7, "result": "LEHV6n"}));
        let bad = serde_json::to_value(HostMessage::Response { id: 8, result: None, error: Some("no".into()) }).unwrap();
        assert_eq!(bad, serde_json::json!({"type": "response", "id": 8, "error": "no"}));
    }

}
