//! React Native layout: Fabric's flat props -> Yoga-style flexbox (via taffy) -> `Scene`.
//!
//! Node ids in the scene are the reactTags; every RN box has `action = Some(tag)` so the scene's
//! hit test returns the deepest tag under a point. Content starts at y=0 (RN handles safe areas).

use super::swiftui::{add_chrome, SceneParams};
use crate::scene::{Color, Node, Rect, Role, Scene, ScrollInfo, TextRun};
use crate::text::{FontSpec, Fonts, TextAlign};
use serde_json::Value;
use simcore_protocol::{Appearance, ViewKind, ViewNode};
use std::collections::BTreeSet;
use taffy::prelude::*;
use taffy::compute_leaf_layout;

/// Text leaf: what the measure function and the painter need.
struct Text {
    text: String,
    spec: FontSpec,
    color: Color,
    strike: bool,
}

struct Item<'a> {
    node: &'a ViewNode,
    props: &'a Value,
    id: NodeId,
    kids: Vec<Item<'a>>,
}

struct Cx {
    tree: TaffyTree<Text>,
    notes: BTreeSet<String>,
    scale: f64,
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

/// Descendant `RCTRawText` content of a text subtree.
fn text_of(n: &ViewNode, out: &mut String) {
    if let ViewKind::Rn { component, props, children } = &n.kind {
        if component == "RCTRawText" {
            out.push_str(props.get("text").and_then(Value::as_str).unwrap_or(""));
        }
        children.iter().for_each(|c| text_of(c, out));
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

impl Cx {
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

    // ponytail: nested RCTVirtualText spans take the outer paragraph style, per-span styles need a rich text shaper;
    // also ignored: fontStyle, fontFamily, letterSpacing, textDecorationLine underline, numberOfLines.
    fn text(&mut self, node: &ViewNode, p: &Value) -> Text {
        let mut text = String::new();
        text_of(node, &mut text);
        let size = num(p, "fontSize").unwrap_or(14.0) as f32;
        let align = match p.get("textAlign").and_then(Value::as_str) {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Trailing,
            _ => TextAlign::Leading,
        };
        let mut spec = FontSpec::new(size, self.line_h(size), weight(p), align);
        if let Some(lh) = num(p, "lineHeight").filter(|l| *l > 0.0) {
            (spec.line_height, spec.pitch) = (lh as f32, lh as f32);
        }
        let color = self.color(p.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let strike = p.get("textDecorationLine").and_then(Value::as_str).is_some_and(|d| d.contains("line-through"));
        Text { text, spec, color, strike }
    }

    fn build<'a>(&mut self, node: &'a ViewNode) -> Option<Item<'a>> {
        let ViewKind::Rn { component, props, children } = &node.kind else { return None };
        match component.as_str() {
            "RCTRawText" | "RCTVirtualText" => return None,
            "RootView" | "RCTView" | "RCTScrollView" | "RCTScrollContentView" | "RCTImageView" | "RCTText" => {}
            other => self.note(format!("unsupported_component: {other}")),
        }
        let st = style(props);
        if component == "RCTText" {
            let t = self.text(node, props);
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item { node, props, id, kids: vec![] });
        }
        let kids: Vec<Item> = children.iter().filter_map(|c| self.build(c)).collect();
        let ids: Vec<NodeId> = kids.iter().map(|k| k.id).collect();
        let id = self.tree.new_with_children(st, &ids).ok()?;
        Some(Item { node, props, id, kids })
    }
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    let mut cx = Cx { tree: TaffyTree::new(), notes: BTreeSet::new(), scale: params.scale.max(1.0) };
    cx.tree.disable_rounding();
    let dark = params.appearance == Appearance::Dark;
    let bg = if dark { 0.0 } else { 1.0 };
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(Color::rgba(bg, bg, bg, 1.0));
    let mut nodes = vec![screen];
    if let Some(item) = cx.build(root) {
        // The surface root fills the viewport.
        let mut st = cx.tree.style(item.id).cloned().unwrap_or_default();
        st.size = taffy::Size { width: length(w as f32), height: length(h as f32) };
        let _ = cx.tree.set_style(item.id, st);
        let scale = cx.scale as f32;
        let _ = cx.tree.compute_layout_with_measure(
            item.id,
            taffy::Size { width: AvailableSpace::Definite(w as f32), height: AvailableSpace::Definite(h as f32) },
            |inp, _, t, style| {
                compute_leaf_layout(inp, style, |_, _| 0.0, |known, avail| {
                    let Some(t) = t else { return taffy::Size::ZERO };
                    let wrap = known.width.or(match avail.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => Some(0.0),
                        AvailableSpace::MaxContent => None,
                    });
                    let (mw, mh) = fonts.measure(&t.text, &t.spec, wrap);
                    // Yoga rounds text sizes up to the pixel grid.
                    let up = |x: f64| (x as f32 * scale).ceil() / scale;
                    taffy::Size { width: known.width.unwrap_or(up(mw)), height: known.height.unwrap_or(up(mh)) }
                })
            },
        );
        paint(&mut cx, &item, (0.0, 0.0), 0, 1.0, params, &mut nodes);
    }
    add_chrome(&mut nodes, params, fonts);
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
    let border_color = cx.color(p.get("borderColor")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
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
        let off = params.scroll.get(&it.node.id).copied().unwrap_or((0.0, 0.0));
        let off = (off.0.clamp(0.0, max.0), off.1.clamp(0.0, max.1));
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None });
        (ox, oy) = (x - off.0, y - off.1);
    }
    // ponytail: images are a backgroundColor placeholder; decode + draw `source[0].uri` later.
    if component == "RCTImageView" {
        n.kind = "image";
        n.role = Role::Image;
    }
    let me = out.len();
    if let Some(t) = cx.tree.get_node_context(it.id).map(|t| Text { text: t.text.clone(), ..*t }) {
        n.kind = "text";
        n.role = Role::Text;
        n.label = Some(t.text.clone());
        let run = TextRun { text: t.text, font: t.spec, color: t.color, strike: t.strike, offset_x: 0.0 };
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
    if b[0] > 0.0 && b.iter().all(|v| *v == b[0]) {
        let bw = b[0];
        let mut c = child("border", Rect { x: x + bw / 2.0, y: y + bw / 2.0, w: w - bw, h: h - bw });
        c.radius = (out[me].radius - bw / 2.0).max(0.0);
        c.stroke = Some((border_color, bw));
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
            c.fill = Some(border_color);
            out.push(c);
        }
    }
    // ponytail: borderColor is one color for all sides; per-side colors need per-strip parsing.
    for k in &it.kids {
        paint(cx, k, (ox, oy), me, eff, params, out);
    }
    // Touchables (Pressable/Touchable*/Button set these flags) are buttons for inspection and `--click LABEL`:
    // label = accessibilityLabel, else their text, like iOS VoiceOver.
    let touchable = ["onClick", "onStartShouldSetResponder"].iter().any(|k| p.get(*k) == Some(&Value::Bool(true)))
        || p.get("accessibilityRole").and_then(Value::as_str) == Some("button");
    if touchable && out[me].role != Role::Text {
        out[me].role = Role::Button;
        out[me].label = p.get("accessibilityLabel").and_then(Value::as_str).map(str::to_owned).or_else(|| {
            let t: Vec<_> = out[me + 1..].iter().filter(|c| c.role == Role::Text).filter_map(|c| c.label.clone()).collect();
            (!t.is_empty()).then(|| t.join(" "))
        });
    }
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
}
