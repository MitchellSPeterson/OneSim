//! Jetpack Compose layout (protocol section K): `Box` / `Column` / `Row` / `Text` primitives with a
//! Modifier chain -> Compose's constraint-based measure/place -> `Scene`.
//!
//! Material components arrive already decomposed into these primitives (the guest's `material3` is built from
//! them, as in AndroidX), so this module only models `androidx.compose.ui` + `foundation.layout` semantics:
//! constraints flow down, sizes flow up, parents place children. Units are dp (= scene points).

use super::common::{Embedded, ModalLayers, Placement, SceneParams};
use crate::chrome::{android_nav_handle, android_status_bar};
use crate::scene::{Color, Node, Rect, Role, Scene, ScrollInfo, Shadow, TextRun};
use crate::text::{android_glyph_size, FontSpec, Fonts, TextAlign};
use serde_json::Value;
use onesim_protocol::{Appearance, GestureMod, ViewKind, ViewNode};
use std::collections::BTreeSet;

const INF: f64 = f64::INFINITY;

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
    Click { action: Option<u64>, role: Role, label: Option<String>, gesture: Option<GestureMod> },
    /// Pointer input the guest handles (`pointerInput`, `draggable`, `scrollable`, ...): touches go to it by key.
    Gesture(GestureMod),
    /// An indication (ripple) drawn over the content.
    Ripple(Ripple),
    Text(TextRun),
    Control(Box<Node>),
    Embedded(Embedded),
    /// The component's own node (`Box`, `Column`, `Row`).
    Core(&'static str),
}

/// Ripple and state layer of an indication, drawn after the content it wraps (protocol section K). The guest sends
/// animation progress; the geometry follows the layer's bounds, as Material's `RippleAnimation` does.
#[derive(Debug, Clone, PartialEq)]
struct Ripple {
    color: Color,
    /// Clipped to the layer's bounds (bounded ripple) or not.
    bounded: bool,
    /// State layer opacity (hover, focus, drag), over the whole layer (bounded) or a `radius` circle (unbounded).
    layer: f32,
    /// Explicit radius (dp).
    radius: Option<f64>,
    ripples: Vec<RippleCircle>,
}

#[derive(Debug, Clone, PartialEq)]
struct RippleCircle {
    /// Press point (dp, relative to the layer); `None` = the centre.
    origin: Option<(f64, f64)>,
    /// Progress 0...1 of the radius (eased) and of the centre's move from the press point to the middle.
    radius: f64,
    center: f64,
    alpha: f32,
}

impl Ripple {
    fn parse(v: &Value) -> Ripple {
        let circles = v.get("ripples").and_then(Value::as_array).map(|a| {
            a.iter().map(|c| RippleCircle {
                origin: c.get("origin").and_then(Value::as_array).and_then(|o| Some((o.first()?.as_f64()?, o.get(1)?.as_f64()?))),
                radius: f(c, "radius").unwrap_or(0.0).clamp(0.0, 1.0),
                center: f(c, "center").unwrap_or(0.0).clamp(0.0, 1.0),
                alpha: f(c, "alpha").unwrap_or(0.0).clamp(0.0, 1.0) as f32,
            }).collect()
        });
        Ripple {
            color: color(v.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0)),
            bounded: v.get("bounded").and_then(Value::as_bool).unwrap_or(true),
            layer: f(v, "layer").unwrap_or(0.0).clamp(0.0, 1.0) as f32,
            radius: f(v, "radius").filter(|r| r.is_finite() && *r >= 0.0),
            ripples: circles.unwrap_or_default(),
        }
    }

    /// Final radius in a `w` x `h` layer: explicit, or covering it (bounded: 10 dp past the corners).
    fn target_radius(&self, w: f64, h: f64) -> f64 {
        let covering = w.hypot(h) / 2.0;
        self.radius.unwrap_or(if self.bounded { covering + 10.0 } else { covering })
    }
}

fn gesture_mod(v: &Value) -> Option<GestureMod> {
    serde_json::from_value::<GestureMod>(v.clone()).ok().filter(|g| !g.key.is_empty())
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
    /// Tab positions of the `TabRow`s being laid out (innermost last), read by `tabIndicator` modifiers.
    tabs: Vec<Vec<TabPos>>,
}

