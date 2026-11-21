use crate::symbols::SymbolSpec;
use crate::text::{FontSpec, Span};
use onesim_protocol::{Appearance, UiFrame, UiNode};

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

pub use onesim_display::Color;

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
    Slider,
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
            Role::Slider => "slider",
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
    /// Styled runs of a concatenated or nested `Text` (then `text` is their concatenation); empty otherwise.
    pub spans: Vec<Span>,
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
    /// Android status-bar wifi (two stroked arcs over a dot), drawn in a 16x11.8 dp box.
    AndroidWifi,
    SignalDots,
    Battery,
    /// iOS 27 status bar icons: thicker wifi arcs, a solid battery with a round nub.
    Wifi27,
    Battery27,
    SignalDots27,
    /// iPhone Duo status column: battery as a green ring (open at the bottom, bolt in a gap at the top)
    /// and the four signal dots on an arc below it.
    BatteryRing,
    DotsArc,
    // Software keyboard icons, drawn by `chrome`.
    Shift,
    ShiftFilled,
    Backspace,
    Return,
    Emoji,
    Mic,
    /// Popover arrow (filled triangle) pointing down / up.
    ArrowDown,
    ArrowUp,
    ArrowLeft,
    /// Activity indicator (static frame).
    Spinner,
    /// Large activity indicator / refresh control: thicker spokes.
    SpinnerLarge,
    /// Android circular activity indicator in a settled frame.
    AndroidSpinner,
    /// Stroked arc with round caps, centred in the box and touching its edges: degrees clockwise from the top,
    /// stroke width in hundredths of a point (SwiftUI `Gauge`).
    Arc { from: i16, to: i16, width: u16 },
}

/// An SF Symbol painted as an alpha mask in `color`, `size` points, centred in the node's bounds
/// (cropped to them, for scaledToFill).
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolRun {
    pub spec: SymbolSpec,
    pub color: Color,
    pub size: (f64, f64),
}

/// A decoded bitmap drawn into `dest` (points, absolute), clipped to the node's bounds and corner radius.
#[derive(Debug, Clone)]
pub struct ImageRun {
    pub pixmap: std::sync::Arc<tiny_skia::Pixmap>,
    pub dest: Rect,
    /// Clockwise rotation in degrees about the centre of `dest`.
    pub rotation: f32,
    /// 0 nearest, 1 bilinear, 2 bicubic.
    pub filter: u8,
}

/// Linear gradient fill over a node's bounds: CSS `angle` (0 = up, 90 = right), evenly spaced stops.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub angle: f32,
    pub stops: Vec<Color>,
    /// Start and end points `[x0, y0, x1, y1]` as fractions of the bounds (CAGradientLayer / expo-linear-gradient), replacing `angle`.
    pub points: Option<[f32; 4]>,
    /// Stop positions 0...1, one per stop (default: evenly spaced).
    pub locations: Option<Vec<f32>>,
}

/// A sheet the user can drag (`Node::sheet`).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetInfo {
    /// Top edge (y, points) of each detent, ascending by detent height, so descending in y.
    pub tops: Vec<f64>,
    /// Index into `tops` the sheet rests at (or is dragged from).
    pub detent: usize,
    /// Current top edge, including a drag in progress.
    pub top: f64,
    /// Guest action that dismisses it; `None` = `interactiveDismissDisabled`.
    pub dismiss: Option<u64>,
}

/// Interaction state of one sheet layer (`Interaction::sheets`, keyed by layer id).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SheetPos {
    pub detent: usize,
    /// Top edge while a finger drags it.
    pub top: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollInfo {
    pub vertical: bool,
    pub offset: (f64, f64),
    pub max: (f64, f64),
    /// Vertical offset where a large navigation title has fully collapsed into the inline bar.
    /// Scrolling never comes to rest between 0 and this (iOS snaps to expanded or collapsed).
    pub snap: Option<f64>,
    /// Real content size when the layout knows it (React Native); else viewport + `max`.
    pub content: Option<(f64, f64)>,
    /// Page length on the main axis when `.scrollTargetBehavior(.paging)`.
    pub page: Option<f64>,
    /// `.scrollTargetBehavior(.viewAligned)`.
    pub aligned: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Pager {
    pub selected: usize,
    pub actions: Vec<Option<u64>>,
}

/// SwiftUI animation / transition metadata attached to a scene node (protocol section P).
/// Boxed on `Node` so non-animated nodes pay only a null pointer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeAnim {
    pub animation: Option<onesim_protocol::AnimationSpec>,
    pub transition: Option<onesim_protocol::TransitionSpec>,
    pub matched: Option<onesim_protocol::MatchedGeometryMod>,
    pub content_transition: onesim_protocol::ContentTransitionKind,
}

