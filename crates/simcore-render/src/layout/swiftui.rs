//! SwiftUI-semantics layout: parent proposes, child chooses, parent places.
//!
//! Stacks probe each child at 0 and infinity before the final proposal; `Ctx::memo`
//! caches results per (node, proposal) so nesting stays polynomial instead of 3^depth.
//!
//! Visual constants (row height, card radius, nav bar heights, control sizes) are measured from
//! an iOS 26 / iPhone 17 Pro simulator, see `tests/fixtures/showcase-ios/README.md`.

use crate::scene::{Color, Glyph, Node, Rect, Role, Scene, ScrollInfo, Shadow, SymbolRun, TextRun};
use crate::symbols::{self, SymbolSpec};
use crate::text::{FontSpec, Fonts, TextAlign};
use simcore_protocol::{
    Alignment, Appearance, ButtonStyle, DynamicTypeSize, GlassShapeKind, GlassVariant, ContentMode, DisplayMode, Font, ImageScale, ListStyle, Modifier, Section, Semantic, ShapeKind, Stroke,
    ViewKind, ViewNode,
};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

const INF: f64 = f64::INFINITY;
const DEFAULT_SPACING: f64 = 8.0;
/// Radius that always clamps to a capsule / circle.
const PILL: f64 = 1.0e6;

// iOS 26 measurements (points).
const ROW_MIN_H: f64 = 52.0;
const ROW_VPAD: f64 = 10.0;
/// Rows holding bordered buttons get extra vertical padding (measured: 34.3pt button -> 64.3pt row).
const ROW_VPAD_BORDERED: f64 = 15.0;
const ROW_HPAD: f64 = 16.0;
const CARD_MARGIN: f64 = 16.0;
const CARD_RADIUS: f64 = 26.0;
const SECTION_GAP: f64 = 17.7;
const HEADER_PAD: f64 = 20.0;
const NAV_INLINE_H: f64 = 54.0;
/// Scroll distance over which the inline title fades in as the large title finishes collapsing.
const INLINE_FADE: f64 = 12.0;
const SWITCH: (f64, f64) = (63.0, 28.0);
const STEPPER: (f64, f64) = (94.0, 32.0);
/// `Label` in a list row: icon centred in a leading column of this width, title starts after it.
/// Measured: icon centred 12pt in, title at +40 (column 24 + gap 16), icons at imageScale large.
const LABEL_ICON_COL: f64 = 24.0;
const LABEL_ROW_GAP: f64 = 16.0;
/// Icon-to-title gap of a `Label` outside a list (not measured).
const LABEL_GAP: f64 = 8.0;

#[derive(Clone, Copy)]
struct Prop {
    w: Option<f64>,
    h: Option<f64>,
}

#[derive(Clone, Copy, Default)]
struct Size {
    w: f64,
    h: f64,
}

/// Relative-positioned layout result. `node` (if any) becomes a scene node.
#[derive(Clone)]
struct Lay {
    size: Size,
    /// Boxed: layout recursion holds many `Lay`s per frame and `Node` is large.
    node: Option<Box<Node>>,
    kids: Vec<(f64, f64, Lay)>,
    /// Hit/paint bounds growth (left, top, right, bottom) beyond `size`.
    infl: (f64, f64, f64, f64),
}

impl Lay {
    fn new(size: Size, node: Option<Node>, kids: Vec<(f64, f64, Lay)>) -> Self {
        Self { size, node: node.map(Box::new), kids, infl: (0.0, 0.0, 0.0, 0.0) }
    }
}

#[derive(Clone, Copy)]
struct Env {
    fg: Option<Color>,
    /// Axis of the directly enclosing stack (true = vertical), for Spacer/Divider.
    axis: Option<bool>,
    size: f32,
    base_weight: u16,
    weight: Option<u16>,
    strike: bool,
    align: TextAlign,
    /// Accent tint in effect (automatic buttons recolour their label content).
    tint: Option<Color>,
    /// Top/bottom safe-area insets for full-bleed scrollers (consumed by list/scroll).
    safe_top: f64,
    bottom_inset: f64,
    /// `imageScale` in effect for symbols.
    img: ImageScale,
    /// Laid out as (part of) a list row: `Label` uses the fixed icon column and accent icon.
    in_row: bool,
}

impl Env {
    fn new() -> Self {
        Env {
            fg: None,
            axis: None,
            size: 17.0,
            base_weight: 400,
            weight: None,
            strike: false,
            align: TextAlign::Leading,
            tint: None,
            safe_top: 0.0,
            bottom_inset: 0.0,
            img: ImageScale::Medium,
            in_row: false,
        }
    }
}

struct Ctx<'a> {
    fonts: &'a mut Fonts,
    dark: bool,
    dts: DynamicTypeSize,
    scale: f64,
    scroll: &'a HashMap<String, (f64, f64)>,
    overscroll: bool,
    focus: Option<&'a str>,
    /// Keyed by (node address, proposal bits). A node's Env is fixed by its ancestors,
    /// so node + proposal fully determines its layout within one pass.
    memo: HashMap<(usize, u64, u64), Lay>,
    /// Symbols that fell back to a placeholder.
    unsupported: BTreeSet<String>,
}

/// Inputs to one layout pass.
pub struct SceneParams<'a> {
    pub viewport: (f64, f64),
    pub safe_top: f64,
    pub safe_bottom: f64,
    pub scale: f64,
    pub appearance: Appearance,
    pub dynamic_type: DynamicTypeSize,
    /// Scroll offsets keyed by node id.
    pub scroll: &'a HashMap<String, (f64, f64)>,
    /// Node id of the focused text field.
    pub focus: Option<&'a str>,
    /// Let offsets past the edges show (rubber band) instead of clamping them: only while
    /// `Interaction::live`, so offsets left over from other content never show as overscroll.
    pub overscroll: bool,
    pub status_bar: bool,
    pub chrome: crate::chrome::ChromeParams,
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    // Keyboard avoidance: the keyboard top replaces the bottom safe-area inset.
    let kb = params.chrome.keyboard && params.focus.is_some();
    let kb_h = crate::chrome::KB_BASE_H + params.safe_bottom;
    let safe_bottom = if kb { kb_h } else { params.safe_bottom };
    let mut cx = Ctx {
        fonts,
        dark: params.appearance == Appearance::Dark,
        dts: params.dynamic_type,
        scale: params.scale.max(1.0),
        scroll: params.scroll,
        overscroll: params.overscroll,
        focus: params.focus,
        memo: HashMap::new(),
        unsupported: BTreeSet::new(),
    };
    // Unstyled text is `.body`, which scales with Dynamic Type.
    let body = simcore_protocol::TextStyle::Body.size_at(params.dynamic_type) as f32;
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(cx.semantic(Semantic::Background));
    let mut nodes = vec![screen];
    let full_bleed = matches!(root.kind, ViewKind::Navigation { .. } | ViewKind::List { .. } | ViewKind::Scroll { .. });
    if full_bleed {
        let env = Env { safe_top: params.safe_top, bottom_inset: safe_bottom, size: body, ..Env::new() };
        let lay = cx.view(root, Prop { w: Some(w), h: Some(h) }, env);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    } else {
        // Root view is centered in the safe area.
        let sh = (h - params.safe_top - safe_bottom).max(0.0);
        let lay = cx.view(root, Prop { w: Some(w), h: Some(sh) }, Env { size: body, ..Env::new() });
        let (x, y) = ((w - lay.size.w) / 2.0, params.safe_top + (sh - lay.size.h) / 2.0);
        flatten(lay, x, y, Some(0), 1.0, &mut nodes);
    }
    if params.status_bar {
        let lay = cx.status_bar(w);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    }
    let dark = params.appearance == Appearance::Dark;
    if kb {
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift);
    }
    if let Some(isl) = params.chrome.island {
        crate::chrome::island(&mut nodes, isl);
    }
    Scene { size: params.viewport, appearance: params.appearance, nodes, unsupported_symbols: cx.unsupported.into_iter().collect() }
}

fn flatten(lay: Lay, x: f64, y: f64, mut parent: Option<usize>, mut opacity: f32, out: &mut Vec<Node>) {
    if let Some(mut n) = lay.node {
        let (l, t, r, b) = lay.infl;
        n.bounds = Rect { x: x - l, y: y - t, w: lay.size.w + l + r, h: lay.size.h + t + b };
        n.parent = parent;
        opacity *= n.opacity;
        n.opacity = opacity;
        out.push(*n);
        parent = Some(out.len() - 1);
    }
    for (dx, dy, k) in lay.kids {
        flatten(k, x + dx, y + dy, parent, opacity, out);
    }
}

fn h_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Leading | TopLeading | BottomLeading => 0.0,
        Trailing | TopTrailing | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

fn v_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Top | TopLeading | TopTrailing => 0.0,
        Bottom | BottomLeading | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

fn first_text(v: &ViewNode) -> Option<String> {
    match &v.kind {
        ViewKind::Text { text } => Some(text.clone()),
        ViewKind::VStack { children, .. } | ViewKind::HStack { children, .. } | ViewKind::ZStack { children, .. } => {
            children.iter().find_map(first_text)
        }
        ViewKind::Button { label, .. } | ViewKind::Toggle { label, .. } | ViewKind::Stepper { label, .. } => {
            first_text(label)
        }
        ViewKind::Label { title, .. } => first_text(title),
        _ => None,
    }
}

/// Corner radius of whatever a modifier chain painted last (for shadows).
fn lay_radius(l: &Lay) -> f64 {
    match &l.node {
        Some(n) if n.clip_radius.is_some() => n.clip_radius.unwrap_or(0.0),
        Some(n) if n.radius > 0.0 => n.radius,
        _ if l.kids.len() == 1 => lay_radius(&l.kids[0].2),
        _ => 0.0,
    }
}

