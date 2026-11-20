//! ViewNode schema (guest -> host).

use serde::{Deserialize, Serialize};

/// A Fabric view whose single child is a SwiftUI or Compose subtree.
pub const EXPO_UI_HOST: &str = "ViewManagerAdapter_ExpoUI_HostView";
/// A React Native subtree placed inside a SwiftUI or Compose layout.
pub const EXPO_UI_RN_HOST: &str = "ViewManagerAdapter_ExpoUI_RNHostView";

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
    /// `.offset(x:y:)`: draws (and hit-tests) the view moved by (x, y) points without changing its layout.
    Offset(Offset),
    /// The view carries SwiftUI gestures (section L).
    Gesture(GestureMod),
    /// `.containerShape(_:)`: records the container shape for descendant `ContainerRelativeShape` and `ConcentricRectangle` (section N).
    ContainerShape(ContainerShapeSpec),
    /// `.id(_:)`: names the view for `ScrollViewProxy.scrollTo` (section M).
    #[serde(rename = "id")]
    Identity(String),
    /// Pinned section header or footer inside a lazy stack (`"header"` / `"footer"`).
    Pin(String),
    /// `.gridCellColumns(_:)`.
    GridCellColumns(u32),
    /// `.contentMargins`: the inset already resolved onto each edge.
    ContentMargins(Edges),
    /// `.scrollIndicators(_:axes:)`.
    ScrollIndicators(ScrollIndicators),
    /// `.scrollTargetBehavior` (`.paging` or `.viewAligned`).
    ScrollTargetBehavior(ScrollSnap),
    /// `.scrollTargetLayout(isEnabled:)`.
    ScrollTargetLayout(bool),
    /// `.scrollPosition(id:)` is attached to this scroll view.
    ScrollPosition,
    /// `.animation` / `withAnimation` (section O). `None` disables animation for this subtree.
    Animation(Option<Box<AnimationSpec>>),
    /// `.transition` (section O): insertion and removal effects.
    Transition(Box<TransitionSpec>),
    /// `.matchedGeometryEffect` (section O).
    MatchedGeometry(Box<MatchedGeometryMod>),
    /// `.contentTransition` (section O), inherited by `text` and `image` descendants.
    ContentTransition(ContentTransitionKind),
}

/// Animation timing curve (section O).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationCurve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Cubic Bezier control points `[c1x, c1y, c2x, c2y]`.
    Bezier([f64; 4]),
    /// Damped harmonic spring parameters.
    Spring {
        #[serde(default = "one")]
        mass: f64,
        stiffness: f64,
        damping: f64,
        #[serde(default, rename = "initialVelocity")]
        initial_velocity: f64,
        #[serde(default = "default_spring_settling")]
        settling: f64,
    },
    /// Sampled `CustomAnimation` progress values in `0...1` over `duration`.
    Samples(Vec<f64>),
}

/// Finite repeat count or `"forever"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepeatCount {
    Count(u32),
    Forever(ForeverTag),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForeverTag {
    Forever,
}

fn default_anim_duration() -> f64 {
    0.35
}
fn default_spring_settling() -> f64 {
    0.5
}
fn default_true() -> bool {
    true
}

/// Wire description of a SwiftUI `Animation` (section O).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationSpec {
    pub curve: AnimationCurve,
    #[serde(default = "default_anim_duration")]
    pub duration: f64,
    #[serde(default)]
    pub delay: f64,
    #[serde(default = "one")]
    pub speed: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<RepeatCount>,
    #[serde(default = "default_true")]
    pub autoreverses: bool,
    /// Monotonic token bumped by the guest when `withAnimation` / `.animation(value:)` triggers.
    /// `0` = deprecated `.animation(_:)` (animate any property change).
    #[serde(default)]
    pub token: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionEdge {
    Top,
    Bottom,
    Leading,
    Trailing,
}

/// Visual state at the active (inserted/removed) end of a `.transition` (section O).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TransitionEffect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<TransitionEdge>,
}

/// `.transition(_:)` (section O).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TransitionSpec {
    #[serde(default)]
    pub insertion: TransitionEffect,
    #[serde(default)]
    pub removal: TransitionEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchedProps {
    #[default]
    Frame,
    Position,
    Size,
}

