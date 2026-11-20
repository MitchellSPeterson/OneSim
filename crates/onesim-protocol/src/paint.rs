//! SwiftUI paint and effects (protocol section R).

use serde::{Deserialize, Serialize};

use crate::Color;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShapePaint {
    Color(Color),
    Gradient { gradient: Box<GradientPaint> },
    Style { style: StyleReference },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleReference {
    Foreground,
    Background,
    Tint,
    Primary,
    Secondary,
    Tertiary,
    Quaternary,
    Quinary,
    Fill,
    SecondaryFill,
    TertiaryFill,
    QuaternaryFill,
    QuinaryFill,
    Link,
    Placeholder,
    Selection,
    Separator,
    WindowBackground,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientColorSpace {
    #[default]
    Device,
    Perceptual,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub location: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeshPoint {
    pub position: [f64; 2],
    /// Leading, top, trailing and bottom controls, respectively.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<[[f64; 2]; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GradientPaint {
    Linear {
        stops: Vec<GradientStop>,
        #[serde(default)]
        color_space: GradientColorSpace,
        start: [f64; 2],
        end: [f64; 2],
        #[serde(default)]
        absolute: bool,
        #[serde(default)]
        options: u32,
    },
    Radial {
        stops: Vec<GradientStop>,
        #[serde(default)]
        color_space: GradientColorSpace,
        center: [f64; 2],
        #[serde(default)]
        start_center: Option<[f64; 2]>,
        start_radius: f64,
        end_radius: f64,
        #[serde(default)]
        absolute: bool,
        #[serde(default)]
        options: u32,
        #[serde(default)]
        elliptical: bool,
    },
    Angular {
        stops: Vec<GradientStop>,
        #[serde(default)]
        color_space: GradientColorSpace,
        center: [f64; 2],
        start_angle: f64,
        end_angle: f64,
        #[serde(default)]
        absolute: bool,
        #[serde(default)]
        options: u32,
    },
    Mesh {
        width: usize,
        height: usize,
        points: Vec<MeshPoint>,
        colors: Vec<Color>,
        background: Color,
        smooths_colors: bool,
        #[serde(default)]
        color_space: GradientColorSpace,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    SourceAtop,
    DestinationOver,
    DestinationOut,
    PlusDarker,
    PlusLighter,
    Clear,
    Copy,
    SourceIn,
    SourceOut,
    DestinationIn,
    DestinationAtop,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VisualOp {
    Affine {
        matrix: [f64; 6],
        anchor: [f64; 2],
    },
    Projection {
        matrix: [f64; 9],
    },
    Blur {
        radius: f64,
        opaque: bool,
    },
    Blend {
        mode: BlendMode,
    },
    Brightness {
        amount: f64,
    },
    Contrast {
        amount: f64,
    },
    Saturation {
        amount: f64,
    },
    Grayscale {
        amount: f64,
    },
    HueRotation {
        radians: f64,
    },
    ColorMultiply {
        color: Color,
    },
    ColorInvert {
        #[serde(default = "one")]
        amount: f64,
    },
    AlphaThreshold {
        min: f64,
        max: f64,
        color: Color,
    },
    LuminanceToAlpha,
    Opacity {
        amount: f64,
    },
    Group {
        opaque: bool,
    },
}

fn one() -> f64 {
    1.0
}
