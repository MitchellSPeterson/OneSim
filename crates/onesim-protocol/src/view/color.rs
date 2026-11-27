use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    Hierarchical { semantic: Semantic, hierarchical: bool },
    Semantic {
        semantic: Semantic,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        a: Option<f64>,
    },
    /// `dark`: optional rgb used instead of `r,g,b` in dark appearance (adaptive system colors).
    Rgba {
        r: f64,
        g: f64,
        b: f64,
        a: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dark: Option<Box<[f64; 3]>>,
    },
}
