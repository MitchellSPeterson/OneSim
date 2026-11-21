use serde::{Deserialize, Serialize};

use super::one;

/// Animation timing curve (section P).
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

/// Wire description of a SwiftUI `Animation` (section P).
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

/// Visual state at the active (inserted/removed) end of a `.transition` (section P).
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

/// `.transition(_:)` (section P).
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

/// `.matchedGeometryEffect(id:in:properties:anchor:isSource:)` (section P).
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

/// `.contentTransition(_:)` (section P).
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