/// x offset of the first text inside `l` (separator inset), relative to `l`'s origin.
fn first_text_x(l: &Lay, ox: f64) -> Option<f64> {
    if l.node.as_ref().is_some_and(|n| n.text.is_some()) {
        return Some(ox);
    }
    l.kids.iter().find_map(|(dx, _, k)| first_text_x(k, ox + dx))
}

/// Grow the row's button hit area to the whole row. `row` is (x0,y0,x1,y1) in `l`'s parent space.
fn expand_button(l: &mut Lay, ox: f64, oy: f64, row: (f64, f64, f64, f64)) -> bool {
    if let Some(n) = &l.node {
        if n.kind == "button" && n.action.is_some() {
            l.infl = (ox - row.0, oy - row.1, row.2 - (ox + l.size.w), row.3 - (oy + l.size.h));
            return true;
        }
        return false;
    }
    if l.kids.len() == 1 {
        let (dx, dy, k) = &mut l.kids[0];
        return expand_button(k, ox + *dx, oy + *dy, row);
    }
    false
}

fn has_bordered(v: &ViewNode) -> bool {
    match &v.kind {
        ViewKind::Button { style, label, .. } => {
            matches!(style, ButtonStyle::Bordered | ButtonStyle::BorderedProminent) || has_bordered(label)
        }
        ViewKind::VStack { children, .. } | ViewKind::HStack { children, .. } | ViewKind::ZStack { children, .. } => {
            children.iter().any(has_bordered)
        }
        _ => false,
    }
}

fn is_list(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { style: ListStyle::InsetGrouped, .. })
}

fn is_scroller(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { .. } | ViewKind::Scroll { .. })
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