/// Fill configuration for a [`VectorShape`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorFill {
    pub color: Color,
    pub eo_fill: bool,
    pub antialiased: bool,
}

/// Evaluation state of a custom SwiftUI `Shape` whose `path(in:)` runs on the guest for its laid-out bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CustomShapeEval {
    Pending,
    Evaluated([f64; 2]),
}

/// Stroke configuration for a [`VectorShape`].
#[derive(Debug, Clone, PartialEq)]
pub struct VectorStroke {
    pub color: Color,
    pub line_width: f64,
    pub line_cap: onesim_protocol::LineCap,
    pub line_join: onesim_protocol::LineJoin,
    pub miter_limit: f64,
    pub dash: Vec<f64>,
    pub dash_phase: f64,
    pub antialiased: bool,
    pub inset: f64,
}

/// Geometry of a [`VectorShape`], in the node's bounds.
#[derive(Debug, Clone, PartialEq)]
pub enum VectorGeom {
    Ellipse,
    RoundedRect {
        /// `[top_left, top_right, bottom_right, bottom_left]` each `(rx, ry)` in points.
        radii: [(f64, f64); 4],
        continuous: bool,
    },
    Path {
        ops: Vec<onesim_protocol::PathOp>,
        mirror_x: bool,
    },
    /// Resolved after layout flattening from the nearest ancestor's `container_shape`.
    ContainerRelative,
    /// Resolved after layout flattening from the nearest ancestor's `container_shape`.
    Concentric(onesim_protocol::ConcentricCorners),
}

