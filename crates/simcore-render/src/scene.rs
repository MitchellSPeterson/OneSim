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
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::None => "none",
            Role::Button => "button",
            Role::Text => "text",
            Role::Image => "image",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub size: f32,
    pub line_height: f32,
    pub color: Color,
}

/// One positioned element. `bounds` are absolute, in points.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: &'static str,
    pub parent: Option<usize>,
    pub bounds: Rect,
    pub fill: Option<Color>,
    pub text: Option<TextRun>,
    /// Clips all descendants to `bounds` with this corner radius.
    pub clip_radius: Option<f64>,
    /// Effective opacity (already multiplied by ancestors).
    pub opacity: f32,
    pub role: Role,
    pub label: Option<String>,
    pub action: Option<u64>,
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
            text: None,
            clip_radius: None,
            opacity: 1.0,
            role: Role::None,
            label: None,
            action: None,
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
            n.action.is_some() && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
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
            children: kids[i].iter().map(|&k| self.ui_node(k, kids)).collect(),
        }
    }
}
