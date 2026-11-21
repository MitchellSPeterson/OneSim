use serde::{Deserialize, Serialize};

use crate::{ForegroundPalette, ShapePaint, VisualOp};
use super::animation::{AnimationSpec, ContentTransitionKind, MatchedGeometryMod, TransitionSpec};
use super::color::Color;
use super::inf;
use super::node::{Axis, ListRowMod, ViewNode};
use super::shape::ContainerShapeSpec;
use super::text::{
    Font, FontDesign, FontWidth, LineLimit, LineStyle, TextCase, TextScale, Truncation, Weight,
};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The pan follows only this axis (a list row's swipe): a finger moving mostly along the other axis goes to the
    /// scroller under it instead (section T).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<Axis>,
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

/// `SymbolRenderingMode` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolMode {
    Monochrome,
    Hierarchical,
    Palette,
    Multicolor,
}

/// `Image.TemplateRenderingMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TemplateMode {
    Template,
    Original,
}

/// `Image.Interpolation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Interpolation {
    None,
    Low,
    Medium,
    High,
}

/// `Image.DynamicRange`. Drawing stays standard range (no HDR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DynamicRange {
    Standard,
    ConstrainedHigh,
    High,
}

/// `SymbolVariableValueMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolVariable {
    Color,
    Draw,
}

/// `SymbolColorRenderingMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolColorMode {
    Flat,
    Gradient,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Padding(Edges),
    Frame(Box<Frame>),
    ForegroundColor(Color),
    ForegroundPaint(Box<ShapePaint>),
    ForegroundPalette(Box<ForegroundPalette>),
    BackgroundPaint(Box<ShapePaint>),
    Tint(Option<Box<ShapePaint>>),
    Effect(Box<VisualOp>),
    Mask(Overlay),
    InvertedMask(Overlay),
    AlignedBackground(Overlay),
    Background(Box<ViewNode>),
    CornerRadius(f64),
    Opacity(f64),
    Font(Font),
    FontWeight(Weight),
    Strikethrough(bool),
    MultilineTextAlignment(Alignment),
    Overlay(Overlay),
    Shadow(Box<Shadow>),
    NavigationTitle(String),
    ImageScale(ImageScale),
    Glass(Box<GlassMod>),
    /// `.offset(x:y:)`: draws (and hit-tests) the view moved by (x, y) points without changing its layout.
    Offset(Offset),
    /// The view carries SwiftUI gestures (section L).
    Gesture(GestureMod),
    /// `.containerShape(_:)`: records the container shape for descendant `ContainerRelativeShape` and `ConcentricRectangle` (section N).
    ContainerShape(Box<ContainerShapeSpec>),
    /// `.id(_:)`: names the view for `ScrollViewProxy.scrollTo` (section M).
    #[serde(rename = "id")]
    Identity(String),
    /// Pinned section header or footer inside a lazy stack (`"header"` / `"footer"`).
    Pin(String),
    /// `.gridCellColumns(_:)`.
    GridCellColumns(u32),
    /// Minimum height of the list row this view is (section S: a `ColorPicker` row is 58 pt). Read by `list`.
    RowMinHeight(f64),
    /// List row state (section T): swipe actions. Read by `list`.
    ListRow(Box<ListRowMod>),
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
    /// `.kerning(_:)` in points, added to the font's fitted tracking.
    Kerning(f64),
    /// `.tracking(_:)` in points, added to the font's fitted tracking.
    Tracking(f64),
    /// `.baselineOffset(_:)` in points. Positive lengthens the line downward. Glyphs stay at the top.
    BaselineOffset(f64),
    /// `.fontDesign(_:)`. `null` restores the default design.
    FontDesign(Option<FontDesign>),
    /// `.fontWidth(_:)`. `null` restores the standard width.
    FontWidth(Option<FontWidth>),
    /// `.monospaced(_:)`.
    Monospaced(bool),
    /// `.monospacedDigit()`.
    MonospacedDigit(bool),
    /// `.textScale(_:isEnabled:)`.
    TextScale(TextScale),
    /// `.truncationMode(_:)`.
    TruncationMode(Truncation),
    /// `.lineLimit`. `null` removes the limit.
    LineLimit(Option<LineLimit>),
    /// `.underline(isActive:pattern:color:)`.
    Underline(Box<LineStyle>),
    /// `.italic(_:)`.
    Italic(bool),
    /// `.textCase(_:)`. `null` restores the original case.
    TextCase(Option<TextCase>),
    /// `.allowsTightening(_:)`.
    AllowsTightening(bool),
    /// `.minimumScaleFactor(_:)`.
    MinimumScaleFactor(f64),
    /// `.symbolRenderingMode(_:)`.
    SymbolRenderingMode(Option<SymbolMode>),
    /// `.symbolVariableValueMode(_:)`.
    SymbolVariableValueMode(Option<SymbolVariable>),
    /// `.symbolColorRenderingMode(_:)`.
    SymbolColorRenderingMode(Option<SymbolColorMode>),
    /// `.allowedDynamicRange(_:)`.
    AllowedDynamicRange(Option<DynamicRange>),
    /// `.strikethrough(_:pattern:color:)`. The bool overload stays `strikethrough`.
    StrikethroughStyle(Box<LineStyle>),
    /// `.animation` / `withAnimation` (section P). `None` disables animation for this subtree.
    Animation(Option<Box<AnimationSpec>>),
    /// `.transition` (section P): insertion and removal effects.
    Transition(Box<TransitionSpec>),
    /// `.matchedGeometryEffect` (section P).
    MatchedGeometry(Box<MatchedGeometryMod>),
    /// `.contentTransition` (section P), inherited by `text` and `image` descendants.
    ContentTransition(ContentTransitionKind),
}