/// Rich vector shape (ellipse, uneven/continuous rounded rect, custom path, styled fill/stroke).
#[derive(Debug, Clone, PartialEq)]
pub struct VectorShape {
    pub geom: VectorGeom,
    pub fill: Option<VectorFill>,
    pub stroke: Option<VectorStroke>,
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
    /// Rich vector shape (boxed to keep `Node` small on the stack during layout recursion).
    pub vector: Option<Box<VectorShape>>,
    /// `.containerShape(_:)` carried by a layout-only container node for descendant relative/concentric shapes.
    pub container_shape: Option<Box<onesim_protocol::ContainerShapeSpec>>,
    /// Custom SwiftUI `Shape` whose `path(in:)` is evaluated by the guest for its laid-out size.
    pub custom_shape: Option<CustomShapeEval>,
    pub shadow: Option<Shadow>,
    /// Liquid Glass backdrop effect painted before this node's own fill and content.
    pub glass: Option<crate::glass::GlassSpec>,
    /// iOS 26 scroll-edge effect over `bounds` (progressive blur + fade of content scrolled under bars).
    pub edge: Option<crate::glass::ScrollEdge>,
    pub glyph: Option<(Glyph, Color)>,
    pub text: Option<TextRun>,
    pub symbol: Option<SymbolRun>,
    pub image: Option<ImageRun>,
    /// Painted over `fill`, inside the corner radius.
    pub gradient: Option<Gradient>,
    pub(crate) fill_paint: Option<std::sync::Arc<crate::paint::Gradient>>,
    pub(crate) stroke_paint: Option<std::sync::Arc<crate::paint::Gradient>>,
    pub(crate) content_paint: Option<std::sync::Arc<crate::paint::Gradient>>,
    pub(crate) composite: Option<Box<crate::effects::Composite>>,
    pub(crate) decoration: bool,
    pub(crate) visual_frame: Option<[f64; 4]>,
    /// Clips all descendants to `bounds` with this corner radius.
    pub clip_radius: Option<f64>,
    /// Effective opacity (already multiplied by ancestors).
    pub opacity: f32,
    pub role: Role,
    pub label: Option<String>,
    pub action: Option<u64>,
    /// React Native reactTag (the node is a hit-test target and a layout-frame source); SwiftUI nodes have none.
    pub tag: Option<u64>,
    /// Text field id: tapping focuses the node.
    pub field: Option<u64>,
    pub input: Option<Box<onesim_protocol::TextInputOptions>>,
    pub secure: bool,
    pub value: Option<String>,
    /// Slider: (min, max, thumb width); a tap or drag at x sets the value through `field`.
    pub range: Option<(f64, f64, f64)>,
    pub focused: bool,
    pub disabled: bool,
    /// Caret x offset inside bounds (focused text field).
    pub caret: Option<f64>,
    pub scroll: Option<ScrollInfo>,
    pub(crate) pager: Option<Box<Pager>>,
    pub sheet: Option<SheetInfo>,
    /// SwiftUI gestures on this view (protocol section L). Boxed: nodes live on the stack in deep layout recursion.
    pub gesture: Option<Box<onesim_protocol::GestureMod>>,
    /// `.id` value, used by `scrollTo`.
    pub anchor: Option<String>,
    /// Size a `GeometryReader` built its child with, `[w, h]`.
    pub proposal: Option<[f64; 2]>,
    /// `"header"` when this view is a pinned section header.
    pub pinned: Option<String>,
    /// Direct child of `.scrollTargetLayout()`.
    pub scroll_target: bool,
    /// A scroller the user cannot scroll (React Native `scrollEnabled={false}`): `scroll_target` skips it, so drags
    /// and the wheel go to the content or an outer scroller; programmatic offsets still apply.
    pub scroll_disabled: bool,
    /// Animation / transition / matchedGeometryEffect metadata (protocol section P).
    pub anim: Option<Box<NodeAnim>>,
    /// False for layout-only helper nodes (clip/opacity layers); hidden from inspection.
    pub inspect: bool,
    /// Vertical scale of `text` about the bounds' centre (rows of a date picker's wheel, section S); 1 = none.
    pub text_squash: f32,
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
            vector: None,
            container_shape: None,
            custom_shape: None,
            shadow: None,
            glass: None,
            edge: None,
            glyph: None,
            text: None,
            symbol: None,
            image: None,
            gradient: None,
            fill_paint: None,
            stroke_paint: None,
            content_paint: None,
            composite: None,
            decoration: false,
            visual_frame: None,
            clip_radius: None,
            opacity: 1.0,
            role: Role::None,
            label: None,
            action: None,
            tag: None,
            field: None,
            input: None,
            secure: false,
            value: None,
            range: None,
            focused: false,
            disabled: false,
            caret: None,
            scroll: None,
            pager: None,
            sheet: None,
            gesture: None,
            anchor: None,
            proposal: None,
            pinned: None,
            scroll_target: false,
            scroll_disabled: false,
            anim: None,
            inspect: true,
            text_squash: 1.0,
        }
    }

    /// Strip interactive and inspection metadata (used for decoration layers and animation ghosts).
    pub fn strip_interactive(&mut self) {
        self.inspect = false;
        self.action = None;
        self.field = None;
        self.tag = None;
        self.gesture = None;
        self.scroll = None;
        self.sheet = None;
        self.role = Role::None;
    }
}

/// Nodes are in paint order (parents before children, backgrounds before content).
#[derive(Debug, Clone)]
pub struct Scene {
    pub size: (f64, f64),
    pub appearance: Appearance,
    /// iOS release whose look the rasterizer draws (glass).
    pub ios: onesim_protocol::IosVersion,
    pub nodes: Vec<Node>,
    /// Symbol names the host could not render (placeholders were drawn), sorted, deduplicated.
    pub unsupported_symbols: Vec<String>,
    /// Full host-side diagnostics (e.g. `unsupported_component: X`), sorted, deduplicated.
    pub notes: Vec<String>,
}