/// One tab of a `TabRow` (Material `TabPosition`): left edge and width in the row, and its content's width.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TabPos {
    left: f64,
    width: f64,
    content: f64,
}

/// Material's `HorizontalTextPadding` on each side of a tab's text.
const TAB_TEXT_PADDING: f64 = 16.0;

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
        let make_id = || format!("{}#m{i}", n.id);
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
            let p = [
                self.px(p.first().and_then(Value::as_f64).unwrap_or(0.0)),
                self.px(p.get(1).and_then(Value::as_f64).unwrap_or(0.0)),
                self.px(p.get(2).and_then(Value::as_f64).unwrap_or(0.0)),
                self.px(p.get(3).and_then(Value::as_f64).unwrap_or(0.0)),
            ];
            let (h, v) = (p[0] + p[2], p[1] + p[3]);
            let inner = next(self, c.shrink(h, v));
            let (w, ht) = c.size(inner.w + h, inner.h + v);
            return Laid { id: make_id(), w, h: ht, deco: Deco::None, kids: vec![(p[0], p[1], inner)] };
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
            return Laid { id: make_id(), w: ow, h: oh, deco: Deco::None, kids: vec![(self.bias(ow, inner.w, 0.0), 0.0, inner)] };
        }
        if let Some(h) = self.opx(f(m, "requiredHeight")) {
            let inner = next(self, C { min_h: h, max_h: h, ..c });
            let (ow, oh) = c.size(inner.w, inner.h);
            return Laid { id: make_id(), w: ow, h: oh, deco: Deco::None, kids: vec![(0.0, self.bias(oh, inner.h, 0.0), inner)] };
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
            return Laid { id: make_id(), w, h, deco: Deco::None, kids: vec![(x, y, inner)] };
        }
        if let Some(min) = self.opx(f(m, "minInteractive")) {
            // `minimumInteractiveComponentSize`: at least `min` on each side, content centred; may exceed the
            // incoming constraints (Material buttons are 40 dp tall inside a 48 dp slot).
            let inner = next(self, c);
            let (w, h) = (inner.w.max(min), inner.h.max(min));
            return Laid { id: make_id(), w, h, deco: Deco::None, kids: vec![(self.bias(w, inner.w, 0.0), self.bias(h, inner.h, 0.0), inner)] };
        }
        if let Some(t) = m.get("tabIndicator") {
            // `tabIndicatorOffset`: inside a TabRow's indicator, the wrapped content takes the selected tab's (or its
            // content's) width at its place in the row, sliding from where the previous selection was.
            let Some(tabs) = self.tabs.last().filter(|t| !t.is_empty()).cloned() else { return next(self, c) };
            let content = t.get("match").and_then(Value::as_str) == Some("content");
            let at = |i: f64| -> (f64, f64) {
                let i = (i.max(0.0) as usize).min(tabs.len() - 1);
                let p = tabs[i];
                if content { (p.left + (p.width - p.content) / 2.0, p.content) } else { (p.left, p.width) }
            };
            let lerp = |a: (f64, f64), b: (f64, f64), f: f64| (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
            let to = at(f(t, "index").unwrap_or(0.0));
            let from = t.get("from").and_then(Value::as_array).map_or(to, |a| {
                let n = |k: usize| a.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                lerp(at(n(0)), at(n(1)), n(2))
            });
            let (x, w) = lerp(from, to, f(t, "progress").unwrap_or(1.0).clamp(-1.0, 2.0));
            let (x, w) = (self.px(x), self.px(w.max(0.0)));
            let inner = next(self, C { min_w: w, max_w: w, min_h: 0.0, max_h: c.max_h });
            let row_w = if c.max_w.is_finite() { c.max_w } else { x + w };
            return Laid { id: make_id(), w: row_w.max(c.min_w), h: inner.h.max(c.min_h), deco: Deco::None, kids: vec![(x, 0.0, inner)] };
        }
        if m.get("weight").is_some() || m.get("align").is_some() {
            return next(self, c); // parent data, read by the parent
        }
        if let Some(o) = m.get("offset") {
            let (x, y) = pair(o);
            let (x, y) = (self.opx(x), self.opx(y));
            let inner = next(self, c);
            return Laid { id: make_id(), w: inner.w, h: inner.h, deco: Deco::None, kids: vec![(x.unwrap_or(0.0), y.unwrap_or(0.0), inner)] };
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
            let gesture = k.get("key").and_then(Value::as_str).filter(|_| enabled)
                .map(|key| GestureMod { key: key.to_owned(), pans: false, axis: None });
            Deco::Click { action, role, label: k.get("label").and_then(Value::as_str).map(str::to_owned), gesture }
        } else if let Some(g) = m.get("gesture") {
            match gesture_mod(g) {
                Some(g) => Deco::Gesture(g),
                None => Deco::None,
            }
        } else if let Some(r) = m.get("ripple") {
            Deco::Ripple(Ripple::parse(r))
        } else {
            let key = m.as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
            self.notes.insert(format!("unsupported_modifier: {key}"));
            Deco::None
        };
        let inner = next(self, c);
        Laid::wrap(make_id(), deco, inner)
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
            "TabRow" => self.tab_row(n, props, children, c),
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
        if let Deco::Text(run) = &mut label.deco {
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

    fn text(&mut self, n: &ViewNode, p: &Value, c: C) -> Laid {
        let text = p.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        let size = f(p, "fontSize").unwrap_or(14.0) as f32;
        let lh = f(p, "lineHeight").map_or(size * 1.1719, |l| l as f32); // Roboto's own line gap when unspecified
        let weight = f(p, "fontWeight").unwrap_or(400.0) as u16;
        let align = match p.get("textAlign").and_then(Value::as_str) {
            Some("center") => TextAlign::Center,
            Some("end") => TextAlign::Trailing,
            _ => TextAlign::Leading,
        };
        let mut spec = FontSpec::new(android_glyph_size(size, self.density), lh, weight, align);
        spec.pitch = lh;
        spec.tracking = f(p, "letterSpacing").map_or(0.0, |ls| ls as f32 / size);
        spec.android = true;
        let soft = p.get("softWrap").and_then(Value::as_bool) != Some(false);
        spec.lines = p.get("maxLines").and_then(Value::as_u64).map(|l| l as usize).or((!soft).then_some(1));
        let maxw = c.max_w.is_finite().then_some(c.max_w as f32);
        let (tw, th) = self.fonts.measure(&text, &spec, maxw, &[]);
        let (w, h) = c.size(self.ceil_px(tw), self.ceil_px(th));
        let col = color(p.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let run = TextRun { text, font: spec, color: col, strike: false, offset_x: 0.0, spans: vec![] };
        Laid { id: n.id.clone(), w, h, deco: Deco::Text(run), kids: vec![] }
    }

    /// Material's tab rows (`TabRow`: the tabs, then the divider slot and the indicator slot). Fixed rows
    /// split the width equally (`TabRowImpl`); scrollable ones give each tab its own width, at least `minTabWidth`,
    /// between `edgePadding`s, and scroll horizontally with the selected tab centred (`ScrollableTabData`).
    fn tab_row(&mut self, n: &ViewNode, p: &Value, children: &[ViewNode], c: C) -> Laid {
        // The tabs, then the divider slot and the indicator slot.
        let count = children.len().saturating_sub(2);
        let (tabs, slots) = children.split_at(count);
        let (divider, indicator) = (slots.first(), slots.get(1));
        let scrollable = p.get("scrollable").and_then(Value::as_bool) == Some(true);
        let loose = C { min_w: 0.0, max_w: INF, min_h: 0.0, max_h: INF };
        // A tab's content width (`maxIntrinsicWidth` less the text padding, at least 24 dp).
        let natural: Vec<Laid> = tabs.iter().map(|t| self.measure(t, loose)).collect();
        let content_of = |l: &Laid, cap: f64| (l.w.min(cap) - 2.0 * TAB_TEXT_PADDING).max(24.0);
        let d = self.density as f64;
        let width = if c.max_w.is_finite() { c.max_w } else { natural.iter().map(|l| l.w).sum::<f64>() };
        if !scrollable {
            // Integer division in pixels, as `tabRowWidth / tabCount`.
            let tw = if count > 0 && d > 0.0 { ((width * d) / count as f64).floor() / d } else { 0.0 };
            let h = tabs.iter().map(|t| self.measure(t, C { min_w: tw, max_w: tw, min_h: 0.0, max_h: INF }).h).fold(0.0, f64::max);
            let positions: Vec<TabPos> =
                natural.iter().enumerate().map(|(i, l)| TabPos { left: tw * i as f64, width: tw, content: content_of(l, tw) }).collect();
            let mut kids = vec![];
            for (i, t) in tabs.iter().enumerate() {
                kids.push((tw * i as f64, 0.0, self.measure(t, C { min_w: tw, max_w: tw, min_h: h, max_h: h })));
            }
            if let Some(dv) = divider {
                let l = self.measure(dv, C { min_w: 0.0, max_w: width, min_h: 0.0, max_h: h });
                kids.push((0.0, h - l.h, l));
            }
            if let Some(ind) = indicator {
                self.tabs.push(positions);
                let l = self.measure(ind, C { min_w: 0.0, max_w: width, min_h: 0.0, max_h: h });
                self.tabs.pop();
                kids.push((0.0, h - l.h, l));
            }
            let (w, h) = c.size(width, h);
            return Laid { id: n.id.clone(), w, h, deco: Deco::Core("tab_row"), kids };
        }
        let edge = self.px(f(p, "edgePadding").unwrap_or(52.0));
        let min_tab = self.px(f(p, "minTabWidth").unwrap_or(90.0));
        let h = natural.iter().map(|l| l.h).fold(0.0, f64::max);
        let mut positions = vec![];
        let mut row = vec![];
        let mut left = edge;
        for (t, l) in tabs.iter().zip(&natural) {
            let laid = self.measure(t, C { min_w: min_tab, max_w: INF, min_h: h, max_h: h });
            let tw = laid.w.max(min_tab);
            positions.push(TabPos { left, width: tw, content: content_of(l, INF) });
            row.push((left, 0.0, laid));
            left += tw;
        }
        let content_w = left + edge;
        if let Some(ind) = indicator {
            self.tabs.push(positions.clone());
            let l = self.measure(ind, C { min_w: 0.0, max_w: content_w, min_h: 0.0, max_h: h });
            self.tabs.pop();
            row.push((0.0, h - l.h, l));
        }
        // The scroller: `fillMaxWidth().wrapContentSize(CenterStart).horizontalScroll()`.
        let viewport = width.min(content_w);
        let max = (content_w - viewport).max(0.0);
        let centred = |i: f64| -> f64 {
            let i = (i.max(0.0) as usize).min(positions.len().saturating_sub(1));
            positions.get(i).map_or(0.0, |t| (t.left - (viewport / 2.0 - t.width / 2.0)).clamp(0.0, max))
        };
        let selected = f(p, "selected").unwrap_or(0.0);
        let scroll = p.get("scroll");
        let target = centred(selected);
        let animated = scroll.and_then(|s| Some((f(s, "from")?, f(s, "progress")?)))
            .map(|(from, q)| centred(from) + (target - centred(from)) * q.clamp(0.0, 1.0));
        // Each selection is a fresh scroll position (centred on the selected tab); a finger scroll keeps its offset.
        let seq = scroll.and_then(|s| f(s, "seq")).unwrap_or(0.0);
        let sid = format!("{}@{seq}", n.id);
        let offset = self.params.and_then(|pp| pp.scroll.get(&sid)).map(|o| o.0).or(animated).unwrap_or(target).clamp(0.0, max);
        let inner = Laid { id: format!("{}#tabs", n.id), w: content_w, h, deco: Deco::Core("row"), kids: row };
        let mut node = Node::new(sid.clone(), "compose_scroll");
        node.clip_radius = Some(0.0);
        node.scroll = Some(ScrollInfo { max: (max, 0.0), offset: (offset, 0.0), vertical: false, snap: None, content: Some((content_w, h)), page: None, aligned: false });
        let scroller = Laid { id: sid, w: viewport, h, deco: Deco::Control(Box::new(node)), kids: vec![(-offset, 0.0, inner)] };
        let mut kids = vec![];
        let total_h = h;
        if let Some(dv) = divider {
            let l = self.measure(dv, C { min_w: 0.0, max_w: width, min_h: 0.0, max_h: INF });
            kids.push((0.0, total_h - l.h, l));
        }
        kids.push((0.0, 0.0, scroller));
        let (w, h) = c.size(width, total_h);
        Laid { id: n.id.clone(), w, h, deco: Deco::Core("tab_row"), kids }
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
        Laid { id: n.id.clone(), w, h, deco: Deco::Core("box"), kids }
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
    if let Deco::Text(t) = &l.deco {
        if !t.text.is_empty() {
            out.push(t.text.clone());
        }
    }
    for (_, _, k) in &l.kids {
        label_of(k, out);
    }
}

fn flatten(l: Laid, x: f64, y: f64, parent: usize, opacity: f32, out: &mut Vec<Node>, modals: &mut ModalLayers) {
    let Laid { id, w, h, deco, kids } = l;
    let mut n = Node::new(id, "layer");
    n.bounds = Rect { x, y, w, h };
    n.parent = Some(parent);
    n.inspect = false;
    let mut op = opacity;
    match deco {
        Deco::None => {}
        Deco::Background(c, s) => {
            n.kind = "background";
            n.fill = Some(c);
            n.radius = s.radius(w, h);
        }
        Deco::Border(bw, c, s) => {
            // Compose draws the border inside the bounds; the scene strokes on the outline.
            n.kind = "border";
            n.stroke = Some((c, bw));
            n.radius = (s.radius(w, h) - bw / 2.0).max(0.0);
            n.bounds = Rect { x: x + bw / 2.0, y: y + bw / 2.0, w: (w - bw).max(0.0), h: (h - bw).max(0.0) };
        }
        Deco::Clip(s) => {
            n.kind = "clip";
            n.clip_radius = Some(s.radius(w, h));
        }
        Deco::Shadow(e, s) if e > 0.0 => {
            // Android's elevation shadow: an ambient shadow plus a spot shadow cast by a light above the top of the
            // screen, so it shifts down more the lower the element sits. Fitted (RMS 1.3/255) to an elevated card
            // (1 dp) and an extended FAB (6 dp) on the Android 17 emulator.
            let r = s.radius(w, h);
            let mut layer = |lid: String, bounds: Rect, radius: f64, sh: Shadow| {
                let mut m = Node::new(lid, "shadow");
                m.bounds = bounds;
                m.parent = Some(parent);
                m.inspect = false;
                m.radius = radius;
                m.opacity = opacity;
                m.shadow = Some(sh);
                out.push(m);
            };
            let black = |a: f32| Color::rgba(0.0, 0.0, 0.0, a);
            layer(format!("{}#ambient", n.id), n.bounds, r, Shadow { color: black(0.088), radius: 0.285 * e, dx: 0.0, dy: 0.0 });
            const LIGHT_Z: f64 = 398.0;
            let inset = (0.686 * e).min(w / 2.0).min(h / 2.0);
            let spot = Rect { x: x + inset, y: y + inset, w: w - 2.0 * inset, h: h - 2.0 * inset };
            let dy = e * (y + h / 2.0) / (LIGHT_Z - e).max(1.0);
            layer(format!("{}#spot", n.id), spot, (r - inset).max(0.0), Shadow { color: black(0.199), radius: 0.861 * e, dx: 0.0, dy });
        }
        Deco::Shadow(..) => {}
        Deco::Alpha(a) => op *= a,
        Deco::Click { action, role, label, gesture } => {
            n.kind = "clickable";
            n.inspect = true;
            n.action = action;
            n.gesture = gesture.map(Box::new);
            n.disabled = action.is_none();
            n.role = role;
            n.label = label.or_else(|| {
                let mut parts = vec![];
                for (_, _, k) in &kids {
                    label_of(k, &mut parts);
                }
                (!parts.is_empty()).then(|| parts.join(" "))
            });
        }
        Deco::Gesture(g) => n.gesture = Some(Box::new(g)),
        Deco::Ripple(r) => {
            n.opacity = op;
            let idx = out.len();
            out.push(n);
            for (dx, dy, k) in kids {
                flatten(k, x + dx, y + dy, idx, op, out, modals);
            }
            ripple(&r, idx, op, out);
            return;
        }
        Deco::Text(t) => {
            n.kind = "text";
            n.inspect = true;
            n.role = Role::Text;
            n.text = Some(t);
        }
        Deco::Control(control) => {
            let (bounds, parent) = (n.bounds, n.parent);
            n = *control;
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
    for (dx, dy, k) in kids {
        flatten(k, x + dx, y + dy, idx, op, out, modals);
    }
}

/// The ripple of layer `idx`, painted after its content: an overlay clipped to the layer (bounded) holding the state
/// layer and one circle per ripple.
fn ripple(r: &Ripple, idx: usize, opacity: f32, out: &mut Vec<Node>) {
    if r.layer <= 0.0 && r.ripples.iter().all(|c| c.alpha <= 0.0) {
        return;
    }
    let (b, base) = (out[idx].bounds, out[idx].id.clone());
    let mut overlay = Node::new(format!("{base}#ripple"), "ripple");
    overlay.bounds = b;
    overlay.parent = Some(idx);
    overlay.inspect = false;
    overlay.opacity = opacity;
    if r.bounded {
        overlay.clip_radius = Some(0.0);
    }
    let parent = out.len();
    out.push(overlay);
    let tint = |alpha: f32| Color::rgba(r.color.r, r.color.g, r.color.b, r.color.a * alpha);
    let mut paint = |id: String, bounds: Rect, radius: f64, alpha: f32| {
        let mut c = Node::new(id, "ripple_circle");
        c.bounds = bounds;
        c.radius = radius;
        c.parent = Some(parent);
        c.inspect = false;
        c.opacity = opacity;
        c.fill = Some(tint(alpha));
        out.push(c);
    };
    let circle = |cx: f64, cy: f64, radius: f64| Rect { x: b.x + cx - radius, y: b.y + cy - radius, w: 2.0 * radius, h: 2.0 * radius };
    let target = r.target_radius(b.w, b.h);
    if r.layer > 0.0 {
        if r.bounded {
            paint(format!("{base}#ripple-layer"), b, 0.0, r.layer);
        } else {
            paint(format!("{base}#ripple-layer"), circle(b.w / 2.0, b.h / 2.0, target), target, r.layer);
        }
    }
    // Material's RippleAnimation: from 30% of the larger side at the press point to `target` at the centre.
    let start = b.w.max(b.h) * 0.3;
    let (mx, my) = (b.w / 2.0, b.h / 2.0);
    for (i, c) in r.ripples.iter().enumerate() {
        let radius = start + (target - start) * c.radius;
        let (ox, oy) = c.origin.filter(|_| r.bounded).unwrap_or((mx, my));
        let (cx, cy) = (ox + (mx - ox) * c.center, oy + (my - oy) * c.center);
        if radius > 0.0 && c.alpha > 0.0 {
            paint(format!("{base}#ripple{i}"), circle(cx, cy, radius), radius, c.alpha);
        }
    }
}

pub(super) fn embedded(
    root: &ViewNode,
    available: (Option<f64>, Option<f64>),
    params: &SceneParams,
    fonts: &mut Fonts,
    place: Option<Placement<'_>>,
) -> (f64, f64) {
    let mut cx = Cx { fonts, notes: BTreeSet::new(), density: params.device.scale as f32, params: Some(params), tabs: vec![] };
    let laid = cx.measure(root, C {
        min_w: 0.0, max_w: available.0.unwrap_or(INF),
        min_h: 0.0, max_h: available.1.unwrap_or(INF),
    });
    let size = (laid.w, laid.h);
    if let Some(Placement { bounds, parent, opacity, nodes, modals }) = place {
        flatten(laid, bounds.x, bounds.y, parent, opacity, nodes, modals);
    }
    size
}

/// Lay out a Compose tree full screen (edge to edge) and add the Android system bars.
pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    let dark = params.appearance == Appearance::Dark;
    let mut cx = Cx { fonts, notes: BTreeSet::new(), density: params.device.scale as f32, params: Some(params), tabs: vec![] };
    let bg = if dark { 0.0 } else { 1.0 };
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(Color::rgba(bg, bg, bg, 1.0));
    let mut nodes = vec![screen];
    // The content view gets the window's constraints, like ComposeView in an Activity: loose, bounded by the screen.
    let laid = cx.measure(root, C { min_w: 0.0, max_w: w, min_h: 0.0, max_h: h });
    let mut modals = ModalLayers::default();
    flatten(laid, 0.0, 0.0, 0, 1.0, &mut nodes, &mut modals);
    modals.paint(&mut nodes);
    if params.status_bar {
        android_status_bar(&mut nodes, params, cx.fonts, dark);
    }
    android_nav_handle(&mut nodes, params, dark);
    // The app's window: edge swipes and the back key are Android system back (section N). Not drawn, not hit.
    let mut back = Node::new("system_back", crate::interaction::SYSTEM_BACK);
    back.bounds = Rect { x: 0.0, y: 0.0, w, h };
    back.parent = Some(0);
    back.inspect = false;
    nodes.push(back);
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
        let mut cx = Cx { fonts: &mut fonts, notes: BTreeSet::new(), density, params: None, tabs: vec![] };
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

    #[test]
    fn clickable_gets_action_and_label_in_scene() {
        let root = json!({"kind":"compose","id":"root","component":"Box","props":{"modifier":[{"clickable":{"action":7,"enabled":true}},{"padding":[24,8,24,8]}]},"children":[
            {"kind":"compose","id":"t","component":"Text","props":{"text":"Increment","fontSize":14,"lineHeight":20,"fontWeight":500}}
        ]});
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
        let scene = layout(&node(root), &params, &mut fonts);
        let i = scene.hit_test(30.0, 15.0).expect("clickable hit");
        assert_eq!(scene.nodes[i].action, Some(7));
        assert_eq!(scene.nodes[i].label.as_deref(), Some("Increment"));
        let t = scene.nodes.iter().find(|n| n.id == "t").unwrap();
        // A 20 sp line is 52.5 px, rounded up to 53 px like Compose's text layout.
        assert_eq!((t.bounds.x, t.bounds.y, t.bounds.h), (24.0, 8.0, 53.0 / 2.625));
    }

    fn scene_of(root: Value) -> Scene {
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
    fn keyed_clickable_and_pointer_input_are_gesture_targets() {
        let root = json!({"kind":"compose","id":"root","component":"Box","props":{"modifier":[
            {"gesture":{"key":"root#m0","pans":true,"axis":"horizontal"}},{"width":200},{"height":100}]},"children":[
            {"kind":"compose","id":"b","component":"Box","props":{"modifier":[{"width":80},{"height":40},
                {"clickable":{"action":3,"enabled":true,"role":"button","key":"b#m2"}}]}},
            {"kind":"compose","id":"off","component":"Box","props":{"modifier":[{"offset":[100,50]},{"width":40},{"height":40},
                {"clickable":{"action":4,"enabled":false,"key":"off#m3"}}]}}
        ]});
        let scene = scene_of(root);
        // The clickable is the innermost target, its pointer-input parent follows; the control does not block it.
        let keys = |x, y| scene.gesture_targets(x, y).iter().map(|&i| scene.nodes[i].gesture.as_ref().unwrap().key.clone()).collect::<Vec<_>>();
        assert_eq!(keys(10.0, 10.0), ["b#m2", "root#m0"]);
        assert!(scene.gesture_touch_at(10.0, 10.0));
        let outer = scene.nodes.iter().find(|n| n.gesture.as_ref().is_some_and(|g| g.key == "root#m0")).unwrap();
        // Sizes snap to whole device pixels (2.625 px per dp).
        assert_eq!((outer.bounds.w, outer.bounds.h), (200.0, 263.0 / 2.625));
        assert_eq!(outer.gesture.as_ref().unwrap().axis, Some(onesim_protocol::Axis::Horizontal));
        assert!(outer.gesture.as_ref().unwrap().pans);
        // A disabled clickable has no key and no action: touches reach the parent's pointer input.
        assert_eq!(keys(110.0, 60.0), ["root#m0"]);
        assert!(scene.gesture_touch_at(110.0, 60.0));
        // A malformed gesture entry (no key) is ignored.
        let bad = scene_of(json!({"kind":"compose","id":"root","component":"Box","props":{"modifier":[{"gesture":{"pans":true}},{"width":10},{"height":10}]}}));
        assert!(bad.nodes.iter().all(|n| n.gesture.is_none()));
        assert!(bad.notes.is_empty(), "gesture is a known modifier: {:?}", bad.notes);
    }

    #[test]
    fn ripple_is_painted_over_the_content_and_grows_to_cover_bounded_layers() {
        let ripple = |r: Value| json!({"kind":"compose","id":"root","component":"Box","props":{"modifier":[
            {"width":100},{"height":40},{"ripple":r},{"background":{"color":[0,0,1,1]}}]}});
        let scene = scene_of(ripple(json!({"color":[1,1,1,1],"bounded":true,"layer":0,
            "ripples":[{"origin":[10,10],"radius":0,"center":0,"alpha":0.1},{"origin":[10,10],"radius":1,"center":1,"alpha":0.05}]})));
        let bg = scene.nodes.iter().position(|n| n.kind == "background").unwrap();
        let overlay = scene.nodes.iter().position(|n| n.kind == "ripple").unwrap();
        assert!(overlay > bg, "drawn after the content it wraps");
        assert_eq!(scene.nodes[overlay].clip_radius, Some(0.0), "bounded ripples are clipped");
        let circles: Vec<_> = scene.nodes.iter().filter(|n| n.kind == "ripple_circle").collect();
        assert_eq!(circles.len(), 2);
        let b = scene.nodes[overlay].bounds;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Start: 30% of the larger side around the press point.
        let start = &circles[0];
        assert!(near(start.radius, b.w.max(b.h) * 0.3) && near(start.bounds.x + start.radius, 10.0) && near(start.bounds.y + start.radius, 10.0));
        assert_eq!(start.fill.unwrap().a, 0.1);
        // End: half the diagonal + 10 dp, centred.
        let end = &circles[1];
        assert!(near(end.radius, b.w.hypot(b.h) / 2.0 + 10.0));
        assert!(near(end.bounds.x + end.radius, b.w / 2.0) && near(end.bounds.y + end.radius, b.h / 2.0));
        // Unbounded with an explicit radius: centred, not clipped; a state layer is a circle of that radius.
        let scene = scene_of(ripple(json!({"color":[1,0,0,1],"bounded":false,"radius":20,"layer":0.16,
            "ripples":[{"origin":null,"radius":1,"center":0,"alpha":0.1}]})));
        let overlay = scene.nodes.iter().find(|n| n.kind == "ripple").unwrap();
        assert_eq!(overlay.clip_radius, None);
        let circles: Vec<_> = scene.nodes.iter().filter(|n| n.kind == "ripple_circle").collect();
        assert_eq!(circles.len(), 2);
        let b = overlay.bounds;
        assert!(circles.iter().all(|c| c.radius == 20.0 && (c.bounds.x + 20.0 - b.w / 2.0).abs() < 1e-9 && (c.bounds.y + 20.0 - b.h / 2.0).abs() < 1e-9));
        assert_eq!(circles[0].fill.unwrap().a, 0.16);
        // Nothing to draw: no overlay at all.
        let scene = scene_of(ripple(json!({"color":[1,1,1,1],"bounded":true,"layer":0,"ripples":[]})));
        assert!(scene.nodes.iter().all(|n| n.kind != "ripple"));
        // A bounded state layer fills the layer.
        let scene = scene_of(ripple(json!({"color":[0,0,0,1],"bounded":true,"layer":0.3})));
        let layer = scene.nodes.iter().find(|n| n.kind == "ripple_circle").unwrap();
        let overlay = scene.nodes.iter().find(|n| n.kind == "ripple").unwrap();
        assert_eq!((layer.bounds, layer.radius), (overlay.bounds, 0.0));
        assert!((layer.fill.unwrap().a - 0.3).abs() < 1e-6);
    }
}