fn default_center_anchor() -> [f64; 2] {
    [0.5, 0.5]
}

/// `.matchedGeometryEffect(id:in:properties:anchor:isSource:)` (section O).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchedGeometryMod {
    pub id: String,
    pub namespace: String,
    #[serde(default)]
    pub properties: MatchedProps,
    #[serde(default = "default_center_anchor")]
    pub anchor: [f64; 2],
    #[serde(default = "default_true", rename = "isSource")]
    pub is_source: bool,
}

/// `.contentTransition(_:)` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentTransitionKind {
    #[default]
    Identity,
    Interpolate,
    Opacity,
    NumericTextUp,
    NumericTextDown,
    SymbolEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Offset {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

/// `.gesture` / `.simultaneousGesture` / `.highPriorityGesture` (section L). `key` names the view's gestures in
/// touch messages; `pans` is true when any of them follows a moving finger (drag, magnify, rotate), so a
/// scroller under it does not take the touch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureMod {
    pub key: String,
    #[serde(default)]
    pub pans: bool,
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

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn ten() -> f64 {
    10.0
}

fn is_ten(v: &f64) -> bool {
    (*v - 10.0).abs() < 1e-9
}

fn is_default_cap(c: &LineCap) -> bool {
    *c == LineCap::Butt
}

fn is_default_join(j: &LineJoin) -> bool {
    *j == LineJoin::Miter
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
    /// Menu of a menu-style `Picker`, anchored to it; `dismiss` closes it (tap outside).
    Menu {
        #[serde(default)]
        anchor: Option<String>,
        #[serde(default)]
        items: Vec<PickerOption>,
        #[serde(default)]
        dismiss: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PickerStyle {
    #[default]
    Menu,
    Segmented,
}

/// One `Picker` choice: its label text and the action that selects it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PickerOption {
    pub text: String,
    pub action: u64,
    /// Menu layers only: the selected item (drawn with a checkmark).
    #[serde(default, skip_serializing_if = "is_false")]
    pub checked: bool,
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
    Ellipse,
    UnevenRoundedRectangle,
    ContainerRelative,
    ConcentricRectangle,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoundedCornerStyle {
    Circular,
    #[default]
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillStyle {
    #[serde(rename = "eoFill", default, skip_serializing_if = "is_false")]
    pub eo_fill: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub antialiased: bool,
}

impl Default for FillStyle {
    fn default() -> Self {
        Self { eo_fill: false, antialiased: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Absent means inherit the current foreground color (`ForegroundStyle`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(rename = "lineWidth", default = "one")]
    pub line_width: f64,
    #[serde(rename = "lineCap", default, skip_serializing_if = "is_default_cap")]
    pub line_cap: LineCap,
    #[serde(rename = "lineJoin", default, skip_serializing_if = "is_default_join")]
    pub line_join: LineJoin,
    #[serde(rename = "miterLimit", default = "ten", skip_serializing_if = "is_ten")]
    pub miter_limit: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
    #[serde(rename = "dashPhase", default, skip_serializing_if = "is_zero")]
    pub dash_phase: f64,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub antialiased: bool,
    /// Inset distance for `InsettableShape.strokeBorder` (points, applied inside the bounds before stroking).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub inset: f64,
}

/// One vector path command in a shape's local coordinate space (section N).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathOp {
    M([f64; 2]),
    L([f64; 2]),
    Q([f64; 4]),
    C([f64; 6]),
    Z,
}

/// One corner specification for `ConcentricRectangle` (`Edge.Corner.Style`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CornerStyleSpec {
    Fixed(f64),
    Concentric(Option<f64>),
}

/// Which corners of a `ConcentricRectangle` share a uniform minimum concentric radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UniformCorners {
    All,
    Top,
    Bottom,
    Leading,
    Trailing,
    TopAndBottom,
    LeadingAndTrailing,
}

/// Four-corner configuration for `ConcentricRectangle`: `[topLeading, topTrailing, bottomTrailing, bottomLeading]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConcentricCorners {
    pub corners: [CornerStyleSpec; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uniform: Option<UniformCorners>,
}

/// Sizing hint from a custom `Shape.sizeThatFits(_:)` override.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeSizeHint {
    Square,
    Fixed([f64; 2]),
    Aspect(f64),
    Ideal([f64; 2]),
}

/// Shape carried by `.containerShape(_:)` for descendant `ContainerRelativeShape` and `ConcentricRectangle`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum ContainerShapeSpec {
    Rectangle,
    Circle,
    Capsule {
        #[serde(default, rename = "cornerStyle")]
        corner_style: RoundedCornerStyle,
    },
    Ellipse,
    RoundedRectangle {
        #[serde(default, rename = "cornerRadius", skip_serializing_if = "Option::is_none")]
        corner_radius: Option<f64>,
        #[serde(default, rename = "cornerSize", skip_serializing_if = "Option::is_none")]
        corner_size: Option<[f64; 2]>,
        #[serde(default, rename = "cornerStyle")]
        corner_style: RoundedCornerStyle,
    },
    UnevenRoundedRectangle {
        #[serde(rename = "cornerRadii")]
        corner_radii: [f64; 4],
        #[serde(default, rename = "cornerStyle")]
        corner_style: RoundedCornerStyle,
    },
    ConcentricRectangle {
        concentric: ConcentricCorners,
    },
    ContainerRelative,
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

/// Which section views a lazy stack pins (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinMode {
    Headers,
    Footers,
    Both,
}

/// `GridItem` size (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackSize {
    Fixed,
    Flexible,
    Adaptive,
}