impl Scene {
    /// Topmost node with an action (or RN tag) under the point (points, respecting clips).
    pub fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        // Content scrolled under the navigation bar / status bar is not tappable there.
        // Under an alert or sheet only the presentation layer (and chrome drawn after it) is reachable.
        let modal = self.nodes.iter().rposition(|n| n.kind == "scrim");
        let popover = modal.and_then(|i| self.nodes.get(i + 1)).filter(|n| n.kind == "popover");
        let inside_popover = popover.is_some_and(|card| card.bounds.contains(x, y));
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
            // The scrim catches taps outside the card. Empty or disabled content inside is inert.
            if modal.is_some_and(|m| i < m || (i == m && inside_popover)) {
                return false;
            }
            if y < bar_bottom && modal.is_none() && under_bar(i) && n.kind != "keyboard" && n.role != Role::Key {
                return false;
            }
            (n.action.is_some() || n.tag.is_some() || n.field.is_some() || n.role == Role::Key) && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
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
        self.scroll_target_where(x, y, |_| true)
    }

    pub(crate) fn scroll_target_where(&self, x: f64, y: f64, matches: impl Fn(&Node) -> bool) -> Option<usize> {
        // Scrollers span the screen (content scrolls under the bars), but the status bar is inert.
        if self.nodes.iter().any(|n| n.kind == "navbar" && y < n.bounds.y) {
            return None;
        }
        let modal = self.nodes.iter().rposition(|n| n.kind == "scrim");
        (0..self.nodes.len()).rev().find(|&i| {
            let n = &self.nodes[i];
            modal.is_none_or(|m| i > m) && n.scroll.is_some() && !n.scroll_disabled && n.bounds.contains(x, y) && self.clipped_in(i, x, y) && matches(n)
        })
    }

    /// SwiftUI gesture views under a finger at (x, y), innermost first (protocol section L): the topmost one
    /// containing the point, then every ancestor with gestures (a parent's gesture covers its moved children too).
    /// Under an alert or sheet only the presentation layer is reachable.
    pub fn gesture_targets(&self, x: f64, y: f64) -> Vec<usize> {
        let modal = self.nodes.iter().rposition(|n| n.kind == "scrim");
        let Some(top) = (0..self.nodes.len()).rev().find(|&i| {
            let n = &self.nodes[i];
            modal.is_none_or(|m| i > m) && n.gesture.is_some() && n.bounds.contains(x, y) && self.clipped_in(i, x, y)
        }) else {
            return vec![];
        };
        let mut out = vec![top];
        let mut p = self.nodes[top].parent;
        while let Some(i) = p {
            if self.nodes[i].gesture.is_some() {
                out.push(i);
            }
            p = self.nodes[i].parent;
        }
        out
    }

    /// Custom SwiftUI shape nodes whose laid-out size differs from the size the guest last evaluated `path(in:)` for.
    pub fn pending_shape_frames(&self) -> std::collections::BTreeMap<String, [f64; 4]> {
        let mut out = std::collections::BTreeMap::new();
        for n in &self.nodes {
            if let Some(previous) = n.visual_frame {
                let actual = [n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h];
                if previous.into_iter().zip(actual).any(|(a, b)| (a - b).abs() >= 0.01) {
                    out.insert(n.id.clone(), actual);
                }
            }
            if let Some(eval) = n.custom_shape {
                let (w, h) = (n.bounds.w, n.bounds.h);
                let matches = matches!(eval, CustomShapeEval::Evaluated([ew, eh]) if (ew - w).abs() < 0.01 && (eh - h).abs() < 0.01);
                if !matches && ((w > 0.0 && h > 0.0) || n.kind == "canvas") {
                    out.insert(n.id.clone(), [n.bounds.x, n.bounds.y, w, h]);
                }
            }
        }
        out
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
            value: n.value.as_ref().map(|value| if n.secure { "\u{2022}".repeat(unicode_segmentation::UnicodeSegmentation::graphemes(value.as_str(), true).count()) } else { value.clone() }),
            focused: n.focused,
            disabled: n.disabled,
            children: kids[i].iter().map(|&k| self.ui_node(k, kids)).collect(),
        }
    }
}
