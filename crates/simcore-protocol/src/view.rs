//! ViewNode schema (guest -> host).

use serde::{Deserialize, Serialize};

/// Float that may be the string `"inf"` on the wire.
pub mod inf {
    use serde::{Deserialize, Deserializer, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        S(String),
    }

    pub fn serialize<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(x) if x.is_infinite() && *x > 0.0 => s.serialize_str("inf"),
            Some(x) => s.serialize_f64(*x),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        match Option::<Num>::deserialize(d)? {
            None => Ok(None),
            Some(Num::F(x)) => Ok(Some(x)),
            Some(Num::S(s)) if s == "inf" || s == "+inf" => Ok(Some(f64::INFINITY)),
            Some(Num::S(s)) if s == "-inf" => Ok(Some(f64::NEG_INFINITY)),
            Some(Num::S(s)) => Err(serde::de::Error::custom(format!("bad number string {s:?}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Semantic {
    Primary,
    Secondary,
    Tertiary,
    Accent,
    Background,
    Separator,
    GroupedBackground,
    SecondaryGroupedBackground,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    Semantic { semantic: Semantic },
    /// `dark`: optional rgb used instead of `r,g,b` in dark appearance (adaptive system colors).
    Rgba { r: f64, g: f64, b: f64, a: f64, #[serde(default, skip_serializing_if = "Option::is_none")] dark: Option<[f64; 3]> },
}

/// One enum covers stack and frame alignments (vstack: leading/center/trailing,
/// hstack: top/center/bottom, zstack/frame: all nine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    #[default]
    Center,
    TopLeading,
    Top,
    TopTrailing,
    Leading,
    Trailing,
    BottomLeading,
    Bottom,
    BottomTrailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Edges {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub leading: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub trailing: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub height: Option<f64>,
    #[serde(default, rename = "minWidth", skip_serializing_if = "Option::is_none", with = "inf")]
    pub min_width: Option<f64>,
    #[serde(default, rename = "maxWidth", skip_serializing_if = "Option::is_none", with = "inf")]
    pub max_width: Option<f64>,
    #[serde(default, rename = "minHeight", skip_serializing_if = "Option::is_none", with = "inf")]
    pub min_height: Option<f64>,
    #[serde(default, rename = "maxHeight", skip_serializing_if = "Option::is_none", with = "inf")]
    pub max_height: Option<f64>,
    #[serde(default)]
    pub alignment: Alignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Weight {
    UltraLight,
    Thin,
    Light,
    #[default]
    Regular,
    Medium,
    Semibold,
    Bold,
    Heavy,
    Black,
}

impl Weight {
    /// OpenType `wght` value.
    pub fn value(self) -> u16 {
        match self {
            Weight::UltraLight => 100,
            Weight::Thin => 200,
            Weight::Light => 300,
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::Semibold => 600,
            Weight::Bold => 700,
            Weight::Heavy => 800,
            Weight::Black => 900,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextStyle {
    LargeTitle,
    Title,
    Title2,
    Title3,
    Headline,
    Subheadline,
    Body,
    Callout,
    Footnote,
    Caption,
    Caption2,
}

impl TextStyle {
    /// Point size of this style at a Dynamic Type size (Apple's published iOS table; columns are
    /// xSmall, small, medium, large, xLarge, xxLarge, xxxLarge, accessibility1...5).
    pub fn size_at(self, d: crate::wire::DynamicTypeSize) -> f64 {
        let row: [u8; 12] = match self {
            TextStyle::LargeTitle => [31, 32, 33, 34, 36, 38, 40, 44, 48, 52, 56, 60],
            TextStyle::Title => [25, 26, 27, 28, 30, 32, 34, 38, 43, 48, 53, 58],
            TextStyle::Title2 => [19, 20, 21, 22, 24, 26, 28, 34, 39, 44, 50, 56],
            TextStyle::Title3 => [17, 18, 19, 20, 22, 24, 26, 31, 37, 43, 49, 55],
            TextStyle::Headline | TextStyle::Body => [14, 15, 16, 17, 19, 21, 23, 28, 33, 40, 47, 53],
            TextStyle::Callout => [13, 14, 15, 16, 18, 20, 22, 26, 32, 38, 44, 51],
            TextStyle::Subheadline => [12, 13, 14, 15, 17, 19, 21, 25, 30, 36, 42, 49],
            TextStyle::Footnote => [12, 12, 12, 13, 15, 17, 19, 23, 27, 33, 38, 44],
            TextStyle::Caption => [11, 11, 11, 12, 14, 16, 18, 22, 26, 32, 37, 43],
            TextStyle::Caption2 => [11, 11, 11, 11, 13, 15, 17, 20, 24, 29, 34, 40],
        };
        row[d.index()] as f64
    }

    /// (size, weight) at the default Dynamic Type size.
    pub fn spec(self) -> (f64, Weight) {
        match self {
            TextStyle::LargeTitle => (34.0, Weight::Regular),
            TextStyle::Title => (28.0, Weight::Regular),
            TextStyle::Title2 => (22.0, Weight::Regular),
            TextStyle::Title3 => (20.0, Weight::Regular),
            TextStyle::Headline => (17.0, Weight::Semibold),
            TextStyle::Subheadline => (15.0, Weight::Regular),
            TextStyle::Body => (17.0, Weight::Regular),
            TextStyle::Callout => (16.0, Weight::Regular),
            TextStyle::Footnote => (13.0, Weight::Regular),
            TextStyle::Caption => (12.0, Weight::Regular),
            TextStyle::Caption2 => (11.0, Weight::Regular),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Font {
    Style { style: TextStyle, #[serde(default, skip_serializing_if = "Option::is_none")] weight: Option<Weight> },
    System { size: f64, #[serde(default)] weight: Weight },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    #[serde(default = "default_shadow_color")]
    pub color: Color,
    #[serde(default = "default_shadow_radius")]
    pub radius: f64,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

fn default_shadow_color() -> Color {
    Color::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.33, dark: None }
}
fn default_shadow_radius() -> f64 {
    3.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    pub view: Box<ViewNode>,
    #[serde(default)]
    pub alignment: Alignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Padding(Edges),
    Frame(Frame),
    ForegroundColor(Color),
    Background(Box<ViewNode>),
    CornerRadius(f64),
    Opacity(f64),
    Font(Font),
    FontWeight(Weight),
    Strikethrough(bool),
    MultilineTextAlignment(Alignment),
    Overlay(Overlay),
    Shadow(Shadow),
    NavigationTitle(String),
    ImageScale(ImageScale),
    Glass(GlassMod),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlassVariant {
    #[default]
    Regular,
    Clear,
    Identity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlassShapeKind {
    #[default]
    Capsule,
    Circle,
    Rect,
    RoundedRectangle,
    Ellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct GlassShape {
    #[serde(default)]
    pub shape: GlassShapeKind,
    #[serde(default, rename = "cornerRadius", skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
}

/// `.glassEffect(_:in:)` (protocol section F).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct GlassMod {
    #[serde(default)]
    pub variant: GlassVariant,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Color>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub shape: GlassShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageScale {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentMode {
    Fit,
    Fill,
}

/// `ButtonRole`: `cancel` / `destructive` (alerts order and color buttons by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonRole {
    Cancel,
    Destructive,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// One styled run of a concatenated `Text` (`Text + Text`). Unset fields inherit from the environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<Font>,
    #[serde(default, rename = "fontWeight", skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<Weight>,
    #[serde(default, rename = "foregroundColor", skip_serializing_if = "Option::is_none")]
    pub foreground_color: Option<Color>,
}

/// `PresentationDetent`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Detent {
    Medium,
    Large,
    /// Fraction of the large detent's height.
    Fraction { value: f64 },
    /// Height in points.
    Height { value: f64 },
}

/// One presented layer, bottom to top in `presentation.layers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Layer {
    /// `.sheet`. `detents` ascending by height (empty = large only); the first is the initial one.
    Sheet {
        content: Box<ViewNode>,
        #[serde(default)]
        detents: Vec<Detent>,
        #[serde(default, rename = "dragIndicator")]
        drag_indicator: bool,
        /// Guest action that clears the binding (swipe-to-dismiss); `None` when dismissal is disabled.
        #[serde(default)]
        dismiss: Option<u64>,
    },
    /// `.fullScreenCover`.
    Cover { content: Box<ViewNode> },
    /// `.alert`; `actions` are `button` nodes in source order.
    Alert {
        title: String,
        #[serde(default)]
        message: Option<Box<ViewNode>>,
        #[serde(default)]
        actions: Vec<ViewNode>,
    },
    /// `.confirmationDialog`: a popover on iPhone, anchored to the view it is attached to.
    Dialog {
        #[serde(default)]
        title: String,
        #[serde(default, rename = "titleVisible")]
        title_visible: bool,
        #[serde(default)]
        message: Option<Box<ViewNode>>,
        #[serde(default)]
        actions: Vec<ViewNode>,
        /// Id of the node the popover points at.
        #[serde(default)]
        anchor: Option<String>,
        /// Guest action that clears the binding (tap outside the popover).
        #[serde(default)]
        dismiss: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonStyle {
    #[default]
    Automatic,
    Plain,
    Bordered,
    BorderedProminent,
    NavigationLink,
    Glass,
    GlassProminent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeKind {
    Rectangle,
    Circle,
    Capsule,
    RoundedRectangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    #[serde(rename = "lineWidth", default = "one")]
    pub line_width: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    #[default]
    InsetGrouped,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub rows: Vec<ViewNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackButton {
    pub action: u64,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    #[default]
    Large,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ViewKind {
    /// React Native Fabric node (`id` = reactTag). `props` is Fabric's flat attribute payload.
    Rn {
        component: String,
        #[serde(default)]
        props: serde_json::Value,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Text {
        text: String,
        /// Styled runs of a concatenation; `text` is their plain concatenation. Empty for a plain `Text`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        runs: Vec<Run>,
    },
    VStack {
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    HStack {
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    ZStack {
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Spacer {
        #[serde(default, rename = "minLength")]
        min_length: Option<f64>,
    },
    Button {
        action: u64,
        label: Box<ViewNode>,
        #[serde(default)]
        style: ButtonStyle,
        #[serde(default)]
        disabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<ButtonRole>,
    },
    /// Modal layer above the whole navigation tree; present only while an alert or sheet is shown.
    Presentation {
        content: Box<ViewNode>,
        #[serde(default)]
        layers: Vec<Layer>,
    },
    Shape {
        shape: ShapeKind,
        #[serde(default, rename = "cornerRadius")]
        corner_radius: Option<f64>,
        #[serde(default)]
        fill: Option<Color>,
        #[serde(default)]
        stroke: Option<Stroke>,
    },
    Divider,
    Scroll {
        #[serde(default)]
        axis: Axis,
        child: Box<ViewNode>,
    },
    List {
        #[serde(default)]
        style: ListStyle,
        #[serde(default)]
        sections: Vec<Section>,
    },
    Navigation {
        #[serde(default)]
        title: Option<String>,
        #[serde(default, rename = "displayMode")]
        display_mode: DisplayMode,
        #[serde(default)]
        back: Option<BackButton>,
        content: Box<ViewNode>,
    },
    Toggle {
        #[serde(rename = "isOn")]
        is_on: bool,
        action: u64,
        label: Box<ViewNode>,
        #[serde(default)]
        disabled: bool,
    },
    Stepper {
        label: Box<ViewNode>,
        #[serde(default)]
        decrement: Option<u64>,
        #[serde(default)]
        increment: Option<u64>,
    },
    #[serde(rename = "text_field")]
    TextField {
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        text: String,
        field: u64,
    },
    /// SF Symbol (host capability).
    Image {
        #[serde(rename = "systemName")]
        system_name: String,
        #[serde(default)]
        resizable: bool,
        #[serde(default, rename = "contentMode")]
        content_mode: Option<ContentMode>,
        #[serde(default)]
        scale: Option<ImageScale>,
    },
    Label {
        title: Box<ViewNode>,
        icon: Box<ViewNode>,
    },
    Color {
        color: Color,
    },
    Rectangle,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<Modifier>,
    #[serde(flatten)]
    pub kind: ViewKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counter_tree_round_trip() {
        let j = json!({
            "kind":"vstack","id":"0","spacing":12.0,"alignment":"center",
            "modifiers":[{"padding":{"top":16.0,"leading":16.0,"bottom":16.0,"trailing":16.0}},
                         {"frame":{"maxWidth":"inf","alignment":"center"}},
                         {"background":{"kind":"color","id":"0.bg","color":{"semantic":"background"}}},
                         {"cornerRadius":8.0},{"opacity":0.5},
                         {"foregroundColor":{"r":1.0,"g":0.0,"b":0.0,"a":1.0}}],
            "children":[
                {"kind":"text","id":"0.0","text":"Count: 1"},
                {"kind":"spacer","id":"0.1","minLength":null},
                {"kind":"button","id":"0.2","action":7,"label":{"kind":"text","id":"0.2.l","text":"Increment"}},
                {"kind":"rectangle","id":"0.3"},{"kind":"empty","id":"0.4"}]
        });
        let n: ViewNode = serde_json::from_value(j.clone()).unwrap();
        let ViewKind::VStack { children, .. } = &n.kind else { panic!() };
        assert_eq!(children.len(), 5);
        let Modifier::Frame(f) = &n.modifiers[1] else { panic!() };
        assert_eq!(f.max_width, Some(f64::INFINITY));
        assert_eq!(n.modifiers[5], Modifier::ForegroundColor(Color::Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0, dark: None }));
        // re-serialise and re-parse: stable
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
        assert_eq!(serde_json::to_value(&n).unwrap()["modifiers"][1]["frame"]["maxWidth"], "inf");
    }

    #[test]
    fn semantic_color() {
        let c: Color = serde_json::from_str(r#"{"semantic":"accent"}"#).unwrap();
        assert_eq!(c, Color::Semantic { semantic: Semantic::Accent });
    }

    #[test]
    fn section_d_nodes_parse() {
        let j = json!({"kind":"navigation","id":"n","title":"T","displayMode":"inline","back":{"action":9,"label":"B"},
          "modifiers":[{"font":{"style":"headline"}},{"font":{"size":22,"weight":"semibold"}},{"fontWeight":"bold"},
                       {"strikethrough":true},{"multilineTextAlignment":"center"},{"navigationTitle":"T"},
                       {"shadow":{"color":{"semantic":"separator"},"radius":6,"x":0,"y":2}},
                       {"overlay":{"view":{"kind":"shape","id":"o","shape":"circle","stroke":{"color":{"semantic":"tertiary"},"lineWidth":2}},"alignment":"topTrailing"}}],
          "content":{"kind":"list","id":"l","style":"insetGrouped","sections":[{"header":"H","rows":[
            {"kind":"toggle","id":"a","isOn":true,"action":1,"label":{"kind":"text","id":"a.l","text":"x"}},
            {"kind":"stepper","id":"b","label":{"kind":"text","id":"b.l","text":"y"},"decrement":null,"increment":3},
            {"kind":"text_field","id":"c","placeholder":"p","text":"","field":4},
            {"kind":"button","id":"d","action":5,"style":"borderedProminent","disabled":true,"label":{"kind":"divider","id":"d.l"}},
            {"kind":"scroll","id":"e","axis":"horizontal","child":{"kind":"shape","id":"e.c","shape":"roundedRectangle","cornerRadius":4,"fill":{"semantic":"groupedBackground"}}}]}]}});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers.len(), 8);
        assert_eq!(n.modifiers[1], Modifier::Font(Font::System { size: 22.0, weight: Weight::Semibold }));
        assert_eq!(n.modifiers[0], Modifier::Font(Font::Style { style: TextStyle::Headline, weight: None }));
        let ViewKind::Navigation { display_mode, back, content, .. } = &n.kind else { panic!() };
        assert_eq!(*display_mode, DisplayMode::Inline);
        assert_eq!(back.as_ref().unwrap().action, 9);
        let ViewKind::List { sections, .. } = &content.kind else { panic!() };
        assert!(matches!(sections[0].rows[2].kind, ViewKind::TextField { field: 4, .. }));
        assert!(matches!(sections[0].rows[3].kind, ViewKind::Button { style: ButtonStyle::BorderedProminent, disabled: true, .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
        assert_eq!(TextStyle::Body.spec(), (17.0, Weight::Regular));
        assert_eq!(Weight::Bold.value(), 700);
    }

    #[test]
    fn new_semantics_and_button_defaults() {
        let c: Color = serde_json::from_str(r#"{"semantic":"secondaryGroupedBackground"}"#).unwrap();
        assert_eq!(c, Color::Semantic { semantic: Semantic::SecondaryGroupedBackground });
        let b: ViewNode = serde_json::from_str(r#"{"kind":"button","id":"b","action":1,"label":{"kind":"empty","id":"e"}}"#).unwrap();
        assert!(matches!(b.kind, ViewKind::Button { style: ButtonStyle::Automatic, disabled: false, .. }));
    }

    #[test]
    fn runs_roles_and_presentation_parse() {
        let j = json!({"kind":"presentation","id":"p",
          "content":{"kind":"text","id":"t","text":"ab","runs":[{"text":"a","fontWeight":"bold","italic":true},{"text":"b","underline":true,"foregroundColor":{"semantic":"secondary"}}]},
          "layers":[
            {"kind":"sheet","content":{"kind":"empty","id":"s"},"detents":[{"kind":"medium"},{"kind":"fraction","value":0.8},{"kind":"large"}],"dragIndicator":true,"dismiss":4},
            {"kind":"alert","title":"T","message":{"kind":"text","id":"m","text":"M"},
             "actions":[{"kind":"button","id":"b","action":2,"role":"destructive","label":{"kind":"empty","id":"e"}}]},
            {"kind":"dialog","title":"D","titleVisible":true,"actions":[],"anchor":"x","dismiss":9},
            {"kind":"cover","content":{"kind":"empty","id":"c"}}]});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        let ViewKind::Presentation { content, layers } = &n.kind else { panic!() };
        let ViewKind::Text { runs, .. } = &content.kind else { panic!() };
        assert_eq!(runs[0].font_weight, Some(Weight::Bold));
        assert!(runs[0].italic && runs[1].underline && !runs[1].italic && runs[1].foreground_color.is_some());
        assert!(matches!(&layers[0], Layer::Sheet { detents, drag_indicator: true, dismiss: Some(4), .. } if detents[1] == Detent::Fraction { value: 0.8 }));
        let Layer::Alert { actions, .. } = &layers[1] else { panic!() };
        assert!(matches!(actions[0].kind, ViewKind::Button { role: Some(ButtonRole::Destructive), .. }));
        assert!(matches!(&layers[2], Layer::Dialog { title_visible: true, dismiss: Some(9), .. }) && matches!(&layers[3], Layer::Cover { .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn symbol_nodes_parse() {
        let j = json!({"kind":"label","id":"l","title":{"kind":"text","id":"l.t","text":"A"},
          "icon":{"kind":"image","id":"l.i","systemName":"star.fill","resizable":true,"contentMode":"fit"},
          "modifiers":[{"imageScale":"large"},{"font":{"style":"footnote","weight":"semibold"}}]});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers[0], Modifier::ImageScale(ImageScale::Large));
        assert_eq!(n.modifiers[1], Modifier::Font(Font::Style { style: TextStyle::Footnote, weight: Some(Weight::Semibold) }));
        let ViewKind::Label { icon, .. } = &n.kind else { panic!() };
        assert!(matches!(&icon.kind, ViewKind::Image { resizable: true, content_mode: Some(ContentMode::Fit), scale: None, .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }
}
