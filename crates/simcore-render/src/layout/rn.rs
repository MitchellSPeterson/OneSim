//! React Native layout: Fabric's flat props -> Yoga-style flexbox (via taffy) -> `Scene`.
//!
//! Node ids in the scene are the reactTags; every RN box has `action = Some(tag)` so the scene's
//! hit test returns the deepest tag under a point. Content starts at y=0 (RN handles safe areas).

use super::swiftui::{add_chrome, switch_style, SceneParams, KNOB, SWITCH};
use crate::scene::{Color, ImageRun, Node, Rect, Role, Scene, ScrollInfo, TextRun};
use crate::text::{FontSpec, Fonts, Rich, Span, TextAlign};
use serde_json::Value;
use simcore_protocol::{Appearance, ViewKind, ViewNode};
use std::collections::BTreeSet;
use taffy::prelude::*;
use taffy::compute_leaf_layout;

/// Text leaf: what the measure function and the painter need.
#[derive(Clone)]
struct Text {
    text: String,
    spec: FontSpec,
    color: Color,
    strike: bool,
    /// Text input: one line unless multiline, never shorter than a line.
    field: bool,
    single: bool,
    /// Intrinsic size of a control (`RCTSwitch`).
    fixed: Option<(f32, f32)>,
    rich: Option<Rich>,
}

struct Item<'a> {
    node: &'a ViewNode,
    props: &'a Value,
    id: NodeId,
    kids: Vec<Item<'a>>,
}

struct Cx<'a, 'f> {
    tree: TaffyTree<Text>,
    notes: BTreeSet<String>,
    scale: f64,
    dark: bool,
    fonts: &'f mut Fonts,
    /// Visible `RCTModalHostView`s: laid out against the viewport and painted last.
    modals: Vec<Item<'a>>,
}

#[derive(Clone, Copy)]
enum V {
    Auto,
    Pt(f32),
    Pct(f32),
}

fn val(v: Option<&Value>) -> V {
    match v {
        Some(Value::Number(n)) => V::Pt(n.as_f64().unwrap_or(0.0) as f32),
        Some(Value::String(s)) => match s.strip_suffix('%').and_then(|p| p.trim().parse::<f32>().ok()) {
            Some(p) => V::Pct(p / 100.0),
            None => s.parse().map_or(V::Auto, V::Pt),
        },
        _ => V::Auto,
    }
}

