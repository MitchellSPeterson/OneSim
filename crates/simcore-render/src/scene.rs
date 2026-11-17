use crate::symbols::SymbolSpec;
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
    /// Software keyboard key (chrome).
    Key,
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
            Role::Key => "key",
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
    // Software keyboard icons, drawn by `chrome`.
    Shift,
    ShiftFilled,
    Backspace,
    Return,
    Emoji,
    Mic,
}

/// An SF Symbol painted as an alpha mask in `color`, `size` points, centred in the node's bounds
/// (cropped to them, for scaledToFill).
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolRun {
    pub spec: SymbolSpec,
    pub color: Color,
    pub size: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollInfo {
    pub vertical: bool,
    pub offset: (f64, f64),
    pub max: (f64, f64),
    /// Vertical offset where a large navigation title has fully collapsed into the inline bar.
    /// Scrolling never comes to rest between 0 and this (iOS snaps to expanded or collapsed).
    pub snap: Option<f64>,
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
    /// Liquid Glass backdrop effect painted before this node's own fill and content.
    pub glass: Option<crate::glass::GlassSpec>,
    /// iOS 26 scroll-edge effect over `bounds` (progressive blur + fade of content scrolled under bars).
    pub edge: Option<crate::glass::ScrollEdge>,
    pub glyph: Option<(Glyph, Color)>,
    pub text: Option<TextRun>,
    pub symbol: Option<SymbolRun>,
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
            glass: None,
            edge: None,
            glyph: None,
            text: None,
            symbol: None,
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
    /// Symbol names the host could not render (placeholders were drawn), sorted, deduplicated.
    pub unsupported_symbols: Vec<String>,
}

impl Scene {
    /// Topmost node with an action under the point (points, respecting clips).
    pub fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        // Content scrolled under the navigation bar / status bar is not tappable there.
        let bar = self.nodes.iter().position(|n| n.kind == "navbar");
        let bar_bottom = bar.map_or(0.0, |b| self.nodes[b].bounds.y + self.nodes[b].bounds.h);
        let under_bar = |mut i: usize| {
            while let Some(p) = self.nodes[i].parent {
                if Some(p) == bar {
                    return false;
                }
                i = p;
            }
            true
        };
        (0..self.nodes.len()).rev().find(|&i| {
            let n = &self.nodes[i];
            if y < bar_bottom && under_bar(i) && n.kind != "keyboard" && n.role != Role::Key {
                return false;
            }
            (n.action.is_some() || n.field.is_some() || n.role == Role::Key) && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
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
        // Scrollers span the screen (content scrolls under the bars), but the status bar is inert.
        if self.nodes.iter().any(|n| n.kind == "navbar" && y < n.bounds.y) {
            return None;
        }
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
