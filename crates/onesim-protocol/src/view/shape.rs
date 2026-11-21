use serde::{Deserialize, Serialize};

use crate::ShapePaint;
use super::{is_false, is_ten, is_true, is_zero, one, ten, yes};

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

fn is_default_cap(c: &LineCap) -> bool {
    *c == LineCap::Butt
}

fn is_default_join(j: &LineJoin) -> bool {
    *j == LineJoin::Miter
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
    /// Absent means inherit the current foreground style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ShapePaint>,
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
    pub fill: Option<ShapePaint>,
    #[serde(default, rename = "fillStyle", skip_serializing_if = "Option::is_none")]
    pub fill_style: Option<FillStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
}
