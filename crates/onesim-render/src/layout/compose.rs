//! Jetpack Compose layout (protocol section K): `Box` / `Column` / `Row` / `Text` primitives with a
//! Modifier chain -> Compose's constraint-based measure/place -> `Scene`.
//!
//! Material components arrive already decomposed into these primitives (the guest's `material3` is built from
//! them, as in AndroidX), so this module only models `androidx.compose.ui` + `foundation.layout` semantics:
//! constraints flow down, sizes flow up, parents place children. Units are dp (= scene points).

use super::rn::{ModalLayers, Placement};
use super::swiftui::SceneParams;
use crate::chrome::{android_nav_handle, android_status_bar};
use crate::compose_text::{self, ComposeText};
use crate::scene::{Color, Node, Rect, Role, Scene, ScrollInfo, Shadow, TextRun};
use crate::text::{android_glyph_size, ComposeOverflow, FontSpec, Fonts, Span, TextAlign};
use serde_json::Value;
use onesim_protocol::{Appearance, ViewKind, ViewNode};
use std::collections::BTreeSet;

const INF: f64 = f64::INFINITY;

/// Measure an offscreen Compose Text with the regular text shaper, independent of the scene tree.
pub fn measure_text(request: onesim_protocol::ComposeMeasureText, fonts: &mut Fonts) -> Option<String> {
    let density = request.density;
    let [min_w, max_w, min_h, max_h] = request.constraints;
    if !density.is_finite() || density <= 0.0 || min_w < 0 || min_h < 0 || min_w > max_w || min_h > max_h {
        return None;
    }
    let logical = |pixels| if pixels == i32::MAX { INF } else { f64::from(pixels) / f64::from(density) };
    let c = C { min_w: logical(min_w), max_w: logical(max_w), min_h: logical(min_h), max_h: logical(max_h) };
    fonts.register_compose_fonts(&request.fonts);
    let mut cx = Cx { fonts, notes: BTreeSet::new(), density, params: None };
    let mut props = request.props;
    props.as_object_mut()?.insert("textLayout".into(), Value::from(1));
    let node = ViewNode {
        id: String::new(),
        modifiers: vec![],
        kind: ViewKind::Compose { component: "Text".into(), props, children: vec![] },
    };
    let measured = cx.text(&node, parts(&node).1, c);
    if let Some(paint) = request.paint {
        return paint_text(&measured, paint, density, cx.fonts);
    }
    let Deco::Text(run, Some(layout)) = measured.deco else { return None };
    if request.intrinsic != 0 {
        let width = cx.fonts.compose_intrinsic_width(&run, request.intrinsic < 0);
        return Some((width * density).to_string());
    }
    let bounds = Rect { x: 0.0, y: 0.0, w: measured.w, h: measured.h };
    layout.layout_data(&run, bounds).map(|layout| layout.geometry)
}

fn paint_text(
    measured: &Laid,
    paint: onesim_protocol::ComposeTextPaint,
    density: f32,
    fonts: &mut Fonts,
) -> Option<String> {
    use base64::Engine as _;
    let (width, height) = (f64::from(paint.width), f64::from(paint.height));
    if width * height > 16_777_216.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let scale = f64::from(density);
    let logical = |px: f32| f64::from(px) / scale;
    let [x, y] = paint.origin.map(logical);
    let [left, top, right, bottom] = paint.clip.map(logical);
    let mut clip = Node::new("clip", "clip");
    clip.bounds = Rect { x: left, y: top, w: right - left, h: bottom - top };
    clip.clip_radius = Some(0.0);
    let visible = clip.bounds.w > 0.0 && clip.bounds.h > 0.0;
    let mut nodes = vec![clip];
    if visible {
        flatten(measured, x, y, 0, 1.0, &mut nodes, &mut ModalLayers::default());
    }
    let scene = Scene {
        size: (width / scale, height / scale), appearance: Appearance::Light, ios: onesim_protocol::IosVersion::default(),
        nodes, unsupported_symbols: vec![], notes: vec![],
    };
    Some(base64::engine::general_purpose::STANDARD.encode(crate::raster::render(&scene, fonts, scale).data()))
}

/// Compose `Constraints` (dp; `max_*` may be infinite).
#[derive(Debug, Clone, Copy, PartialEq)]
struct C {
    min_w: f64,
    max_w: f64,
    min_h: f64,
    max_h: f64,
}

impl C {
    fn loose(self) -> C {
        C { min_w: 0.0, min_h: 0.0, ..self }
    }
    /// `Constraints.constrain(size)`.
    fn size(self, w: f64, h: f64) -> (f64, f64) {
        (w.max(self.min_w).min(self.max_w), h.max(self.min_h).min(self.max_h))
    }
    /// `Constraints.offset(dx, dy)` with negative deltas: shrink, never below zero (infinity stays).
    fn shrink(self, dw: f64, dh: f64) -> C {
        C {
            min_w: (self.min_w - dw).max(0.0),
            max_w: if self.max_w.is_finite() { (self.max_w - dw).max(0.0) } else { INF },
            min_h: (self.min_h - dh).max(0.0),
            max_h: if self.max_h.is_finite() { (self.max_h - dh).max(0.0) } else { INF },
        }
    }
    /// `Constraints.constrain(other)`: `other` coerced into `self`.
    fn constrain(self, o: C) -> C {
        let cw = |v: f64| v.max(self.min_w).min(self.max_w);
        let ch = |v: f64| v.max(self.min_h).min(self.max_h);
        C { min_w: cw(o.min_w), max_w: cw(o.max_w), min_h: ch(o.min_h), max_h: ch(o.max_h) }
    }
}

/// Wire shape: corner radii in dp, or percent of the shorter side; none = rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Shape {
    #[default]
    Rect,
    Corners(f64),
    Percent(f64),
}

impl Shape {
    // ponytail: per-corner radii collapse to the largest (the scene has one radius per node).
    fn parse(v: Option<&Value>) -> Shape {
        let max = |a: &Value| a.as_array().map(|a| a.iter().filter_map(Value::as_f64).fold(0.0, f64::max));
        match v {
            Some(v) if v.get("percent").is_some() => Shape::Percent(max(&v["percent"]).unwrap_or(0.0)),
            Some(v) if v.get("corners").is_some() => Shape::Corners(max(&v["corners"]).unwrap_or(0.0)),
            _ => Shape::Rect,
        }
    }
    fn radius(self, w: f64, h: f64) -> f64 {
        match self {
            Shape::Rect => 0.0,
            Shape::Corners(r) => r.min(w.min(h) / 2.0),
            Shape::Percent(p) => w.min(h) * p.clamp(0.0, 50.0) / 100.0,
        }
    }
}

/// What a laid-out layer draws or does.
#[derive(Debug, Clone)]
enum Deco {
    None,
    Background(Color, Shape),
    Border(f64, Color, Shape),
    Clip(Shape),
    Shadow(f64, Shape),
    Alpha(f32),
    Click { action: Option<u64>, role: Role, label: Option<String> },
    Text(TextRun, Option<Box<ComposeText>>),
    Control(Box<Node>),
    Embedded(super::rn::Embedded),
    /// The component's own node (`Box`, `Column`, `Row`).
    Core(&'static str),
    SelectionContainer,
    Bullet(Box<crate::text::TextBullet>, Color, f32),
}

/// A measured and placed layer: one modifier or the component itself; children are relative to it.
#[derive(Debug, Clone)]
struct Laid {
    id: String,
    w: f64,
    h: f64,
    deco: Deco,
    kids: Vec<(f64, f64, Laid)>,
}

impl Laid {
    fn wrap(id: String, deco: Deco, inner: Laid) -> Laid {
        Laid { id, w: inner.w, h: inner.h, deco, kids: vec![(0.0, 0.0, inner)] }
    }
}

struct Cx<'a> {
    fonts: &'a mut Fonts,
    notes: BTreeSet<String>,
    /// Device px per dp.
    density: f32,
    params: Option<&'a SceneParams<'a>>,
}

