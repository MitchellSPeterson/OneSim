use crate::text::FontSpec;
use simcore_protocol::{Appearance, UiFrame, UiNode};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    None,
    Button,
    Text,
    Image,
    Toggle,
    TextField,
    Stepper,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::None => "none",
            Role::Button => "button",
            Role::Text => "text",
            Role::Image => "image",
            Role::Toggle => "toggle",
            Role::TextField => "textfield",
            Role::Stepper => "stepper",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    pub strike: bool,
    /// Horizontal shift of the text inside its bounds (scrolled text fields).
    pub offset_x: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub radius: f64,
    pub dx: f64,
    pub dy: f64,
}

/// Hand-drawn vector glyphs (icons) painted inside a node's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Chevron,
    BackChevron,
    Minus,
    Plus,
    Wifi,
    SignalDots,
    Battery,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollInfo {
    pub vertical: bool,
    pub offset: (f64, f64),
    pub max: (f64, f64),
}

/// One positioned element. `bounds` are absolute, in points.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: &'static str,
    pub parent: Option<usize>,
    pub bounds: Rect,
    pub fill: Option<Color>,
    /// Corner radius of `fill`/`stroke`/`shadow` (capsule/circle: a huge value, clamped on draw).
    pub radius: f64,
    /// Stroke centred on the node's outline.
    pub stroke: Option<(Color, f64)>,
    pub shadow: Option<Shadow>,
    pub glyph: Option<(Glyph, Color)>,
    pub text: Option<TextRun>,
    /// Clips all descendants to `bounds` with this corner radius.
    pub clip_radius: Option<f64>,
    /// Effective opacity (already multiplied by ancestors).
    pub opacity: f32,
    pub role: Role,
    pub label: Option<String>,
    pub action: Option<u64>,
    /// Text field id: tapping focuses the node.
    pub field: Option<u64>,
    pub value: Option<String>,
    pub focused: bool,
    pub disabled: bool,
    /// Caret x offset inside bounds (focused text field).
    pub caret: Option<f64>,
    pub scroll: Option<ScrollInfo>,
    /// False for layout-only helper nodes (clip/opacity layers); hidden from inspection.
    pub inspect: bool,
}

impl Node {
    pub fn new(id: impl Into<String>, kind: &'static str) -> Self {
        Self {
            id: id.into(),
            kind,
            parent: None,
            bounds: Rect::default(),
            fill: None,
            radius: 0.0,
            stroke: None,
            shadow: None,
            glyph: None,
            text: None,
            clip_radius: None,
            opacity: 1.0,
            role: Role::None,
            label: None,
            action: None,
            field: None,
            value: None,
            focused: false,
            disabled: false,
            caret: None,
            scroll: None,
            inspect: true,
        }
    }
}

/// Nodes are in paint order (parents before children, backgrounds before content).
#[derive(Debug, Clone)]
pub struct Scene {
    pub size: (f64, f64),
    pub appearance: Appearance,
    pub nodes: Vec<Node>,
}

impl Scene {
    /// Topmost node with an action under the point (points, respecting clips).
    pub fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        (0..self.nodes.len()).rev().find(|&i| {
            let n = &self.nodes[i];
            (n.action.is_some() || n.field.is_some()) && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
        })
    }

    fn clipped_in(&self, mut i: usize, x: f64, y: f64) -> bool {
        while let Some(p) = self.nodes[i].parent {
            let n = &self.nodes[p];
            if let Some(r) = n.clip_radius {
                let b = n.bounds;
                if !b.contains(x, y) {
                    return false;
                }
                let r = r.min(b.w / 2.0).min(b.h / 2.0);
                let cx = x.clamp(b.x + r, b.x + b.w - r);
                let cy = y.clamp(b.y + r, b.y + b.h - r);
                if (x - cx).powi(2) + (y - cy).powi(2) > r * r {
                    return false;
                }
            }
            i = p;
        }
        true
    }

    /// Innermost scroll container under the point.
    pub fn scroll_target(&self, x: f64, y: f64) -> Option<usize> {
        (0..self.nodes.len()).rev().find(|&i| {
            let n = &self.nodes[i];
            n.scroll.is_some() && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
        })
    }

    /// Inspection tree (protocol `UiNode`), skipping layout-only nodes.
    pub fn ui_tree(&self) -> UiNode {
        let mut kids: Vec<Vec<usize>> = vec![vec![]; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate().skip(1) {
            let mut p = n.parent;
            while let Some(pi) = p {
                if self.nodes[pi].inspect {
                    break;
                }
                p = self.nodes[pi].parent;
            }
            if n.inspect {
                kids[p.unwrap_or(0)].push(i);
            }
        }
        self.ui_node(0, &kids)
    }

    fn ui_node(&self, i: usize, kids: &[Vec<usize>]) -> UiNode {
        let n = &self.nodes[i];
        UiNode {
            id: n.id.clone(),
            kind: n.kind.to_string(),
            frame: UiFrame { x: n.bounds.x, y: n.bounds.y, width: n.bounds.w, height: n.bounds.h },
            text: n.text.as_ref().map(|t| t.text.clone()),
            role: Some(n.role.as_str().to_string()),
            label: n.label.clone(),
            action: n.action,
            value: n.value.clone(),
            focused: n.focused,
            disabled: n.disabled,
            children: kids[i].iter().map(|&k| self.ui_node(k, kids)).collect(),
        }
    }
}
