//! SwiftUI-semantics layout: parent proposes, child chooses, parent places.
//!
//! Stacks probe each child at 0 and infinity before the final proposal; `Ctx::memo`
//! caches results per (node, proposal) so nesting stays polynomial instead of 3^depth.

use crate::scene::{Color, Node, Rect, Role, Scene, TextRun};
use crate::text::Fonts;
use simcore_protocol::{Alignment, Appearance, Modifier, Semantic, ViewKind, ViewNode};
use std::cmp::Ordering;
use std::collections::HashMap;

const INF: f64 = f64::INFINITY;
const FONT_SIZE: f32 = 17.0;
const LINE_HEIGHT: f32 = 22.0;
const DEFAULT_SPACING: f64 = 8.0;

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
    node: Option<Node>,
    kids: Vec<(f64, f64, Lay)>,
}

#[derive(Clone, Copy)]
struct Env {
    fg: Option<Color>,
    /// Axis of the directly enclosing stack (true = vertical), for Spacer.
    axis: Option<bool>,
}

struct Ctx<'a> {
    fonts: &'a mut Fonts,
    dark: bool,
    /// Keyed by (node address, proposal bits). A node's Env is fixed by its ancestors,
    /// so node + proposal fully determines its layout within one pass.
    memo: HashMap<(usize, u64, u64), Lay>,
}

pub fn layout(root: &ViewNode, viewport: (f64, f64), appearance: Appearance, fonts: &mut Fonts) -> Scene {
    let mut cx = Ctx { fonts, dark: appearance == Appearance::Dark, memo: HashMap::new() };
    let lay = cx.view(root, Prop { w: Some(viewport.0), h: Some(viewport.1) }, Env { fg: None, axis: None });
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w: viewport.0, h: viewport.1 };
    screen.fill = Some(cx.semantic(Semantic::Background));
    let mut nodes = vec![screen];
    // Root view is centered in the viewport.
    let (x, y) = ((viewport.0 - lay.size.w) / 2.0, (viewport.1 - lay.size.h) / 2.0);
    flatten(lay, x, y, Some(0), 1.0, &mut nodes);
    Scene { size: viewport, appearance, nodes }
}

fn flatten(lay: Lay, x: f64, y: f64, mut parent: Option<usize>, mut opacity: f32, out: &mut Vec<Node>) {
    if let Some(mut n) = lay.node {
        n.bounds = Rect { x, y, w: lay.size.w, h: lay.size.h };
        n.parent = parent;
        opacity *= n.opacity;
        n.opacity = opacity;
        out.push(n);
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
        ViewKind::Button { label, .. } => first_text(label),
        _ => None,
    }
}