fn color(v: Option<&Value>) -> Option<Color> {
    let a = v?.as_array()?;
    let c = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(1.0) as f32;
    Some(Color::rgba(c(0), c(1), c(2), c(3)))
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

/// `[a, b]` pair where either side may be null (unspecified).
fn pair(v: &Value) -> (Option<f64>, Option<f64>) {
    (v.get(0).and_then(Value::as_f64), v.get(1).and_then(Value::as_f64))
}

fn parts(n: &ViewNode) -> (&str, &Value, &[ViewNode]) {
    match &n.kind {
        ViewKind::Compose { component, props, children } => (component, props, children),
        _ => ("", &Value::Null, &[]),
    }
}

fn modifiers(props: &Value) -> &[Value] {
    props.get("modifier").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `RowScope`/`ColumnScope.weight` of a child: `(weight, fill)`.
fn weight(n: &ViewNode) -> Option<(f64, bool)> {
    modifiers(parts(n).1).iter().find_map(|m| Some((f(m, "weight")?, m.get("fill").and_then(Value::as_bool).unwrap_or(true))))
}

/// `BoxScope.align` (`[h, v]`) or `RowScope`/`ColumnScope.align` (cross-axis bias) of a child.
fn align(n: &ViewNode) -> Option<&Value> {
    modifiers(parts(n).1).iter().find_map(|m| m.get("align"))
}

/// Position of `inner` in `outer` along one axis for an alignment bias (-1 start ... 1 end).
fn bias(outer: f64, inner: f64, b: f64) -> f64 {
    (outer - inner) * (1.0 + b) / 2.0
}

impl Cx<'_> {
    /// `Dp.roundToPx()`: Compose measures and places in whole device pixels.
    fn px(&self, dp: f64) -> f64 {
        let d = self.density as f64;
        if d > 0.0 { (dp * d).round() / d } else { dp }
    }
    fn opx(&self, dp: Option<f64>) -> Option<f64> {
        dp.map(|v| self.px(v))
    }
    /// A size rounded up to whole pixels (`ceil` of a text layout's width/height).
    fn ceil_px(&self, dp: f64) -> f64 {
        let d = self.density as f64;
        if d > 0.0 { (dp * d - 1e-6).ceil() / d } else { dp }
    }
    /// `BiasAlignment.align`: `round(free / 2 * (1 + bias))` in pixels.
    fn bias(&self, outer: f64, inner: f64, b: f64) -> f64 {
        self.px(bias(outer, inner, b))
    }

    fn measure(&mut self, n: &ViewNode, c: C) -> Laid {
        let mods = modifiers(parts(n).1);
        self.chain(n, mods, 0, c)
    }

    /// Modifier `i` (outermost first) wraps everything after it.
    fn chain(&mut self, n: &ViewNode, mods: &[Value], i: usize, c: C) -> Laid {
        let Some(m) = mods.get(i) else { return self.core(n, c) };
        let id = format!("{}#m{i}", n.id);
        let next = |cx: &mut Self, c: C| cx.chain(n, mods, i + 1, c);
        if let Some(vertical) = m.get("scroll").and_then(Value::as_bool) {
            let inner = next(self, if vertical { C { max_h: INF, ..c } } else { C { max_w: INF, ..c } });
            let (w, h) = c.size(inner.w, inner.h);
            let (mx, my) = if vertical { (0.0, (inner.h - h).max(0.0)) } else { ((inner.w - w).max(0.0), 0.0) };
            let want = self.params.and_then(|p| p.scroll.get(&n.id)).copied().unwrap_or_default();
            let offset = (want.0.clamp(0.0, mx), want.1.clamp(0.0, my));
            let mut node = Node::new(&n.id, "compose_scroll");
            node.clip_radius = Some(0.0);
            node.scroll = Some(ScrollInfo { max: (mx, my), offset, vertical, snap: None, content: Some((inner.w, inner.h)), page: None, aligned: false });
            return Laid { id: n.id.clone(), w, h, deco: Deco::Control(Box::new(node)), kids: vec![(-offset.0, -offset.1, inner)] };
        }
        // Size modifiers: target constraints coerced into the incoming ones (`enforceIncoming`).
        let sized = |cx: &mut Self, min_w: Option<f64>, max_w: Option<f64>, min_h: Option<f64>, max_h: Option<f64>| {
            let t = C { min_w: min_w.unwrap_or(0.0), max_w: max_w.unwrap_or(INF), min_h: min_h.unwrap_or(0.0), max_h: max_h.unwrap_or(INF) };
            // Unspecified sides keep the incoming bounds.
            let t = C {
                min_w: if min_w.is_some() { t.min_w } else { c.min_w },
                max_w: if max_w.is_some() { t.max_w } else { c.max_w },
                min_h: if min_h.is_some() { t.min_h } else { c.min_h },
                max_h: if max_h.is_some() { t.max_h } else { c.max_h },
            };
            next(cx, c.constrain(t))
        };
        if let Some(p) = m.get("padding").and_then(Value::as_array) {
            let p: Vec<f64> = (0..4).map(|k| self.px(p.get(k).and_then(Value::as_f64).unwrap_or(0.0))).collect();
            let (h, v) = (p[0] + p[2], p[1] + p[3]);
            let inner = next(self, c.shrink(h, v));
            let (w, ht) = c.size(inner.w + h, inner.h + v);
            return Laid { id, w, h: ht, deco: Deco::None, kids: vec![(p[0], p[1], inner)] };
        }
        if let Some(w) = self.opx(f(m, "width")) {
            return sized(self, Some(w), Some(w), None, None);
        }
        if let Some(h) = self.opx(f(m, "height")) {
            return sized(self, None, None, Some(h), Some(h));
        }
        if let Some(r) = m.get("widthIn") {
            let (lo, hi) = pair(r);
            let (lo, hi) = (self.opx(lo), self.opx(hi));
            return sized(self, lo, hi, None, None);
        }
        if let Some(r) = m.get("heightIn") {
            let (lo, hi) = pair(r);
            let (lo, hi) = (self.opx(lo), self.opx(hi));
            return sized(self, None, None, lo, hi);
        }
        if let Some(w) = self.opx(f(m, "requiredWidth")) {
            let inner = next(self, C { min_w: w, max_w: w, ..c });
            let (ow, oh) = c.size(inner.w, inner.h);
            return Laid { id, w: ow, h: oh, deco: Deco::None, kids: vec![(self.bias(ow, inner.w, 0.0), 0.0, inner)] };
        }
        if let Some(h) = self.opx(f(m, "requiredHeight")) {
            let inner = next(self, C { min_h: h, max_h: h, ..c });
            let (ow, oh) = c.size(inner.w, inner.h);
            return Laid { id, w: ow, h: oh, deco: Deco::None, kids: vec![(0.0, self.bias(oh, inner.h, 0.0), inner)] };
        }
        if let Some(d) = m.get("defaultMinSize") {
            let (w, h) = pair(d);
            let (w, h) = (self.opx(w), self.opx(h));
            let min_w = match w {
                Some(w) if c.min_w == 0.0 => w.min(c.max_w),
                _ => c.min_w,
            };
            let min_h = match h {
                Some(h) if c.min_h == 0.0 => h.min(c.max_h),
                _ => c.min_h,
            };
            return next(self, C { min_w, min_h, ..c });
        }
        if let Some(fr) = f(m, "fillMaxWidth") {
            let c2 = if c.max_w.is_finite() {
                let w = self.px(c.max_w * fr).max(c.min_w).min(c.max_w);
                C { min_w: w, max_w: w, ..c }
            } else {
                c
            };
            return next(self, c2);
        }
        if let Some(fr) = f(m, "fillMaxHeight") {
            let c2 = if c.max_h.is_finite() {
                let h = self.px(c.max_h * fr).max(c.min_h).min(c.max_h);
                C { min_h: h, max_h: h, ..c }
            } else {
                c
            };
            return next(self, c2);
        }
        if let Some(a) = m.get("wrapContent") {
            let (bh, bv) = pair(a);
            let inner = next(self, C {
                min_w: if bh.is_some() { 0.0 } else { c.min_w },
                min_h: if bv.is_some() { 0.0 } else { c.min_h },
                ..c
            });
            let (w, h) = (inner.w.max(c.min_w), inner.h.max(c.min_h));
            let x = bh.map_or(0.0, |bias| self.bias(w, inner.w, bias));
            let y = bv.map_or(0.0, |bias| self.bias(h, inner.h, bias));
            return Laid { id, w, h, deco: Deco::None, kids: vec![(x, y, inner)] };
        }
        if let Some(min) = self.opx(f(m, "minInteractive")) {
            // `minimumInteractiveComponentSize`: at least `min` on each side, content centred; may exceed the
            // incoming constraints (Material buttons are 40 dp tall inside a 48 dp slot).
            let inner = next(self, c);
            let (w, h) = (inner.w.max(min), inner.h.max(min));
            return Laid { id, w, h, deco: Deco::None, kids: vec![(self.bias(w, inner.w, 0.0), self.bias(h, inner.h, 0.0), inner)] };
        }
        if m.get("weight").is_some() || m.get("align").is_some() {
            return next(self, c); // parent data, read by the parent
        }
        if let Some(o) = m.get("offset") {
            let (x, y) = pair(o);
            let (x, y) = (self.opx(x), self.opx(y));
            let inner = next(self, c);
            return Laid { id, w: inner.w, h: inner.h, deco: Deco::None, kids: vec![(x.unwrap_or(0.0), y.unwrap_or(0.0), inner)] };
        }
        let deco = if let Some(b) = m.get("background") {
            Deco::Background(color(b.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 0.0)), Shape::parse(b.get("shape")))
        } else if let Some(b) = m.get("border") {
            Deco::Border(f(b, "width").unwrap_or(1.0), color(b.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0)), Shape::parse(b.get("shape")))
        } else if let Some(s) = m.get("clip") {
            Deco::Clip(Shape::parse(Some(s)))
        } else if let Some(s) = m.get("shadow") {
            Deco::Shadow(f(s, "elevation").unwrap_or(0.0), Shape::parse(s.get("shape")))
        } else if let Some(a) = f(m, "alpha") {
            Deco::Alpha(a.clamp(0.0, 1.0) as f32)
        } else if let Some(k) = m.get("clickable") {
            let enabled = k.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let role = match k.get("role").and_then(Value::as_str) {
                Some("switch" | "checkbox" | "radio") => Role::Toggle,
                _ => Role::Button,
            };
            let action = k.get("action").and_then(Value::as_u64).filter(|_| enabled);
            Deco::Click { action, role, label: k.get("label").and_then(Value::as_str).map(str::to_owned) }
        } else {
            let key = m.as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
            self.notes.insert(format!("unsupported_modifier: {key}"));
            Deco::None
        };
        let inner = next(self, c);
        Laid::wrap(id, deco, inner)
    }

    /// The component itself, under all of its modifiers.
    fn core(&mut self, n: &ViewNode, c: C) -> Laid {
        if let ViewKind::Rn { component, .. } = &n.kind {
            if let Some(params) = self.params.filter(|_| component == onesim_protocol::EXPO_UI_RN_HOST) {
                let available = (c.max_w.is_finite().then_some(c.max_w), c.max_h.is_finite().then_some(c.max_h));
                let subtree = super::rn::embedded(n, available, params, self.fonts);
                let (w, h) = c.size(subtree.size.0, subtree.size.1);
                return Laid { id: n.id.clone(), w, h, deco: Deco::Embedded(subtree), kids: vec![] };
            }
        }
        let (component, props, children) = parts(n);
        match component {
            "Text" => self.text(n, props, c),
            "TextField" => self.field(n, props, c),
            "Slider" => self.slider(n, props, c),
            "Column" => self.linear(n, props, children, c, true),
            "Row" => self.linear(n, props, children, c, false),
            "Box" => self.boxed(n, props, children, c),
            other => {
                self.notes.insert(format!("unsupported_component: {other}"));
                self.boxed(n, props, children, c)
            }
        }
    }

    fn field(&mut self, n: &ViewNode, p: &Value, c: C) -> Laid {
        let text = p.get("text").and_then(Value::as_str).unwrap_or_default();
        let placeholder = p.get("placeholder").and_then(Value::as_str).unwrap_or_default();
        let plain = p.get("plain").and_then(Value::as_bool) == Some(true);
        let disabled = p.get("enabled").and_then(Value::as_bool) == Some(false);
        let focused = !disabled && self.params.is_some_and(|params| params.focus == Some(n.id.as_str()));
        let value = if text.is_empty() {
            placeholder.to_owned()
        } else if p.get("secure").and_then(Value::as_bool) == Some(true) {
            "•".repeat(text.chars().count())
        } else {
            text.to_owned()
        };
        let pad = if plain { 0.0 } else { self.px(16.0) };
        let mut text_props = p.clone();
        text_props["text"] = Value::String(value);
        let mut label = self.text(n, &text_props, c.loose().shrink(pad * 2.0, 0.0));
        let caret_x = if text.is_empty() { 0.0 } else { label.w };
        if let Deco::Text(run, _) = &mut label.deco {
            if text.is_empty() && !plain {
                run.color.a *= 0.65;
            }
        }
        label.id = format!("{}#text", n.id);
        let (w, h) = c.size(
            if plain { label.w.max(self.px(1.0)) } else { self.px(280.0) },
            if plain { label.h } else { self.px(56.0) },
        );
        let mut node = Node::new(&n.id, "text_field");
        node.role = Role::TextField;
        node.field = p.get("field").and_then(Value::as_u64).filter(|_| !disabled);
        node.value = Some(text.to_owned());
        node.label = if placeholder.is_empty() { Some(text.to_owned()) } else { Some(placeholder.to_owned()) };
        node.focused = focused;
        node.disabled = disabled;
        node.clip_radius = Some(0.0);
        if !plain {
            node.fill = Some(Color::rgba(0.91, 0.88, 0.92, 1.0));
            node.radius = self.px(4.0);
        }
        let mut kids = vec![(pad, (h - label.h) / 2.0, label)];
        if focused {
            let width = self.px(2.0);
            let height = self.px(18.0).min(h);
            let accent = Color::rgba(0.40, 0.31, 0.64, 1.0);
            let caret = Laid { id: format!("{}#caret", n.id), w: width, h: height, deco: Deco::Background(accent, Shape::Rect), kids: vec![] };
            kids.push(((pad + caret_x).min((w - width).max(0.0)), (h - height) / 2.0, caret));
        }
        Laid { id: n.id.clone(), w, h, deco: Deco::Control(Box::new(node)), kids }
    }

    fn slider(&self, n: &ViewNode, p: &Value, c: C) -> Laid {
        let (lo, hi) = (f(p, "min").unwrap_or(0.0), f(p, "max").unwrap_or(1.0));
        let value = f(p, "value").unwrap_or(lo);
        let disabled = p.get("enabled").and_then(Value::as_bool) == Some(false);
        let w = if c.max_w.is_finite() { c.max_w } else { self.px(200.0) };
        let (w, h) = c.size(w, self.px(48.0));
        let tw = self.px(4.0).min(w);
        let th = self.px(44.0).min(h);
        let fraction = if lo < hi { ((value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
        let x = self.px(fraction * (w - tw));
        let track_h = self.px(16.0).min(h);
        let track_y = (h - track_h) / 2.0;
        let gap = self.px(6.0);
        let active = color(p.get("activeColor")).unwrap_or(Color::rgba(0.40, 0.31, 0.64, 1.0));
        let inactive = color(p.get("inactiveColor")).unwrap_or(Color::rgba(0.91, 0.87, 0.97, 1.0));
        let background = |part: &str, w, h, color| Laid {
            id: format!("{}#{part}", n.id), w, h,
            deco: Deco::Background(color, Shape::Corners(self.px(8.0))), kids: vec![],
        };
        let end = (x + tw + gap).min(w);
        let mut node = Node::new(&n.id, "slider");
        node.role = Role::Slider;
        node.range = Some((lo, hi, tw));
        node.field = p.get("field").and_then(Value::as_u64).filter(|_| !disabled && lo < hi);
        node.value = Some(value.to_string());
        node.disabled = disabled;
        node.opacity = if disabled { 0.38 } else { 1.0 };
        let kids = vec![
            (0.0, track_y, background("active", (x - gap).max(0.0), track_h, active)),
            (end, track_y, background("inactive", w - end, track_h, inactive)),
            (x, (h - th) / 2.0, background("thumb", tw, th, active)),
        ];
        Laid { id: n.id.clone(), w, h, deco: Deco::Control(Box::new(node)), kids }
    }

    fn text_run(&mut self, text: &str, p: &Value, size: f32, inline: &[onesim_protocol::ComposeInlineObject]) -> TextRun {
        let weight = f(p, "fontWeight").unwrap_or(400.0) as u16;
        let align = match p.get("textAlign").and_then(Value::as_str) {
            Some("center") => TextAlign::Center,
            Some("end") => TextAlign::Trailing,
            Some("justify") => TextAlign::Justify,
            _ => TextAlign::Leading,
        };
        let glyph_size = |sp| android_glyph_size(sp, self.density).max(1.0 / self.density.max(1.0));
        let col = color(p.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let decoration = p.get("decoration").and_then(Value::as_u64).unwrap_or(0);
        let family = p["fontFamily"].as_str().filter(|family| *family != "default");
        let features = p["fontFeatureSettings"].as_str();
        let brush = serde_json::from_value(p["brush"].clone()).ok();
        let shadow = serde_json::from_value(p["textShadow"].clone()).ok();
        let paint_alpha = p["paintAlpha"].as_f64().map(|alpha| alpha as f32);
        let transform = serde_json::from_value(p["textTransform"].clone()).ok();
        let extra = crate::text::ComposeSpanStyle {
            family: family.map(str::to_owned), feature_settings: features.map(str::to_owned),
            brush, shadow, paint_alpha, transform,
            stroke: serde_json::from_value(p["textStroke"].clone()).ok(),
            line_break: serde_json::from_value(p["lineBreak"].clone()).ok(),
            auto_hyphens: p["autoHyphens"].as_bool(), locale: p["textLocale"].as_str().map(str::to_owned),
            ..Default::default()
        };
        let extra = (extra != crate::text::ComposeSpanStyle::default()).then(|| Box::new(extra));
        let base = Span { text: String::new(), pt: glyph_size(size), weight, color: col, italic: p["italic"] == true,
            underline: decoration & 1 != 0, strike: decoration & 2 != 0, mono: false, compose: extra,
        };
        let line_height = f(p, "lineHeightEm").map(|em| em * f64::from(size)).or_else(|| f(p, "lineHeight"));
        let inline_styles = if inline.is_empty() {
            None
        } else {
            // The object replacement glyph carries the reserved advance. Its ink is transparent.
            let mut probe = FontSpec::new(base.pt, size * 1.1719, weight, TextAlign::Leading);
            probe.tracking = 0.0;
            probe.android = true;
            let intrinsic = self.fonts.measure("\u{fffc}", &probe, None, &[]).0;
            let mut styles = p["spans"].as_array().cloned().unwrap_or_default();
            let line_height = line_height.unwrap_or(f64::from(size) * 1.1719);
            styles.extend(inline.iter().map(|object| serde_json::json!({
                "start":object.0,"end":object.0.saturating_add(1),"color":[0,0,0,0],"textShadow":[[0,0,0,0],0,0,0],"textTransform":[1,0],"textFill":true,
                "fontFamily":"default","fontSize":size,"fontWeight":weight,"italic":false,"decoration":0,
                "letterSpacingEm":(object.3 - intrinsic) / f64::from(base.pt),
                "paragraphHeight":line_height.max(object.4),
            })));
            Some(Value::Array(styles))
        };
        let mut spans = compose_text::spans(text, inline_styles.as_ref().or_else(|| p.get("spans")), base.clone(), glyph_size, size);
        if spans.is_empty() && (base.italic || base.strike || base.underline || base.compose.is_some()) && !text.is_empty() {
            spans.push(Span { text: text.to_owned(), ..base });
        }
        let largest = spans.iter().map(|s| s.pt).fold(glyph_size(size), f32::max);
        // Android's default line-height mode expands a line to fit its tallest font.
        let natural_height = size.max(largest) * 1.1719;
        let lh = line_height.map_or(natural_height, |height| (height as f32).max(natural_height));
        let mut spec = FontSpec::new(largest, lh, weight, align);
        spec.pitch = lh;
        spec.tracking = f(p, "letterSpacingEm").map(|em| em as f32)
            .unwrap_or_else(|| f(p, "letterSpacing").map_or(0.0, |ls| ls as f32 / size));
        spec.android = true;
        spec.fixed_pitch = true;
        spec.no_wrap = p["softWrap"] == false;
        spec.compose_overflow = Some(match p["textOverflow"].as_str() {
            Some("visible") => ComposeOverflow::Visible,
            Some("ellipsis") => ComposeOverflow::Ellipsis,
            Some("startEllipsis") => ComposeOverflow::StartEllipsis,
            Some("middleEllipsis") => ComposeOverflow::MiddleEllipsis,
            _ if p["clipText"] == false => ComposeOverflow::Ellipsis,
            _ => ComposeOverflow::Clip,
        });
        spec.lines = p.get("maxLines").and_then(Value::as_u64).map(|l| (l as usize).max(1));
        TextRun { text: text.to_owned(), font: spec, color: col, strike: false, offset_x: 0.0, spans }
    }

    fn text(&mut self, n: &ViewNode, p: &Value, c: C) -> Laid {
        let text = p["text"].as_str().unwrap_or("");
        let base_size = f(p, "fontSize").filter(|size| *size >= 0.0).unwrap_or(14.0) as f32;
        let inline: Vec<onesim_protocol::ComposeInlineObject> = serde_json::from_value(p["inlineObjects"].clone()).unwrap_or_default();
        let resize_inline = |size: f32| {
            let mut inline = inline.clone();
            let ratio = f64::from(size / base_size.max(0.001));
            for object in &mut inline {
                if object.6 { object.3 *= ratio; }
                if object.7 { object.4 *= ratio; }
            }
            inline
        };
        let maxw = c.max_w.is_finite().then_some(c.max_w as f32);
        let auto = serde_json::from_value::<[onesim_protocol::ComposeTextUnit; 3]>(p["autoSize"].clone()).ok();
        let auto_size = auto.and_then(|units| {
            let [min, max, step] = units.map(|unit| f64::from(unit.at(base_size)));
            let min = min.min(max);
            if min < 0.0 || step < 0.0001 || !max.is_finite() {
                return None;
            }
            let mut low = 0;
            let mut high = ((max - min) / step).floor() as u64;
            while low < high {
                let mid = low + (high - low).div_ceil(2);
                let size = (min + mid as f64 * step) as f32;
                let candidate = self.text_run(text, p, size.max(0.001), &resize_inline(size));
                if self.fonts.compose_fits(&candidate, maxw, c.max_h) {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            Some((min + low as f64 * step) as f32)
        });
        let size = auto_size.unwrap_or(base_size);
        let inline = resize_inline(size);
        let run = self.text_run(text, p, size.max(0.001), &inline);
        let spec = run.font;
        let (tw, th) = self.fonts.measure(text, &spec, maxw, &run.spans);
        let min_h = f(p, "minLines").unwrap_or(1.0) * f64::from(spec.line_height);
        let (w, h) = c.size(self.ceil_px(tw), self.ceil_px(th.max(min_h)));
        let has_bullets = run.spans.iter().any(|span| span.compose.as_ref().is_some_and(|style| style.bullet.is_some()));
        let (action, selection_color) = (p["textClick"].as_u64(), color(p.get("selectionColor")));
        let layout_callback = p["textLayout"].as_u64();
        let mut interactive = (action.is_some() || selection_color.is_some() || layout_callback.is_some() || !inline.is_empty() || has_bullets).then(|| {
            let scale = self.params.map_or(self.density, |p| p.scale as f32);
            let mut text = ComposeText::new(self.fonts, &run, w, scale, action, selection_color);
            text.handle_color = color(p.get("selectionHandleColor"));
            text.layout_callback = layout_callback;
            let ellipsis = spec.compose_overflow == Some(ComposeOverflow::Ellipsis)
                || ((spec.no_wrap || spec.lines == Some(1)) &&
                    matches!(spec.compose_overflow, Some(ComposeOverflow::StartEllipsis | ComposeOverflow::MiddleEllipsis)));
            if layout_callback.is_some() && ellipsis {
                text.untruncated_extent = Some(self.fonts.compose_extent(&run, Some(w as f32)));
            }
            text.constraints = [c.min_w, c.max_w, c.min_h, c.max_h];
            text.auto_font_size = auto_size;
            text.pending_auto_size = p["customAutoSize"] == true;
            text.link_ranges = serde_json::from_value(p["linkRanges"].clone()).unwrap_or_default();
            text.speech_ranges = serde_json::from_value(p["speechRanges"].clone()).unwrap_or_default();
            text.speech_ranges.sort_by_key(|(start, _, _)| *start);
            text.inline = inline;
            text.original_text = p["sourceText"].as_str().map(str::to_owned);
            Box::new(text)
        });
        let mut kids = Vec::new();
        if let Some(layout) = &mut interactive {
            if has_bullets {
                for (i, (rect, bullet)) in layout.bullet_rects(&run.spans, w).into_iter().enumerate() {
                    if rect.w <= 0.0 || rect.h <= 0.0 || (rect.y >= h && spec.compose_overflow != Some(ComposeOverflow::Visible)) {
                        continue;
                    }
                    let shape = Shape::parse(bullet.wire.shape.as_ref());
                    let mut color = bullet.wire.color.map(|[r, g, b, a]| Color::rgba(r, g, b, a)).unwrap_or(run.color);
                    color.a *= bullet.wire.alpha.unwrap_or(1.0);
                    let outlined = bullet.wire.shape.as_ref().is_some_and(|shape|
                        shape["path"].is_array() || shape["roundedOutline"].is_array()
                    );
                    let deco = if bullet.wire.brush.is_some() || bullet.wire.stroke_style.is_some() || outlined {
                        Deco::Bullet(Box::new(bullet.clone()), color, self.density)
                    } else if let Some(stroke) = bullet.wire.stroke {
                        Deco::Border(f64::from(stroke).max(1.0) / f64::from(self.density), color, shape)
                    } else {
                        Deco::Background(color, shape)
                    };
                    kids.push((rect.x, rect.y, Laid { id: format!("{}/bullet/{i}", n.id), w: rect.w, h: rect.h, deco, kids: vec![] }));
                }
            }
            let rects = layout.inline_rects(&run.text, run.font.pt);
            let children = parts(n).2;
            let scale = self.params.map_or(f64::from(self.density), |params| params.scale);
            if layout.pending_auto_size {
                for probe in children.iter().skip(layout.inline.len()) {
                    let (_, props, _) = parts(probe);
                    let Ok(physical) = serde_json::from_value::<[i32; 4]>(props["probeConstraints"].clone()) else { continue };
                    let [min_w, max_w, min_h, max_h] = physical.map(|pixels| {
                        if pixels == i32::MAX { f64::INFINITY } else { f64::from(pixels.max(0)) / scale }
                    });
                    let measured = self.text(probe, props, C { min_w, max_w, min_h, max_h });
                    if let Deco::Text(run, Some(info)) = &measured.deco {
                        if let Some(data) = info.layout_data(run, Rect { x: 0.0, y: 0.0, w: measured.w, h: measured.h }) {
                            layout.probe_layouts.push(data);
                        }
                    }
                }
            }
            kids.extend(children.iter().zip(rects).filter_map(|(child, rect)| {
                let rect = rect?;
                if spec.compose_overflow != Some(ComposeOverflow::Visible) && (rect.y >= h || rect.x >= w) {
                    return None;
                }
                let child = self.measure(child, C { min_w: 0.0, max_w: rect.w.max(0.0), min_h: 0.0, max_h: rect.h.max(0.0) });
                Some((rect.x, rect.y, child))
            }));
        }
        Laid { id: n.id.clone(), w, h, deco: Deco::Text(run, interactive), kids }
    }

    fn boxed(&mut self, n: &ViewNode, p: &Value, children: &[ViewNode], c: C) -> Laid {
        let (ah, av) = p.get("contentAlignment").map(pair).unwrap_or((Some(-1.0), Some(-1.0)));
        let propagate = p.get("propagateMinConstraints").and_then(Value::as_bool) == Some(true);
        let kc = if propagate { c } else { c.loose() };
        let laid: Vec<(Laid, Option<&Value>)> = children.iter().map(|k| (self.measure(k, kc), align(k))).collect();
        let (mw, mh) = laid.iter().fold((0.0f64, 0.0f64), |(w, h), (l, _)| (w.max(l.w), h.max(l.h)));
        let (w, h) = c.size(mw, mh);
        let kids = laid
            .into_iter()
            .map(|(l, a)| {
                let (bh, bv) = a.map(pair).unwrap_or((ah, av));
                (self.bias(w, l.w, bh.unwrap_or(-1.0)), self.bias(h, l.h, bv.unwrap_or(-1.0)), l)
            })
            .collect();
        let deco = if p["selectionContainer"] == true { Deco::SelectionContainer } else { Deco::Core("box") };
        Laid { id: n.id.clone(), w, h, deco, kids }
    }

    /// `Column` (`vertical`) / `Row`: Compose's RowColumnMeasurePolicy.
    fn linear(&mut self, n: &ViewNode, p: &Value, children: &[ViewNode], c: C, vertical: bool) -> Laid {
        // Main/cross views of the constraints.
        let (main_min, main_max, cross_max) = if vertical { (c.min_h, c.max_h, c.max_w) } else { (c.min_w, c.max_w, c.max_h) };
        let cross_min = if vertical { c.min_w } else { c.min_h };
        let arr = p.get("arrangement").cloned().unwrap_or(Value::Null);
        let kind = arr.get("type").and_then(Value::as_str).unwrap_or("start");
        let space = if kind == "spacedBy" { self.px(f(&arr, "space").unwrap_or(0.0)) } else { 0.0 };
        let total_space = space * children.len().saturating_sub(1) as f64;
        let mk = |main_lo: f64, main_hi: f64| {
            if vertical {
                C { min_w: 0.0, max_w: cross_max, min_h: main_lo, max_h: main_hi }
            } else {
                C { min_w: main_lo, max_w: main_hi, min_h: 0.0, max_h: cross_max }
            }
        };
        let main_of = |l: &Laid| if vertical { l.h } else { l.w };
        let mut laid: Vec<Option<Laid>> = vec![None; children.len()];
        let mut fixed = 0.0;
        for (i, k) in children.iter().enumerate() {
            if weight(k).is_none() {
                let rem = if main_max.is_finite() { (main_max - fixed - total_space).max(0.0) } else { INF };
                let l = self.measure(k, mk(0.0, rem));
                fixed += main_of(&l);
                laid[i] = Some(l);
            }
        }
        let total_w: f64 = children.iter().filter_map(weight).map(|w| w.0).sum();
        if total_w > 0.0 {
            let target = if main_max.is_finite() { main_max } else { main_min };
            let remaining = (target - fixed - total_space).max(0.0);
            for (i, k) in children.iter().enumerate() {
                if let Some((wt, fill)) = weight(k) {
                    let share = self.px(remaining * wt / total_w).max(0.0);
                    laid[i] = Some(self.measure(k, mk(if fill { share } else { 0.0 }, share)));
                }
            }
        }
        let laid: Vec<Laid> = laid.into_iter().map(|l| l.expect("every child measured")).collect();
        let used: f64 = laid.iter().map(main_of).sum::<f64>() + total_space;
        let main = used.max(main_min).min(main_max.max(main_min));
        let cross_used = laid.iter().map(|l| if vertical { l.w } else { l.h }).fold(0.0, f64::max);
        let cross = cross_used.max(cross_min).min(cross_max.max(cross_min));
        // Main-axis positions (Arrangement).
        let sizes: Vec<f64> = laid.iter().map(main_of).collect();
        let content: f64 = sizes.iter().sum();
        let free = (main - content - total_space).max(0.0);
        let cnt = sizes.len() as f64;
        let (mut pos, gap) = match kind {
            "end" | "bottom" => (free, 0.0),
            "center" => (free / 2.0, 0.0),
            "spaceBetween" if cnt > 1.0 => (0.0, free / (cnt - 1.0)),
            "spaceAround" if cnt > 0.0 => (free / cnt / 2.0, free / cnt),
            "spaceEvenly" => (free / (cnt + 1.0), free / (cnt + 1.0)),
            "spacedBy" => (bias(free, 0.0, f(&arr, "align").unwrap_or(-1.0)), 0.0),
            _ => (0.0, 0.0),
        };
        let cross_bias = f(p, "alignment").unwrap_or(-1.0);
        let mut kids = Vec::with_capacity(laid.len());
        for (k, l) in children.iter().zip(laid) {
            let b = align(k).and_then(Value::as_f64).unwrap_or(cross_bias);
            let (lm, lc) = if vertical { (l.h, l.w) } else { (l.w, l.h) };
            let off = self.bias(cross, lc, b);
            // Arrangement positions are fractional sums, each rounded to a pixel.
            let at = self.px(pos);
            let (x, y) = if vertical { (off, at) } else { (at, off) };
            pos += lm + space + gap;
            kids.push((x, y, l));
        }
        let (w, h) = if vertical { (cross, main) } else { (main, cross) };
        Laid { id: n.id.clone(), w, h, deco: Deco::Core(if vertical { "column" } else { "row" }), kids }
    }
}

/// Concatenated text under a layer (accessibility label of a clickable, like Compose's merged semantics).
fn label_of(l: &Laid, out: &mut Vec<String>) {
    if let Deco::Text(t, metadata) = &l.deco {
        if !t.text.is_empty() {
            out.push(metadata.as_ref().and_then(|text| text.original_text.clone()).unwrap_or_else(|| t.text.clone()));
        }
    }
    for (_, _, k) in &l.kids {
        label_of(k, out);
    }
}

fn flatten(l: &Laid, x: f64, y: f64, parent: usize, opacity: f32, out: &mut Vec<Node>, modals: &mut ModalLayers) {
    let mut n = Node::new(&l.id, "layer");
    n.bounds = Rect { x, y, w: l.w, h: l.h };
    n.parent = Some(parent);
    n.inspect = false;
    let mut op = opacity;
    match &l.deco {
        Deco::None => {}
        Deco::Background(c, s) => {
            n.kind = "background";
            n.fill = Some(*c);
            n.radius = s.radius(l.w, l.h);
        }
        Deco::Border(w, c, s) => {
            // Compose draws the border inside the bounds; the scene strokes on the outline.
            n.kind = "border";
            n.stroke = Some((*c, *w));
            n.radius = (s.radius(l.w, l.h) - w / 2.0).max(0.0);
            n.bounds = Rect { x: x + w / 2.0, y: y + w / 2.0, w: (l.w - w).max(0.0), h: (l.h - w).max(0.0) };
        }
        Deco::Clip(s) => {
            n.kind = "clip";
            n.clip_radius = Some(s.radius(l.w, l.h));
        }
        Deco::Shadow(e, s) if *e > 0.0 => {
            // Android's elevation shadow: an ambient shadow plus a spot shadow cast by a light above the top of the
            // screen, so it shifts down more the lower the element sits. Fitted (RMS 1.3/255) to an elevated card
            // (1 dp) and an extended FAB (6 dp) on the Android 17 emulator.
            let r = s.radius(l.w, l.h);
            let mut layer = |id: String, bounds: Rect, radius: f64, sh: Shadow| {
                let mut m = Node::new(&id, "shadow");
                m.bounds = bounds;
                m.parent = Some(parent);
                m.inspect = false;
                m.radius = radius;
                m.opacity = opacity;
                m.shadow = Some(sh);
                out.push(m);
            };
            let black = |a: f32| Color::rgba(0.0, 0.0, 0.0, a);
            layer(format!("{}#ambient", l.id), n.bounds, r, Shadow { color: black(0.088), radius: 0.285 * e, dx: 0.0, dy: 0.0 });
            const LIGHT_Z: f64 = 398.0;
            let inset = (0.686 * e).min(l.w / 2.0).min(l.h / 2.0);
            let spot = Rect { x: x + inset, y: y + inset, w: l.w - 2.0 * inset, h: l.h - 2.0 * inset };
            let dy = e * (y + l.h / 2.0) / (LIGHT_Z - e).max(1.0);
            layer(format!("{}#spot", l.id), spot, (r - inset).max(0.0), Shadow { color: black(0.199), radius: 0.861 * e, dx: 0.0, dy });
        }
        Deco::Shadow(..) => {}
        Deco::Alpha(a) => op *= a,
        Deco::Click { action, role, label } => {
            n.kind = "clickable";
            n.inspect = true;
            n.action = *action;
            n.disabled = action.is_none();
            n.role = *role;
            n.label = label.clone().or_else(|| {
                let mut parts = vec![];
                label_of(l, &mut parts);
                (!parts.is_empty()).then(|| parts.join(" "))
            });
        }
        Deco::Text(t, interactive) => {
            n.kind = "text";
            n.inspect = true;
            n.role = Role::Text;
            if interactive.as_ref().is_some_and(|layout| !layout.inline.is_empty())
                && t.font.compose_overflow != Some(ComposeOverflow::Visible)
            {
                n.clip_radius = Some(0.0);
            }
            n.text = Some(t.clone());
            n.compose_text = interactive.clone();
        }
        Deco::Bullet(bullet, color, density) => {
            use crate::scene::{VectorFill, VectorGeom, VectorShape};
            let max_radius = l.w.min(l.h) / 2.0;
            let radii = std::array::from_fn(|i| {
                let shape = bullet.wire.shape.as_ref();
                let radius = shape.and_then(|shape| shape["corners"][i].as_f64())
                    .or_else(|| shape.and_then(|shape| shape["percent"][i].as_f64()).map(|pct| pct * max_radius / 50.0))
                    .unwrap_or_default().clamp(0.0, max_radius);
                (radius, radius)
            });
            let ink = if bullet.wire.brush.is_some() {
                Color::rgba(1.0, 1.0, 1.0, bullet.wire.alpha.unwrap_or(1.0))
            } else { *color };
            let stroke = bullet.wire.stroke_style.as_ref().map(|style| crate::compose_paint::stroke(style, ink, *density));
            n.kind = "bullet";
            let shape = bullet.wire.shape.as_ref().unwrap_or(&Value::Null);
            let geom = if let Ok(ops) = serde_json::from_value(shape["path"].clone()) {
                VectorGeom::Path { ops, mirror_x: false }
            } else if let (Ok([left, top, right, bottom]), Ok(sizes)) = (
                serde_json::from_value::<[f64; 4]>(shape["roundedOutline"].clone()),
                serde_json::from_value(shape["cornerRadii"].clone()),
            ) {
                n.bounds = Rect { x: x + left, y: y + top, w: (right - left).max(0.0), h: (bottom - top).max(0.0) };
                VectorGeom::RoundedRect { radii: sizes, continuous: false }
            } else {
                VectorGeom::RoundedRect { radii, continuous: false }
            };
            n.vector = Some(Box::new(VectorShape {
                geom,
                fill: stroke.is_none().then_some(VectorFill { color: ink, eo_fill: shape["eoFill"] == true, antialiased: true }),
                stroke,
            }));
            n.vector_brush.clone_from(&bullet.wire.brush);
        }
        Deco::SelectionContainer => {
            n.kind = "box";
            n.inspect = true;
            n.selection_container = true;
        }
        Deco::Control(control) => {
            let (bounds, parent) = (n.bounds, n.parent);
            n = (**control).clone();
            n.bounds = bounds;
            n.parent = parent;
            op *= n.opacity;
        }
        Deco::Embedded(embedded) => {
            embedded.place((x, y), Some(parent), opacity, out, modals);
            return;
        }
        Deco::Core(k) => {
            n.kind = k;
            n.inspect = true;
        }
    }
    n.opacity = op;
    let idx = out.len();
    out.push(n);
    for (dx, dy, k) in &l.kids {
        flatten(k, x + dx, y + dy, idx, op, out, modals);
    }
}

pub(super) fn embedded(
    root: &ViewNode,
    available: (Option<f64>, Option<f64>),
    params: &SceneParams,
    fonts: &mut Fonts,
    place: Option<Placement<'_>>,
) -> (f64, f64) {
    if let ViewKind::Compose { props, .. } = &root.kind {
        fonts.register_compose_fonts(&props["fontResources"]);
    }
    let mut cx = Cx { fonts, notes: BTreeSet::new(), density: params.device.scale as f32, params: Some(params) };
    let laid = cx.measure(root, C {
        min_w: 0.0, max_w: available.0.unwrap_or(INF),
        min_h: 0.0, max_h: available.1.unwrap_or(INF),
    });
    if let Some(Placement { bounds, parent, opacity, nodes, modals }) = place {
        flatten(&laid, bounds.x, bounds.y, parent, opacity, nodes, modals);
    }
    (laid.w, laid.h)
}

/// Lay out a Compose tree full screen (edge to edge) and add the Android system bars.
pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    let dark = params.appearance == Appearance::Dark;
    if let ViewKind::Compose { props, .. } = &root.kind {
        fonts.register_compose_fonts(&props["fontResources"]);
    }
    let mut cx = Cx { fonts, notes: BTreeSet::new(), density: params.device.scale as f32, params: Some(params) };
    let bg = if dark { 0.0 } else { 1.0 };
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(Color::rgba(bg, bg, bg, 1.0));
    let mut nodes = vec![screen];
    // The content view gets the window's constraints, like ComposeView in an Activity: loose, bounded by the screen.
    let laid = cx.measure(root, C { min_w: 0.0, max_w: w, min_h: 0.0, max_h: h });
    let mut modals = ModalLayers::default();
    flatten(&laid, 0.0, 0.0, 0, 1.0, &mut nodes, &mut modals);
    modals.paint(&mut nodes);
    if params.status_bar {
        android_status_bar(&mut nodes, params, cx.fonts, dark);
    }
    android_nav_handle(&mut nodes, params, dark);
    Scene {
        size: params.viewport,
        appearance: params.appearance,
        ios: params.ios,
        nodes,
        unsupported_symbols: vec![],
        notes: cx.notes.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(v: Value) -> ViewNode {
        serde_json::from_value(v).expect("compose node")
    }

    /// Lays out at density 1 (dp = px), so the model's arithmetic is exact.
    fn lay(v: Value, c: C) -> Laid {
        lay_at(v, c, 1.0)
    }

    fn lay_at(v: Value, c: C, density: f32) -> Laid {
        let mut fonts = Fonts::with_fallback(true);
        let mut cx = Cx { fonts: &mut fonts, notes: BTreeSet::new(), density, params: None };
        cx.measure(&node(v), c)
    }

    fn sized(id: &str, w: f64, h: f64) -> Value {
        json!({"kind":"compose","id":id,"component":"Box","props":{"modifier":[{"width":w},{"height":h}]}})
    }

    #[test]
    fn column_stacks_and_wraps() {
        let l = lay(json!({"kind":"compose","id":"c","component":"Column","props":{},"children":[sized("a",50.0,10.0), sized("b",30.0,20.0)]}), C { min_w: 0.0, max_w: 400.0, min_h: 0.0, max_h: 800.0 });
        assert_eq!((l.w, l.h), (50.0, 30.0));
        assert_eq!((l.kids[0].0, l.kids[0].1), (0.0, 0.0));
        assert_eq!((l.kids[1].0, l.kids[1].1), (0.0, 10.0));
    }

    #[test]
    fn padding_then_background_is_outside_the_fill() {
        // Modifier.padding(8).background(red).size(20): the fill is 20x20 at (8, 8).
        let v = json!({"kind":"compose","id":"b","component":"Box","props":{"modifier":[{"padding":[8,8,8,8]},{"background":{"color":[1,0,0,1]}},{"width":20},{"height":20}]}});
        let l = lay(v, C { min_w: 0.0, max_w: 400.0, min_h: 0.0, max_h: 800.0 });
        assert_eq!((l.w, l.h), (36.0, 36.0));
        let (x, y, bg) = &l.kids[0];
        assert_eq!((*x, *y, bg.w, bg.h), (8.0, 8.0, 20.0, 20.0));
        assert!(matches!(bg.deco, Deco::Background(..)));
    }

    #[test]
    fn row_weights_split_remaining_space_and_arrangement_centers() {
        let row = json!({"kind":"compose","id":"r","component":"Row","props":{"modifier":[{"fillMaxWidth":1.0}]},"children":[
            sized("fixed", 100.0, 10.0),
            {"kind":"compose","id":"w1","component":"Box","props":{"modifier":[{"weight":1.0,"fill":true}]}},
            {"kind":"compose","id":"w3","component":"Box","props":{"modifier":[{"weight":3.0,"fill":true}]}}
        ]});
        let l = lay(row, C { min_w: 0.0, max_w: 300.0, min_h: 0.0, max_h: 800.0 });
        assert_eq!(l.w, 300.0);
        let xs: Vec<(f64, f64)> = l.kids.iter().map(|k| (k.0, k.2.w)).collect();
        assert_eq!(xs, vec![(0.0, 100.0), (100.0, 50.0), (150.0, 150.0)]);

        let centered = json!({"kind":"compose","id":"c","component":"Column","props":{"arrangement":{"type":"center"},"alignment":0.0,"modifier":[{"fillMaxWidth":1.0},{"fillMaxHeight":1.0}]},"children":[sized("a",40.0,20.0)]});
        let l = lay(centered, C { min_w: 0.0, max_w: 200.0, min_h: 0.0, max_h: 100.0 });
        assert_eq!((l.kids[0].0, l.kids[0].1), (80.0, 40.0));
    }

    #[test]
    fn spaced_by_and_size_coercion() {
        let col = json!({"kind":"compose","id":"c","component":"Column","props":{"arrangement":{"type":"spacedBy","space":8}},"children":[sized("a",10.0,10.0), sized("b",10.0,10.0), sized("c",500.0,10.0)]});
        let l = lay(col, C { min_w: 0.0, max_w: 200.0, min_h: 0.0, max_h: 800.0 });
        let ys: Vec<f64> = l.kids.iter().map(|k| k.1).collect();
        assert_eq!(ys, vec![0.0, 18.0, 36.0]);
        // width(500) is coerced into the 200 dp incoming maximum.
        assert_eq!(l.w, 200.0);
    }

    #[test]
    fn dp_round_to_whole_pixels() {
        // Pixel 10a (2.625x): 12 dp spacing is 31.5 px -> 32 px; a 10 dp child is 26.25 px -> 26 px.
        let col = json!({"kind":"compose","id":"c","component":"Column","props":{"arrangement":{"type":"spacedBy","space":12}},"children":[sized("a",10.0,10.0), sized("b",10.0,10.0)]});
        let l = lay_at(col, C { min_w: 0.0, max_w: 200.0, min_h: 0.0, max_h: 800.0 }, 2.625);
        let px = |dp: f64| (dp * 2.625 * 1e6).round() / 1e6;
        assert_eq!(l.kids.iter().map(|k| px(k.1)).collect::<Vec<_>>(), vec![0.0, 58.0]);
        assert_eq!(px(l.h), 26.0 + 32.0 + 26.0);
    }

    #[test]
    fn default_min_size_and_box_alignment() {
        // Material Button: defaultMinSize(58, 40) around a small label, centred.
        let b = json!({"kind":"compose","id":"b","component":"Box","props":{"contentAlignment":[0,0],"modifier":[{"defaultMinSize":[58,40]}]},"children":[sized("l",20.0,10.0)]});
        let l = lay(b, C { min_w: 0.0, max_w: 400.0, min_h: 0.0, max_h: 800.0 });
        assert_eq!((l.w, l.h), (58.0, 40.0));
        assert_eq!((l.kids[0].0, l.kids[0].1), (19.0, 15.0));
    }

    #[test]
    fn wrap_content_preserves_constraints_on_the_other_axis() {
        let constraints = C { min_w: 200.0, max_w: 200.0, min_h: 100.0, max_h: 100.0 };
        let child = sized("child", 50.0, 10.0);
        let column = json!({"kind":"compose","id":"column","component":"Column",
            "props":{"modifier":[{"wrapContent":[1.0,null]}]},"children":[child]});
        let column = lay(column, constraints);
        let (x, y, inner) = &column.kids[0];
        assert_eq!((*x, *y, inner.w, inner.h), (150.0, 0.0, 50.0, 100.0));

        let row = json!({"kind":"compose","id":"row","component":"Row",
            "props":{"modifier":[{"wrapContent":[null,1.0]}]},"children":[child]});
        let row = lay(row, constraints);
        let (x, y, inner) = &row.kids[0];
        assert_eq!((*x, *y, inner.w, inner.h), (0.0, 90.0, 200.0, 10.0));
    }

    fn android_scene(root: Value) -> Scene {
        let mut fonts = Fonts::with_fallback(true);
        let dev = crate::device(crate::DEFAULT_ANDROID_DEVICE).unwrap();
        let scroll = Default::default();
        let sheets = Default::default();
        let params = SceneParams {
            viewport: (dev.viewport.width, dev.viewport.height),
            device: dev,
            scale: dev.scale,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            ios: Default::default(),
            scroll: &scroll,
            sheets: &sheets,
            focus: None,
            overscroll: false,
            status_bar: true,
            chrome: Default::default(),
        };
        layout(&node(root), &params, &mut fonts)
    }

    #[test]
    fn clickable_gets_action_and_label_in_scene() {
        let scene = android_scene(json!({"kind":"compose","id":"root","component":"Box","props":{"modifier":[{"clickable":{"action":7,"enabled":true}},{"padding":[24,8,24,8]}]},"children":[
            {"kind":"compose","id":"t","component":"Text","props":{"text":"Increment","fontSize":14,"lineHeight":20,"fontWeight":500}}
        ]}));
        let i = scene.hit_test(30.0, 15.0).expect("clickable hit");
        assert_eq!(scene.nodes[i].action, Some(7));
        assert_eq!(scene.nodes[i].label.as_deref(), Some("Increment"));
        let t = scene.nodes.iter().find(|n| n.id == "t").unwrap();
        // A 20 sp line is 52.5 px, rounded up to 53 px like Compose's text layout.
        assert_eq!((t.bounds.x, t.bounds.y, t.bounds.h), (24.0, 8.0, 53.0 / 2.625));
    }

    #[test]
    fn annotated_text_preserves_utf16_ranges_and_overlapping_properties() {
        let scene = android_scene(json!({"kind":"compose","id":"rich","component":"Text","props":{
            "text":"Aé😀Z","spans":[{"start":1,"end":4,"color":[0,0,1,1]},{"start":2,"end":5,"fontWeight":700},
            {"start":3,"end":4,"fontSize":100}]
        }}));
        let run = scene.nodes.iter().find_map(|n| n.text.as_ref().filter(|t| t.text == "Aé😀Z")).unwrap();
        let styles: Vec<_> = run.spans.iter().map(|s| (s.text.as_str(), s.color.b, s.weight)).collect();
        assert_eq!(styles, vec![("A", 0.0, 400), ("é", 1.0, 400), ("😀", 1.0, 700), ("Z", 0.0, 700)]);
        assert!(run.font.size < 20.0, "a range splitting a surrogate pair is ignored");
    }

    #[test]
    fn text_click_uses_the_nearest_caret_on_the_tapped_line() {
        use crate::{Effect, Interaction};
        use onesim_protocol::{PointerEvent, PointerKind};
        let scene = android_scene(json!({"kind":"compose","id":"label","component":"Text","props":{
            "text":"😀\r\nA","lineHeight":24,"textAlign":"center","textClick":7,
            "modifier":[{"width":80},{"padding":[4,4,4,4]}],"spans":[{"start":0,"end":5,"fontWeight":700}]
        }}));
        let mut input = Interaction::default();
        input.version = 5;
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Click, 10.0, 35.0)),
            Effect::TextClick { id: 7, version: 5, offset: 4 });
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Click, 70.0, 35.0)),
            Effect::TextClick { id: 7, version: 5, offset: 5 });
    }

    #[test]
    fn text_selection_skips_disabled_descendants_and_clears_on_tap() {
        use crate::{Effect, Interaction};
        use onesim_protocol::{PointerEvent, PointerKind};
        let mut scene = android_scene(json!({"kind":"compose","id":"group","component":"Box","props":{"selectionContainer":true},"children":[
            {"kind":"compose","id":"column","component":"Column","children":[
                {"kind":"compose","id":"a","component":"Text","props":{"text":"alpha","selectionColor":[0.3,0.2,0.5,0.4]}},
                {"kind":"compose","id":"b","component":"Text","props":{"text":"exclude"}},
                {"kind":"compose","id":"c","component":"Text","props":{"text":"charlie","selectionColor":[0.3,0.2,0.5,0.4]}}
            ]}
        ]}));
        let mut input = Interaction::default();
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Down, 8.0, 8.0)), Effect::None);
        assert!(input.holding());
        assert!(!input.tick(&scene, 0.2));
        assert!(input.tick(&scene, 0.31), "selection appears before the pointer is released");
        input.apply_text_selection(&mut scene);
        let first = scene.nodes.iter().find(|n| n.id == "a").unwrap();
        assert!(!first.compose_text.as_ref().unwrap().highlight.is_empty());
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Up, 8.0, 8.0)), Effect::Redraw);
        assert_eq!(input.pointer(&scene, PointerEvent::moved(PointerKind::Drag, 1.0, 8.0, 25.0, 33.0)), Effect::Redraw);
        input.apply_text_selection(&mut scene);
        let highlighted: Vec<_> = scene.nodes.iter().filter_map(|n| {
            if !n.inspect { return None; }
            n.compose_text.as_ref().filter(|t| !t.highlight.is_empty()).map(|_| n.id.as_str())
        }).collect();
        assert_eq!(highlighted, vec!["a", "c"]);
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Click, 200.0, 200.0)), Effect::Redraw);
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::LongPress, 8.0, 22.0)), Effect::None);
    }

    #[test]
    fn touch_selection_toolbar_copies_selected_text_and_uses_handle_color() {
        use crate::{Effect, Interaction, SelectionOutput};
        use onesim_protocol::{PointerEvent, PointerKind};
        let mut scene = android_scene(json!({
            "kind":"compose","id":"group","component":"Box","props":{"selectionContainer":true,"modifier":[{"padding":[24,105,0,0]}]},
            "children":[{"kind":"compose","id":"text","component":"Text","props":{
                "text":"one two","selectionColor":[0.3,0.2,0.5,0.4],"selectionHandleColor":[0.2,0.5,0.8,1]
            }}]
        }));
        let mut input = Interaction::default();
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::LongPress, 30.0, 113.0)), Effect::Redraw);
        let original = scene.clone();
        input.apply_text_selection(&mut scene);
        let handle = scene.nodes.iter().find(|n| n.id == "compose-selection-start").unwrap();
        assert_eq!(handle.vector.as_ref().unwrap().fill.unwrap().color, Color::rgba(0.2, 0.5, 0.8, 1.0));
        let end = scene.nodes.iter().find(|n| n.id == "compose-selection-end").unwrap();
        let (x, y) = end.bounds.center();
        assert_eq!(input.pointer(&scene, PointerEvent::moved(PointerKind::Drag, x, y, 50.0, 0.0)), Effect::Redraw);
        scene = original;
        input.apply_text_selection(&mut scene);
        let copy = scene.nodes.iter().find(|n| n.label.as_deref() == Some("Copy")).unwrap();
        let (x, y) = copy.bounds.center();
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Click, x, y)), Effect::Redraw);
        assert_eq!(input.take_selection_output(), Some(SelectionOutput::Copy("one two".into())));
    }

    #[test]
    fn read_aloud_uses_verbatim_annotation() {
        use crate::{Effect, Interaction, SelectionOutput};
        use onesim_protocol::{PointerEvent, PointerKind};
        let mut scene = android_scene(json!({
            "kind":"compose","id":"selection","component":"Box",
            "props":{"selectionContainer":true,"modifier":[{"padding":[24,105,0,0]}]},
            "children":[{"kind":"compose","id":"text","component":"Text","props":{
                "text":"GPU","selectionColor":[0.3,0.2,0.5,0.4],"speechRanges":[[0,3,"graphics processor"]]
            }}]
        }));
        let mut input = Interaction::default();
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::LongPress, 30.0, 113.0)), Effect::Redraw);
        input.apply_text_selection(&mut scene);
        let read = scene.nodes.iter().find(|node| node.label.as_deref() == Some("Read aloud")).unwrap();
        let (x, y) = read.bounds.center();
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Click, x, y)), Effect::Redraw);
        assert_eq!(input.take_selection_output(), Some(SelectionOutput::ReadAloud("graphics processor".into())));
    }

    #[test]
    fn callback_reports_a_single_line_ellipsis() {
        let scene = android_scene(json!({"kind":"compose","id":"text","component":"Text","props":{
            "text":"This text does not fit", "textLayout":1, "textOverflow":"ellipsis",
            "softWrap":false, "maxLines":1, "modifier":[{"width":45}]
        }}));
        let output = &scene.compose_text_layouts()[0].geometry;
        let flags = output.rsplit(';').next().unwrap().parse::<u8>().unwrap();
        assert_eq!(flags & 8, 8);
    }

    #[test]
    fn link_annotation_dispatches_only_on_link_glyphs() {
        use crate::{Effect, Interaction};
        use onesim_protocol::{PointerEvent, PointerKind};
        let scene = android_scene(json!({"kind":"compose","id":"parent","component":"Box","props":{"modifier":[{"clickable":{"action":8}}]},
            "children":[{"kind":"compose","id":"link","component":"Text","props":{"text":"MMMM GO","textClick":4,"linkRanges":[[5,7]]}}]
        }));
        let text = scene.nodes.iter().find(|n| n.id == "link").unwrap();
        let inside = |x| PointerEvent::new(PointerKind::Click, x, text.bounds.y + text.bounds.h / 2.0);
        let mut input = Interaction::default();
        assert_eq!(input.pointer(&scene, inside(3.0)), Effect::Action { id: 8, version: 0 });
        assert_eq!(input.pointer(&scene, inside(text.bounds.w - 4.0)), Effect::TextClick { id: 4, version: 0, offset: 6 });
    }

    #[test]
    fn inline_composables_reserve_space_and_report_original_offsets() {
        let scene = android_scene(json!({"kind":"compose","id":"text","component":"Text","props":{
            "text":"A \u{fffc} B","sourceText":"A badge B","textLayout":1,"inlineObjects":[[2,2,7,24,16,"center"]]
        },"children":[{"kind":"compose","id":"badge","component":"Box","props":{"modifier":[{"width":24},{"height":16}]}}]}));
        let badge = scene.nodes.iter().find(|n| n.id == "badge").unwrap();
        assert!(badge.bounds.x > 10.0);
        assert!((badge.bounds.w - 24.0).abs() < 0.01);
        let measurements = scene.compose_text_layouts();
        assert_eq!(measurements.len(), 1);
        assert!(measurements[0].geometry.split(';').nth(2).unwrap().starts_with("0,9,9,"));
    }

    #[test]
    fn paragraph_styles_split_lines_and_apply_alignment_and_height() {
        let scene = android_scene(json!({"kind":"compose","id":"text","component":"Text","props":{
            "text":"firstsecond","textLayout":1,"modifier":[{"width":180}],"spans":[
                {"start":0,"end":5,"paragraph":1},
                {"start":5,"end":11,"paragraph":2,"paragraphAlign":"end","paragraphHeight":32}
            ]
        }}));
        let layouts = scene.compose_text_layouts();
        let lines = layouts[0].geometry.split(';').skip(2).take(2).map(|line| {
            line.split(',').take(9).map(|field| field.parse::<f32>().unwrap()).collect::<Vec<_>>()
        }).collect::<Vec<_>>();
        assert_eq!((lines[0][0], lines[1][0]), (0.0, 5.0));
        assert!(lines[1][3] > 200.0, "the second paragraph is aligned to the trailing edge");
        assert!((lines[1][6] - lines[1][5] - 84.0).abs() < 0.01, "32 sp at 2.625x");
    }

    #[test]
    fn bullets_and_indentation_preserve_source_offsets_and_selection_geometry() {
        let scene = android_scene(json!({
            "kind":"compose","id":"text","component":"Text","props":{
                "text":"A wrapped bullet with more words","textClick":1,"textLayout":1,
                "modifier":[{"width":96}],"spans":[
                    {"start":0,"end":32,"paragraph":1,"paragraphIndent":[[20,false],[12,false]]},
                    {"start":0,"end":32,"bullet":{"width":[4,false],"height":[4,false],"padding":[5,false],"color":[0,0,1,1]}}
                ]
            }
        }));
        let marker = scene.nodes.iter().find(|node| node.id == "text/bullet/0").expect("one bullet");
        assert_eq!(marker.bounds.w, 4.0);
        let text = scene.nodes.iter().find(|node| node.id == "text").unwrap();
        let layout = text.compose_text.as_ref().unwrap();
        let bounds = text.bounds;
        let boxes = layout.rects(0..32, bounds);
        assert!(boxes.len() > 1);
        assert!((boxes[0].x - 20.0).abs() < 0.4);
        assert!((boxes[1].x - 12.0).abs() < 0.4);
        assert_eq!(layout.offset_at(&text.text.as_ref().unwrap().text, boxes[0].x, boxes[0].y + 1.0), 0);
    }

    #[test]
    fn auto_size_uses_the_largest_step_that_fits_untruncated_text() {
        let measure = |width, height| {
            let scene = android_scene(json!({"kind":"compose","id":"text","component":"Text","props":{
                "text":"Fits the space","textLayout":1,"fontSize":14,"maxLines":1,"textOverflow":"ellipsis",
                "autoSize":[[10,false],[38,false],[1,false]],
                "modifier":[{"width":width},{"height":height}]
            }}));
            let layout = &scene.compose_text_layouts()[0].geometry;
            layout.split(';').next().unwrap().split(',').nth(7).unwrap().parse::<u32>().unwrap()
        };
        assert_eq!(measure(148, 32), 23);
        assert_eq!(measure(400, 100), 38);
        assert_eq!(measure(148, 12), 10);
    }

    #[test]
    fn link_style_precedence_and_keyboard_activation_follow_link_ranges() {
        use crate::{Effect, Interaction};
        use onesim_protocol::{KeyboardEvent, PointerEvent, PointerKind};
        let tree = json!({"kind":"compose","id":"text","component":"Text","props":{
            "text":"One Two","textClick":4,"linkRanges":[[0,3],[4,7]],
            "linkStyles":[
                [{"start":0,"end":3,"color":[1,0,0,1]},{"start":0,"end":3,"color":[0,1,0,1]},{"start":0,"end":3,"color":[0,0,1,1]}],
                [null,null,null]
            ]
        }});
        let root = serde_json::from_value(tree.clone()).unwrap();
        let scene = android_scene(tree);
        let mut input = Interaction::default();
        let key = |key: &str| KeyboardEvent::Key { key: key.into() };
        assert_eq!(input.keyboard(&scene, key("tab")), Ok(Effect::Redraw));
        let color = |input: &Interaction| {
            let tree = serde_json::to_value(input.styled_compose_tree(&root)).unwrap();
            tree["props"]["spans"].as_array().unwrap().last().unwrap()["color"].clone()
        };
        assert_eq!(color(&input), json!([1,0,0,1]));
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Hover, 4.0, 8.0)), Effect::Redraw);
        assert_eq!(color(&input), json!([0,1,0,1]));
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Down, 4.0, 8.0)), Effect::Redraw);
        assert_eq!(color(&input), json!([0,0,1,1]));
        assert_eq!(input.pointer(&scene, PointerEvent::new(PointerKind::Up, 42.0, 8.0)), Effect::Redraw);
        assert_eq!(color(&input), json!([0,1,0,1]));
        assert_eq!(input.keyboard(&scene, key("tab")), Ok(Effect::Redraw));
        assert_eq!(input.keyboard(&scene, key("enter")), Ok(Effect::TextClick { id: 4, version: 0, offset: 4 }));
    }

}
