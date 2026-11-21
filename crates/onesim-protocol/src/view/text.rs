use serde::{Deserialize, Serialize};

use super::color::Color;
use super::is_false;
use super::node::{Axis, ViewNode};

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
    Style {
        style: TextStyle,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<Weight>,
    },
    System {
        size: f64,
        #[serde(default)]
        weight: Weight,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        design: Option<FontDesign>,
    },
}

/// `Font.Design` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontDesign {
    Default,
    Serif,
    Rounded,
    Monospaced,
}

/// `Font.Width` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontWidth {
    Compressed,
    Condensed,
    Standard,
    Expanded,
}

/// `Text.Scale` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextScale {
    Default,
    Secondary,
}

/// `Text.TruncationMode` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Truncation {
    Head,
    #[default]
    Tail,
    Middle,
}

/// `Text.LineStyle.Pattern` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinePattern {
    #[default]
    Solid,
    Dot,
    Dash,
    DashDot,
    DashDotDot,
}

/// `.underline(pattern:color:)` / `.strikethrough(pattern:color:)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineStyle {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub pattern: LinePattern,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// `.lineLimit`. `min` reserves that many lines; `max` truncates. `None` as a modifier clears the limit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineLimit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reserves: bool,
}

/// `Text.Case` (section O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextCase {
    Uppercase,
    Lowercase,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kerning: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<f64>,
    #[serde(default, rename = "baselineOffset", skip_serializing_if = "Option::is_none")]
    pub baseline_offset: Option<f64>,
    #[serde(default, rename = "fontDesign", skip_serializing_if = "Option::is_none")]
    pub font_design: Option<FontDesign>,
    #[serde(default, rename = "fontWidth", skip_serializing_if = "Option::is_none")]
    pub font_width: Option<FontWidth>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub monospaced: bool,
    #[serde(default, rename = "monospacedDigit", skip_serializing_if = "is_false")]
    pub monospaced_digit: bool,
    #[serde(default, rename = "textScale", skip_serializing_if = "Option::is_none")]
    pub text_scale: Option<TextScale>,
    #[serde(default, rename = "underlinePattern", skip_serializing_if = "Option::is_none")]
    pub underline_pattern: Option<LinePattern>,
    #[serde(default, rename = "underlineColor", skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<Color>,
    #[serde(default, rename = "strikethroughPattern", skip_serializing_if = "Option::is_none")]
    pub strikethrough_pattern: Option<LinePattern>,
    #[serde(default, rename = "strikethroughColor", skip_serializing_if = "Option::is_none")]
    pub strikethrough_color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextInputStyle {
    Automatic,
    Plain,
    Bordered,
    RoundedBorder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextInputBorderShape {
    Automatic,
    Capsule,
    RoundedRectangle,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextInputOptions {
    #[serde(default, skip_serializing_if = "is_false")]
    pub editor: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_style: Option<TextInputStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_border_shape: Option<TextInputBorderShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<Axis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_action: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_action: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_action: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autocapitalization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autocorrection_disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Box<ViewNode>>,
}