/// One `GridItem`. `value` is the fixed size; `min`/`max` bound flexible and adaptive tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridTrack {
    pub size: TrackSize,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
}

/// One `GridRow`. `alignment` `None` inherits the grid's alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    #[serde(default)]
    pub cells: Vec<ViewNode>,
}

/// Axes `ViewThatFits` checks (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitAxes {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

/// Built-in scroll target behaviors (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrollSnap {
    Paging,
    ViewAligned,
}

/// `.scrollIndicators` per axis: `"automatic"`, `"visible"`, or `"hidden"`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScrollIndicators {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<String>,
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

/// A `tabview` tab bar item; running `action` selects the tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabItem {
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "systemImage", skip_serializing_if = "Option::is_none")]
    pub system_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    pub action: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    #[default]
    Large,
    Inline,
}

/// Extra fields for `ViewKind::Shape` (boxed so `ViewKind` stays small on the stack).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShapeExtra {
    #[serde(default, rename = "cornerRadius", skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    #[serde(default, rename = "cornerSize", skip_serializing_if = "Option::is_none")]
    pub corner_size: Option<[f64; 2]>,
    #[serde(default, rename = "cornerRadii", skip_serializing_if = "Option::is_none")]
    pub corner_radii: Option<[f64; 4]>,
    #[serde(default, rename = "cornerStyle", skip_serializing_if = "Option::is_none")]
    pub corner_style: Option<RoundedCornerStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concentric: Option<ConcentricCorners>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<PathOp>>,
    #[serde(default, rename = "pathSize", skip_serializing_if = "Option::is_none")]
    pub path_size: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub custom: bool,
    #[serde(default, rename = "sizeHint", skip_serializing_if = "Option::is_none")]
    pub size_hint: Option<ShapeSizeHint>,
    #[serde(default, rename = "mirrorRtl", skip_serializing_if = "is_false")]
    pub mirror_rtl: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, rename = "fillStyle", skip_serializing_if = "Option::is_none")]
    pub fill_style: Option<FillStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ViewKind {
    /// Jetpack Compose layout primitive (section K): `Box`, `Column`, `Row` or `Text`, modifier chain in `props.modifier`.
    Compose {
        component: String,
        #[serde(default)]
        props: serde_json::Value,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
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
        /// Lazy stack pinning. Absent on an ordinary stack.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pinned: Option<PinMode>,
    },
    HStack {
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pinned: Option<PinMode>,
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
        #[serde(flatten)]
        extra: Box<ShapeExtra>,
    },
    Divider,
    Scroll {
        #[serde(default)]
        axis: Axis,
        child: Box<ViewNode>,
        /// `"automatic"` or `"hidden"`. Absent means automatic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        indicators: Option<String>,
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
    /// `TabView`: `content` is the selected tab's (the others are not sent); `selected` indexes `tabs`.
    TabView {
        #[serde(default)]
        selected: usize,
        #[serde(default)]
        tabs: Vec<TabItem>,
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
        /// `SecureField`: drawn as bullets.
        #[serde(default, skip_serializing_if = "is_false")]
        secure: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        disabled: bool,
    },
    /// `Picker`; `selected` indexes `options`. Menu style: `open` shows the options as a `menu` layer.
    Picker {
        label: Box<ViewNode>,
        #[serde(default)]
        style: PickerStyle,
        #[serde(default)]
        options: Vec<PickerOption>,
        #[serde(default)]
        selected: Option<usize>,
        #[serde(default)]
        open: Option<u64>,
        #[serde(default)]
        disabled: bool,
    },
    /// `Slider`: the host sets a new value with `text_input` on `field` (the number as text).
    Slider {
        value: f64,
        #[serde(default)]
        min: f64,
        #[serde(default = "one")]
        max: f64,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        field: Option<u64>,
        #[serde(default)]
        disabled: bool,
    },
    /// `ProgressView`: `value` is the fraction done (0...1); `None` = indeterminate spinner.
    Progress {
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        label: Option<Box<ViewNode>>,
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
    /// `Grid` (section M). Does not stretch columns to the proposed width.
    Grid {
        #[serde(default)]
        alignment: Alignment,
        #[serde(default, rename = "horizontalSpacing", skip_serializing_if = "Option::is_none")]
        horizontal_spacing: Option<f64>,
        #[serde(default, rename = "verticalSpacing", skip_serializing_if = "Option::is_none")]
        vertical_spacing: Option<f64>,
        #[serde(default)]
        rows: Vec<GridLine>,
    },
    /// `LazyVGrid` (`axis: vertical`) or `LazyHGrid` (`axis: horizontal`).
    #[serde(rename = "lazyGrid")]
    LazyGrid {
        #[serde(default)]
        axis: Axis,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<f64>,
        #[serde(default)]
        tracks: Vec<GridTrack>,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    /// `GeometryReader`. `proposal` is the size the guest built the child with.
    Geometry {
        proposal: [f64; 2],
        child: Box<ViewNode>,
    },
    /// `ViewThatFits`. The host shows the first child that fits `axes`.
    #[serde(rename = "viewThatFits")]
    ViewThatFits {
        #[serde(default)]
        axes: FitAxes,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
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
        let g: ViewNode = serde_json::from_value(json!({"kind":"empty","id":"0","modifiers":[{"offset":{"x":3,"y":-4}},{"gesture":{"key":"0","pans":true}}]})).unwrap();
        assert_eq!(g.modifiers, vec![Modifier::Offset(Offset { x: 3.0, y: -4.0 }), Modifier::Gesture(GestureMod { key: "0".into(), pans: true })]);
        assert_eq!(n.modifiers[1], Modifier::Font(Font::Style { style: TextStyle::Footnote, weight: Some(Weight::Semibold) }));
        let ViewKind::Label { icon, .. } = &n.kind else { panic!() };
        assert!(matches!(&icon.kind, ViewKind::Image { resizable: true, content_mode: Some(ContentMode::Fit), scale: None, .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn section_m_round_trip() {
        let j = json!({
            "kind": "grid", "id": "g", "alignment": "leading",
            "horizontalSpacing": 8.0, "verticalSpacing": 4.0,
            "rows": [{"alignment": "top", "cells": [
                {"kind": "text", "id": "a", "text": "A", "modifiers": [
                    {"id": "cell-a"}, {"gridCellColumns": 2}, {"pin": "header"},
                    {"contentMargins": {"top": 1.0, "leading": 2.0, "bottom": 3.0, "trailing": 4.0}},
                    {"scrollIndicators": {"vertical": "hidden"}},
                    {"scrollTargetBehavior": "viewAligned"},
                    {"scrollTargetLayout": true},
                    {"scrollPosition": null}
                ]}
            ]}]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        let ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows } = &n.kind else { panic!() };
        assert_eq!((*alignment, *horizontal_spacing, *vertical_spacing, rows[0].alignment), (Alignment::Leading, Some(8.0), Some(4.0), Some(Alignment::Top)));
        assert_eq!(rows[0].cells[0].modifiers[0], Modifier::Identity("cell-a".into()));
        assert_eq!(rows[0].cells[0].modifiers[1], Modifier::GridCellColumns(2));
        assert_eq!(rows[0].cells[0].modifiers[2], Modifier::Pin("header".into()));
        assert_eq!(rows[0].cells[0].modifiers[5], Modifier::ScrollTargetBehavior(ScrollSnap::ViewAligned));
        assert_eq!(rows[0].cells[0].modifiers[6], Modifier::ScrollTargetLayout(true));
        assert_eq!(rows[0].cells[0].modifiers[7], Modifier::ScrollPosition);
        let lazy: ViewNode = serde_json::from_value(json!({
            "kind": "lazyGrid", "id": "lg", "axis": "horizontal", "spacing": 8.0,
            "tracks": [{"size": "flexible", "min": 10.0, "max": "inf"}, {"size": "fixed", "value": 40.0}, {"size": "adaptive", "min": 20.0}],
            "children": [{"kind": "empty", "id": "c"}]
        })).unwrap();
        let ViewKind::LazyGrid { axis, tracks, .. } = &lazy.kind else { panic!() };
        assert_eq!(*axis, Axis::Horizontal);
        assert_eq!(tracks[0].max, Some(f64::INFINITY));
        assert_eq!(tracks[1].size, TrackSize::Fixed);
        assert_eq!(serde_json::to_value(&lazy).unwrap()["tracks"][0]["max"], "inf");
        let geo: ViewNode = serde_json::from_value(json!({
            "kind": "geometry", "id": "geo", "proposal": [100.0, 80.0],
            "child": {"kind": "empty", "id": "geo/0"}
        })).unwrap();
        assert!(matches!(geo.kind, ViewKind::Geometry { proposal, .. } if proposal == [100.0, 80.0]));
        let fits: ViewNode = serde_json::from_value(json!({
            "kind": "viewThatFits", "id": "f", "axes": "horizontal",
            "children": [{"kind": "empty", "id": "f/0"}]
        })).unwrap();
        assert!(matches!(fits.kind, ViewKind::ViewThatFits { axes: FitAxes::Horizontal, .. }));
        let stack: ViewNode = serde_json::from_value(json!({
            "kind": "vstack", "id": "v", "pinned": "headers", "children": []
        })).unwrap();
        assert!(matches!(stack.kind, ViewKind::VStack { pinned: Some(PinMode::Headers), .. }));
        let scroll: ViewNode = serde_json::from_value(json!({
            "kind": "scroll", "id": "s", "axis": "horizontal", "indicators": "hidden",
            "child": {"kind": "empty", "id": "s/0"}
        })).unwrap();
        assert!(matches!(scroll.kind, ViewKind::Scroll { indicators: Some(ref ind), .. } if ind == "hidden"));
        for node in [&n, &lazy, &geo, &fits, &stack, &scroll] {
            let again: ViewNode = serde_json::from_value(serde_json::to_value(node).unwrap()).unwrap();
            assert_eq!(node, &again);
        }
        // An ordinary stack still parses with no pinned field.
        let plain: ViewNode = serde_json::from_str(r#"{"kind":"hstack","id":"h","children":[]}"#).unwrap();
        assert!(matches!(plain.kind, ViewKind::HStack { pinned: None, .. }));
    }

    #[test]
    fn section_n_shapes_and_paths_parse() {
        let j = json!({
            "kind": "vstack", "id": "0",
            "modifiers": [{"containerShape": {"shape": "roundedRectangle", "cornerRadius": 24.0, "cornerStyle": "continuous"}}],
            "children": [
                {"kind": "shape", "id": "0/0", "shape": "ellipse", "fill": {"semantic": "accent"}},
                {"kind": "shape", "id": "0/1", "shape": "unevenRoundedRectangle", "cornerRadii": [20.0, 4.0, 16.0, 8.0], "cornerStyle": "circular",
                 "stroke": {"color": {"semantic": "primary"}, "lineWidth": 3.0, "lineCap": "round", "lineJoin": "bevel", "miterLimit": 5.0, "dash": [8.0, 4.0], "dashPhase": 2.0, "antialiased": false, "inset": 1.5}},
                {"kind": "shape", "id": "0/2", "shape": "containerRelative"},
                {"kind": "shape", "id": "0/3", "shape": "concentricRectangle",
                 "concentric": {"corners": [{"concentric": null}, {"concentric": 6.0}, {"fixed": 12.0}, {"concentric": null}], "uniform": "top"}},
                {"kind": "shape", "id": "0/4", "shape": "path",
                 "path": [{"m": [0.0, 0.0]}, {"l": [40.0, 0.0]}, {"q": [50.0, 20.0, 40.0, 40.0]}, {"c": [30.0, 50.0, 10.0, 50.0, 0.0, 40.0]}, "z"],
                 "pathSize": [40.0, 40.0], "custom": true, "sizeHint": {"fixed": [40.0, 40.0]}, "mirrorRtl": true,
                 "fill": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0},
                 "fillStyle": {"eoFill": true, "antialiased": false}}
            ]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert!(matches!(n.modifiers[0], Modifier::ContainerShape(ContainerShapeSpec::RoundedRectangle { corner_radius: Some(24.0), .. })));
        let ViewKind::VStack { children, .. } = &n.kind else { panic!() };
        assert!(matches!(children[0].kind, ViewKind::Shape { shape: ShapeKind::Ellipse, .. }));
        let ViewKind::Shape { extra, .. } = &children[1].kind else { panic!() };
        let (Some(st), Some(radii)) = (&extra.stroke, &extra.corner_radii) else { panic!() };
        assert_eq!(*radii, [20.0, 4.0, 16.0, 8.0]);
        assert_eq!(st.line_cap, LineCap::Round);
        assert_eq!(st.line_join, LineJoin::Bevel);
        assert_eq!(st.dash, vec![8.0, 4.0]);
        assert_eq!(st.inset, 1.5);
        assert!(!st.antialiased);
        let ViewKind::Shape { extra: ex4, .. } = &children[4].kind else { panic!() };
        let (Some(ops), Some(fs), Some(ShapeSizeHint::Fixed(sz))) = (&ex4.path, &ex4.fill_style, &ex4.size_hint) else { panic!() };
        assert!(ex4.custom && ex4.mirror_rtl);
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[4], PathOp::Z);
        assert!(fs.eo_fill && !fs.antialiased);
        assert_eq!(*sz, [40.0, 40.0]);
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn section_o_round_trip() {
        let j = json!({
            "kind": "text", "id": "t", "text": "42",
            "modifiers": [
                {"animation": {"curve": {"spring": {"mass": 1.0, "stiffness": 157.9, "damping": 25.1, "initialVelocity": 0.0, "settling": 0.5}}, "duration": 0.5, "delay": 0.1, "speed": 1.5, "repeat": "forever", "autoreverses": false, "token": 7}},
                {"animation": null},
                {"transition": {"insertion": {"opacity": 0.0, "edge": "leading"}, "removal": {"scale": 0.5, "anchor": [0.5, 0.5], "offset": [10.0, -5.0]}}},
                {"matchedGeometry": {"id": "pill", "namespace": "ns1", "properties": "position", "anchor": [0.0, 0.5], "isSource": false}},
                {"contentTransition": "numericTextUp"}
            ]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers.len(), 5);
        assert!(matches!(&n.modifiers[0], Modifier::Animation(Some(spec)) if spec.token == 7 && spec.repeat == Some(RepeatCount::Forever(ForeverTag::Forever)) && !spec.autoreverses));
        assert_eq!(n.modifiers[1], Modifier::Animation(None));
        assert!(matches!(&n.modifiers[2], Modifier::Transition(t) if t.insertion.edge == Some(TransitionEdge::Leading) && t.removal.scale == Some(0.5)));
        assert!(matches!(&n.modifiers[3], Modifier::MatchedGeometry(m) if m.id == "pill" && m.namespace == "ns1" && m.properties == MatchedProps::Position && !m.is_source));
        assert_eq!(n.modifiers[4], Modifier::ContentTransition(ContentTransitionKind::NumericTextUp));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }
}
