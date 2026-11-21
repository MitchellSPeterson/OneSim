//! Accessibility modifier payload (protocol section U).
//!
//! Each `.accessibility*` call emits one partial object. The host merges them outermost-last.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A11yAction {
    pub name: String,
    pub action: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yAdjust {
    pub increment: u64,
    pub decrement: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct A11yScroll {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leading: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct A11yZoom {
    #[serde(rename = "in", default, skip_serializing_if = "Option::is_none")]
    pub zoom_in: Option<u64>,
    #[serde(rename = "out", default, skip_serializing_if = "Option::is_none")]
    pub zoom_out: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A11yCustom {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub importance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yRotorEntry {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yRotor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<A11yRotorEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yPair {
    pub role: String,
    pub id: String,
}

/// One `.accessibility*` attachment. Absent fields leave the merged record unchanged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessibilityMod {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_traits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_traits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_priority: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_labels: Vec<String>,
    /// `"ignore"`, `"contain"`, or `"combine"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<A11yAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustable: Option<A11yAdjust>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll: Option<A11yScroll>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<A11yZoom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<A11yPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<A11yPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop: Option<A11yPoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<A11yCustom>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotors: Vec<A11yRotor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotor_entry: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default_focus: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub technologies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responds: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignores_invert_colors: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_touch: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_touch_options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_content: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair: Option<A11yPair>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<String>,
}