impl Ctx<'_> {
    /// Offset along a scroll axis: within `[0, max]`, or up to one viewport `len` past an edge
    /// while overscroll is live (the rubber band never goes further).
    fn clamp_offset(&self, o: f64, max: f64, len: f64) -> f64 {
        let slack = if self.overscroll { len } else { 0.0 };
        o.clamp(-slack, max + slack)
    }

    fn semantic(&self, s: Semantic) -> Color {
        let d = self.dark;
        match s {
            Semantic::Primary if d => rgba(255, 255, 255, 1.0),
            Semantic::Primary => rgba(0, 0, 0, 1.0),
            Semantic::Secondary if d => rgba(235, 235, 245, 0.6),
            Semantic::Secondary => rgba(60, 60, 67, 0.6),
            Semantic::Tertiary if d => rgba(235, 235, 245, 0.3),
            Semantic::Tertiary => rgba(60, 60, 67, 0.3),
            Semantic::Accent if d => rgba(10, 145, 255, 1.0),
            Semantic::Accent => rgba(0, 136, 255, 1.0),
            Semantic::Background if d => rgba(0, 0, 0, 1.0),
            Semantic::Background => rgba(255, 255, 255, 1.0),
            Semantic::Separator if d => rgba(56, 56, 59, 1.0),
            Semantic::Separator => rgba(232, 232, 232, 1.0),
            Semantic::GroupedBackground if d => rgba(0, 0, 0, 1.0),
            Semantic::GroupedBackground => rgba(242, 242, 247, 1.0),
            Semantic::SecondaryGroupedBackground if d => rgba(28, 28, 30, 1.0),
            Semantic::SecondaryGroupedBackground => rgba(255, 255, 255, 1.0),
        }
    }

    fn glass_spec(&self, variant: GlassVariant, tint: Option<&simcore_protocol::Color>, interactive: bool) -> crate::glass::GlassSpec {
        crate::glass::GlassSpec { variant, tint: tint.map(|c| self.color(c)), interactive }
    }

    /// Neutral control fill (systemFill-like) of the given strength.
    fn control_fill(&self, light_a: f32) -> Color {
        if self.dark {
            Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, light_a * 2.0)
        } else {
            Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, light_a)
        }
    }

    fn color(&self, c: &simcore_protocol::Color) -> Color {
        match c {
            simcore_protocol::Color::Semantic { semantic } => self.semantic(*semantic),
            simcore_protocol::Color::Rgba { r, g, b, a, dark } => {
                let [r, g, b] = if self.dark { dark.unwrap_or([*r, *g, *b]) } else { [*r, *g, *b] };
                Color::rgba(r as f32, g as f32, b as f32, *a as f32)
            }
        }
    }

    /// SF line height, rounded up to the device pixel like UIKit.
    fn line_h(&self, size: f32) -> f32 {
        let s = self.scale as f32;
        (size * 1.1934 * s).ceil() / s
    }

    fn spec(&self, env: &Env) -> FontSpec {
        FontSpec::new(env.size, self.line_h(env.size), env.weight.unwrap_or(env.base_weight), env.align)
    }

    fn hairline(&self) -> f64 {
        1.0 / self.scale
    }

    /// A text leaf with an explicit font.
    fn text_lay(&mut self, id: &str, text: &str, spec: FontSpec, color: Color, wrap: Option<f32>) -> Lay {
        let (w, h) = self.fonts.measure(text, &spec, wrap);
        // SwiftUI rounds text sizes up to the device pixel.
        let w = (w * self.scale).ceil() / self.scale;
        let mut n = Node::new(id, "text");
        n.text = Some(TextRun { text: text.to_string(), font: spec, color, strike: false, offset_x: 0.0 });
        n.role = Role::Text;
        n.label = Some(text.to_string());
        Lay::new(Size { w, h }, Some(n), vec![])
    }

    fn view(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let bits = |x: Option<f64>| x.map_or(u64::MAX, f64::to_bits);
        let key = (v as *const ViewNode as usize, bits(p.w), bits(p.h));
        if let Some(l) = self.memo.get(&key) {
            return l.clone();
        }
        let l = self.mods(v, &v.modifiers, p, env);
        self.memo.insert(key, l.clone());
        l
    }

    /// Applies modifiers outermost-first by recursion (array order is innermost first).
    fn mods(&mut self, v: &ViewNode, mods: &[Modifier], p: Prop, env: Env) -> Lay {
        let Some((last, rest)) = mods.split_last() else {
            return self.base(v, p, env);
        };
        match last {
            Modifier::Padding(e) => {
                let ph = e.leading + e.trailing;
                let pv = e.top + e.bottom;
                let inner = self.mods(
                    v,
                    rest,
                    Prop { w: p.w.map(|w| (w - ph).max(0.0)), h: p.h.map(|h| (h - pv).max(0.0)) },
                    env,
                );
                let size = Size { w: inner.size.w + ph, h: inner.size.h + pv };
                Lay::new(size, None, vec![(e.leading, e.top, inner)])
            }
            Modifier::Frame(f) => {
                let cw = child_prop(p.w, f.width, f.min_width, f.max_width);
                let ch = child_prop(p.h, f.height, f.min_height, f.max_height);
                let inner = self.mods(v, rest, Prop { w: cw, h: ch }, env);
                let w = frame_len(p.w, inner.size.w, f.width, f.min_width, f.max_width);
                let h = frame_len(p.h, inner.size.h, f.height, f.min_height, f.max_height);
                let (dx, dy) = ((w - inner.size.w) * h_frac(f.alignment), (h - inner.size.h) * v_frac(f.alignment));
                Lay::new(Size { w, h }, None, vec![(dx, dy, inner)])
            }
            Modifier::ForegroundColor(c) => {
                let mut env = Env { fg: Some(self.color(c)), ..env };
                // Inside a tinted control the hierarchical styles stay accent-coloured, dimmed.
                if let (Some(t), simcore_protocol::Color::Semantic { semantic }) = (env.tint, c) {
                    let k = match semantic {
                        Semantic::Primary => Some(1.0),
                        Semantic::Secondary => Some(0.5),
                        Semantic::Tertiary => Some(0.25),
                        _ => None,
                    };
                    if let Some(k) = k {
                        env.fg = Some(Color { a: t.a * k, ..t });
                    }
                }
                self.mods(v, rest, p, env)
            }
            Modifier::Background(bg) => {
                let inner = self.mods(v, rest, p, env);
                let bgl = self.view(bg, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                Lay::new(inner.size, None, vec![(0.0, 0.0, bgl), (0.0, 0.0, inner)])
            }
            Modifier::Overlay(o) => {
                let inner = self.mods(v, rest, p, env);
                let ovl = self.view(&o.view, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let (dx, dy) = ((inner.size.w - ovl.size.w) * h_frac(o.alignment), (inner.size.h - ovl.size.h) * v_frac(o.alignment));
                Lay::new(inner.size, None, vec![(0.0, 0.0, inner), (dx, dy, ovl)])
            }
            Modifier::CornerRadius(r) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#clip{}", v.id, rest.len()), "clip");
                n.clip_radius = Some(*r);
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Opacity(o) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#opacity{}", v.id, rest.len()), "opacity");
                n.opacity = *o as f32;
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Shadow(s) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#shadow{}", v.id, rest.len()), "shadow");
                n.radius = lay_radius(&inner);
                n.shadow = Some(Shadow { color: self.color(&s.color), radius: s.radius, dx: s.x, dy: s.y });
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Font(f) => {
                let (size, weight) = match f {
                    Font::Style { style, weight } => {
                        let (_, w) = style.spec();
                        (style.size_at(self.dts), weight.unwrap_or(w))
                    }
                    Font::System { size, weight } => (*size, *weight),
                };
                self.mods(v, rest, p, Env { size: size as f32, base_weight: weight.value(), ..env })
            }
            Modifier::FontWeight(w) => {
                let weight = env.weight.or(Some(w.value()));
                self.mods(v, rest, p, Env { weight, ..env })
            }
            Modifier::Strikethrough(b) => self.mods(v, rest, p, Env { strike: *b, ..env }),
            Modifier::MultilineTextAlignment(a) => {
                let align = match a {
                    Alignment::Center | Alignment::Top | Alignment::Bottom => TextAlign::Center,
                    a if h_frac(*a) == 1.0 => TextAlign::Trailing,
                    _ => TextAlign::Leading,
                };
                self.mods(v, rest, p, Env { align, ..env })
            }
            Modifier::ImageScale(img) => self.mods(v, rest, p, Env { img: *img, ..env }),
            Modifier::NavigationTitle(_) => self.mods(v, rest, p, env),
            Modifier::Glass(g) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#glass{}", v.id, rest.len()), "glass");
                n.radius = match (g.shape.shape, g.shape.corner_radius) {
                    (GlassShapeKind::Rect, None) => 0.0,
                    (GlassShapeKind::Rect | GlassShapeKind::RoundedRectangle, r) => r.unwrap_or(0.0),
                    _ => PILL,
                };
                n.glass = Some(self.glass_spec(g.variant, g.tint.as_ref(), g.interactive));
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
        }
    }

    fn base(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let child_env = Env { axis: None, ..env };
        match &v.kind {
            ViewKind::Text { text } => {
                let wrap = p.w.filter(|w| w.is_finite()).map(|w| w.max(1.0) as f32);
                let spec = self.spec(&env);
                let mut l = self.text_lay(&v.id, text, spec, fg, wrap);
                if let Some(t) = l.node.as_mut().and_then(|n| n.text.as_mut()) {
                    t.strike = env.strike;
                }
                l
            }
            ViewKind::VStack { spacing, alignment, children } => {
                self.stack(v, "vstack", children, *spacing, *alignment, true, p, child_env)
            }
            ViewKind::HStack { spacing, alignment, children } => {
                self.stack(v, "hstack", children, *spacing, *alignment, false, p, child_env)
            }
            ViewKind::ZStack { alignment, children } => {
                let lays: Vec<Lay> = children.iter().map(|c| self.view(c, p, child_env)).collect();
                let size = Size {
                    w: lays.iter().map(|l| l.size.w).fold(0.0, f64::max),
                    h: lays.iter().map(|l| l.size.h).fold(0.0, f64::max),
                };
                let kids = lays
                    .into_iter()
                    .map(|l| ((size.w - l.size.w) * h_frac(*alignment), (size.h - l.size.h) * v_frac(*alignment), l))
                    .collect();
                Lay::new(size, Some(Node::new(&v.id, "zstack")), kids)
            }
            ViewKind::Spacer { min_length } => {
                let min = min_length.unwrap_or(0.0);
                let main = |prop: Option<f64>| prop.unwrap_or(0.0).max(min);
                let size = match env.axis {
                    Some(true) => Size { w: 0.0, h: main(p.h) },
                    Some(false) => Size { w: main(p.w), h: 0.0 },
                    None => Size { w: min, h: min },
                };
                Lay::new(size, Some(Node::new(&v.id, "spacer")), vec![])
            }
            ViewKind::Button { action, label, style, disabled } => self.button(v, *action, label, *style, *disabled, p, env),
            ViewKind::Color { color } => {
                let mut n = Node::new(&v.id, "color");
                n.fill = Some(self.color(color));
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Rectangle => {
                let mut n = Node::new(&v.id, "rectangle");
                n.fill = Some(fg);
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Shape { shape, corner_radius, fill, stroke } => {
                let size = if *shape == ShapeKind::Circle {
                    let side = match (p.w, p.h) {
                        (Some(w), Some(h)) => w.min(h),
                        (Some(w), None) => w,
                        (None, Some(h)) => h,
                        _ => 10.0,
                    };
                    Size { w: side, h: side }
                } else {
                    greedy(p)
                };
                let mut n = Node::new(&v.id, "shape");
                n.radius = match shape {
                    ShapeKind::Rectangle => 0.0,
                    ShapeKind::Circle | ShapeKind::Capsule => PILL,
                    ShapeKind::RoundedRectangle => corner_radius.unwrap_or(0.0),
                };
                n.fill = fill.as_ref().map(|c| self.color(c));
                n.stroke = stroke.as_ref().map(|Stroke { color, line_width }| (self.color(color), *line_width));
                if n.fill.is_none() && n.stroke.is_none() {
                    n.fill = Some(fg);
                }
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Divider => {
                let hl = self.hairline();
                let len = |x: Option<f64>| x.filter(|x| x.is_finite()).unwrap_or(0.0);
                let size = if env.axis == Some(false) { Size { w: hl, h: len(p.h) } } else { Size { w: len(p.w), h: hl } };
                let mut n = Node::new(&v.id, "divider");
                // UIKit's translucent separator (measured 198 on white).
                n.fill = Some(if self.dark { rgba(84, 84, 88, 0.6) } else { rgba(60, 60, 67, 0.29) });
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Empty => Lay::new(Size::default(), Some(Node::new(&v.id, "empty")), vec![]),
            ViewKind::Scroll { axis, child } => self.scroll(v, *axis == simcore_protocol::Axis::Vertical, child, p, env),
            ViewKind::List { style, sections } => self.list(v, *style, sections, p, env),
            ViewKind::Navigation { title, display_mode, back, content } => {
                self.navigation(v, title.as_deref(), *display_mode, back.as_ref(), content, p, env)
            }
            ViewKind::Toggle { is_on, action, label, disabled } => self.toggle(v, *is_on, *action, label, *disabled, p, env),
            ViewKind::Stepper { label, decrement, increment } => self.stepper(v, label, *decrement, *increment, p, env),
            ViewKind::TextField { placeholder, text, field } => self.text_field(v, placeholder, text, *field, p, env),
            ViewKind::Image { system_name, resizable, content_mode, scale } => {
                self.image(v, system_name, *resizable, *content_mode, scale.unwrap_or(env.img), p, env, fg)
            }
            ViewKind::Label { title, icon } => self.label(v, title, icon, p, child_env),
        }
    }

    /// SF Symbol. Non-resizable images are glyph-sized from the font; resizable ones stretch to
    /// the proposal (or fit / fill it keeping the symbol's aspect ratio).
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, v: &ViewNode, name: &str, resizable: bool, mode: Option<ContentMode>, scale: ImageScale, p: Prop, env: Env, fg: Color) -> Lay {
        let spec = SymbolSpec { name: name.to_string(), point_size: env.size as f64, weight: env.weight.unwrap_or(env.base_weight), scale };
        let nat = symbols::metrics(&spec);
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (nw, nh) = nat.map_or((env.size as f64, env.size as f64), |m| (m.w, m.h));
        let (size, glyph) = if !resizable {
            (Size { w: nw, h: nh }, (nw, nh))
        } else {
            let (pw, ph) = (fin(p.w), fin(p.h));
            let ratio = nw / nh;
            match mode {
                None => {
                    let s = Size { w: pw.unwrap_or(nw), h: ph.unwrap_or(nh) };
                    (s, (s.w, s.h))
                }
                Some(m) => {
                    let (bw, bh) = match (pw, ph) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => (w, w / ratio),
                        (None, Some(h)) => (h * ratio, h),
                        (None, None) => (nw, nh),
                    };
                    let by_w = (bw, bw / ratio);
                    let by_h = (bh * ratio, bh);
                    let fit = if by_w.1 <= bh { by_w } else { by_h };
                    let fill = if by_w.1 >= bh { by_w } else { by_h };
                    match m {
                        // Fit shrink-wraps the symbol; fill occupies the whole proposal and crops.
                        ContentMode::Fit => (Size { w: fit.0, h: fit.1 }, fit),
                        ContentMode::Fill => (Size { w: bw, h: bh }, fill),
                    }
                }
            }
        };
        let mut n = Node::new(&v.id, "image");
        n.role = Role::Image;
        n.label = Some(name.to_string());
        if nat.is_some() {
            n.symbol = Some(SymbolRun { spec, color: fg, size: glyph });
        } else {
            // Placeholder: rounded-square outline, reported as `unsupported_symbol`.
            self.unsupported.insert(name.to_string());
            n.stroke = Some((fg, 1.0));
            n.radius = size.w.min(size.h) * 0.2;
        }
        Lay::new(size, Some(n), vec![])
    }

    /// iOS `Label`: icon then title. In a list row the icon is centred in a fixed leading column.
    fn label(&mut self, v: &ViewNode, title: &ViewNode, icon: &ViewNode, p: Prop, env: Env) -> Lay {
        let icon_fg = env.fg.or_else(|| env.in_row.then(|| self.semantic(Semantic::Accent)));
        // ponytail: list rows force imageScale large on the icon, an explicit imageScale on the Label is ignored there.
        let img = if env.in_row { ImageScale::Large } else { env.img };
        let il = self.view(icon, Prop { w: None, h: None }, Env { fg: icon_fg, img, ..env });
        let (col, gap) = if env.in_row { (LABEL_ICON_COL, LABEL_ROW_GAP) } else { (il.size.w, LABEL_GAP) };
        let tw = p.w.filter(|w| w.is_finite()).map(|w| (w - col - gap).max(0.0));
        let tl = self.view(title, Prop { w: tw, h: None }, env);
        let h = il.size.h.max(tl.size.h);
        let w = col + gap + tl.size.w;
        let ix = (col - il.size.w) / 2.0;
        let mut n = Node::new(&v.id, "label");
        n.label = first_text(title);
        Lay::new(
            Size { w, h },
            Some(n),
            vec![(ix, (h - il.size.h) / 2.0, il), (col + gap, (h - tl.size.h) / 2.0, tl)],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn button(&mut self, v: &ViewNode, action: u64, label: &ViewNode, style: ButtonStyle, disabled: bool, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let accent = self.semantic(Semantic::Accent);
        let dim = self.semantic(Semantic::Tertiary);
        let mut n = Node::new(&v.id, "button");
        n.role = Role::Button;
        n.action = (!disabled).then_some(action);
        n.disabled = disabled;
        n.label = first_text(label);
        match style {
            ButtonStyle::Automatic | ButtonStyle::Plain => {
                let (fg, tint) = match (style, disabled) {
                    (_, true) => (dim, None),
                    (ButtonStyle::Automatic, _) => (accent, Some(accent)),
                    _ => (env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)), None),
                };
                let l = self.view(label, p, Env { fg: Some(fg), tint, ..child_env });
                Lay::new(l.size, Some(n), vec![(0.0, 0.0, l)])
            }
            ButtonStyle::Bordered | ButtonStyle::BorderedProminent => {
                let prominent = style == ButtonStyle::BorderedProminent;
                let (fg, bg) = match (prominent, disabled) {
                    (_, true) => (dim, self.control_fill(0.16)),
                    (true, _) => (rgba(255, 255, 255, 1.0), accent),
                    (false, _) => (accent, self.control_fill(0.16)),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.fill = Some(bg);
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::Glass | ButtonStyle::GlassProminent => {
                let prominent = style == ButtonStyle::GlassProminent;
                let fg = match (prominent, disabled) {
                    (_, true) => dim,
                    (true, _) => rgba(255, 255, 255, 1.0),
                    (false, _) => self.semantic(Semantic::Primary),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.glass = Some(self.glass_spec(GlassVariant::Regular, prominent.then_some(simcore_protocol::Color::Semantic { semantic: Semantic::Accent }).as_ref(), true));
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::NavigationLink => {
                let fg = if disabled { dim } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
                // 7x12 at body size, growing with Dynamic Type (21.7x37.3, 8.6pt inset at AX5; measured).
                let k = (simcore_protocol::TextStyle::Body.size_at(self.dts) / 17.0).max(1.0);
                let (chev_w, chev_h, inset) = (7.0 * k, 12.0 * k, 2.3 + 2.97 * (k - 1.0));
                let inner_p = Prop { w: p.w.filter(|w| w.is_finite()).map(|w| (w - chev_w - 12.0).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let w = p.w.filter(|w| w.is_finite()).unwrap_or(l.size.w + 12.0 + chev_w);
                let h = l.size.h.max(chev_h);
                let mut c = Node::new(format!("{}#chevron", v.id), "chevron");
                c.glyph = Some((Glyph::Chevron, self.semantic(Semantic::Tertiary)));
                c.inspect = false;
                let cl = Lay::new(Size { w: chev_w, h: chev_h }, Some(c), vec![]);
                let ly = (h - l.size.h) / 2.0;
                Lay::new(Size { w, h }, Some(n), vec![(0.0, ly, l), (w - chev_w - inset, (h - chev_h) / 2.0, cl)])
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn toggle(&mut self, v: &ViewNode, on: bool, action: u64, label: &ViewNode, disabled: bool, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - SWITCH.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + SWITCH.0);
        let h = l.size.h.max(SWITCH.1);
        let mut sw = Node::new(format!("{}#switch", v.id), "switch");
        sw.role = Role::Toggle;
        sw.label = first_text(label);
        sw.value = Some(if on { "on" } else { "off" }.into());
        sw.action = (!disabled).then_some(action);
        sw.disabled = disabled;
        sw.radius = PILL;
        sw.fill = Some(if on { rgba(52, 199, 89, 1.0) } else { self.control_fill(0.16) });
        if disabled {
            sw.opacity = 0.5;
        }
        let (kw, kh) = (37.0, 24.0);
        let mut knob = Node::new(format!("{}#knob", v.id), "knob");
        knob.fill = Some(rgba(255, 255, 255, 1.0));
        knob.radius = PILL;
        knob.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.18), radius: 1.0, dx: 0.0, dy: 1.0 });
        knob.inspect = false;
        let kx = if on { SWITCH.0 - 2.0 - kw } else { 2.0 };
        let kl = Lay::new(Size { w: kw, h: kh }, Some(knob), vec![]);
        let sl = Lay::new(Size { w: SWITCH.0, h: SWITCH.1 }, Some(sw), vec![(kx, (SWITCH.1 - kh) / 2.0, kl)]);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(Node::new(&v.id, "toggle")), vec![(0.0, ly, l), (w - SWITCH.0 + 2.0, (h - SWITCH.1) / 2.0, sl)])
    }

    fn stepper(&mut self, v: &ViewNode, label: &ViewNode, dec: Option<u64>, inc: Option<u64>, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - STEPPER.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + STEPPER.0);
        let h = l.size.h.max(STEPPER.1);
        let mut root = Node::new(&v.id, "stepper");
        root.role = Role::Stepper;
        root.label = first_text(label);
        let mut ctl = Node::new(format!("{}#control", v.id), "stepper-control");
        ctl.fill = Some(self.control_fill(0.12));
        ctl.radius = PILL;
        ctl.inspect = false;
        let half = STEPPER.0 / 2.0;
        let mut kids = vec![];
        for (name, action, glyph, x) in [("decrement", dec, Glyph::Minus, 0.0), ("increment", inc, Glyph::Plus, half)] {
            let mut b = Node::new(format!("{}#{name}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(name.into());
            b.action = action;
            b.disabled = action.is_none();
            let c = if action.is_some() { self.semantic(Semantic::Primary) } else { self.semantic(Semantic::Tertiary) };
            b.glyph = Some((glyph, c));
            kids.push((x, 0.0, Lay::new(Size { w: half, h: STEPPER.1 }, Some(b), vec![])));
        }
        let mut div = Node::new(format!("{}#divider", v.id), "divider");
        div.fill = Some(self.semantic(Semantic::Tertiary));
        div.inspect = false;
        kids.push((half - self.hairline() / 2.0, 8.0, Lay::new(Size { w: self.hairline(), h: 16.0 }, Some(div), vec![])));
        let cl = Lay::new(Size { w: STEPPER.0, h: STEPPER.1 }, Some(ctl), kids);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(root), vec![(0.0, ly, l), (w - STEPPER.0, (h - STEPPER.1) / 2.0, cl)])
    }

    fn text_field(&mut self, v: &ViewNode, placeholder: &str, text: &str, field: u64, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(120.0);
        let h = 22.0;
        let spec = self.spec(&Env { align: TextAlign::Leading, ..env });
        let focused = self.focus == Some(v.id.as_str());
        let empty = text.is_empty();
        let shown = if empty { placeholder } else { text };
        let color = if empty { self.semantic(Semantic::Tertiary) } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
        let mut t = self.text_lay(&format!("{}#text", v.id), shown, spec, color, None);
        let tw = if empty { 0.0 } else { t.size.w };
        let overflow = (tw - (w - 2.0)).max(0.0);
        if let Some(tr) = t.node.as_mut().and_then(|n| n.text.as_mut()) {
            tr.offset_x = -overflow as f32;
        }
        if let Some(n) = t.node.as_mut() {
            n.inspect = false;
        }
        let mut n = Node::new(&v.id, "text_field");
        n.role = Role::TextField;
        n.field = Some(field);
        n.value = Some(text.to_string());
        n.label = Some(placeholder.to_string());
        n.focused = focused;
        n.clip_radius = Some(0.0);
        let mut kids = vec![(0.0, (h - t.size.h) / 2.0, t)];
        if focused {
            let mut c = Node::new(format!("{}#caret", v.id), "caret");
            c.fill = Some(self.semantic(Semantic::Accent));
            c.radius = 1.0;
            c.inspect = false;
            kids.push(((tw - overflow).min(w - 2.0) + 1.0, 1.0, Lay::new(Size { w: 2.0, h: h - 2.0 }, Some(c), vec![])));
        }
        Lay::new(Size { w, h }, Some(n), kids)
    }

    fn scroll(&mut self, v: &ViewNode, vertical: bool, child: &ViewNode, p: Prop, env: Env) -> Lay {
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let child_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, ..env };
        let cp = if vertical {
            Prop { w: p.w, h: None }
        } else {
            Prop { w: None, h: p.h }
        };
        let c = self.view(child, cp, child_env);
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (cw, ch) = if vertical { (c.size.w, c.size.h + top + bottom) } else { (c.size.w, c.size.h) };
        let w = fin(p.w).unwrap_or(cw);
        let h = fin(p.h).unwrap_or(ch);
        let max = ((cw - w).max(0.0), (ch - h).max(0.0));
        let off = self.scroll.get(&v.id).copied().unwrap_or((0.0, 0.0));
        let off = if vertical { (0.0, self.clamp_offset(off.1, max.1, h)) } else { (self.clamp_offset(off.0, max.0, w), 0.0) };
        let mut n = Node::new(&v.id, "scroll");
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None });
        n.inspect = true;
        let cx = if vertical { (w - c.size.w) / 2.0 } else { 0.0 };
        Lay::new(Size { w, h }, Some(n), vec![(cx - off.0, top - off.1, c)])
    }

    fn list(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], p: Prop, env: Env) -> Lay {
        let inset = style == ListStyle::InsetGrouped;
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let margin = if inset { CARD_MARGIN } else { 0.0 };
        let gw = w - 2.0 * margin;
        let inner_w = gw - 2.0 * ROW_HPAD;
        let row_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, fg: None, in_row: true, ..env };
        let sec_col = self.semantic(Semantic::SecondaryGroupedBackground);
        let sep_col = self.semantic(Semantic::Separator);
        let mut y = top;
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        for (si, sec) in sections.iter().enumerate() {
            let sid = format!("{}#s{si}", v.id);
            if let Some(h) = &sec.header {
                let hs = simcore_protocol::TextStyle::Headline.size_at(self.dts) as f32;
                let spec = FontSpec::new(hs, self.line_h(hs), 600, TextAlign::Leading);
                let mut t = self.text_lay(&format!("{sid}#header"), h, spec, self.semantic(Semantic::Secondary), Some(inner_w as f32));
                if let Some(n) = t.node.as_mut() {
                    n.role = Role::Text;
                }
                let hh = t.size.h + HEADER_PAD;
                kids.push((margin + ROW_HPAD, y + (hh - t.size.h) / 2.0, t));
                y += hh;
            }
            let mut sec_kids: Vec<(f64, f64, Lay)> = vec![];
            let mut ry = 0.0;
            for (ri, row) in sec.rows.iter().enumerate() {
                let mut c = self.view(row, Prop { w: Some(inner_w), h: None }, row_env);
                // Measured: 34.3pt buttons and 44pt content get 15pt vertical padding, 32pt steppers 10pt.
                let vpad = if has_bordered(row) || c.size.h > 33.0 { ROW_VPAD_BORDERED } else { ROW_VPAD };
                let rh = ROW_MIN_H.max(c.size.h + 2.0 * vpad);
                let yo = (rh - c.size.h) / 2.0;
                expand_button(&mut c, 0.0, yo, (-ROW_HPAD, 0.0, gw - ROW_HPAD, rh));
                let tx = first_text_x(&c, 0.0).unwrap_or(0.0);
                let mut rn = Node::new(format!("{sid}#r{ri}"), "row");
                rn.inspect = false;
                let last = ri + 1 == sec.rows.len();
                let mut rkids = vec![(ROW_HPAD, yo, c)];
                if !last {
                    let mut sp = Node::new(format!("{sid}#r{ri}#sep"), "separator");
                    sp.fill = Some(sep_col);
                    sp.inspect = false;
                    let x0 = ROW_HPAD + tx;
                    // Measured: 1pt (3px @3x) solid line ending at the row boundary.
                    rkids.push((x0, rh - 1.0, Lay::new(Size { w: (gw - ROW_HPAD - x0).max(0.0), h: 1.0 }, Some(sp), vec![])));
                }
                sec_kids.push((0.0, ry, Lay::new(Size { w: gw, h: rh }, Some(rn), rkids)));
                ry += rh;
            }
            let mut sn = Node::new(&sid, "section");
            if inset {
                sn.fill = Some(sec_col);
                sn.radius = CARD_RADIUS;
            }
            sn.inspect = true;
            kids.push((margin, y, Lay::new(Size { w: gw, h: ry }, Some(sn), sec_kids)));
            y += ry;
            if let Some(f) = &sec.footer {
                let spec = FontSpec::new(13.0, self.line_h(13.0), 400, TextAlign::Leading);
                let t = self.text_lay(&format!("{sid}#footer"), f, spec, self.semantic(Semantic::Secondary), Some(inner_w as f32));
                y += 8.0;
                let th = t.size.h;
                kids.push((margin + ROW_HPAD, y, t));
                y += th;
            }
            // Between sections 17.7; after the last one the list ends 20pt above the bottom inset (measured).
            y += if si + 1 == sections.len() { 20.0 } else { SECTION_GAP };
        }
        let ch = y + bottom;
        let fin_h = p.h.filter(|h| h.is_finite());
        let h = fin_h.unwrap_or(ch);
        let max = (0.0, (ch - h).max(0.0));
        let off = self.clamp_offset(self.scroll.get(&v.id).map_or(0.0, |o| o.1), max.1, h);
        let mut n = Node::new(&v.id, "list");
        n.fill = Some(self.semantic(if inset { Semantic::GroupedBackground } else { Semantic::Background }));
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max, snap: None });
        let kids = kids.into_iter().map(|(x, ky, k)| (x, ky - off, k)).collect();
        Lay::new(Size { w, h }, Some(n), kids)
    }

    #[allow(clippy::too_many_arguments)]
    // Out of line: inlined into the recursive `view` dispatcher it inflates every stack frame.
    #[inline(never)]
    fn navigation(
        &mut self,
        v: &ViewNode,
        title: Option<&str>,
        mode: DisplayMode,
        back: Option<&simcore_protocol::BackButton>,
        content: &ViewNode,
        p: Prop,
        env: Env,
    ) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(874.0);
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let large = mode == DisplayMode::Large && title.is_some();
        let primary = self.semantic(Semantic::Primary);
        // Large title: LargeTitle bold, growing with Dynamic Type. Its band below the inline bar is
        // 52pt, or the title's height + 1.33 when taller (measured: 106pt bar at large, 127 at AX5).
        let large_title = title.filter(|_| large).map(|t| {
            let sz = simcore_protocol::TextStyle::LargeTitle.size_at(self.dts) as f32;
            let spec = FontSpec::new(sz, self.line_h(sz), 700, TextAlign::Leading);
            self.text_lay(&format!("{}#title", v.id), t, spec, primary, Some((w - 32.0) as f32))
        });
        let title_h = large_title.as_ref().map_or(0.0, |l| l.size.h);
        let band = if large { (title_h + 1.33).max(52.0) } else { 0.0 };
        let bar_h = if title.is_none() && back.is_none() { 0.0 } else { NAV_INLINE_H + band };
        let ctop = top + bar_h;
        let scroller = is_scroller(content);
        let edge_bg = self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background });
        let mut bg = Node::new(&v.id, "navigation");
        bg.fill = Some(self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background }));
        bg.label = title.map(str::to_string);
        let cenv = Env { safe_top: 0.0, bottom_inset: bottom, fg: None, ..env };
        let mut kids = vec![];
        let mut offset = 0.0;
        if scroller {
            // Content scrolls under the bar: the scroller spans the screen with the bar as a top inset.
            let mut c = self.view(content, Prop { w: Some(w), h: Some(h) }, Env { safe_top: ctop, ..cenv });
            if let Some(s) = c.node.as_mut().and_then(|n| n.scroll.as_mut()).filter(|_| large) {
                s.snap = Some(band);
            }
            offset = c.node.as_ref().and_then(|n| n.scroll).map_or(0.0, |s| s.offset.1);
            kids.push((0.0, 0.0, c));
            // Content reaches under the bar once the large title has collapsed.
            if offset > band {
                let mut e = Node::new(format!("{}#edge", v.id), "scrolledge");
                e.edge = Some(crate::glass::ScrollEdge {
                    bar: (top + NAV_INLINE_H) as f32,
                    color: edge_bg,
                    tint: if self.dark { 0.0 } else { 3.0 / 255.0 },
                });
                e.inspect = false;
                kids.push((0.0, 0.0, Lay::new(Size { w, h: ctop + 33.0 }, Some(e), vec![])));
            }
        } else {
            let avail = (h - ctop - bottom).max(0.0);
            let c = self.view(content, Prop { w: Some(w), h: Some(avail) }, Env { bottom_inset: 0.0, ..cenv });
            kids.push(((w - c.size.w) / 2.0, ctop + (avail - c.size.h) / 2.0, c));
        }
        // Bar (no background at rest on iOS 26): title + glass back button above content.
        // The large title scrolls with the content and disappears under the inline bar; the bar
        // shrinks with it and the inline title fades in over the last INLINE_FADE points.
        let collapse = offset.clamp(0.0, band);
        let mut bar = Node::new(format!("{}#bar", v.id), "navbar");
        bar.label = title.map(str::to_string);
        let mut bar_kids = vec![];
        if let Some(tl) = large_title.filter(|_| offset < band) {
            // Title top: 54pt under the bar's top, lowered when the band has spare room (57.7 at large).
            let ty = NAV_INLINE_H + (band - title_h - 7.6).max(0.0) - offset;
            if offset > 0.0 {
                // Mid-collapse: cut off where it slides under the inline bar.
                let mut clip = Node::new(format!("{}#largetitle", v.id), "clip");
                clip.clip_radius = Some(0.0);
                clip.inspect = false;
                let clip_h = h - top - NAV_INLINE_H;
                bar_kids.push((0.0, NAV_INLINE_H, Lay::new(Size { w, h: clip_h }, Some(clip), vec![(16.0, ty - NAV_INLINE_H, tl)])));
            } else {
                bar_kids.push((16.0, ty, tl));
            }
        }
        if let Some(t) = title {
            let alpha = if large { ((collapse - (band - INLINE_FADE)) / INLINE_FADE).clamp(0.0, 1.0) as f32 } else { 1.0 };
            if alpha > 0.0 {
                // Inline titles grow with Dynamic Type but cap at x1.225 (measured at accessibility5).
                let nsz = 17.0 * (simcore_protocol::TextStyle::Body.size_at(self.dts) as f32 / 17.0).min(1.225);
                let spec = FontSpec::new(nsz, self.line_h(nsz), 600, TextAlign::Leading);
                let id = if large { format!("{}#inline", v.id) } else { format!("{}#title", v.id) };
                let mut tl = self.text_lay(&id, t, spec, primary, Some((w - 120.0) as f32));
                if let Some(n) = tl.node.as_mut() {
                    n.opacity = alpha;
                }
                let (tw, th) = (tl.size.w, tl.size.h);
                bar_kids.push(((w - tw) / 2.0, 22.0 - th / 2.0, tl));
            }
        }
        if let Some(b) = back {
            let mut n = Node::new(format!("{}#back", v.id), "button");
            n.role = Role::Button;
            n.action = Some(b.action);
            n.label = Some(if b.label.is_empty() { "Back".into() } else { b.label.clone() });
            n.glass = Some(self.glass_spec(GlassVariant::Regular, None, true));
            n.radius = PILL;
            n.glyph = Some((Glyph::BackChevron, self.semantic(Semantic::Primary)));
            bar_kids.push((16.0, 0.0, Lay::new(Size { w: 44.0, h: 44.0 }, Some(n), vec![])));
        }
        kids.push((0.0, top, Lay::new(Size { w, h: bar_h - collapse }, Some(bar), bar_kids)));
        Lay::new(Size { w, h }, Some(bg), kids)
    }

    /// iOS status bar: "9:41" at the left, signal / wifi / battery at the right.
    fn status_bar(&mut self, w: f64) -> Lay {
        let primary = self.semantic(Semantic::Primary);
        let spec = FontSpec::new(17.0, self.line_h(17.0), 600, TextAlign::Leading);
        let mut t = self.text_lay("statusbar#time", "9:41", spec, primary, None);
        if let Some(n) = t.node.as_mut() {
            n.inspect = false;
        }
        let tw = t.size.w;
        let th = t.size.h;
        let glyph = |id: &str, g: Glyph, c: Color, sz: Size| {
            let mut n = Node::new(id, "statusbar");
            n.glyph = Some((g, c));
            n.inspect = false;
            Lay::new(sz, Some(n), vec![])
        };
        let dots = glyph("statusbar#signal", Glyph::SignalDots, self.semantic(Semantic::Tertiary), Size { w: 20.0, h: 4.0 });
        let wifi = glyph("statusbar#wifi", Glyph::Wifi, primary, Size { w: 16.0, h: 12.0 });
        let batt = glyph("statusbar#battery", Glyph::Battery, primary, Size { w: 27.0, h: 13.0 });
        let right = w - 402.0 + 340.0; // anchored to the trailing edge of a 402pt screen
        let mut root = Node::new("statusbar", "statusbar");
        root.inspect = false;
        Lay::new(
            Size { w, h: 62.0 },
            Some(root),
            vec![(73.0 - tw / 2.0, 32.4 - th / 2.0, t), (right - 52.0, 34.7, dots), (right - 24.7, 26.2, wifi), (right - 0.6, 26.0, batt)],
        )
    }

    /// SwiftUI stack algorithm: least flexible children are laid out first, each
    /// proposed an equal share of the space still remaining.
    #[allow(clippy::too_many_arguments)]
    fn stack(
        &mut self,
        v: &ViewNode,
        kind: &'static str,
        children: &[ViewNode],
        spacing: Option<f64>,
        align: Alignment,
        vertical: bool,
        p: Prop,
        env: Env,
    ) -> Lay {
        let env = Env { axis: Some(vertical), ..env };
        let (pm, pc) = if vertical { (p.h, p.w) } else { (p.w, p.h) };
        let mk = |m: Option<f64>| if vertical { Prop { w: pc, h: m } } else { Prop { w: m, h: pc } };
        let main_cross = |s: Size| if vertical { (s.h, s.w) } else { (s.w, s.h) };
        let spacer = |c: &ViewNode| matches!(c.kind, ViewKind::Spacer { .. });
        let sp = spacing.unwrap_or(DEFAULT_SPACING);
        let gaps: Vec<f64> = children
            .windows(2)
            .map(|w| if spacer(&w[0]) || spacer(&w[1]) { 0.0 } else { sp })
            .collect();
        let total_gap: f64 = gaps.iter().sum();

        let mut lays: Vec<Option<Lay>> = children.iter().map(|_| None).collect();
        match pm.filter(|m| m.is_finite()) {
            // Unconstrained (or probing with infinity): children take their ideal size.
            None => {
                for (i, c) in children.iter().enumerate() {
                    lays[i] = Some(self.view(c, mk(pm), env));
                }
            }
            Some(m) => {
                let mut order: Vec<(usize, f64)> = children
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let lo = main_cross(self.view(c, mk(Some(0.0)), env).size).0;
                        let hi = main_cross(self.view(c, mk(Some(INF)), env).size).0;
                        (i, hi - lo)
                    })
                    .collect();
                // SwiftUI sizes spacers last: other children share the space among
                // themselves first, spacers split whatever is left.
                order.sort_by(|a, b| {
                    (spacer(&children[a.0]), a.1)
                        .partial_cmp(&(spacer(&children[b.0]), b.1))
                        .unwrap_or(Ordering::Equal)
                });
                let mut remaining = m - total_gap;
                let n_spacers = children.iter().filter(|c| spacer(c)).count();
                let mut left = [children.len() - n_spacers, n_spacers];
                for (i, _) in order {
                    let group = spacer(&children[i]) as usize;
                    let share = (remaining / left[group] as f64).max(0.0);
                    let l = self.view(&children[i], mk(Some(share)), env);
                    remaining -= main_cross(l.size).0;
                    left[group] -= 1;
                    lays[i] = Some(l);
                }
            }
        }

        let lays: Vec<Lay> = lays.into_iter().map(Option::unwrap).collect();
        let cross = lays.iter().map(|l| main_cross(l.size).1).fold(0.0, f64::max);
        let frac = if vertical { h_frac(align) } else { v_frac(align) };
        let mut pos = 0.0;
        let mut kids = Vec::with_capacity(lays.len());
        for (i, l) in lays.into_iter().enumerate() {
            let (m, c) = main_cross(l.size);
            let off = (cross - c) * frac;
            kids.push(if vertical { (off, pos, l) } else { (pos, off, l) });
            pos += m + gaps.get(i).copied().unwrap_or(0.0);
        }
        let size = if vertical { Size { w: cross, h: pos } } else { Size { w: pos, h: cross } };
        Lay::new(size, Some(Node::new(&v.id, kind)), kids)
    }
}