impl Ctx<'_> {
    fn semantic(&self, s: Semantic) -> Color {
        let d = self.dark;
        match s {
            Semantic::Primary if d => Color::rgba(1.0, 1.0, 1.0, 1.0),
            Semantic::Primary => Color::rgba(0.0, 0.0, 0.0, 1.0),
            Semantic::Secondary if d => Color::rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.6),
            Semantic::Secondary => Color::rgba(60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.6),
            Semantic::Accent if d => Color::rgba(10.0 / 255.0, 132.0 / 255.0, 1.0, 1.0),
            Semantic::Accent => Color::rgba(0.0, 122.0 / 255.0, 1.0, 1.0),
            Semantic::Background if d => Color::rgba(0.0, 0.0, 0.0, 1.0),
            Semantic::Background => Color::rgba(1.0, 1.0, 1.0, 1.0),
        }
    }

    fn color(&self, c: &simcore_protocol::Color) -> Color {
        match c {
            simcore_protocol::Color::Semantic { semantic } => self.semantic(*semantic),
            simcore_protocol::Color::Rgba { r, g, b, a } => Color::rgba(*r as f32, *g as f32, *b as f32, *a as f32),
        }
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
                Lay { size, node: None, kids: vec![(e.leading, e.top, inner)] }
            }
            Modifier::Frame(f) => {
                let cw = child_prop(p.w, f.width, f.min_width, f.max_width);
                let ch = child_prop(p.h, f.height, f.min_height, f.max_height);
                let inner = self.mods(v, rest, Prop { w: cw, h: ch }, env);
                let w = frame_len(p.w, inner.size.w, f.width, f.min_width, f.max_width);
                let h = frame_len(p.h, inner.size.h, f.height, f.min_height, f.max_height);
                let (dx, dy) = ((w - inner.size.w) * h_frac(f.alignment), (h - inner.size.h) * v_frac(f.alignment));
                Lay { size: Size { w, h }, node: None, kids: vec![(dx, dy, inner)] }
            }
            Modifier::ForegroundColor(c) => {
                let env = Env { fg: Some(self.color(c)), ..env };
                self.mods(v, rest, p, env)
            }
            Modifier::Background(bg) => {
                let inner = self.mods(v, rest, p, env);
                let bgl = self.view(bg, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                Lay { size: inner.size, node: None, kids: vec![(0.0, 0.0, bgl), (0.0, 0.0, inner)] }
            }
            Modifier::CornerRadius(r) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#clip{}", v.id, rest.len()), "clip");
                n.clip_radius = Some(*r);
                n.inspect = false;
                Lay { size: inner.size, node: Some(n), kids: vec![(0.0, 0.0, inner)] }
            }
            Modifier::Opacity(o) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#opacity{}", v.id, rest.len()), "opacity");
                n.opacity = *o as f32;
                n.inspect = false;
                Lay { size: inner.size, node: Some(n), kids: vec![(0.0, 0.0, inner)] }
            }
        }
    }

    fn base(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let child_env = Env { axis: None, ..env };
        match &v.kind {
            ViewKind::Text { text } => {
                let wrap = p.w.filter(|w| w.is_finite()).map(|w| w.max(1.0) as f32);
                let (w, h) = self.fonts.measure(text, FONT_SIZE, LINE_HEIGHT, wrap);
                let mut n = Node::new(&v.id, "text");
                n.text = Some(TextRun { text: text.clone(), size: FONT_SIZE, line_height: LINE_HEIGHT, color: fg });
                n.role = Role::Text;
                n.label = Some(text.clone());
                Lay { size: Size { w, h }, node: Some(n), kids: vec![] }
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
                Lay { size, node: Some(Node::new(&v.id, "zstack")), kids }
            }
            ViewKind::Spacer { min_length } => {
                let min = min_length.unwrap_or(0.0);
                let main = |prop: Option<f64>| prop.unwrap_or(0.0).max(min);
                let size = match env.axis {
                    Some(true) => Size { w: 0.0, h: main(p.h) },
                    Some(false) => Size { w: main(p.w), h: 0.0 },
                    None => Size { w: min, h: min },
                };
                Lay { size, node: Some(Node::new(&v.id, "spacer")), kids: vec![] }
            }
            ViewKind::Button { action, label } => {
                let label_env = Env { fg: Some(env.fg.unwrap_or_else(|| self.semantic(Semantic::Accent))), ..child_env };
                let l = self.view(label, p, label_env);
                let mut n = Node::new(&v.id, "button");
                n.role = Role::Button;
                n.action = Some(*action);
                n.label = first_text(label);
                Lay { size: l.size, node: Some(n), kids: vec![(0.0, 0.0, l)] }
            }
            ViewKind::Color { color } => {
                let mut n = Node::new(&v.id, "color");
                n.fill = Some(self.color(color));
                Lay { size: greedy(p), node: Some(n), kids: vec![] }
            }
            ViewKind::Rectangle => {
                let mut n = Node::new(&v.id, "rectangle");
                n.fill = Some(fg);
                Lay { size: greedy(p), node: Some(n), kids: vec![] }
            }
            ViewKind::Empty => Lay { size: Size::default(), node: Some(Node::new(&v.id, "empty")), kids: vec![] },
        }
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
                order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
                let mut remaining = m - total_gap;
                let mut left = children.len();
                for (i, _) in order {
                    let share = (remaining / left as f64).max(0.0);
                    let l = self.view(&children[i], mk(Some(share)), env);
                    remaining -= main_cross(l.size).0;
                    left -= 1;
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
        Lay { size, node: Some(Node::new(&v.id, kind)), kids }
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

    fn scene(json: &str, vp: (f64, f64)) -> Scene {
        let v: ViewNode = serde_json::from_str(json).unwrap();
        layout(&v, vp, Appearance::Light, &mut Fonts::new())
    }

    fn by_id<'a>(s: &'a Scene, id: &str) -> &'a Node {
        s.nodes.iter().find(|n| n.id == id).unwrap()
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
}