impl V {
    fn dim(self) -> Dimension {
        match self {
            V::Auto => auto(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
    fn lpa(self) -> LengthPercentageAuto {
        match self {
            V::Auto => auto(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
    fn lp(self) -> LengthPercentage {
        match self {
            V::Auto => zero(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
}

fn num(p: &Value, k: &str) -> Option<f64> {
    p.get(k).and_then(Value::as_f64)
}

/// Yoga edge precedence: edge > start/end > horizontal/vertical > all. Order: top, right, bottom, left.
fn edges<'a>(p: &'a Value, b: &str, sfx: &str) -> [Option<&'a Value>; 4] {
    let g = |side: &str| p.get(format!("{b}{side}{sfx}")).filter(|v| !v.is_null());
    let (h, v) = (g("Horizontal").or(g("")), g("Vertical").or(g("")));
    [g("Top").or(v), g("Right").or(g("End")).or(h), g("Bottom").or(v), g("Left").or(g("Start")).or(h)]
}

fn rect<T>(e: [Option<&Value>; 4], f: impl Fn(V) -> T) -> taffy::Rect<T> {
    // Unset margin/padding/border is 0, only an explicit "auto" is auto.
    let [top, right, bottom, left] = e.map(|v| f(v.map_or(V::Pt(0.0), |v| val(Some(v)))));
    taffy::Rect { top, right, bottom, left }
}

fn align(s: Option<&str>, default: AlignItems) -> AlignItems {
    match s {
        Some("flex-start") => AlignItems::FLEX_START,
        Some("flex-end") => AlignItems::FLEX_END,
        Some("center") => AlignItems::CENTER,
        Some("baseline") => AlignItems::BASELINE,
        Some("stretch") => AlignItems::STRETCH,
        _ => default,
    }
}

fn justify(s: Option<&str>, default: AlignContent) -> AlignContent {
    match s {
        Some("flex-start") => AlignContent::FLEX_START,
        Some("flex-end") => AlignContent::FLEX_END,
        Some("center") => AlignContent::CENTER,
        Some("stretch") => AlignContent::STRETCH,
        Some("space-between") => AlignContent::SPACE_BETWEEN,
        Some("space-around") => AlignContent::SPACE_AROUND,
        Some("space-evenly") => AlignContent::SPACE_EVENLY,
        _ => default,
    }
}

fn style(p: &Value) -> Style {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let wrap = matches!(s("flexWrap"), Some("wrap" | "wrap-reverse"));
    let horizontal = p.get("horizontal").and_then(Value::as_bool) == Some(true);
    let flex = num(p, "flex");
    // Yoga: flexGrow/flexShrink/flexBasis override `flex`; positive flex = grow n, basis 0; negative = shrink.
    let grow = num(p, "flexGrow").or(flex.filter(|f| *f > 0.0)).unwrap_or(0.0);
    let shrink = num(p, "flexShrink").or(flex.filter(|f| *f < 0.0).map(|f| -f)).unwrap_or(0.0);
    let basis = match p.get("flexBasis") {
        Some(b) if !b.is_null() => val(Some(b)).dim(),
        _ if flex.is_some_and(|f| f > 0.0) => length(0.0),
        _ => auto(),
    };
    let gap = |k: &str| val(p.get(k).filter(|v| !v.is_null()).or(p.get("gap"))).lp();
    let zero_min = |k: &str| match val(p.get(k)) {
        V::Auto => length(0.0),
        v => v.lpa(),
    };
    Style {
        display: if s("display") == Some("none") { Display::None } else { Display::Flex },
        position: if s("position") == Some("absolute") { Position::Absolute } else { Position::Relative },
        flex_direction: match s("flexDirection") {
            Some("row") => FlexDirection::Row,
            Some("row-reverse") => FlexDirection::RowReverse,
            Some("column-reverse") => FlexDirection::ColumnReverse,
            None if horizontal => FlexDirection::Row,
            _ => FlexDirection::Column,
        },
        flex_wrap: match s("flexWrap") {
            Some("wrap") => FlexWrap::Wrap,
            Some("wrap-reverse") => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        },
        align_items: align(s("alignItems"), AlignItems::STRETCH),
        align_self: s("alignSelf").filter(|a| *a != "auto").map(|a| align(Some(a), AlignItems::STRETCH)),
        // Yoga applies alignContent to wrapped containers only (RN default flex-start); taffy would shrink the single line.
        align_content: if wrap { justify(s("alignContent"), AlignContent::FLEX_START) } else { AlignContent::STRETCH },
        justify_content: justify(s("justifyContent"), AlignContent::FLEX_START),
        gap: taffy::Size { width: gap("columnGap"), height: gap("rowGap") },
        margin: rect(edges(p, "margin", ""), V::lpa),
        padding: rect(edges(p, "padding", ""), V::lp),
        border: rect(edges(p, "border", "Width"), V::lp),
        inset: taffy::Rect {
            top: val(p.get("top")).lpa(),
            right: val(p.get("right")).lpa(),
            bottom: val(p.get("bottom")).lpa(),
            left: val(p.get("left")).lpa(),
        },
        size: taffy::Size { width: val(p.get("width")).dim(), height: val(p.get("height")).dim() },
        // Yoga has no CSS "automatic minimum size": unset min is 0.
        min_size: taffy::Size { width: zero_min("minWidth"), height: zero_min("minHeight") },
        max_size: taffy::Size { width: val(p.get("maxWidth")).lpa(), height: val(p.get("maxHeight")).lpa() },
        flex_grow: grow as f32,
        flex_shrink: shrink as f32,
        flex_basis: basis,
        aspect_ratio: num(p, "aspectRatio").map(|a| a as f32),
        box_sizing: BoxSizing::BorderBox,
        ..Style::default()
    }
}

fn weight(p: &Value) -> u16 {
    match p.get("fontWeight") {
        Some(Value::String(s)) => match s.as_str() {
            "bold" => 700,
            "normal" => 400,
            n => n.parse().unwrap_or(400),
        },
        Some(Value::Number(n)) => n.as_u64().unwrap_or(400) as u16,
        _ => 400,
    }
}

impl<'a> Cx<'a, '_> {
    fn note(&mut self, m: impl Into<String>) {
        self.notes.insert(m.into());
    }

    /// RN colors are 0xAARRGGBB numbers (`processColor`); anything else is ignored and reported once.
    fn color(&mut self, v: Option<&Value>) -> Option<Color> {
        let v = v.filter(|v| !v.is_null())?;
        let Some(n) = v.as_u64().or(v.as_i64().map(|i| i as u32 as u64)).filter(|n| *n <= u32::MAX as u64) else {
            self.note("unsupported_color: non-numeric color (PlatformColor/DynamicColorIOS?) ignored");
            return None;
        };
        let c = |shift: u32| ((n >> shift) & 0xff) as f32 / 255.0;
        Some(Color::rgba(c(16), c(8), c(0), c(24)))
    }

    fn line_h(&self, size: f32) -> f32 {
        let s = self.scale as f32;
        (size * 1.1934 * s).ceil() / s
    }

    /// Text of a paragraph subtree; `RCTVirtualText` spans that set `color`/`fontWeight` become spans (inherited by nested spans).
    // ponytail: spans only restyle color and weight; per-span fontSize/fontStyle/fontFamily, letterSpacing,
    // underline are ignored; `numberOfLines` always ellipsizes at the tail (`ellipsizeMode` head/middle/clip too).
    fn collect(&mut self, n: &ViewNode, out: &mut String, spans: &mut Vec<Span>, inherit: (Option<[u8; 3]>, Option<u16>)) {
        let ViewKind::Rn { component, props, children } = &n.kind else { return };
        let mut inh = inherit;
        if component == "RCTVirtualText" {
            if let Some(c) = self.color(props.get("color")) {
                inh.0 = Some([(c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8]);
            }
            if props.get("fontWeight").is_some_and(|w| !w.is_null()) {
                inh.1 = Some(weight(props));
            }
        }
        if component == "RCTRawText" {
            let t = props.get("text").and_then(Value::as_str).unwrap_or("");
            if inh != (None, None) {
                spans.push(Span { start: out.len(), end: out.len() + t.len(), color: inh.0, weight: inh.1 });
            }
            out.push_str(t);
        }
        for c in children {
            self.collect(c, out, spans, inh);
        }
    }

    fn text(&mut self, node: &ViewNode, p: &Value) -> Text {
        let mut text = String::new();
        let mut spans = vec![];
        self.collect(node, &mut text, &mut spans, (None, None));
        let lines = num(p, "numberOfLines").filter(|n| *n >= 1.0).map(|n| n as usize);
        let rich = (lines.is_some() || !spans.is_empty()).then_some(Rich { spans, lines });
        let size = num(p, "fontSize").unwrap_or(14.0) as f32;
        let align = match p.get("textAlign").and_then(Value::as_str) {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Trailing,
            _ => TextAlign::Leading,
        };
        let mut spec = FontSpec::new(size, self.line_h(size), weight(p), align);
        spec.pitch = spec.line_height; // RN lines are one font line height apart (SwiftUI's 22/17 leading does not apply)
        if let Some(lh) = num(p, "lineHeight").filter(|l| *l > 0.0) {
            (spec.line_height, spec.pitch) = (lh as f32, lh as f32);
        }
        let color = self.color(p.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let strike = p.get("textDecorationLine").and_then(Value::as_str).is_some_and(|d| d.contains("line-through"));
        Text { text, spec, color, strike, field: false, single: false, fixed: None, rich }
    }

    fn build(&mut self, node: &'a ViewNode) -> Option<Item<'a>> {
        let ViewKind::Rn { component, props, children } = &node.kind else { return None };
        let input = matches!(component.as_str(), "RCTSinglelineTextInputView" | "RCTMultilineTextInputView");
        match component.as_str() {
            "RCTRawText" | "RCTVirtualText" => return None,
            "RootView" | "RCTView" | "RCTScrollView" | "RCTScrollContentView" | "RCTImageView" | "RCTText" | "RCTSwitch"
            | "RCTModalHostView" => {}
            _ if input => {}
            other => self.note(format!("unsupported_component: {other}")),
        }
        let st = style(props);
        if component == "RCTText" {
            let t = self.text(node, props);
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item { node, props, id, kids: vec![] });
        }
        if input {
            let mut t = self.text(node, props);
            let value = props.get("__simcoreText").and_then(Value::as_str).unwrap_or("");
            t.text = if value.is_empty() {
                props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
            } else if props.get("secureTextEntry").and_then(Value::as_bool) == Some(true) {
                "\u{2022}".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            (t.field, t.single) = (true, component == "RCTSinglelineTextInputView");
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item { node, props, id, kids: vec![] });
        }
        if component == "RCTSwitch" {
            let t = Text {
                text: String::new(),
                spec: FontSpec::new(14.0, 17.0, 400, TextAlign::Leading),
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                strike: false,
                field: false,
                single: false,
                fixed: Some((SWITCH.0 as f32, SWITCH.1 as f32)),
                rich: None,
            };
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item { node, props, id, kids: vec![] });
        }
        let kids: Vec<Item> = children.iter().filter_map(|c| self.build(c)).collect();
        let ids: Vec<NodeId> = kids.iter().map(|k| k.id).collect();
        let id = self.tree.new_with_children(st, &ids).ok()?;
        let item = Item { node, props, id, kids };
        if component == "RCTModalHostView" {
            if props.get("visible").and_then(Value::as_bool) != Some(false) {
                self.modals.push(item);
            }
            return None;
        }
        Some(item)
    }

    /// Lay out the subtree at `id` as a root of `w` x `h` points.
    fn compute(&mut self, id: NodeId, w: f64, h: f64) {
        let mut st = self.tree.style(id).cloned().unwrap_or_default();
        st.size = taffy::Size { width: length(w as f32), height: length(h as f32) };
        st.position = Position::Relative;
        let _ = self.tree.set_style(id, st);
        let scale = self.scale as f32;
        let fonts = &mut *self.fonts;
        let _ = self.tree.compute_layout_with_measure(
            id,
            taffy::Size { width: AvailableSpace::Definite(w as f32), height: AvailableSpace::Definite(h as f32) },
            |inp, _, t, style| {
                compute_leaf_layout(inp, style, |_, _| 0.0, |known, avail| {
                    let Some(t) = t else { return taffy::Size::ZERO };
                    if let Some((fw, fh)) = t.fixed {
                        return taffy::Size { width: known.width.unwrap_or(fw), height: known.height.unwrap_or(fh) };
                    }
                    let wrap = known.width.or(match avail.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => Some(0.0),
                        AvailableSpace::MaxContent => None,
                    });
                    let (mw, mh) = fonts.measure_rich(&t.text, &t.spec, if t.single { None } else { wrap }, t.rich.as_ref());
                    let mh = if t.field { mh.max(t.spec.line_height as f64) } else { mh };
                    // Yoga rounds text sizes up to the pixel grid.
                    let up = |x: f64| (x as f32 * scale).ceil() / scale;
                    taffy::Size { width: known.width.unwrap_or(up(mw)), height: known.height.unwrap_or(up(mh)) }
                })
            },
        );
    }
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    let dark = params.appearance == Appearance::Dark;
    let mut cx = Cx { tree: TaffyTree::new(), notes: BTreeSet::new(), scale: params.scale.max(1.0), dark, fonts, modals: vec![] };
    cx.tree.disable_rounding();
    let bg = if dark { 0.0 } else { 1.0 };
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(Color::rgba(bg, bg, bg, 1.0));
    let mut nodes = vec![screen];
    if let Some(item) = cx.build(root) {
        // The surface root fills the viewport.
        cx.compute(item.id, w, h);
        paint(&mut cx, &item, (0.0, 0.0), 0, 1.0, params, &mut nodes);
        for m in std::mem::take(&mut cx.modals) {
            cx.compute(m.id, w, h);
            let first = nodes.len();
            paint(&mut cx, &m, (0.0, 0.0), 0, 1.0, params, &mut nodes);
            if m.props.get("transparent").and_then(Value::as_bool) != Some(true) {
                nodes[first].fill = Some(Color::rgba(bg, bg, bg, 1.0));
            }
        }
    }
    // RN does not avoid the keyboard: it just covers the bottom of the screen.
    if params.chrome.keyboard && nodes.iter().any(|n| n.field.is_some() && Some(n.id.as_str()) == params.focus) {
        let kb_h = params.device.kb_base_h + params.device.safe_bottom;
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift, params.device);
    } else if params.device.safe_bottom > 0.0 && !dark {
        // Home indicator (measured on iOS 26.5: 144 x 4.7 pt, 13 pt above the bottom edge). ponytail: light only,
        // the dark references show none; it also dims/hides on iOS, which is not modelled.
        let mut bar = Node::new("home_indicator", "home_indicator");
        bar.bounds = Rect { x: (w - 144.0) / 2.0, y: h - 13.0, w: 144.0, h: 4.7 };
        bar.parent = Some(0);
        bar.radius = 3.0;
        bar.fill = Some(Color::rgba(0.0, 0.0, 0.0, 0.61));
        bar.inspect = false;
        nodes.push(bar);
    }
    add_chrome(&mut nodes, params, cx.fonts);
    Scene {
        size: params.viewport,
        appearance: params.appearance,
        nodes,
        unsupported_symbols: vec![],
        notes: cx.notes.into_iter().collect(),
    }
}

fn corner_radius(p: &Value) -> f64 {
    num(p, "borderRadius").unwrap_or_else(|| {
        // ponytail: per-corner radii collapse to the largest, per-corner paths need a custom clip/fill.
        ["TopLeft", "TopRight", "BottomLeft", "BottomRight"]
            .iter()
            .filter_map(|c| num(p, &format!("border{c}Radius")))
            .fold(0.0, f64::max)
    })
}

/// `origin`: page position of the parent's content origin (scroll offset already subtracted).
fn paint(cx: &mut Cx, it: &Item, origin: (f64, f64), parent: usize, parent_opacity: f32, params: &SceneParams, out: &mut Vec<Node>) {
    let Ok(&l) = cx.tree.layout(it.id) else { return };
    let p = it.props;
    if cx.tree.style(it.id).is_ok_and(|s| s.display == Display::None) {
        return;
    }
    // ponytail: transform supports translateX/translateY only; scale/rotate/skew are ignored (Scene has no transforms).
    let (mut tx, mut ty) = (0.0, 0.0);
    for t in p.get("transform").and_then(Value::as_array).into_iter().flatten() {
        tx += num(t, "translateX").unwrap_or(0.0);
        ty += num(t, "translateY").unwrap_or(0.0);
    }
    let (x, y) = (origin.0 + l.location.x as f64 + tx, origin.1 + l.location.y as f64 + ty);
    let (w, h) = (l.size.width as f64, l.size.height as f64);
    let ViewKind::Rn { component, .. } = &it.node.kind else { return };
    let mut n = Node::new(&it.node.id, "view");
    n.bounds = Rect { x, y, w, h };
    n.parent = Some(parent);
    n.action = it.node.id.parse().ok();
    n.opacity = (num(p, "opacity").unwrap_or(1.0) as f32).clamp(0.0, 1.0) * parent_opacity;
    n.fill = cx.color(p.get("backgroundColor"));
    let black = Color::rgba(0.0, 0.0, 0.0, 1.0);
    let all = cx.color(p.get("borderColor")).unwrap_or(black);
    let side = ["Top", "Right", "Bottom", "Left"].map(|e| cx.color(p.get(format!("border{e}Color"))).unwrap_or(all));
    n.radius = corner_radius(p);
    if matches!(p.get("overflow").and_then(Value::as_str), Some("hidden" | "scroll")) || component == "RCTScrollView" {
        n.clip_radius = Some(n.radius);
    }
    let (mut ox, mut oy) = (x, y);
    if component == "RCTScrollView" {
        n.kind = "scroll";
        let vertical = p.get("horizontal").and_then(Value::as_bool) != Some(true);
        let content = it.kids.first().and_then(|k| cx.tree.layout(k.id).ok()).map_or((0.0, 0.0), |c| (c.size.width as f64, c.size.height as f64));
        let max = if vertical { (0.0, (content.1 - h).max(0.0)) } else { ((content.0 - w).max(0.0), 0.0) };
        // `contentOffset` is only the initial offset: any host scroll state wins.
        let initial = p.get("contentOffset").map_or((0.0, 0.0), |o| (num(o, "x").unwrap_or(0.0), num(o, "y").unwrap_or(0.0)));
        let off = params.scroll.get(&it.node.id).copied().unwrap_or(initial);
        let off = (off.0.clamp(0.0, max.0), off.1.clamp(0.0, max.1));
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None, content: Some(content) });
        (ox, oy) = (x - off.0, y - off.1);
    }
    let me = out.len();
    let mut extras: Vec<Node> = vec![];
    let ctx = cx.tree.get_node_context(it.id).cloned();
    let a11y = p.get("accessibilityLabel").and_then(Value::as_str).map(str::to_owned);
    if component == "RCTImageView" {
        n.kind = "image";
        n.role = Role::Image;
        n.label = a11y.clone();
        if let Some(img) = p.get("__simcoreImage") {
            let path = img.get("path").and_then(Value::as_str).unwrap_or("");
            match crate::image::load(path) {
                Some(pixmap) => {
                    let dest = image_dest(p, (w, h), (pixmap.width() as f64, pixmap.height() as f64), params.scale);
                    n.image = Some(ImageRun { pixmap, dest: Rect { x: x + dest.x, y: y + dest.y, ..dest } });
                }
                None => cx.note(format!("image_decode_failed: {path}")),
            }
        }
    }
    if component == "RCTSwitch" {
        let on = p.get("value").and_then(Value::as_bool) == Some(true);
        let disabled = p.get("disabled").and_then(Value::as_bool) == Some(true);
        // RN's off track is `tintColor`; unset it is a darker gray than SwiftUI's (measured on iOS 26.5).
        let fill = if on {
            cx.color(p.get("onTintColor"))
        } else {
            cx.color(p.get("tintColor")).or(Some(Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, if cx.dark { 0.36 } else { 0.43 })))
        };
        let thumb = cx.color(p.get("thumbTintColor"));
        let base = n.opacity;
        let mut knob = switch_style(&mut n, &it.node.id, cx.dark, on, disabled, fill, thumb);
        n.kind = "switch";
        n.label = a11y;
        n.bounds = Rect { x, y, w: SWITCH.0, h: SWITCH.1 };
        n.opacity = base * if disabled { 0.5 } else { 1.0 };
        knob.bounds = Rect { x: x + if on { SWITCH.0 - 2.0 - KNOB.0 } else { 2.0 }, y: y + (SWITCH.1 - KNOB.1) / 2.0, w: KNOB.0, h: KNOB.1 };
        knob.parent = Some(me);
        knob.opacity = n.opacity;
        extras.push(knob);
    }
    if matches!(component.as_str(), "RCTSinglelineTextInputView" | "RCTMultilineTextInputView") {
        extras = text_field(cx, &mut n, it, me, &l, params);
    }
    if let Some(t) = ctx.filter(|_| component == "RCTText") {
        n.kind = "text";
        n.role = Role::Text;
        n.label = Some(t.text.clone());
        let run = TextRun { text: t.text, font: t.spec, color: t.color, strike: t.strike, offset_x: 0.0, rich: t.rich };
        let (b, pd) = (l.border, l.padding);
        let (ix, iy) = ((b.left + pd.left) as f64, (b.top + pd.top) as f64);
        let (iw, ih) = (w - ix - (b.right + pd.right) as f64, h - iy - (b.bottom + pd.bottom) as f64);
        if (ix, iy, iw, ih) == (0.0, 0.0, w, h) {
            n.text = Some(run);
            out.push(n);
        } else {
            out.push(n);
            let mut c = Node::new(format!("{}#text", it.node.id), "text");
            c.bounds = Rect { x: x + ix, y: y + iy, w: iw, h: ih };
            c.parent = Some(me);
            c.opacity = out[me].opacity;
            c.inspect = false;
            c.text = Some(run);
            out.push(c);
        }
    } else {
        out.push(n);
    }
    out.extend(extras);
    // Borders: uniform = one stroked outline, otherwise one filled strip per side (square corners).
    let b = [l.border.top, l.border.right, l.border.bottom, l.border.left].map(f64::from);
    let eff = out[me].opacity;
    let child = |id: &str, r: Rect| {
        let mut c = Node::new(format!("{}#{id}", it.node.id), "border");
        c.bounds = r;
        c.parent = Some(me);
        c.opacity = eff;
        c.inspect = false;
        c
    };
    if b[0] > 0.0 && b.iter().all(|v| *v == b[0]) && side.iter().all(|c| *c == side[0]) {
        let bw = b[0];
        let mut c = child("border", Rect { x: x + bw / 2.0, y: y + bw / 2.0, w: w - bw, h: h - bw });
        c.radius = (out[me].radius - bw / 2.0).max(0.0);
        c.stroke = Some((side[0], bw));
        out.push(c);
    } else {
        let strips = [
            Rect { x, y, w, h: b[0] },
            Rect { x: x + w - b[1], y, w: b[1], h },
            Rect { x, y: y + h - b[2], w, h: b[2] },
            Rect { x, y, w: b[3], h },
        ];
        for (i, r) in strips.into_iter().enumerate().filter(|(i, _)| b[*i] > 0.0) {
            let mut c = child(&format!("edge{i}"), r);
            c.fill = Some(side[i]);
            out.push(c);
        }
    }
    for k in &it.kids {
        paint(cx, k, (ox, oy), me, eff, params, out);
    }
    // Touchables (Pressable/Touchable*/Button set these flags) are buttons for inspection and `--click LABEL`:
    // label = accessibilityLabel, else their text, like iOS VoiceOver.
    let touchable = ["onClick", "onStartShouldSetResponder"].iter().any(|k| p.get(*k) == Some(&Value::Bool(true)))
        || p.get("accessibilityRole").and_then(Value::as_str) == Some("button");
    if touchable && !matches!(out[me].role, Role::Text | Role::TextField | Role::Toggle) {
        out[me].role = Role::Button;
        out[me].label = p.get("accessibilityLabel").and_then(Value::as_str).map(str::to_owned).or_else(|| {
            let t: Vec<_> = out[me + 1..].iter().filter(|c| c.role == Role::Text).filter_map(|c| c.label.clone()).collect();
            (!t.is_empty()).then(|| t.join(" "))
        });
    }
}

/// Where the bitmap lands inside a `size` box (box-relative points) for the `resizeMode` prop.
fn image_dest(p: &Value, (bw, bh): (f64, f64), (iw, ih): (f64, f64), scale: f64) -> Rect {
    let fit = |s: f64| Rect { x: (bw - iw * s) / 2.0, y: (bh - ih * s) / 2.0, w: iw * s, h: ih * s };
    match p.get("resizeMode").and_then(Value::as_str) {
        Some("stretch") => Rect { x: 0.0, y: 0.0, w: bw, h: bh },
        Some("contain") => fit((bw / iw).min(bh / ih)),
        // Natural size (pixels / device scale), scaled down to fit.
        Some("center") => fit((1.0 / scale).min(bw / iw).min(bh / ih)),
        // ponytail: `repeat` (tiling) draws as cover.
        _ => fit((bw / iw).max(bh / ih)),
    }
}

/// A text input: node fields, the text run (placeholder when empty) and the caret. Returns the child nodes.
fn text_field(cx: &mut Cx, n: &mut Node, it: &Item, me: usize, l: &taffy::Layout, params: &SceneParams) -> Vec<Node> {
    let p = it.props;
    let Some(t) = cx.tree.get_node_context(it.id).cloned() else { return vec![] };
    let value = p.get("__simcoreText").and_then(Value::as_str).unwrap_or("");
    let placeholder = p.get("placeholder").and_then(Value::as_str).unwrap_or("");
    let (x, y, w, h) = (n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h);
    n.kind = "text_field";
    n.role = Role::TextField;
    n.field = it.node.id.parse().ok();
    n.value = Some(value.to_string());
    n.label = Some(if placeholder.is_empty() { p.get("accessibilityLabel").and_then(Value::as_str).unwrap_or("") } else { placeholder }.to_string());
    n.focused = params.focus == Some(it.node.id.as_str());
    n.disabled = p.get("editable").and_then(Value::as_bool) == Some(false);
    n.clip_radius = Some(n.radius);
    let empty = value.is_empty();
    let color = if empty {
        cx.color(p.get("placeholderTextColor")).unwrap_or(if cx.dark { Color::rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.3) } else { Color::rgba(60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.3) })
    } else {
        t.color
    };
    let (b, pd) = (l.border, l.padding);
    let (ix, iy) = (x + (b.left + pd.left) as f64, y + (b.top + pd.top) as f64);
    let (cw, ch) = (w - (b.left + pd.left + b.right + pd.right) as f64, h - (b.top + pd.top + b.bottom + pd.bottom) as f64);
    let lh = t.spec.line_height as f64;
    let single = t.single;
    let shown = t.text.clone();
    let (tw, th) = cx.fonts.measure(&shown, &t.spec, if single { None } else { Some(cw as f32) });
    let tw = if empty { 0.0 } else { tw };
    let overflow = if single { (tw - cw).max(0.0) } else { 0.0 };
    let ty = if single { iy + (ch - lh) / 2.0 } else { iy };
    let tbox = Rect { x: ix, y: ty, w: if single { tw.max(cw) } else { cw }, h: if single { lh } else { th.max(lh) } };
    let mut text = Node::new(format!("{}#text", it.node.id), "text");
    text.bounds = tbox;
    text.parent = Some(me);
    text.opacity = n.opacity;
    text.inspect = false;
    text.text = Some(TextRun { text: shown, font: t.spec, color, strike: false, offset_x: -overflow as f32, rich: None });
    let mut kids = vec![text];
    if n.focused && !n.disabled {
        let (cx0, cy0) = if single {
            let start = match t.spec.align {
                TextAlign::Center => (cw - tw).max(0.0) / 2.0,
                TextAlign::Trailing => (cw - tw).max(0.0),
                TextAlign::Leading => 0.0,
            };
            ((start + tw - overflow).min(cw - 2.0) + 1.0, ty)
        } else {
            let last = value.rsplit('\n').next().unwrap_or("");
            let lw = if last.is_empty() { 0.0 } else { cx.fonts.measure(last, &t.spec, None).0 };
            (lw.min(cw - 2.0) + 1.0, ty + if empty { 0.0 } else { (th - lh).max(0.0) })
        };
        let mut c = Node::new(format!("{}#caret", it.node.id), "caret");
        c.bounds = Rect { x: ix + cx0, y: cy0, w: 2.0, h: lh };
        c.fill = Some(if cx.dark { Color::rgba(10.0 / 255.0, 145.0 / 255.0, 1.0, 1.0) } else { Color::rgba(0.0, 136.0 / 255.0, 1.0, 1.0) });
        c.radius = 1.0;
        c.parent = Some(me);
        c.opacity = n.opacity;
        c.inspect = false;
        kids.push(c);
    }
    kids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_scene, device, DEFAULT_DEVICE};
    use serde_json::json;
    use std::collections::HashMap;

    fn raw(id: &str, text: &str) -> Value {
        json!({"kind":"rn","id":id,"component":"RCTRawText","props":{"text":text},"children":[]})
    }

    #[test]
    fn centered_text_and_padded_pressable() {
        let tree: ViewNode = serde_json::from_value(json!({
            "kind":"rn","id":"1","component":"RootView","props":{},"children":[
              {"kind":"rn","id":"2","component":"RCTView",
               "props":{"flex":1,"alignItems":"center","justifyContent":"center","backgroundColor":0xFFFFFFFFu64},
               "children":[
                 {"kind":"rn","id":"3","component":"RCTText","props":{"fontSize":20},"children":[raw("4","Count: 0")]},
                 {"kind":"rn","id":"5","component":"RCTView",
                  "props":{"padding":12,"borderRadius":8,"backgroundColor":0xFF0080FFu64,"marginTop":10},
                  "children":[{"kind":"rn","id":"6","component":"RCTText","props":{"color":0xFFFFFFFFu64},"children":[raw("7","Press")]}]}]}]
        }))
        .unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0),
            device: dev,
            scale: 3.0,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            scroll: &scroll,
            focus: None,
            overscroll: false,
            status_bar: true,
            chrome: Default::default(),
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let f = |id: &str| scene.nodes.iter().find(|n| n.id == id).unwrap().bounds;
        let (root, col, count, btn, label) = (f("1"), f("2"), f("3"), f("5"), f("6"));
        assert_eq!((root.x, root.y, root.w, root.h), (0.0, 0.0, 402.0, 874.0));
        assert_eq!((col.w, col.h), (402.0, 874.0));
        // Text is content sized and centered on both axes together with the button below it.
        let (tw, th) = (count.w, count.h);
        assert!(tw > 50.0 && th > 15.0 && th < 30.0, "{tw}x{th}");
        assert!((count.x + tw / 2.0 - 201.0).abs() < 0.01, "text centered horizontally");
        assert!((btn.x + btn.w / 2.0 - 201.0).abs() < 0.01, "button centered horizontally");
        let block_h = th + 10.0 + btn.h;
        assert!((count.y - (874.0 - block_h) / 2.0).abs() < 0.01, "block centered vertically");
        assert!((btn.y - (count.y + th + 10.0)).abs() < 0.01, "marginTop respected");
        // Padding 12 around the label; the label hugs its content.
        assert!((label.x - (btn.x + 12.0)).abs() < 0.01 && (label.y - (btn.y + 12.0)).abs() < 0.01);
        assert!((btn.w - (label.w + 24.0)).abs() < 0.01 && (btn.h - (label.h + 24.0)).abs() < 0.01);
        let b = scene.nodes.iter().find(|n| n.id == "5").unwrap();
        assert_eq!((b.radius, b.fill.map(|c| (c.r, c.g, c.b, c.a))), (8.0, Some((0.0, 0.5019608, 1.0, 1.0))));
        // Hit testing returns the deepest tag.
        let (cx, cy) = label.center();
        assert_eq!(scene.hit_test(cx, cy).map(|i| scene.nodes[i].id.as_str()), Some("6"));
        assert_eq!(scene.hit_test(1.0, 400.0).map(|i| scene.nodes[i].id.as_str()), Some("2"));
    }