/// Color / Rectangle: take whatever is proposed (10pt when unspecified).
fn greedy(p: Prop) -> Size {
    Size { w: p.w.unwrap_or(10.0), h: p.h.unwrap_or(10.0) }
}

/// Proposal passed to the child of a frame modifier along one axis.
fn child_prop(prop: Option<f64>, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> Option<f64> {
    match (fixed, prop) {
        (Some(f), _) => Some(f),
        (None, Some(p)) => Some(p.clamp(min.unwrap_or(0.0), max.unwrap_or(INF).max(min.unwrap_or(0.0)))),
        (None, None) => None,
    }
}

/// Resulting frame length along one axis. A `max: .infinity` frame fills the proposal.
fn frame_len(prop: Option<f64>, child: f64, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> f64 {
    if let Some(f) = fixed {
        return f;
    }
    let mut r = child;
    match (max, prop) {
        (Some(m), Some(p)) if m == INF => r = p.max(child),
        (Some(m), _) if m.is_finite() => r = r.min(m),
        _ => {}
    }
    if let Some(mn) = min {
        r = r.max(mn);
    }
    r
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const HOME: &str = include_str!("../../tests/data/home.json");
    const CONTROLS: &str = include_str!("../../tests/data/controls.json");
    const TODOS: &str = include_str!("../../tests/data/todos.json");
    const TYPOGRAPHY: &str = include_str!("../../tests/data/typography.json");

    fn scene_with(json: &str, vp: (f64, f64), dark: bool, safe: (f64, f64), scroll: &HashMap<String, (f64, f64)>, focus: Option<&str>) -> Scene {
        let v: ViewNode = serde_json::from_str(json).unwrap();
        let params = SceneParams {
            viewport: vp,
            safe_top: safe.0,
            safe_bottom: safe.1,
            scale: 3.0,
            appearance: if dark { Appearance::Dark } else { Appearance::Light },
            dynamic_type: DynamicTypeSize::Large,
            scroll,
            focus,
            overscroll: false,
            status_bar: safe.0 > 0.0,
            chrome: Default::default(),
        };
        layout(&v, &params, &mut Fonts::new())
    }

    fn scene(json: &str, vp: (f64, f64)) -> Scene {
        scene_with(json, vp, false, (0.0, 0.0), &HashMap::new(), None)
    }

    fn phone(json: &str) -> Scene {
        scene_with(json, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), None)
    }

    fn by_id<'a>(s: &'a Scene, id: &str) -> &'a Node {
        s.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 0.2, "{a} != {b}");
    }

    #[test]
    fn spacer_pushes_and_padding_applies() {
        let s = scene(
            r#"{"kind":"vstack","id":"v","spacing":0,"modifiers":[{"padding":{"top":10,"leading":10,"bottom":10,"trailing":10}},{"frame":{"maxWidth":"inf","maxHeight":"inf"}}],
                "children":[{"kind":"color","id":"a","color":{"r":1,"g":0,"b":0,"a":1},"modifiers":[{"frame":{"height":50}}]},
                            {"kind":"spacer","id":"sp"},
                            {"kind":"color","id":"b","color":{"r":0,"g":0,"b":1,"a":1},"modifiers":[{"frame":{"width":30,"height":50}}]}]}"#,
            (200.0, 400.0),
        );
        let a = by_id(&s, "a").bounds;
        let b = by_id(&s, "b").bounds;
        assert_eq!((a.x, a.y, a.h), (10.0, 10.0, 50.0));
        assert_eq!(a.w, 180.0);
        assert_eq!(b.y, 340.0); // pushed to bottom: 400 - 10 - 50
        assert_eq!(b.w, 30.0);
    }

    #[test]
    fn text_beats_spacer_for_space() {
        // HStack { Text(long); Spacer() } in 300pt: text must not wrap at half width.
        let json = r#"{"kind":"hstack","id":"h","alignment":"center","children":[
            {"kind":"text","id":"t","text":"Ship the SwiftUI subset"},
            {"kind":"spacer","id":"s"}]}"#;
        let s = scene(json, (300.0, 400.0));
        let t = by_id(&s, "t");
        assert!(t.bounds.h < 30.0, "text wrapped: {:?}", t.bounds);
    }

    #[test]
    fn deep_nesting_is_fast() {
        let mut json = r#"{"kind":"text","id":"t","text":"leaf"}"#.to_string();
        for i in 0..30 {
            json = format!(r#"{{"kind":"vstack","id":"v{i}","alignment":"center","children":[{json},{{"kind":"spacer","id":"s{i}"}}]}}"#);
        }
        let t = std::time::Instant::now();
        let s = scene(&json, (393.0, 852.0));
        assert!(by_id(&s, "t").bounds.w > 0.0);
        assert!(t.elapsed().as_secs() < 5, "layout is exponential in depth");
    }

    #[test]
    fn button_hit_and_clip() {
        let s = scene(
            r#"{"kind":"button","id":"b","action":3,"label":{"kind":"text","id":"t","text":"Go",
                "modifiers":[{"padding":{"top":8,"leading":8,"bottom":8,"trailing":8}},{"background":{"kind":"color","id":"bg","color":{"semantic":"accent"}}}]},
                "modifiers":[{"cornerRadius":10}]}"#,
            (200.0, 200.0),
        );
        let b = by_id(&s, "b");
        assert_eq!(b.action, Some(3));
        let (cx, cy) = b.bounds.center();
        assert_eq!(s.hit_test(cx, cy).map(|i| s.nodes[i].id.as_str()), Some("b"));
        // the very corner is outside the rounded clip
        assert_eq!(s.hit_test(b.bounds.x + 0.2, b.bounds.y + 0.2), None);
        assert_eq!(s.hit_test(1.0, 1.0), None);
    }

    /// Frames measured on an iOS 26 simulator (tests/fixtures/showcase-ios/README.md).
    #[test]
    fn inset_grouped_list_matches_ios_frames() {
        let s = phone(HOME);
        let row = by_id(&s, "l.0");
        near(row.bounds.x, 16.0);
        near(row.bounds.y, 208.3);
        near(row.bounds.w, 370.0);
        near(row.bounds.h, 52.0);
        near(by_id(&s, "l.1").bounds.y, 260.3);
        near(by_id(&s, "l.3").bounds.y, 364.3);
        // header (17pt semibold text, 40.3pt cell) then card
        near(by_id(&s, "l#s0#header").bounds.h, 20.3);
        near(by_id(&s, "l#s1").bounds.y, 474.3);
        // card: 4 rows, radius 26, secondary grouped background
        let card = by_id(&s, "l#s0");
        near(card.bounds.h, 208.0);
        assert_eq!(card.radius, CARD_RADIUS);
        assert_eq!(card.fill, Some(rgba(255, 255, 255, 1.0)));
        assert_eq!(by_id(&s, "l").fill, Some(rgba(242, 242, 247, 1.0)));
        // chevron 7x12, right edge 367.7
        let ch = by_id(&s, "l.0#chevron").bounds;
        near(ch.x, 360.7);
        near(ch.w, 7.0);
        near(ch.h, 12.0);
        // 3 separators in the 4-row card, none after the last row; inset to text (x=32)
        let seps: Vec<_> = s.nodes.iter().filter(|n| n.kind == "separator").collect();
        assert_eq!(seps.len(), 3);
        near(seps[0].bounds.x, 32.0);
        near(seps[0].bounds.x + seps[0].bounds.w, 370.0);
        near(seps[0].bounds.y + seps[0].bounds.h, 260.3);
        // the whole row is the hit target, e.g. right next to the chevron
        let hit = s.hit_test(383.0, 234.0).map(|i| s.nodes[i].id.as_str());
        assert_eq!(hit, Some("l.0"));
        // text metrics: 17pt label is 20.3pt tall
        near(by_id(&s, "l.0.l").bounds.h, 20.33);
        near(by_id(&s, "l.0.l").bounds.x, 32.0);
    }

    #[test]
    fn dark_list_colors() {
        let v: ViewNode = serde_json::from_str(HOME).unwrap();
        let s = layout(
            &v,
            &SceneParams {
                viewport: (402.0, 874.0),
                safe_top: 62.0,
                safe_bottom: 34.0,
                scale: 3.0,
                appearance: Appearance::Dark,
                dynamic_type: DynamicTypeSize::Large,
                scroll: &HashMap::new(),
                focus: None,
                overscroll: false,
                status_bar: true,
                chrome: Default::default(),
            },
            &mut Fonts::new(),
        );
        assert_eq!(by_id(&s, "l#s0").fill, Some(rgba(28, 28, 30, 1.0)));
        assert_eq!(by_id(&s, "l").fill, Some(rgba(0, 0, 0, 1.0)));
        assert_eq!(by_id(&s, "root").fill, Some(rgba(0, 0, 0, 1.0)));
    }

    #[test]
    fn large_title_collapses_into_the_bar() {
        // The HOME list is short; a long one so it can scroll past the 52pt collapse.
        let rows: Vec<String> = (0..30).map(|i| format!(r#"{{"kind":"text","id":"r{i}","text":"Row {i}"}}"#)).collect();
        let json = format!(
            r#"{{"kind":"navigation","id":"n","title":"Long","displayMode":"large","content":{{"kind":"list","id":"l","style":"insetGrouped","sections":[{{"rows":[{}]}}]}}}}"#,
            rows.join(",")
        );
        let at = |off: f64| {
            let mut m = HashMap::new();
            m.insert("l".to_string(), (0.0, off));
            scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &m, None)
        };
        let s = at(0.0);
        near(by_id(&s, "n#title").bounds.y, 119.7);
        assert_eq!(by_id(&s, "l").scroll.unwrap().snap, Some(52.0));
        assert!(s.nodes.iter().all(|n| n.id != "n#inline"), "no inline title while expanded");
        // halfway: the title moved up with the content, the bar shrank, the inline title not yet in
        let s = at(26.0);
        near(by_id(&s, "n#title").bounds.y, 119.7 - 26.0);
        near(by_id(&s, "n#bar").bounds.h, 80.0);
        assert!(s.nodes.iter().all(|n| n.id != "n#inline"));
        // collapsed: large title gone, inline title fully in, inline-height bar
        let s = at(52.0);
        assert!(s.nodes.iter().all(|n| n.id != "n#title"));
        assert_eq!(by_id(&s, "n#inline").opacity, 1.0);
        near(by_id(&s, "n#bar").bounds.h, 54.0);
    }

    #[test]
    fn navigation_bars() {
        let s = phone(HOME);
        // large title below the status bar, left aligned at 16
        let t = by_id(&s, "n#title");
        near(t.bounds.x, 16.0);
        near(t.bounds.y, 119.7);
        near(t.bounds.h, 40.67);
        assert_eq!(t.text.as_ref().unwrap().font.weight, 700);
        near(by_id(&s, "n#bar").bounds.h, 106.0);
        // nav + list are full-bleed
        assert_eq!(by_id(&s, "n").bounds.h, 874.0);
        // the list scrolls under the bar: it spans the screen, content starts below the bar
        near(by_id(&s, "l").bounds.y, 0.0);
        assert!(by_id(&s, "l#s0").bounds.y >= 168.0);
        // inline title is centered at 17pt semibold, back button is a 44pt circle
        let s = phone(TYPOGRAPHY);
        let t = by_id(&s, "n#title");
        near(t.bounds.x + t.bounds.w / 2.0, 201.0);
        near(t.bounds.y, 73.7);
        assert_eq!(t.text.as_ref().unwrap().font.weight, 600);
        near(by_id(&s, "n#bar").bounds.h, 54.0);
        let b = by_id(&s, "n#back");
        assert_eq!((b.bounds.x, b.bounds.y, b.bounds.w, b.bounds.h), (16.0, 62.0, 44.0, 44.0));
        assert_eq!(b.action, Some(99));
        // scroll content starts 16pt below the bar (padding)
        near(by_id(&s, "v.0").bounds.y, 132.0);
        near(by_id(&s, "v.3").bounds.h, 42.33); // 2 wrapped lines: 20.3 + 22 pitch
        // status bar chrome is drawn
        assert!(s.nodes.iter().any(|n| n.id == "statusbar#time"));
    }

    #[test]
    fn toggle_stepper_and_buttons() {
        let s = phone(CONTROLS);
        let sw = by_id(&s, "l.0#switch");
        near(sw.bounds.w, 63.0);
        near(sw.bounds.h, 28.0);
        near(sw.bounds.x, 309.0);
        near(sw.bounds.y, 180.0);
        assert_eq!(sw.fill, Some(rgba(52, 199, 89, 1.0)));
        assert_eq!(sw.value.as_deref(), Some("on"));
        // switch hit target; label area does not toggle
        assert_eq!(s.hit_test(340.0, 194.0).map(|i| s.nodes[i].id.as_str()), Some("l.0#switch"));
        assert_eq!(s.hit_test(60.0, 194.0), None);
        let st = by_id(&s, "l.1#control");
        near(st.bounds.h, 32.0);
        near(st.bounds.x, 276.0);
        near(by_id(&s, "l.1#increment").bounds.w, 47.0);
        // bordered buttons: 34.3pt tall in a 64.3pt row
        near(by_id(&s, "l.2.0").bounds.h, 34.33);
        near(by_id(&s, "l.2.2").bounds.h, 34.33);
        near(by_id(&s, "l#s0#r2").bounds.h, 64.33);
        assert_eq!(by_id(&s, "l.2.2").fill, Some(rgba(0, 136, 255, 1.0)));
    }

    #[test]
    fn tinted_button_label_stays_accent_and_disabled_is_gray() {
        let s = phone(TODOS);
        // done todo: strikethrough text in accent at half strength even with .secondary
        let t = by_id(&s, "l.0.t").text.as_ref().unwrap();
        assert!(t.strike);
        assert!((t.color.b - 1.0).abs() < 1e-6 && (t.color.a - 0.5).abs() < 1e-6);
        // the whole todo row is the button; separators start after the 22pt circle (x=66)
        let r = by_id(&s, "l.0");
        near(r.bounds.w, 370.0);
        let sep = s.nodes.iter().find(|n| n.kind == "separator").unwrap();
        near(sep.bounds.x, 66.0);
        // disabled Add: no action, tertiary text
        let add = by_id(&s, "l.a.b");
        assert!(add.action.is_none() && add.disabled);
        assert_eq!(s.nodes.iter().find(|n| n.id == "l.a.bl").unwrap().text.as_ref().unwrap().color.a, 0.3);
        // text field fills the rest of the row: 299.3 wide at x=32
        let f = by_id(&s, "l.a.f");
        near(f.bounds.x, 32.0);
        assert!((f.bounds.w - 299.3).abs() < 1.0, "field width {}", f.bounds.w); // text widths are within ~1px of iOS
        near(f.bounds.h, 22.0);
        assert_eq!(f.field, Some(1));
        // first headerless section starts right under the nav bar
        near(by_id(&s, "l#s0").bounds.y, 168.0);
        near(by_id(&s, "l#s1#header").bounds.y, 237.7 + 10.0); // 40.3pt header cell, text centred
    }

    #[test]
    fn text_field_focus_draws_caret_and_value() {
        let json = TODOS.replace(r#""text": """#, r#""text": "hello""#);
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), Some("l.a.f"));
        let f = by_id(&s, "l.a.f");
        assert!(f.focused);
        assert_eq!(f.value.as_deref(), Some("hello"));
        let caret = by_id(&s, "l.a.f#caret");
        assert!(caret.bounds.x > f.bounds.x + 20.0, "caret follows the text end");
        // unfocused: no caret
        let s = phone(&json);
        assert!(s.nodes.iter().all(|n| n.id != "l.a.f#caret"));
    }

    #[test]
    fn scroll_offsets_move_and_clip_content() {
        let none = phone(TYPOGRAPHY);
        let y0 = by_id(&none, "v.2").bounds.y;
        let mut off = HashMap::new();
        off.insert("sc".to_string(), (0.0, 40.0));
        let s = scene_with(TYPOGRAPHY, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        // content is short: the offset clamps to the scrollable range (none here)
        let sc = by_id(&s, "sc");
        assert_eq!(sc.scroll.unwrap().max, (0.0, 0.0));
        near(by_id(&s, "v.2").bounds.y, y0);
    }

    #[test]
    fn long_list_scrolls_and_clips_hit_tests() {
        let rows: Vec<String> = (0..30)
            .map(|i| format!(r#"{{"kind":"button","id":"r{i}","action":{i},"style":"navigationLink","label":{{"kind":"text","id":"r{i}.t","text":"Row {i}"}}}}"#))
            .collect();
        let json = format!(
            r#"{{"kind":"navigation","id":"n","title":"Long","displayMode":"large","content":{{"kind":"list","id":"l","style":"insetGrouped","sections":[{{"rows":[{}]}}]}}}}"#,
            rows.join(",")
        );
        let s = phone(&json);
        let sc = by_id(&s, "l").scroll.unwrap();
        assert!(sc.max.1 > 500.0, "content taller than the list: {:?}", sc);
        // row 20 is below the screen: clipped, cannot be hit
        assert_eq!(s.hit_test(100.0, by_id(&s, "r20").bounds.y + 10.0), None);
        // scrolled by 300: row 6 (y=168+312) moves to ~180
        let mut off = HashMap::new();
        off.insert("l".to_string(), (0.0, 300.0));
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        near(by_id(&s, "r6").bounds.y, 168.0 + 6.0 * 52.0 - 300.0);
        assert_eq!(s.scroll_target(200.0, 500.0).map(|i| s.nodes[i].id.as_str()), Some("l"));
        // rows scrolled under the nav bar are clipped: no hit at y=100
        assert_eq!(s.hit_test(100.0, 100.0), None);
        // bottom: last row clears the home indicator inset
        off.insert("l".to_string(), (0.0, 1.0e6));
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        let last = by_id(&s, "r29").bounds;
        assert!(last.y + last.h <= 874.0 - 34.0 + 0.1);
    }

    #[test]
    fn shapes_stroke_overlay_shadow() {
        let s = phone(include_str!("../../tests/data/shapes.json"));
        let c = by_id(&s, "h.0");
        assert_eq!((c.bounds.w, c.bounds.h), (60.0, 60.0));
        assert!(c.radius >= 1.0e5);
        let r = by_id(&s, "r");
        assert_eq!(r.stroke.map(|s| s.1), Some(4.0));
        assert!(r.fill.is_none());
        // overlay text is centred over the rect
        let o = by_id(&s, "r.o");
        near(o.bounds.x + o.bounds.w / 2.0, r.bounds.x + r.bounds.w / 2.0);
        // shadow follows the 12pt corner radius of the card
        let sh = s.nodes.iter().find(|n| n.shadow.is_some() && n.id.starts_with("c#shadow")).unwrap();
        assert_eq!(sh.radius, 12.0);
        // divider hairline in a vstack spans the width
        let t = phone(TYPOGRAPHY);
        let d = by_id(&t, "v.8").bounds;
        near(d.w, 370.0);
        near(d.h, 1.0 / 3.0);
    }

    #[test]
    fn centered_text_aligns_within_its_box() {
        let s = phone(TYPOGRAPHY);
        let t = by_id(&s, "v.9");
        let run = t.text.as_ref().unwrap();
        assert_eq!(run.font.align, TextAlign::Center);
        near(t.bounds.h, 42.33);
        let mid = t.bounds.x + t.bounds.w / 2.0;
        near(mid, 201.0);
    }

    const SYMBOLS: &str = r#"{"kind":"navigation","id":"n","title":"S","displayMode":"large","content":{"kind":"list","id":"l","style":"insetGrouped","sections":[
        {"rows":[{"kind":"label","id":"lab","title":{"kind":"text","id":"lab.t","text":"Favorites"},"icon":{"kind":"image","id":"lab.i","systemName":"star.fill","resizable":false}},
                 {"kind":"label","id":"bad","title":{"kind":"text","id":"bad.t","text":"Bad"},"icon":{"kind":"image","id":"bad.i","systemName":"no.such.symbol.xyz","resizable":false},
                  "modifiers":[{"foregroundColor":{"semantic":"secondary"}}]}]},
        {"rows":[{"kind":"hstack","id":"h","spacing":20,"alignment":"center","children":[
            {"kind":"image","id":"h.0","systemName":"heart.fill","resizable":false},
            {"kind":"image","id":"h.1","systemName":"cloud.sun.fill","resizable":true,"contentMode":"fit","modifiers":[{"frame":{"width":44,"height":44}}]},
            {"kind":"image","id":"h.2","systemName":"checkmark.circle.fill","resizable":false,"modifiers":[{"imageScale":"large"}]}]}]}]}}"#;

    /// Frames measured on iOS 26 (tests/fixtures/showcase-ios/symbols.json): Label icons are
    /// imageScale large, centred 12pt into the row, title at +40; the tall HStack row gets 15pt padding.
    #[test]
    fn symbols_size_like_ios_and_unknown_ones_are_reported() {
        let s = phone(SYMBOLS);
        let cap = crate::symbols::capability();
        let system = cap == "system";
        // Unknown names fall back to an outlined placeholder and are reported.
        let bad = by_id(&s, "bad.i");
        assert!(bad.symbol.is_none() && bad.stroke.is_some());
        assert!(s.unsupported_symbols.contains(&"no.such.symbol.xyz".to_string()));
        assert_eq!(s.unsupported_symbols.len(), if cap == "placeholder" { 5 } else { 1 });
        if !system {
            return; // placeholder host: no glyph metrics to check
        }
        let star = by_id(&s, "lab.i");
        assert!(star.symbol.is_some());
        let c = star.bounds.center();
        near(c.0, 44.0);
        near(star.bounds.w, 28.0); // large star.fill box; ink is 24 wide
        near(by_id(&s, "lab.t").bounds.x, 72.0);
        // accent icon, primary title; an explicit foreground colors both (here secondary on the bad row)
        assert_eq!(star.symbol.as_ref().unwrap().color, rgba(0, 136, 255, 1.0));
        assert_eq!(by_id(&s, "lab.t").text.as_ref().unwrap().color, rgba(0, 0, 0, 1.0));
        assert_eq!(by_id(&s, "bad.i").stroke.map(|s| s.0), by_id(&s, "bad.t").text.as_ref().map(|t| t.color));
        // heart.fill at 17pt: 21x18 box (16.7x15.3 ink on iOS); large is bigger than medium
        let heart = by_id(&s, "h.0").bounds;
        near(heart.w, 21.0);
        near(heart.h, 18.0);
        assert!(by_id(&s, "h.2").bounds.w > 24.0);
        // resizable scaledToFit keeps the symbol's aspect inside the 44pt frame
        let cloud = by_id(&s, "h.1").bounds;
        near(cloud.w, 44.0);
        assert!(cloud.h < 40.0 && cloud.h > 30.0, "{cloud:?}");
        // 44pt content: 74pt row (15pt padding)
        near(by_id(&s, "l#s1#r0").bounds.h, 74.0);
    }

    #[test]
    fn weights_are_distinct() {
        let mut f = Fonts::new();
        let w = |f: &mut Fonts, wt| f.measure("Hamburgefonstiv", &FontSpec::new(17.0, 20.33, wt, TextAlign::Leading), None).0;
        let (r, sb, b) = (w(&mut f, 400), w(&mut f, 600), w(&mut f, 700));
        if f.has_sf() {
            assert!(r < sb && sb < b, "variable wght axis applies: {r} {sb} {b}");
        }
    }
}