    #[test]
    fn text_input_switch_image_and_modal() {
        let png = std::env::temp_dir().join(format!("simcore-rn-img-{}.png", std::process::id()));
        let mut pm = tiny_skia::Pixmap::new(4, 2).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pm.save_png(&png).unwrap();
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        let tree: ViewNode = serde_json::from_value(v("1", "RootView", json!({}), vec![
            v("2", "RCTSinglelineTextInputView", json!({"fontSize":17,"placeholder":"Name","__simcoreText":""}), vec![]),
            v("3", "RCTSwitch", json!({"value":true,"onTintColor":0xFFFF0000u64,"accessibilityLabel":"Notify"}), vec![]),
            v("4", "RCTImageView", json!({"width":100,"height":50,"resizeMode":"contain","borderRadius":10,
                "__simcoreImage":{"path":png.to_str().unwrap(),"width":4,"height":2}}), vec![]),
            v("5", "RCTModalHostView", json!({"visible":true}), vec![v("6", "RCTView", json!({"flex":1,"backgroundColor":0xFF00FF00u64}), vec![])]),
        ])).unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0), device: dev, scale: 3.0, appearance: Appearance::Light, dynamic_type: Default::default(),
            scroll: &scroll, focus: Some("2"), overscroll: false, status_bar: true, chrome: crate::ChromeParams { keyboard: true, shift: false },
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let n = |id: &str| scene.nodes.iter().position(|n| n.id == id).unwrap();
        let field = &scene.nodes[n("2")];
        assert_eq!((field.role, field.field, field.label.as_deref(), field.focused), (Role::TextField, Some(2), Some("Name"), true));
        assert_eq!(field.bounds.w, 402.0, "stretched, no intrinsic padding");
        assert!(field.bounds.h > 17.0 && field.bounds.h < 24.0, "line height, got {}", field.bounds.h);
        assert!(scene.nodes.iter().any(|c| c.id == "2#caret"), "focused field draws a caret");
        assert!(scene.nodes.iter().any(|c| c.kind == "keyboard"), "focus shows the software keyboard");
        let sw = &scene.nodes[n("3")];
        assert_eq!((sw.bounds.w, sw.bounds.h, sw.role, sw.label.as_deref(), sw.value.as_deref()), (63.0, 28.0, Role::Toggle, Some("Notify"), Some("on")));
        assert_eq!(sw.fill.map(|c| (c.r, c.g, c.b)), Some((1.0, 0.0, 0.0)));
        let img = &scene.nodes[n("4")];
        let dest = img.image.as_ref().expect("decoded").dest;
        assert_eq!((img.bounds.w, img.bounds.h, dest.w, dest.h), (100.0, 50.0, 100.0, 50.0));
        // The modal covers the viewport above the page and takes the hit.
        assert_eq!(scene.nodes[n("5")].bounds.w, 402.0);
        assert!(n("5") > n("4"));
        assert_eq!(scene.hit_test(200.0, 300.0).map(|i| scene.nodes[i].id.as_str()), Some("6"));
        let _ = std::fs::remove_file(png);
    }
}
