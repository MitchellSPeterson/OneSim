//! SwiftUI scene animation & transition engine (docs/protocol.md section P).
//!
//! Transport-agnostic (shared by the native host, native window, and browser Wasm build).
//! Given each newly laid-out target [`Scene`], [`Animator`] diffs animatable visual properties,
//! tracks entering/exiting `.transition` subtrees, morphs `.matchedGeometryEffect` pairs across
//! trees, and cross-fades / rolls `.contentTransition` text and symbols. When at rest,
//! [`Animator::apply`] returns the target scene untouched.

use crate::effects::Composite;
use crate::paint::Gradient as PaintGradient;
use crate::scene::{Color, Gradient, Node, Rect, Scene, Shadow};
use onesim_protocol::{
    AnimationCurve, AnimationSpec, ContentTransitionKind, MatchedProps, RepeatCount, TransitionEdge,
    TransitionEffect, TransitionSpec, VisualOp,
};
use std::borrow::{Borrow, Cow};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Structural identity key for diffing nodes across trees: `(node.id, node.anchor)`.
/// Changing `.id(_:)` changes `anchor`, triggering removal + insertion transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeKey {
    id: String,
    anchor: Option<String>,
}

impl Hash for NodeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key_parts().hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct KeyRef<'a>(&'a str, Option<&'a str>);

trait KeyLike {
    fn key_parts(&self) -> (&str, Option<&str>);
}

impl KeyLike for NodeKey {
    fn key_parts(&self) -> (&str, Option<&str>) {
        (&self.id, self.anchor.as_deref())
    }
}

impl KeyLike for KeyRef<'_> {
    fn key_parts(&self) -> (&str, Option<&str>) {
        (self.0, self.1)
    }
}

impl<'a> Borrow<dyn KeyLike + 'a> for NodeKey {
    fn borrow(&self) -> &(dyn KeyLike + 'a) {
        self
    }
}

impl PartialEq for dyn KeyLike + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.key_parts() == other.key_parts()
    }
}

impl Eq for dyn KeyLike + '_ {}

impl Hash for dyn KeyLike + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key_parts().hash(state);
    }
}

fn node_key(n: &Node) -> NodeKey {
    NodeKey { id: n.id.clone(), anchor: n.anchor.clone() }
}

fn key_ref(n: &Node) -> KeyRef<'_> {
    KeyRef(&n.id, n.anchor.as_deref())
}

// ---------------------------------------------------------------------------
// Curve & spring evaluation
// ---------------------------------------------------------------------------

/// Evaluate `spec` at `elapsed` seconds with an optional normalized `init_vel` (units/s)
/// carried over from an interrupted animation. Returns `(progress, velocity, finished)`.
pub fn eval_curve(spec: &AnimationSpec, elapsed: f64, init_vel: f64) -> (f64, f64, bool) {
    let delay = spec.delay.max(0.0);
    if elapsed < delay {
        return (0.0, 0.0, false);
    }
    let speed = spec.speed.max(1e-6);
    let t = (elapsed - delay) * speed;
    let cycle_dur = cycle_duration(spec);
    if cycle_dur <= 1e-9 {
        return (1.0, 0.0, true);
    }

    match spec.repeat {
        None | Some(RepeatCount::Count(1)) => {
            let (p, v, done) = eval_single(&spec.curve, t, cycle_dur, init_vel);
            (p, v * speed, done)
        }
        Some(RepeatCount::Count(0)) => (1.0, 0.0, true),
        Some(RepeatCount::Count(n)) => {
            let total = cycle_dur * f64::from(n);
            if t + 1e-9 >= total {
                let end = if spec.autoreverses && n % 2 == 0 { 0.0 } else { 1.0 };
                return (end, 0.0, true);
            }
            let cycle = (t / cycle_dur).floor() as u32;
            let lt = (t - f64::from(cycle) * cycle_dur).clamp(0.0, cycle_dur);
            let rev = spec.autoreverses && cycle % 2 == 1;
            let (p, v, _) = eval_single(&spec.curve, lt, cycle_dur, if cycle == 0 { init_vel } else { 0.0 });
            if rev { (1.0 - p, -v * speed, false) } else { (p, v * speed, false) }
        }
        Some(RepeatCount::Forever(_)) => {
            let cycle = (t / cycle_dur).floor() as u64;
            let lt = (t - cycle as f64 * cycle_dur).clamp(0.0, cycle_dur);
            let rev = spec.autoreverses && cycle % 2 == 1;
            let (p, v, _) = eval_single(&spec.curve, lt, cycle_dur, if cycle == 0 { init_vel } else { 0.0 });
            if rev { (1.0 - p, -v * speed, false) } else { (p, v * speed, false) }
        }
    }
}

fn cycle_duration(spec: &AnimationSpec) -> f64 {
    match &spec.curve {
        AnimationCurve::Spring { settling, .. } => settling.max(0.01),
        _ => spec.duration.max(0.0),
    }
}

fn eval_single(curve: &AnimationCurve, t: f64, dur: f64, init_vel: f64) -> (f64, f64, bool) {
    match curve {
        AnimationCurve::Linear => {
            let done = t + 1e-9 >= dur;
            let u = if done { 1.0 } else { (t / dur).clamp(0.0, 1.0) };
            (u, if done { 0.0 } else { 1.0 / dur }, done)
        }
        AnimationCurve::EaseIn => eval_bezier([0.42, 0.0, 1.0, 1.0], t, dur),
        AnimationCurve::EaseOut => eval_bezier([0.0, 0.0, 0.58, 1.0], t, dur),
        AnimationCurve::EaseInOut => eval_bezier([0.42, 0.0, 0.58, 1.0], t, dur),
        AnimationCurve::Bezier(pts) => eval_bezier(*pts, t, dur),
        AnimationCurve::Spring { mass, stiffness, damping, initial_velocity, settling } => {
            eval_spring(*mass, *stiffness, *damping, *initial_velocity + init_vel, *settling, t)
        }
        AnimationCurve::Samples(pts) => {
            if pts.is_empty() {
                return (1.0, 0.0, true);
            }
            if t + 1e-9 >= dur {
                return (*pts.last().unwrap_or(&1.0), 0.0, true);
            }
            let u = (t / dur).clamp(0.0, 1.0);
            let last = (pts.len() - 1) as f64;
            if last <= 0.0 {
                return (pts[0], 0.0, false);
            }
            let pos = u * last;
            let idx = (pos.floor() as usize).min(pts.len() - 2);
            let frac = pos - idx as f64;
            let p = pts[idx] + (pts[idx + 1] - pts[idx]) * frac;
            let v = (pts[idx + 1] - pts[idx]) * last / dur;
            (p, v, false)
        }
    }
}

/// CoreAnimation cubic-bezier timing curve `(0,0) -> (x1,y1) -> (x2,y2) -> (1,1)`.
fn eval_bezier(c: [f64; 4], t: f64, dur: f64) -> (f64, f64, bool) {
    if t + 1e-9 >= dur {
        return (1.0, 0.0, true);
    }
    if t <= 0.0 {
        return (0.0, 0.0, false);
    }
    let u = t / dur;
    let (x1, y1, x2, y2) = (c[0], c[1], c[2], c[3]);
    let sample = |a: f64, b: f64, s: f64| -> f64 {
        let ax = 3.0 * a - 3.0 * b + 1.0;
        let bx = 3.0 * b - 6.0 * a;
        let cx = 3.0 * a;
        ((ax * s + bx) * s + cx) * s
    };
    let deriv = |a: f64, b: f64, s: f64| -> f64 {
        let ax = 3.0 * a - 3.0 * b + 1.0;
        let bx = 3.0 * b - 6.0 * a;
        let cx = 3.0 * a;
        (3.0 * ax * s + 2.0 * bx) * s + cx
    };
    // Newton-Raphson with bisection fallback.
    let mut s = u;
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..8 {
        let x = sample(x1, x2, s) - u;
        if x.abs() < 1e-6 {
            break;
        }
        if x < 0.0 { lo = s; } else { hi = s; }
        let d = deriv(x1, x2, s);
        if d.abs() > 1e-6 {
            let next = s - x / d;
            s = if next > lo && next < hi { next } else { 0.5 * (lo + hi) };
        } else {
            s = 0.5 * (lo + hi);
        }
    }
    let y = sample(y1, y2, s);
    let dx = deriv(x1, x2, s).max(1e-6);
    let dy = deriv(y1, y2, s);
    (y, (dy / dx) / dur, false)
}

/// Analytical damped harmonic oscillator from `0` to `1` (`x(0) = -1`, `x'(0) = v0`).
fn eval_spring(mass: f64, stiffness: f64, damping: f64, v0: f64, settling: f64, t: f64) -> (f64, f64, bool) {
    let m = mass.max(1e-4);
    let k = stiffness.max(1e-4);
    let c = damping.max(0.0);
    let max_t = (settling * 2.5).max(3.5);
    if t >= max_t {
        return (1.0, 0.0, true);
    }
    let w0 = (k / m).sqrt();
    let zeta = c / (2.0 * (k * m).sqrt());
    let (x, v) = if zeta < 1.0 - 1e-5 {
        // Underdamped (bouncy overshoot).
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        let decay = (-zeta * w0 * t).exp();
        let a = -1.0;
        let b = (v0 + zeta * w0 * a) / wd;
        let cos = (wd * t).cos();
        let sin = (wd * t).sin();
        let x = decay * (a * cos + b * sin);
        let v = decay * ((b * wd - zeta * w0 * a) * cos - (a * wd + zeta * w0 * b) * sin);
        (x, v)
    } else if zeta > 1.0 + 1e-5 {
        // Overdamped.
        let root = w0 * (zeta * zeta - 1.0).sqrt();
        let r1 = -zeta * w0 + root;
        let r2 = -zeta * w0 - root;
        let a = (v0 + r2) / (r1 - r2);
        let b = -1.0 - a;
        let e1 = (r1 * t).exp();
        let e2 = (r2 * t).exp();
        let x = a * e1 + b * e2;
        let v = a * r1 * e1 + b * r2 * e2;
        (x, v)
    } else {
        // Critically damped.
        let decay = (-w0 * t).exp();
        let a = -1.0;
        let b = v0 + w0 * a;
        let x = decay * (a + b * t);
        let v = decay * (b - w0 * (a + b * t));
        (x, v)
    };
    let done = t > 0.01 && x.abs() < 0.001 && (v / w0.max(1.0)).abs() < 0.002;
    if done { (1.0, 0.0, true) } else { (1.0 + x, v, false) }
}

// ---------------------------------------------------------------------------
// Interpolation helpers
// ---------------------------------------------------------------------------

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_f32(a: f32, b: f32, t: f64) -> f32 {
    a + (b - a) * (t as f32)
}

fn lerp_rect(a: Rect, b: Rect, t: f64) -> Rect {
    Rect { x: lerp(a.x, b.x, t), y: lerp(a.y, b.y, t), w: lerp(a.w, b.w, t).max(0.0), h: lerp(a.h, b.h, t).max(0.0) }
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    Color {
        r: lerp_f32(a.r, b.r, t).clamp(0.0, 1.0),
        g: lerp_f32(a.g, b.g, t).clamp(0.0, 1.0),
        b: lerp_f32(a.b, b.b, t).clamp(0.0, 1.0),
        a: lerp_f32(a.a, b.a, t).clamp(0.0, 1.0),
    }
}

fn rect_diff(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() > 0.05 || (a.y - b.y).abs() > 0.05 || (a.w - b.w).abs() > 0.05 || (a.h - b.h).abs() > 0.05
}

fn color_diff(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() > 0.002 || (a.g - b.g).abs() > 0.002 || (a.b - b.b).abs() > 0.002 || (a.a - b.a).abs() > 0.002
}

fn shadow_diff(a: Shadow, b: Shadow) -> bool {
    color_diff(a.color, b.color)
        || (a.radius - b.radius).abs() > 0.05
        || (a.dx - b.dx).abs() > 0.05
        || (a.dy - b.dy).abs() > 0.05
}

fn lerp_shadow(a: Shadow, b: Shadow, t: f64) -> Shadow {
    Shadow {
        color: lerp_color(a.color, b.color, t),
        radius: lerp(a.radius, b.radius, t).max(0.0),
        dx: lerp(a.dx, b.dx, t),
        dy: lerp(a.dy, b.dy, t),
    }
}

fn gradient_diff(a: &Gradient, b: &Gradient) -> bool {
    if a.stops.len() != b.stops.len() {
        return false;
    }
    (a.angle - b.angle).abs() > 0.05
        || a.stops.iter().zip(&b.stops).any(|(ca, cb)| color_diff(*ca, *cb))
}

fn lerp_gradient(a: &Gradient, b: &Gradient, t: f64) -> Option<Gradient> {
    if a.stops.len() != b.stops.len() {
        return None;
    }
    let stops = a.stops.iter().zip(&b.stops).map(|(ca, cb)| lerp_color(*ca, *cb, t)).collect();
    let points = match (a.points, b.points) {
        (Some(pa), Some(pb)) => Some(std::array::from_fn(|i| lerp_f32(pa[i], pb[i], t))),
        _ => b.points,
    };
    let locations = match (&a.locations, &b.locations) {
        (Some(la), Some(lb)) if la.len() == lb.len() => Some(la.iter().zip(lb).map(|(x, y)| lerp_f32(*x, *y, t)).collect()),
        _ => b.locations.clone(),
    };
    Some(Gradient { angle: lerp_f32(a.angle, b.angle, t), stops, points, locations })
}

fn lerp_affine(a: [f64; 6], b: [f64; 6], t: f64) -> [f64; 6] {
    if a[1].abs() < 1e-6 && a[2].abs() < 1e-6 && b[1].abs() < 1e-6 && b[2].abs() < 1e-6 {
        return std::array::from_fn(|i| lerp(a[i], b[i], t));
    }
    let decomp = |m: [f64; 6]| -> (f64, f64, f64, f64, f64) {
        let sx = m[0].hypot(m[1]);
        let ang = m[1].atan2(m[0]);
        let sy = if sx > 1e-9 { (m[0] * m[3] - m[1] * m[2]) / sx } else { m[3] };
        (sx, sy, ang, m[4], m[5])
    };
    let (sxa, sya, aa, txa, tya) = decomp(a);
    let (sxb, syb, ab, txb, tyb) = decomp(b);
    let mut da = ab - aa;
    while da > std::f64::consts::PI { da -= std::f64::consts::TAU; }
    while da < -std::f64::consts::PI { da += std::f64::consts::TAU; }
    let sx = lerp(sxa, sxb, t);
    let sy = lerp(sya, syb, t);
    let ang = aa + da * t;
    let (s, c) = ang.sin_cos();
    [sx * c, sx * s, -sy * s, sy * c, lerp(txa, txb, t), lerp(tya, tyb, t)]
}

fn composite_diff(a: &Composite, b: &Composite) -> bool {
    let (Composite::Effect(oa, ca), Composite::Effect(ob, cb)) = (a, b) else { return false };
    let col_chg = matches!((ca, cb), (Some(x), Some(y)) if color_diff(*x, *y));
    match (oa, ob) {
        (VisualOp::Affine { matrix: ma, anchor: aa }, VisualOp::Affine { matrix: mb, anchor: ab }) => {
            (0..6).any(|i| (ma[i] - mb[i]).abs() > 1e-4)
                || (aa[0] - ab[0]).abs() > 1e-4
                || (aa[1] - ab[1]).abs() > 1e-4
        }
        (VisualOp::Blur { radius: ra, opaque: oa }, VisualOp::Blur { radius: rb, opaque: ob }) => {
            oa == ob && (ra - rb).abs() > 0.05
        }
        (VisualOp::Brightness { amount: x }, VisualOp::Brightness { amount: y })
        | (VisualOp::Contrast { amount: x }, VisualOp::Contrast { amount: y })
        | (VisualOp::Saturation { amount: x }, VisualOp::Saturation { amount: y })
        | (VisualOp::Grayscale { amount: x }, VisualOp::Grayscale { amount: y })
        | (VisualOp::Opacity { amount: x }, VisualOp::Opacity { amount: y })
        | (VisualOp::ColorInvert { amount: x }, VisualOp::ColorInvert { amount: y }) => (x - y).abs() > 1e-4,
        (VisualOp::HueRotation { radians: x }, VisualOp::HueRotation { radians: y }) => (x - y).abs() > 1e-4,
        (VisualOp::ColorMultiply { .. }, VisualOp::ColorMultiply { .. }) => col_chg,
        _ => false,
    }
}

fn lerp_composite(a: &Composite, b: &Composite, t: f64) -> Option<Composite> {
    let (Composite::Effect(oa, ca), Composite::Effect(ob, cb)) = (a, b) else { return None };
    let col = match (ca, cb) {
        (Some(x), Some(y)) => Some(lerp_color(*x, *y, t)),
        _ => *cb,
    };
    let op = match (oa, ob) {
        (VisualOp::Affine { matrix: ma, anchor: aa }, VisualOp::Affine { matrix: mb, anchor: ab }) => {
            VisualOp::Affine {
                matrix: lerp_affine(*ma, *mb, t),
                anchor: [lerp(aa[0], ab[0], t), lerp(aa[1], ab[1], t)],
            }
        }
        (VisualOp::Blur { radius: ra, opaque: oa }, VisualOp::Blur { radius: rb, opaque: ob }) if oa == ob => {
            VisualOp::Blur { radius: lerp(*ra, *rb, t).max(0.0), opaque: *oa }
        }
        (VisualOp::Brightness { amount: x }, VisualOp::Brightness { amount: y }) => {
            VisualOp::Brightness { amount: lerp(*x, *y, t) }
        }
        (VisualOp::Contrast { amount: x }, VisualOp::Contrast { amount: y }) => {
            VisualOp::Contrast { amount: lerp(*x, *y, t) }
        }
        (VisualOp::Saturation { amount: x }, VisualOp::Saturation { amount: y }) => {
            VisualOp::Saturation { amount: lerp(*x, *y, t) }
        }
        (VisualOp::Grayscale { amount: x }, VisualOp::Grayscale { amount: y }) => {
            VisualOp::Grayscale { amount: lerp(*x, *y, t) }
        }
        (VisualOp::Opacity { amount: x }, VisualOp::Opacity { amount: y }) => {
            VisualOp::Opacity { amount: lerp(*x, *y, t).clamp(0.0, 1.0) }
        }
        (VisualOp::ColorInvert { amount: x }, VisualOp::ColorInvert { amount: y }) => {
            VisualOp::ColorInvert { amount: lerp(*x, *y, t).clamp(0.0, 1.0) }
        }
        (VisualOp::HueRotation { radians: x }, VisualOp::HueRotation { radians: y }) => {
            VisualOp::HueRotation { radians: lerp(*x, *y, t) }
        }
        (VisualOp::ColorMultiply { color }, VisualOp::ColorMultiply { .. }) => {
            VisualOp::ColorMultiply { color: color.clone() }
        }
        _ => return None,
    };
    Some(Composite::Effect(op, col))
}

fn shadow_pair(a: Option<Shadow>, b: Option<Shadow>) -> Option<(Shadow, Shadow)> {
    match (a, b) {
        (Some(sa), Some(sb)) if shadow_diff(sa, sb) => Some((sa, sb)),
        (Some(sa), None) if sa.color.a > 0.002 => Some((sa, Shadow { color: Color { a: 0.0, ..sa.color }, ..sa })),
        (None, Some(sb)) if sb.color.a > 0.002 => Some((Shadow { color: Color { a: 0.0, ..sb.color }, ..sb }, sb)),
        _ => None,
    }
}

fn glass_tint_pair(a: Option<&crate::glass::GlassSpec>, b: Option<&crate::glass::GlassSpec>) -> Option<(Color, Color)> {
    let (Some(ga), Some(gb)) = (a, b) else { return None };
    if ga.variant != gb.variant {
        return None;
    }
    match (ga.tint, gb.tint) {
        (Some(ca), Some(cb)) if color_diff(ca, cb) => Some((ca, cb)),
        (Some(ca), None) if ca.a > 0.002 => Some((ca, Color { a: 0.0, ..ca })),
        (None, Some(cb)) if cb.a > 0.002 => Some((Color { a: 0.0, ..cb }, cb)),
        _ => None,
    }
}

/// Shift every node in `indices` by `(dx, dy)` (and update image dest rects).
fn shift_nodes(nodes: &mut [Node], indices: &[usize], dx: f64, dy: f64) {
    if dx.abs() < 1e-4 && dy.abs() < 1e-4 {
        return;
    }
    for &i in indices {
        let n = &mut nodes[i];
        n.bounds.x += dx;
        n.bounds.y += dy;
        if let Some(im) = n.image.as_mut() {
            im.dest.x += dx;
            im.dest.y += dy;
        }
    }
}

/// Scale every node in `indices` about `(ax, ay)` by `s`.
fn scale_nodes(nodes: &mut [Node], indices: &[usize], ax: f64, ay: f64, s: f64) {
    scale_nodes_xy(nodes, indices, ax, ay, s, s);
}

/// Scale about `(ax, ay)` by `sx` / `sy`; sizes that cannot stretch on one axis (text, strokes, radii) take the smaller.
fn scale_nodes_xy(nodes: &mut [Node], indices: &[usize], ax: f64, ay: f64, sx: f64, sy: f64) {
    if (sx - 1.0).abs() < 1e-4 && (sy - 1.0).abs() < 1e-4 {
        return;
    }
    let (sx, sy) = (sx.max(0.0), sy.max(0.0));
    let sf = sx.min(sy);
    for &i in indices {
        let n = &mut nodes[i];
        n.bounds.x = ax + (n.bounds.x - ax) * sx;
        n.bounds.y = ay + (n.bounds.y - ay) * sy;
        n.bounds.w *= sx;
        n.bounds.h *= sy;
        n.radius *= sf;
        if let Some((_, w)) = n.stroke.as_mut() {
            *w *= sf;
        }
        if let Some(r) = n.clip_radius.as_mut() {
            *r *= sf;
        }
        if let Some(t) = n.text.as_mut() {
            t.font.size *= sf as f32;
            for sp in &mut t.spans {
                sp.pt *= sf as f32;
            }
        }
        if let Some(sym) = n.symbol.as_mut() {
            sym.size.0 *= sf;
            sym.size.1 *= sf;
        }
        if let Some(im) = n.image.as_mut() {
            im.dest.x = ax + (im.dest.x - ax) * sx;
            im.dest.y = ay + (im.dest.y - ay) * sy;
            im.dest.w *= sx;
            im.dest.h *= sy;
        }
        if sf < 0.005 {
            n.opacity = 0.0;
        }
    }
}

fn mul_opacity(nodes: &mut [Node], indices: &[usize], factor: f32) {
    let f = factor.clamp(0.0, 1.0);
    if (f - 1.0).abs() < 1e-4 {
        return;
    }
    for &i in indices {
        nodes[i].opacity *= f;
    }
}

/// Collect `root` and every descendant of `root` in `nodes`.
fn subtree_indices(nodes: &[Node], root: usize) -> Vec<usize> {
    let mut set = vec![false; nodes.len()];
    set[root] = true;
    let mut out = vec![root];
    for i in (root + 1)..nodes.len() {
        if let Some(p) = nodes[i].parent {
            if set[p] {
                set[i] = true;
                out.push(i);
            }
        }
    }
    out
}

/// Resolve an edge move offset in points for a subtree (SwiftUI `.move(edge:)` shifts by the view's own extent).
fn edge_offset(edge: TransitionEdge, sub: Rect, container: Rect) -> (f64, f64) {
    let w = if sub.w > 1.0 { sub.w } else if container.w > 1.0 { container.w } else { 40.0 };
    let h = if sub.h > 1.0 { sub.h } else if container.h > 1.0 { container.h } else { 40.0 };
    match edge {
        TransitionEdge::Leading => (-w, 0.0),
        TransitionEdge::Trailing => (w, 0.0),
        TransitionEdge::Top => (0.0, -h),
        TransitionEdge::Bottom => (0.0, h),
    }
}

/// Apply `effect` at intensity `k` (`k = 0` is identity, `k = 1` is full active effect).
fn apply_transition_effect(nodes: &mut [Node], indices: &[usize], root_bounds: Rect, container: Rect, effect: &TransitionEffect, k: f64) {
    let k = k.clamp(0.0, 1.5);
    if k.abs() < 1e-4 {
        return;
    }
    if let Some(op) = effect.opacity {
        mul_opacity(nodes, indices, lerp(1.0, op, k) as f32);
    }
    if effect.scale.is_some() || effect.scale_x.is_some() || effect.scale_y.is_some() {
        let a = effect.anchor.unwrap_or([0.5, 0.5]);
        let ax = root_bounds.x + root_bounds.w * a[0];
        let ay = root_bounds.y + root_bounds.h * a[1];
        let s = effect.scale.unwrap_or(1.0);
        let sx = lerp(1.0, s * effect.scale_x.unwrap_or(1.0), k).max(0.0);
        let sy = lerp(1.0, s * effect.scale_y.unwrap_or(1.0), k).max(0.0);
        scale_nodes_xy(nodes, indices, ax, ay, sx, sy);
    }
    let mut dx = 0.0;
    let mut dy = 0.0;
    if let Some([ox, oy]) = effect.offset {
        dx += ox * k;
        dy += oy * k;
    }
    if let Some(edge) = effect.edge {
        let (ex, ey) = edge_offset(edge, root_bounds, container);
        dx += ex * k;
        dy += ey * k;
    }
    shift_nodes(nodes, indices, dx, dy);
}

// ---------------------------------------------------------------------------
// Active animation tracks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PropTrack {
    spec: AnimationSpec,
    elapsed: f64,
    init_vel: f64,
    bounds: Option<(Rect, Rect)>,
    opacity: Option<(f32, f32)>,
    fill: Option<(Color, Color)>,
    stroke: Option<((Color, f64), (Color, f64))>,
    vector_fill: Option<(Color, Color)>,
    vector_stroke: Option<((Color, f64), (Color, f64))>,
    radius: Option<(f64, f64)>,
    text_color: Option<(Color, Color)>,
    symbol_color: Option<(Color, Color)>,
    glyph_color: Option<(Color, Color)>,
    composite: Option<(Composite, Composite)>,
    fill_paint: Option<(Arc<PaintGradient>, Arc<PaintGradient>)>,
    gradient: Option<(Gradient, Gradient)>,
    shadow: Option<(Shadow, Shadow)>,
    glass_tint: Option<(Color, Color)>,
}

impl PropTrack {
    fn progress(&self) -> (f64, f64, bool) {
        eval_curve(&self.spec, self.elapsed, self.init_vel)
    }
    fn apply(&self, n: &mut Node) {
        let (p, _, _) = self.progress();
        if let Some((a, b)) = self.bounds {
            let nb = lerp_rect(a, b, p);
            if let Some(im) = n.image.as_mut() {
                im.dest.x += nb.x - n.bounds.x;
                im.dest.y += nb.y - n.bounds.y;
            }
            n.bounds = nb;
        }
        if let Some((a, b)) = self.opacity {
            n.opacity = lerp_f32(a, b, p).clamp(0.0, 1.0);
        }
        if let Some((a, b)) = self.fill {
            n.fill = Some(lerp_color(a, b, p));
        }
        if let Some(((ca, wa), (cb, wb))) = self.stroke {
            n.stroke = Some((lerp_color(ca, cb, p), lerp(wa, wb, p).max(0.0)));
        }
        if let Some((a, b)) = self.vector_fill {
            if let Some(vf) = n.vector.as_mut().and_then(|v| v.fill.as_mut()) {
                vf.color = lerp_color(a, b, p);
            }
        }
        if let Some(((ca, wa), (cb, wb))) = self.vector_stroke {
            if let Some(vs) = n.vector.as_mut().and_then(|v| v.stroke.as_mut()) {
                vs.color = lerp_color(ca, cb, p);
                vs.line_width = lerp(wa, wb, p).max(0.0);
            }
        }
        if let Some((a, b)) = self.radius {
            n.radius = lerp(a, b, p).max(0.0);
        }
        if let Some((a, b)) = self.text_color {
            if let Some(t) = n.text.as_mut() {
                t.color = lerp_color(a, b, p);
            }
        }
        if let Some((a, b)) = self.symbol_color {
            if let Some(s) = n.symbol.as_mut() {
                s.color = lerp_color(a, b, p);
            }
        }
        if let Some((a, b)) = self.glyph_color {
            if let Some((_, c)) = n.glyph.as_mut() {
                *c = lerp_color(a, b, p);
            }
        }
        if let Some((a, b)) = &self.composite {
            if let Some(c) = lerp_composite(a, b, p) {
                n.composite = Some(Box::new(c));
            }
        }
        if let Some((a, b)) = &self.fill_paint {
            if let Some(g) = a.lerp(b, p) {
                n.fill_paint = Some(Arc::new(g));
            }
        }
        if let Some((a, b)) = &self.gradient {
            if let Some(g) = lerp_gradient(a, b, p) {
                n.gradient = Some(g);
            }
        }
        if let Some((a, b)) = self.shadow {
            n.shadow = Some(lerp_shadow(a, b, p));
        }
        if let Some((a, b)) = self.glass_tint {
            if let Some(g) = n.glass.as_mut() {
                g.tint = Some(lerp_color(a, b, p));
            }
        }
    }
}

/// Project an in-flight track's normalized velocity onto a retargeted track's delta so physical
/// velocity is continuous in both magnitude and sign (e.g. reversing mid-flight yields negative
/// normalized velocity that smoothly turns around).
fn retarget_init_vel(
    old: &PropTrack,
    new_bounds: Option<(Rect, Rect)>,
    new_opacity: Option<(f32, f32)>,
    new_radius: Option<(f64, f64)>,
) -> f64 {
    let old_v = old.progress().1;
    if old_v.abs() < 1e-6 {
        return 0.0;
    }
    if let (Some((oa, ob)), Some((na, nb))) = (old.bounds, new_bounds) {
        let od = [ob.x - oa.x, ob.y - oa.y, ob.w - oa.w, ob.h - oa.h];
        let nd = [nb.x - na.x, nb.y - na.y, nb.w - na.w, nb.h - na.h];
        let nd_sq: f64 = nd.iter().map(|v| v * v).sum();
        if nd_sq > 1e-6 {
            let dot: f64 = od.iter().zip(nd).map(|(o, n)| o * n).sum();
            return (old_v * dot / nd_sq).clamp(-12.0, 12.0);
        }
    }
    if let (Some((oa, ob)), Some((na, nb))) = (old.opacity, new_opacity) {
        let nd = f64::from(nb - na);
        if nd.abs() > 1e-6 {
            return (old_v * f64::from(ob - oa) / nd).clamp(-12.0, 12.0);
        }
    }
    if let (Some((oa, ob)), Some((na, nb))) = (old.radius, new_radius) {
        let nd = nb - na;
        if nd.abs() > 1e-6 {
            return (old_v * (ob - oa) / nd).clamp(-12.0, 12.0);
        }
    }
    old_v.clamp(-8.0, 8.0)
}

#[derive(Debug, Clone)]
struct InsertTrack {
    spec: AnimationSpec,
    elapsed: f64,
    effect: TransitionEffect,
    container: Rect,
}

/// Cross-frame `matchedGeometryEffect` entering morph: shifts/resizes subtree from `from` to `to`.
#[derive(Debug, Clone)]
struct MatchInsertTrack {
    spec: AnimationSpec,
    elapsed: f64,
    from: Rect,
    from_opacity: f32,
    props: MatchedProps,
    anchor: [f64; 2],
}

#[derive(Debug, Clone)]
struct RemovalGhost {
    spec: AnimationSpec,
    elapsed: f64,
    effect: TransitionEffect,
    /// Optional matchedGeometry target rect to morph towards while fading out.
    morph_to: Option<(Rect, MatchedProps, [f64; 2])>,
    container: Rect,
    /// Self-contained subtree copy: node 0 is the ghost root; parent indices are relative to this vec.
    nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
struct ContentGhost {
    key: NodeKey,
    spec: AnimationSpec,
    elapsed: f64,
    kind: ContentTransitionKind,
    /// Direction for `.numericText`: `+1.0` rolls upward (value increased), `-1.0` rolls downward.
    dir: f64,
    old_node: Node,
}

/// Per-session animation state machine.
#[derive(Debug, Default)]
pub struct Animator {
    /// Last displayed scene (after applying any in-flight tracks), used as the starting snapshot
    /// for newly triggered animations and fluid retargeting.
    displayed: Option<Arc<Scene>>,
    /// Last target scene (before interpolation), so unrelated tree updates preserve in-flight tracks.
    last_target: Option<Arc<Scene>>,
    /// Last seen animation token per node key so unchanged tokens don't re-trigger.
    tokens: HashMap<NodeKey, u64>,
    props: HashMap<NodeKey, PropTrack>,
    inserts: HashMap<NodeKey, InsertTrack>,
    match_inserts: HashMap<NodeKey, MatchInsertTrack>,
    ghosts: Vec<RemovalGhost>,
    content_ghosts: Vec<ContentGhost>,
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_animating(&self) -> bool {
        !self.props.is_empty()
            || !self.inserts.is_empty()
            || !self.match_inserts.is_empty()
            || !self.ghosts.is_empty()
            || !self.content_ghosts.is_empty()
    }

    /// Ingest a new target `Scene` laid out from a new tree. Diffs against the previously
    /// displayed scene and starts/retargets any animations, transitions, matchedGeometryEffect
    /// morphs, and contentTransitions.
    pub fn update(&mut self, target: &Scene) {
        let Some(prev) = self.displayed.take() else {
            self.record_tokens(target);
            let arc = Arc::new(target.clone());
            self.last_target = Some(Arc::clone(&arc));
            self.displayed = Some(arc);
            return;
        };
        let prev_target = self.last_target.as_deref();
        let mut prev_target_map: HashMap<KeyRef<'_>, usize> = HashMap::new();
        if let Some(pt) = prev_target {
            prev_target_map.reserve(pt.nodes.len());
            for (i, n) in pt.nodes.iter().enumerate() {
                prev_target_map.entry(key_ref(n)).or_insert(i);
            }
        }

        let root_anim = find_root_animation(target);

        let mut prev_map: HashMap<KeyRef<'_>, usize> = HashMap::with_capacity(prev.nodes.len());
        for (i, n) in prev.nodes.iter().enumerate() {
            prev_map.entry(key_ref(n)).or_insert(i);
        }
        let mut target_map: HashMap<KeyRef<'_>, usize> = HashMap::with_capacity(target.nodes.len());
        for (i, n) in target.nodes.iter().enumerate() {
            target_map.entry(key_ref(n)).or_insert(i);
        }

        // Drop stale insert / match_insert tracks for nodes that disappeared or are being retargeted.
        self.inserts.retain(|k, _| target_map.contains_key(&KeyRef(&k.id, k.anchor.as_deref())));
        self.match_inserts.retain(|k, _| target_map.contains_key(&KeyRef(&k.id, k.anchor.as_deref())));

        // 1. Cross-frame matchedGeometryEffect pairing (disappeared old node <-> appeared new node with same (ns, id)).
        let mut matched_handled_prev: HashSet<usize> = HashSet::new();
        let mut matched_handled_new: HashSet<usize> = HashSet::new();

        let mut prev_matched: HashMap<(&str, &str), usize> = HashMap::new();
        for (i, n) in prev.nodes.iter().enumerate() {
            if let Some(m) = n.anim.as_ref().and_then(|a| a.matched.as_ref()) {
                if !target_map.contains_key(&key_ref(n)) {
                    prev_matched.entry((m.namespace.as_str(), m.id.as_str())).or_insert(i);
                }
            }
        }
        for (i, n) in target.nodes.iter().enumerate() {
            let Some(m) = n.anim.as_ref().and_then(|a| a.matched.as_ref()) else { continue };
            let kref = key_ref(n);
            if prev_map.contains_key(&kref) {
                continue;
            }
            let Some(&pi) = prev_matched.get(&(m.namespace.as_str(), m.id.as_str())) else { continue };
            let anim = n.anim.as_ref().and_then(|a| a.animation.clone()).or_else(|| root_anim.clone());
            let Some(spec) = anim else { continue };
            let old_rect = prev.nodes[pi].bounds;
            let new_rect = n.bounds;
            self.match_inserts.insert(
                node_key(n),
                MatchInsertTrack {
                    spec: spec.clone(),
                    elapsed: 0.0,
                    from: old_rect,
                    from_opacity: prev.nodes[pi].opacity,
                    props: m.properties,
                    anchor: m.anchor,
                },
            );
            let (old_sub, prev_indices) = extract_ghost_subtree(&prev.nodes, pi);
            self.ghosts.push(RemovalGhost {
                spec,
                elapsed: 0.0,
                effect: TransitionEffect { opacity: Some(0.0), ..TransitionEffect::default() },
                morph_to: Some((new_rect, m.properties, m.anchor)),
                container: parent_bounds(&prev, pi),
                nodes: old_sub,
            });
            matched_handled_prev.extend(prev_indices);
            matched_handled_new.extend(subtree_indices(&target.nodes, i));
        }

        // 2. Property animations & contentTransitions for nodes present in both scenes.
        let mut next_props = HashMap::new();
        for n in &target.nodes {
            if n.kind == "screen" || is_chrome(n) {
                continue;
            }
            let kref = key_ref(n);
            let Some(&pi) = prev_map.get(&kref) else { continue };
            let pn = &prev.nodes[pi];
            let anim = n.anim.as_ref().and_then(|a| a.animation.as_ref());
            let ct = n.anim.as_ref().map_or(ContentTransitionKind::Identity, |a| a.content_transition);

            // Keep any already-running track if the target hasn't changed and token didn't bump.
            let prev_tok = self.tokens.get(&kref as &dyn KeyLike).copied().unwrap_or(0);
            let should_animate = match anim {
                Some(spec) => spec.token == 0 || spec.token != prev_tok,
                None => false,
            };

            let bounds_chg = rect_diff(pn.bounds, n.bounds);
            let op_chg = (pn.opacity - n.opacity).abs() > 0.002;
            let fill_chg = match (pn.fill, n.fill) {
                (Some(a), Some(b)) => color_diff(a, b),
                _ => false,
            };
            let stroke_chg = match (pn.stroke, n.stroke) {
                (Some((ca, wa)), Some((cb, wb))) => color_diff(ca, cb) || (wa - wb).abs() > 0.05,
                _ => false,
            };
            let vfill_pair = match (pn.vector.as_ref().and_then(|v| v.fill), n.vector.as_ref().and_then(|v| v.fill)) {
                (Some(a), Some(b)) if color_diff(a.color, b.color) => Some((a.color, b.color)),
                _ => None,
            };
            let vstroke_pair = match (pn.vector.as_ref().and_then(|v| v.stroke.as_ref()), n.vector.as_ref().and_then(|v| v.stroke.as_ref())) {
                (Some(a), Some(b)) if color_diff(a.color, b.color) || (a.line_width - b.line_width).abs() > 0.05 => {
                    Some(((a.color, a.line_width), (b.color, b.line_width)))
                }
                _ => None,
            };
            let rad_chg = (pn.radius - n.radius).abs() > 0.05 && pn.radius < 1e4 && n.radius < 1e4;
            let text_col_chg = match (&pn.text, &n.text) {
                (Some(a), Some(b)) => color_diff(a.color, b.color),
                _ => false,
            };
            let sym_col_chg = match (&pn.symbol, &n.symbol) {
                (Some(a), Some(b)) => color_diff(a.color, b.color),
                _ => false,
            };
            let glyph_col_chg = match (pn.glyph, n.glyph) {
                (Some((_, a)), Some((_, b))) => color_diff(a, b),
                _ => false,
            };
            let comp_pair = match (&pn.composite, &n.composite) {
                (Some(a), Some(b)) if composite_diff(a, b) => Some(((**a).clone(), (**b).clone())),
                _ => None,
            };
            let fp_pair = match (&pn.fill_paint, &n.fill_paint) {
                (Some(a), Some(b)) if a.differs(b) => Some((Arc::clone(a), Arc::clone(b))),
                _ => None,
            };
            let grad_pair = match (&pn.gradient, &n.gradient) {
                (Some(a), Some(b)) if gradient_diff(a, b) => Some((a.clone(), b.clone())),
                _ => None,
            };
            let sh_pair = shadow_pair(pn.shadow, n.shadow);
            let gt_pair = glass_tint_pair(pn.glass.as_ref(), n.glass.as_ref());
            let any_prop_chg = bounds_chg
                || op_chg
                || fill_chg
                || stroke_chg
                || vfill_pair.is_some()
                || vstroke_pair.is_some()
                || rad_chg
                || text_col_chg
                || sym_col_chg
                || glyph_col_chg
                || comp_pair.is_some()
                || fp_pair.is_some()
                || grad_pair.is_some()
                || sh_pair.is_some()
                || gt_pair.is_some();

            if should_animate && any_prop_chg {
                // `prev` already holds the displayed frame; let the new PropTrack take over smoothly.
                self.match_inserts.remove(&kref as &dyn KeyLike);
                let spec = anim.unwrap().clone();
                // Preserve physical velocity when retargeting an in-flight track (projecting onto the new delta
                // so reversing mid-flight decelerates smoothly instead of inverting velocity instantaneously).
                let new_bounds = bounds_chg.then_some((pn.bounds, n.bounds));
                let new_opacity = op_chg.then_some((pn.opacity, n.opacity));
                let new_radius = rad_chg.then_some((pn.radius, n.radius));
                let init_vel = self
                    .props
                    .get(&kref as &dyn KeyLike)
                    .map_or(0.0, |old| retarget_init_vel(old, new_bounds, new_opacity, new_radius));
                next_props.insert(
                    node_key(n),
                    PropTrack {
                        spec,
                        elapsed: 0.0,
                        init_vel,
                        bounds: new_bounds,
                        opacity: new_opacity,
                        fill: fill_chg.then(|| (pn.fill.unwrap(), n.fill.unwrap())),
                        stroke: stroke_chg.then(|| (pn.stroke.unwrap(), n.stroke.unwrap())),
                        vector_fill: vfill_pair,
                        vector_stroke: vstroke_pair,
                        radius: new_radius,
                        text_color: text_col_chg.then(|| (pn.text.as_ref().unwrap().color, n.text.as_ref().unwrap().color)),
                        symbol_color: sym_col_chg.then(|| (pn.symbol.as_ref().unwrap().color, n.symbol.as_ref().unwrap().color)),
                        glyph_color: glyph_col_chg.then(|| (pn.glyph.unwrap().1, n.glyph.unwrap().1)),
                        composite: comp_pair,
                        fill_paint: fp_pair,
                        gradient: grad_pair,
                        shadow: sh_pair,
                        glass_tint: gt_pair,
                    },
                );
            } else if let Some((existing_key, mut existing)) = self.props.remove_entry(&kref as &dyn KeyLike) {
                // Keep running existing track when the target itself did not snap to a new value
                // (e.g. an unrelated TimelineView tick arrived while this track is in flight).
                let target_unchanged = prev_target
                    .and_then(|pt| prev_target_map.get(&kref).map(|&ti| &pt.nodes[ti]))
                    .is_some_and(|old_t| !props_differ(old_t, n));
                if target_unchanged {
                    existing.bounds = existing.bounds.map(|(a, _)| (a, n.bounds));
                    next_props.insert(existing_key, existing);
                }
            }

            // Content transition for text / symbol content changes.
            if should_animate && ct != ContentTransitionKind::Identity {
                let text_changed = match (&pn.text, &n.text) {
                    (Some(a), Some(b)) => a.text != b.text,
                    _ => false,
                };
                let sym_changed = match (&pn.symbol, &n.symbol) {
                    (Some(a), Some(b)) => a.spec.name != b.spec.name,
                    _ => false,
                };
                if text_changed || sym_changed {
                    let dir = numeric_direction(ct, pn.text.as_ref().map(|t| t.text.as_str()), n.text.as_ref().map(|t| t.text.as_str()));
                    let mut ghost_node = pn.clone();
                    ghost_node.strip_interactive();
                    let key = node_key(n);
                    self.content_ghosts.retain(|g| g.key != key);
                    self.content_ghosts.push(ContentGhost {
                        key,
                        spec: anim.unwrap().clone(),
                        elapsed: 0.0,
                        kind: ct,
                        dir,
                        old_node: ghost_node,
                    });
                }
            }
        }
        self.props = next_props;

        // 3. Insertion transitions for newly appeared nodes.
        let mut insert_covered: HashSet<usize> = matched_handled_new;
        for (i, n) in target.nodes.iter().enumerate() {
            if insert_covered.contains(&i) || n.kind == "screen" || is_chrome(n) {
                continue;
            }
            if prev_map.contains_key(&key_ref(n)) {
                continue;
            }
            let explicit = n.anim.as_ref().and_then(|a| a.transition.as_ref());
            let anim = explicit
                .and_then(|t| t.animation.clone())
                .or_else(|| n.anim.as_ref().and_then(|a| a.animation.clone()));
            let Some(spec) = anim else { continue };
            let effect = if let Some(t) = explicit {
                t.insertion.clone()
            } else {
                // Default SwiftUI transition inside `withAnimation` / `.animation(value:)`: opacity fade.
                if spec.token == 0 {
                    continue;
                }
                // Only apply default transition at the root of a newly inserted branch (parent existed).
                let parent_existed = n.parent.is_some_and(|p| prev_map.contains_key(&key_ref(&target.nodes[p])));
                if !parent_existed {
                    continue;
                }
                TransitionEffect { opacity: Some(0.0), ..TransitionEffect::default() }
            };
            if is_identity_effect(&effect) {
                insert_covered.extend(subtree_indices(&target.nodes, i));
                continue;
            }
            self.inserts.insert(
                node_key(n),
                InsertTrack { spec, elapsed: 0.0, effect, container: parent_bounds(target, i) },
            );
            insert_covered.extend(subtree_indices(&target.nodes, i));
        }

        // 4. Removal transitions for disappeared nodes.
        // React Native LayoutAnimation: the commit's delete effect and timing (none configured: removals are immediate).
        let layout_delete = target.nodes.iter().find_map(|n| n.anim.as_ref().filter(|a| a.layout)).map(|a| TransitionSpec {
            removal: a.transition.as_ref().map(|t| t.removal.clone()).unwrap_or_default(),
            animation: a.removal.clone(),
            ..TransitionSpec::default()
        });
        let layout_delete = layout_delete.as_ref();
        let mut remove_covered: HashSet<usize> = matched_handled_prev;
        for (i, n) in prev.nodes.iter().enumerate() {
            if remove_covered.contains(&i) || n.kind == "screen" || is_chrome(n) {
                continue;
            }
            if target_map.contains_key(&key_ref(n)) {
                continue;
            }
            // A React Native view leaves with the delete effect of the commit that removed it (if it animates), never
            // with an earlier commit's.
            let explicit = match layout_delete {
                Some(d) => Some(d),
                None if n.anim.as_ref().is_some_and(|a| a.layout) => continue,
                None => n.anim.as_ref().and_then(|a| a.transition.as_ref()),
            };
            // Check if the new tree has an active animation on the parent or root (or on the transition itself).
            let parent_anim = n
                .parent
                .and_then(|p| target_map.get(&key_ref(&prev.nodes[p])))
                .and_then(|&ti| target.nodes[ti].anim.as_ref().filter(|a| !a.layout).and_then(|a| a.animation.clone()));
            let anim = explicit
                .and_then(|t| t.animation.clone())
                .or(parent_anim)
                .or_else(|| root_anim.clone());
            let Some(spec) = anim else { continue };
            let effect = if let Some(t) = explicit {
                t.removal.clone()
            } else {
                if spec.token == 0 {
                    continue;
                }
                let parent_exists = n.parent.is_some_and(|p| target_map.contains_key(&key_ref(&prev.nodes[p])));
                if !parent_exists {
                    continue;
                }
                TransitionEffect { opacity: Some(0.0), ..TransitionEffect::default() }
            };
            if is_identity_effect(&effect) {
                remove_covered.extend(subtree_indices(&prev.nodes, i));
                continue;
            }
            let (sub, prev_indices) = extract_ghost_subtree(&prev.nodes, i);
            self.ghosts.push(RemovalGhost {
                spec,
                elapsed: 0.0,
                effect,
                morph_to: None,
                container: parent_bounds(&prev, i),
                nodes: sub,
            });
            remove_covered.extend(prev_indices);
        }

        self.record_tokens(target);
        let target_arc = Arc::new(target.clone());
        self.displayed = Some(if self.is_animating() {
            Arc::new(self.apply(target))
        } else {
            Arc::clone(&target_arc)
        });
        self.last_target = Some(target_arc);
    }

    fn record_tokens(&mut self, scene: &Scene) {
        self.tokens.clear();
        for n in &scene.nodes {
            if let Some(tok) = n.anim.as_ref().and_then(|a| a.animation.as_ref()).map(|a| a.token) {
                if tok > 0 {
                    self.tokens.insert(node_key(n), tok);
                }
            }
        }
    }

    /// Advance all active animations by `dt` seconds. Returns `true` while any animation remains active.
    pub fn tick(&mut self, dt: f64) -> bool {
        let dt = dt.max(0.0);
        self.props.retain(|_, tr| {
            tr.elapsed += dt;
            !tr.progress().2
        });
        self.inserts.retain(|_, tr| {
            tr.elapsed += dt;
            !eval_curve(&tr.spec, tr.elapsed, 0.0).2
        });
        self.match_inserts.retain(|_, tr| {
            tr.elapsed += dt;
            !eval_curve(&tr.spec, tr.elapsed, 0.0).2
        });
        self.ghosts.retain_mut(|g| {
            g.elapsed += dt;
            !eval_curve(&g.spec, g.elapsed, 0.0).2
        });
        self.content_ghosts.retain_mut(|g| {
            g.elapsed += dt;
            !eval_curve(&g.spec, g.elapsed, 0.0).2
        });
        self.is_animating()
    }

    /// Snapshot the current interpolated frame for `target` and remember it as `displayed`.
    pub fn render_scene(&mut self, target: &Scene) -> Scene {
        if !self.is_animating() {
            if let Some(lt) = &self.last_target {
                self.displayed = Some(Arc::clone(lt));
            } else {
                self.displayed = Some(Arc::new(target.clone()));
            }
            return target.clone();
        }
        let out = self.apply(target);
        self.displayed = Some(Arc::new(out.clone()));
        out
    }

    /// Apply all active tracks, transitions, matchedGeometry morphs, and ghosts to `target`,
    /// returning a borrowed reference when idle.
    pub fn apply_cow<'a>(&self, target: &'a Scene) -> Cow<'a, Scene> {
        if !self.is_animating() {
            Cow::Borrowed(target)
        } else {
            Cow::Owned(self.apply(target))
        }
    }

    /// Apply all active tracks, transitions, matchedGeometry morphs, and ghosts to `target`.
    pub fn apply(&self, target: &Scene) -> Scene {
        if !self.is_animating() {
            return target.clone();
        }
        let mut out = target.clone();

        // 1. Property interpolation tracks.
        if !self.props.is_empty() {
            for n in &mut out.nodes {
                if let Some(tr) = self.props.get(&key_ref(n) as &dyn KeyLike) {
                    tr.apply(n);
                }
            }
        }

        // 2. Insertion transitions.
        if !self.inserts.is_empty() {
            let roots: Vec<(usize, InsertTrack)> = out
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| self.inserts.get(&key_ref(n) as &dyn KeyLike).cloned().map(|tr| (i, tr)))
                .collect();
            for (i, tr) in roots {
                let (p, _, _) = eval_curve(&tr.spec, tr.elapsed, 0.0);
                let remaining = 1.0 - p;
                let sub = subtree_indices(&out.nodes, i);
                let rb = out.nodes[i].bounds;
                apply_transition_effect(&mut out.nodes, &sub, rb, tr.container, &tr.effect, remaining);
            }
        }

        // 3. Cross-frame matchedGeometryEffect entering morphs.
        if !self.match_inserts.is_empty() {
            let roots: Vec<(usize, MatchInsertTrack)> = out
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| self.match_inserts.get(&key_ref(n) as &dyn KeyLike).cloned().map(|tr| (i, tr)))
                .collect();
            for (i, tr) in roots {
                let (p, _, _) = eval_curve(&tr.spec, tr.elapsed, 0.0);
                let sub = subtree_indices(&out.nodes, i);
                let live_to = out.nodes[i].bounds;
                apply_matched_morph(&mut out.nodes, &sub, tr.from, live_to, tr.props, tr.anchor, p);
                // Solve src-over compositing so `a_top + a_bot * (1 - a_top) == lerp(a_old, a_new, p)`:
                // with outgoing ghost on top at `a_top = from_opacity * (1 - p)`, scaling the entering
                // view by `p / (1 - a_top)` keeps opaque views at 1.0 (no 75% dip) and translucent
                // views at their exact interpolated alpha (no initial jump at p = 0).
                let p_c = p.clamp(0.0, 1.0);
                let denom = 1.0 - (1.0 - p_c) * f64::from(tr.from_opacity.clamp(0.0, 1.0));
                let enter_mul = if denom <= 1e-6 { 1.0 } else { (p_c / denom).clamp(0.0, 1.0) };
                mul_opacity(&mut out.nodes, &sub, enter_mul as f32);
            }
        }

        // 4. Content transitions (fade/roll incoming node + insert outgoing ghost).
        let mut extra_ghosts: Vec<Node> = Vec::new();
        for cg in &self.content_ghosts {
            let (p, _, _) = eval_curve(&cg.spec, cg.elapsed, 0.0);
            let p_clamped = p.clamp(0.0, 1.0);
            let Some(idx) = out.nodes.iter().position(|n| key_ref(n).key_parts() == cg.key.key_parts()) else { continue };
            let h = out.nodes[idx].bounds.h.max(16.0);
            let mut ghost = cg.old_node.clone();
            ghost.parent = out.nodes[idx].parent;
            match cg.kind {
                ContentTransitionKind::Opacity | ContentTransitionKind::Interpolate => {
                    out.nodes[idx].opacity *= p_clamped as f32;
                    ghost.opacity *= (1.0 - p_clamped) as f32;
                }
                ContentTransitionKind::SymbolEffect => {
                    let b_new = out.nodes[idx].bounds;
                    let s_new = lerp(0.8, 1.0, p_clamped);
                    scale_nodes(&mut out.nodes, &[idx], b_new.x + b_new.w * 0.5, b_new.y + b_new.h * 0.5, s_new);
                    out.nodes[idx].opacity *= p_clamped as f32;

                    let b_old = ghost.bounds;
                    let s_old = lerp(1.0, 0.8, p_clamped);
                    let mut tmp = vec![ghost];
                    scale_nodes(&mut tmp, &[0], b_old.x + b_old.w * 0.5, b_old.y + b_old.h * 0.5, s_old);
                    tmp[0].opacity *= (1.0 - p_clamped) as f32;
                    ghost = tmp.pop().unwrap();
                }
                ContentTransitionKind::NumericTextUp | ContentTransitionKind::NumericTextDown => {
                    let travel = h * 0.55;
                    out.nodes[idx].bounds.y += cg.dir * travel * (1.0 - p);
                    out.nodes[idx].opacity *= p_clamped as f32;
                    ghost.bounds.y -= cg.dir * travel * p;
                    ghost.opacity *= (1.0 - p_clamped) as f32;
                }
                ContentTransitionKind::Identity => {}
            }
            extra_ghosts.push(ghost);
        }

        // 5. Removal ghosts (inserted before chrome so status bar & island stay on top).
        let chrome_pos = out.nodes.iter().position(is_chrome).unwrap_or(out.nodes.len());
        let mut ghost_nodes: Vec<Node> = Vec::new();
        for g in &self.ghosts {
            let (p, _, _) = eval_curve(&g.spec, g.elapsed, 0.0);
            let mut sub = g.nodes.clone();
            let all: Vec<usize> = (0..sub.len()).collect();
            let rb = sub[0].bounds;
            if let Some((to, props, anchor)) = g.morph_to {
                apply_matched_morph(&mut sub, &all, rb, to, props, anchor, p);
                mul_opacity(&mut sub, &all, (1.0 - p).clamp(0.0, 1.0) as f32);
            } else {
                apply_transition_effect(&mut sub, &all, rb, g.container, &g.effect, p);
            }
            let base = chrome_pos + ghost_nodes.len();
            for (k, mut gn) in sub.into_iter().enumerate() {
                gn.parent = if k == 0 { Some(0) } else { gn.parent.map(|pi| base + pi) };
                ghost_nodes.push(gn);
            }
        }
        for mut gn in extra_ghosts {
            gn.parent = Some(0);
            ghost_nodes.push(gn);
        }

        if !ghost_nodes.is_empty() {
            let added = ghost_nodes.len();
            // Shift parent indices of chrome nodes that sit at or after `chrome_pos`.
            for n in &mut out.nodes[chrome_pos..] {
                if let Some(p) = n.parent {
                    if p >= chrome_pos {
                        n.parent = Some(p + added);
                    }
                }
            }
            out.nodes.splice(chrome_pos..chrome_pos, ghost_nodes);
        }

        out
    }
}

/// Morph subtree `sub` (whose laid-out target root rect is `to`) so at `p = 0` it matches `from`
/// and at `p = 1` it matches `to`, respecting `props` and `anchor`.
fn apply_matched_morph(nodes: &mut [Node], sub: &[usize], from: Rect, to: Rect, props: MatchedProps, anchor: [f64; 2], p: f64) {
    let cur_root = nodes[sub[0]].bounds;
    let w = match props {
        MatchedProps::Frame | MatchedProps::Size => lerp(from.w, to.w, p).max(0.0),
        MatchedProps::Position => cur_root.w,
    };
    let h = match props {
        MatchedProps::Frame | MatchedProps::Size => lerp(from.h, to.h, p).max(0.0),
        MatchedProps::Position => cur_root.h,
    };
    let (x, y) = match props {
        MatchedProps::Frame | MatchedProps::Position => {
            let fax = from.x + from.w * anchor[0];
            let fay = from.y + from.h * anchor[1];
            let tax = to.x + to.w * anchor[0];
            let tay = to.y + to.h * anchor[1];
            (lerp(fax, tax, p) - w * anchor[0], lerp(fay, tay, p) - h * anchor[1])
        }
        MatchedProps::Size => {
            let tax = to.x + to.w * anchor[0];
            let tay = to.y + to.h * anchor[1];
            (tax - w * anchor[0], tay - h * anchor[1])
        }
    };
    let dx = x - cur_root.x;
    let dy = y - cur_root.y;
    let dw = w - cur_root.w;
    let dh = h - cur_root.h;
    for &i in sub {
        let n = &mut nodes[i];
        let fills_root = (n.bounds.w - cur_root.w).abs() < 0.5 && (n.bounds.h - cur_root.h).abs() < 0.5;
        if i == sub[0] || fills_root {
            n.bounds.x += dx;
            n.bounds.y += dy;
            n.bounds.w = (n.bounds.w + dw).max(0.0);
            n.bounds.h = (n.bounds.h + dh).max(0.0);
        } else {
            n.bounds.x += dx + dw * anchor[0];
            n.bounds.y += dy + dh * anchor[1];
        }
        if let Some(im) = n.image.as_mut() {
            im.dest = n.bounds;
        }
    }
}

fn find_root_animation(scene: &Scene) -> Option<AnimationSpec> {
    scene.nodes.iter().skip(1).find_map(|n| n.anim.as_ref().filter(|a| !a.layout)?.animation.clone())
}

fn parent_bounds(scene: &Scene, idx: usize) -> Rect {
    scene.nodes[idx]
        .parent
        .map_or(Rect { x: 0.0, y: 0.0, w: scene.size.0, h: scene.size.1 }, |p| scene.nodes[p].bounds)
}

fn is_chrome(n: &Node) -> bool {
    matches!(n.kind, "status_bar" | "island" | "home_indicator" | "keyboard") || n.id.starts_with("status_")
}

fn is_identity_effect(e: &TransitionEffect) -> bool {
    e.opacity.is_none() && e.scale.is_none() && e.scale_x.is_none() && e.scale_y.is_none() && e.offset.is_none() && e.edge.is_none()
}

fn extract_ghost_subtree(nodes: &[Node], root: usize) -> (Vec<Node>, Vec<usize>) {
    let indices = subtree_indices(nodes, root);
    let mut index_map: HashMap<usize, usize> = HashMap::with_capacity(indices.len());
    for (new_i, &old_i) in indices.iter().enumerate() {
        index_map.insert(old_i, new_i);
    }
    let ghost_nodes = indices
        .iter()
        .map(|&old_i| {
            let mut n = nodes[old_i].clone();
            n.strip_interactive();
            n.parent = n.parent.and_then(|p| index_map.get(&p).copied());
            n
        })
        .collect();
    (ghost_nodes, indices)
}

fn props_differ(a: &Node, b: &Node) -> bool {
    rect_diff(a.bounds, b.bounds)
        || (a.opacity - b.opacity).abs() > 0.002
        || matches!((a.fill, b.fill), (Some(ca), Some(cb)) if color_diff(ca, cb))
        || matches!((a.stroke, b.stroke), (Some((ca, wa)), Some((cb, wb))) if color_diff(ca, cb) || (wa - wb).abs() > 0.05)
        || matches!((a.vector.as_ref().and_then(|v| v.fill), b.vector.as_ref().and_then(|v| v.fill)), (Some(fa), Some(fb)) if color_diff(fa.color, fb.color))
        || matches!((a.vector.as_ref().and_then(|v| v.stroke.as_ref()), b.vector.as_ref().and_then(|v| v.stroke.as_ref())), (Some(sa), Some(sb)) if color_diff(sa.color, sb.color) || (sa.line_width - sb.line_width).abs() > 0.05)
        || ((a.radius - b.radius).abs() > 0.05 && a.radius < 1e4 && b.radius < 1e4)
        || matches!((&a.text, &b.text), (Some(ta), Some(tb)) if color_diff(ta.color, tb.color))
        || matches!((&a.symbol, &b.symbol), (Some(sa), Some(sb)) if color_diff(sa.color, sb.color))
        || matches!((a.glyph, b.glyph), (Some((_, ca)), Some((_, cb))) if color_diff(ca, cb))
        || matches!((&a.composite, &b.composite), (Some(ca), Some(cb)) if composite_diff(ca, cb))
        || matches!((&a.fill_paint, &b.fill_paint), (Some(ga), Some(gb)) if ga.differs(gb))
        || matches!((&a.gradient, &b.gradient), (Some(ga), Some(gb)) if gradient_diff(ga, gb))
        || shadow_pair(a.shadow, b.shadow).is_some()
        || glass_tint_pair(a.glass.as_ref(), b.glass.as_ref()).is_some()
}

fn numeric_direction(kind: ContentTransitionKind, old: Option<&str>, new: Option<&str>) -> f64 {
    if kind == ContentTransitionKind::NumericTextDown {
        return -1.0;
    }
    let parsed_old = old.and_then(extract_number);
    let parsed_new = new.and_then(extract_number);
    if let (Some(a), Some(b)) = (parsed_old, parsed_new) {
        if (b - a).abs() > 1e-9 {
            return if b > a { 1.0 } else { -1.0 };
        }
    }
    1.0
}

fn extract_number(s: &str) -> Option<f64> {
    let start = s.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let rest = &s[start..];
    let end = rest
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && c == '-')))
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{NodeAnim, TextRun};
    use crate::text::FontSpec;
    use onesim_protocol::{Appearance, ForeverTag, IosVersion, MatchedGeometryMod, TransitionSpec};

    fn linear(dur: f64, token: u64) -> AnimationSpec {
        AnimationSpec { curve: AnimationCurve::Linear, duration: dur, delay: 0.0, speed: 1.0, repeat: None, autoreverses: true, token }
    }

    fn make_scene(nodes: Vec<Node>) -> Scene {
        let mut all = vec![Node::new("root", "screen")];
        all[0].bounds = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
        for mut n in nodes {
            if n.parent.is_none() {
                n.parent = Some(0);
            }
            all.push(n);
        }
        Scene { size: (400.0, 800.0), appearance: Appearance::Light, ios: IosVersion::default(), nodes: all, unsupported_symbols: vec![], notes: vec![] }
    }

    #[test]
    fn curves_delay_speed_and_repeat() {
        let l = linear(0.4, 1);
        assert!((eval_curve(&l, 0.2, 0.0).0 - 0.5).abs() < 1e-6);
        assert!(eval_curve(&l, 0.4, 0.0).2);

        let delayed = AnimationSpec { delay: 0.1, speed: 2.0, ..l.clone() };
        assert_eq!(eval_curve(&delayed, 0.05, 0.0).0, 0.0);
        // After 0.1s delay, 0.1s at 2x speed = 0.2s of 0.4s -> 0.5
        assert!((eval_curve(&delayed, 0.2, 0.0).0 - 0.5).abs() < 1e-6);
        assert!(eval_curve(&delayed, 0.3, 0.0).2);

        let ease = AnimationSpec { curve: AnimationCurve::EaseInOut, duration: 1.0, ..l.clone() };
        let q1 = eval_curve(&ease, 0.25, 0.0).0;
        let mid = eval_curve(&ease, 0.5, 0.0).0;
        assert!(q1 < 0.25, "easeInOut starts slow: {q1}");
        assert!((mid - 0.5).abs() < 0.02, "easeInOut symmetric midpoint: {mid}");

        let spring = AnimationSpec {
            curve: AnimationCurve::Spring { mass: 1.0, stiffness: 200.0, damping: 12.0, initial_velocity: 0.0, settling: 0.8 },
            duration: 0.8,
            ..l.clone()
        };
        let peak = (1..40).map(|i| eval_curve(&spring, i as f64 * 0.02, 0.0).0).fold(0.0_f64, f64::max);
        assert!(peak > 1.05, "underdamped spring overshoots 1.0 (peak={peak})");
        assert!(eval_curve(&spring, 3.6, 0.0).2);

        let rep = AnimationSpec { repeat: Some(RepeatCount::Count(2)), autoreverses: true, ..l.clone() };
        assert!((eval_curve(&rep, 0.2, 0.0).0 - 0.5).abs() < 1e-6);
        assert!((eval_curve(&rep, 0.6, 0.0).0 - 0.5).abs() < 1e-6);
        let (end, _, done) = eval_curve(&rep, 0.8, 0.0);
        assert!(done && end.abs() < 1e-6, "2x autoreverse finishes at 0: {end}");

        let forever = AnimationSpec { repeat: Some(RepeatCount::Forever(ForeverTag::Forever)), ..l };
        assert!(!eval_curve(&forever, 100.0, 0.0).2);
    }

    #[test]
    fn property_animation_interpolates_and_settles() {
        let mut a = Animator::new();
        let mut n1 = Node::new("box", "shape");
        n1.bounds = Rect { x: 10.0, y: 20.0, w: 50.0, h: 50.0 };
        n1.fill = Some(Color::rgba(1.0, 0.0, 0.0, 1.0));
        let s1 = make_scene(vec![n1]);
        a.update(&s1);
        assert!(!a.is_animating());

        let mut n2 = Node::new("box", "shape");
        n2.bounds = Rect { x: 110.0, y: 20.0, w: 150.0, h: 50.0 };
        n2.fill = Some(Color::rgba(0.0, 0.0, 1.0, 1.0));
        n2.anim = Some(Box::new(NodeAnim { animation: Some(linear(0.4, 1)), ..Default::default() }));
        let s2 = make_scene(vec![n2]);
        a.update(&s2);
        assert!(a.is_animating());

        // Midpoint at t = 0.2s.
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let b = mid.nodes[1].bounds;
        assert!((b.x - 60.0).abs() < 0.5 && (b.w - 100.0).abs() < 0.5, "mid bounds: {b:?}");
        let c = mid.nodes[1].fill.unwrap();
        assert!((c.r - 0.5).abs() < 0.02 && (c.b - 0.5).abs() < 0.02, "mid color: {c:?}");

        // Settles cleanly at t = 0.4s.
        assert!(!a.tick(0.2));
        let end = a.render_scene(&s2);
        assert_eq!(end.nodes[1].bounds, s2.nodes[1].bounds);
        assert_eq!(end.nodes[1].fill, s2.nodes[1].fill);
    }

    #[test]
    fn transition_insertion_and_removal_ghosts() {
        let mut a = Animator::new();
        let s_empty = make_scene(vec![]);
        a.update(&s_empty);

        let mut card = Node::new("card", "shape");
        card.bounds = Rect { x: 50.0, y: 100.0, w: 100.0, h: 40.0 };
        card.action = Some(9);
        card.anim = Some(Box::new(NodeAnim {
            animation: Some(linear(0.4, 1)),
            transition: Some(TransitionSpec {
                insertion: TransitionEffect { opacity: Some(0.0), offset: Some([0.0, 40.0]), ..Default::default() },
                removal: TransitionEffect { opacity: Some(0.0), scale: Some(0.5), ..Default::default() },
                animation: None,
            }),
            ..Default::default()
        }));
        let s_shown = make_scene(vec![card]);
        a.update(&s_shown);
        assert!(a.tick(0.2));
        let mid_in = a.render_scene(&s_shown);
        assert!((mid_in.nodes[1].opacity - 0.5).abs() < 0.02);
        assert!((mid_in.nodes[1].bounds.y - 120.0).abs() < 0.5);
        assert!(!a.tick(0.2));
        let settled_in = a.render_scene(&s_shown);
        assert_eq!(settled_in.nodes[1].opacity, 1.0);
        assert_eq!(settled_in.nodes[1].bounds.y, 100.0);

        // Now remove the card with a new animated tree.
        let mut marker = Node::new("m", "empty");
        marker.anim = Some(Box::new(NodeAnim { animation: Some(linear(0.4, 2)), ..Default::default() }));
        let s_gone = make_scene(vec![marker]);
        a.update(&s_gone);
        assert!(a.is_animating());
        assert!(a.tick(0.2));
        let mid_out = a.render_scene(&s_gone);
        let ghost = mid_out.nodes.iter().find(|n| n.id == "card").expect("removal ghost present");
        assert!(ghost.action.is_none(), "ghost must not be interactive");
        assert!((ghost.opacity - 0.5).abs() < 0.02);
        assert!((ghost.bounds.w - 75.0).abs() < 0.5, "scaled halfway from 100 to 50: {:?}", ghost.bounds);

        assert!(!a.tick(0.2));
        let settled_out = a.render_scene(&s_gone);
        assert!(settled_out.nodes.iter().all(|n| n.id != "card"), "ghost gone once settled");
    }

    #[test]
    fn matched_geometry_morphs_across_branches() {
        let mut a = Animator::new();
        let mut small = Node::new("0/F", "shape");
        small.bounds = Rect { x: 20.0, y: 20.0, w: 60.0, h: 40.0 };
        small.anim = Some(Box::new(NodeAnim {
            matched: Some(MatchedGeometryMod { id: "hero".into(), namespace: "ns".into(), properties: MatchedProps::Frame, anchor: [0.5, 0.5], is_source: true }),
            ..Default::default()
        }));
        a.update(&make_scene(vec![small]));

        let mut big = Node::new("0/T", "shape");
        big.bounds = Rect { x: 100.0, y: 200.0, w: 200.0, h: 120.0 };
        big.anim = Some(Box::new(NodeAnim {
            animation: Some(linear(0.4, 1)),
            matched: Some(MatchedGeometryMod { id: "hero".into(), namespace: "ns".into(), properties: MatchedProps::Frame, anchor: [0.5, 0.5], is_source: true }),
            ..Default::default()
        }));
        let s2 = make_scene(vec![big]);
        a.update(&s2);
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let hero = mid.nodes.iter().find(|n| n.id == "0/T").unwrap();
        assert!((hero.bounds.w - 130.0).abs() < 0.5 && (hero.bounds.h - 80.0).abs() < 0.5, "morphed size: {:?}", hero.bounds);
        assert!(!a.tick(0.2));
        let settled = a.render_scene(&s2);
        assert_eq!(settled.nodes.iter().find(|n| n.id == "0/T").unwrap().bounds, s2.nodes[1].bounds);

        // Retargeting mid-flight when layout shifts must land cleanly at the latest target rect.
        let mut shifted = Node::new("0/T", "shape");
        shifted.bounds = Rect { x: 100.0, y: 150.0, w: 200.0, h: 120.0 };
        shifted.anim = Some(Box::new(NodeAnim {
            animation: Some(linear(0.3, 2)),
            matched: Some(MatchedGeometryMod { id: "hero".into(), namespace: "ns".into(), properties: MatchedProps::Frame, anchor: [0.5, 0.5], is_source: true }),
            ..Default::default()
        }));
        let s3 = make_scene(vec![shifted]);
        a.update(&s3);
        assert!(a.tick(0.15));
        assert!(!a.tick(0.2));
        let final_scene = a.render_scene(&s3);
        assert_eq!(final_scene.nodes.iter().find(|n| n.id == "0/T").unwrap().bounds, s3.nodes[1].bounds);
    }

    #[test]
    fn matched_geometry_compositing_preserves_opaque_and_translucent_alpha() {
        for (a_old, a_new) in [(1.0_f32, 1.0_f32), (0.5, 0.5), (0.4, 1.0), (1.0, 0.4)] {
            let mut a = Animator::new();
            let mut n1 = Node::new("0/F", "shape");
            n1.bounds = Rect { x: 20.0, y: 20.0, w: 60.0, h: 40.0 };
            n1.opacity = a_old;
            n1.anim = Some(Box::new(NodeAnim {
                matched: Some(MatchedGeometryMod { id: "pill".into(), namespace: "ns".into(), properties: MatchedProps::Frame, anchor: [0.5, 0.5], is_source: true }),
                ..Default::default()
            }));
            a.update(&make_scene(vec![n1]));

            let mut n2 = Node::new("0/T", "shape");
            n2.bounds = Rect { x: 120.0, y: 20.0, w: 60.0, h: 40.0 };
            n2.opacity = a_new;
            n2.anim = Some(Box::new(NodeAnim {
                animation: Some(linear(0.4, 1)),
                matched: Some(MatchedGeometryMod { id: "pill".into(), namespace: "ns".into(), properties: MatchedProps::Frame, anchor: [0.5, 0.5], is_source: true }),
                ..Default::default()
            }));
            let s2 = make_scene(vec![n2]);
            a.update(&s2);

            for (dt, p) in [(0.0, 0.0_f32), (0.2, 0.5_f32)] {
                if dt > 0.0 {
                    assert!(a.tick(dt));
                }
                let mid = a.render_scene(&s2);
                let enter = mid.nodes.iter().find(|n| n.id == "0/T").unwrap().opacity;
                let ghost = mid.nodes.iter().find(|n| n.id == "0/F").unwrap().opacity;
                let combined = ghost + enter * (1.0 - ghost);
                let expected = a_old * (1.0 - p) + a_new * p;
                assert!(
                    (combined - expected).abs() < 0.02,
                    "({a_old}->{a_new} @ p={p}): enter={enter:.3}, ghost={ghost:.3}, combined={combined:.3}, expected={expected:.3}"
                );
            }
        }
    }

    #[test]
    fn content_transition_numeric_rolls_and_settles() {
        let mut a = Animator::new();
        let make_num = |txt: &str, tok: u64| {
            let mut n = Node::new("cnt", "text");
            n.bounds = Rect { x: 20.0, y: 100.0, w: 40.0, h: 20.0 };
            n.text = Some(TextRun { text: txt.into(), font: FontSpec::new(17.0, 20.0, 400, crate::text::TextAlign::Leading), color: Color::rgba(0.0, 0.0, 0.0, 1.0), strike: false, offset_x: 0.0, spans: vec![] });
            n.anim = Some(Box::new(NodeAnim {
                animation: (tok > 0).then(|| linear(0.4, tok)),
                content_transition: ContentTransitionKind::NumericTextUp,
                ..Default::default()
            }));
            make_scene(vec![n])
        };
        a.update(&make_num("1", 0));
        let s2 = make_num("2", 1);
        a.update(&s2);
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let texts: Vec<_> = mid.nodes.iter().filter_map(|n| n.text.as_ref().map(|t| (t.text.as_str(), n.bounds.y, n.opacity))).collect();
        assert_eq!(texts.len(), 2, "old and new text both present mid-transition: {texts:?}");
        assert!(texts[0].1 > 100.0 && texts[1].1 < 100.0, "new rolls up from below, old rolls up above: {texts:?}");
        assert!(!a.tick(0.2));
        let end = a.render_scene(&s2);
        assert_eq!(end.nodes.iter().filter(|n| n.text.is_some()).count(), 1);
    }

    #[test]
    fn unrelated_tree_update_preserves_inflight_track() {
        let mut a = Animator::new();
        let make = |x: f64, tok: u64, tick_txt: &str| {
            let mut b = Node::new("box", "shape");
            b.bounds = Rect { x, y: 40.0, w: 40.0, h: 40.0 };
            b.anim = (tok > 0).then(|| Box::new(NodeAnim { animation: Some(linear(0.4, tok)), ..Default::default() }));
            let mut t = Node::new("clock", "text");
            t.bounds = Rect { x: 0.0, y: 0.0, w: 60.0, h: 20.0 };
            t.text = Some(TextRun {
                text: tick_txt.into(),
                font: FontSpec::new(14.0, 16.0, 400, crate::text::TextAlign::Leading),
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                strike: false,
                offset_x: 0.0,
                spans: vec![],
            });
            make_scene(vec![b, t])
        };
        a.update(&make(0.0, 0, "0"));
        let s1 = make(100.0, 1, "0");
        a.update(&s1);
        assert!(a.tick(0.1));
        let mid1 = a.render_scene(&s1);
        assert!((mid1.nodes[1].bounds.x - 25.0).abs() < 0.5);

        // Unrelated non-animated update (e.g. TimelineView tick changing sibling text) must NOT snap `box`.
        let s2 = make(100.0, 0, "1");
        a.update(&s2);
        assert!(a.is_animating(), "in-flight PropTrack must survive unrelated tree update");
        assert!(a.tick(0.1));
        let mid2 = a.render_scene(&s2);
        assert!((mid2.nodes[1].bounds.x - 50.0).abs() < 0.5, "got {}", mid2.nodes[1].bounds.x);
    }

    #[test]
    fn content_transition_down_and_interpolate() {
        let mut a = Animator::new();
        let make = |txt: &str, tok: u64, ct: ContentTransitionKind| {
            let mut n = Node::new("cnt", "text");
            n.bounds = Rect { x: 20.0, y: 100.0, w: 40.0, h: 20.0 };
            n.text = Some(TextRun {
                text: txt.into(),
                font: FontSpec::new(17.0, 20.0, 400, crate::text::TextAlign::Leading),
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                strike: false,
                offset_x: 0.0,
                spans: vec![],
            });
            n.anim = Some(Box::new(NodeAnim {
                animation: (tok > 0).then(|| linear(0.4, tok)),
                content_transition: ct,
                ..Default::default()
            }));
            make_scene(vec![n])
        };
        // Explicit NumericTextDown rolls downward even when the parsed number increases.
        a.update(&make("1", 0, ContentTransitionKind::NumericTextDown));
        let s2 = make("2", 1, ContentTransitionKind::NumericTextDown);
        a.update(&s2);
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let ys: Vec<_> = mid.nodes.iter().filter_map(|n| n.text.as_ref().map(|_| n.bounds.y)).collect();
        assert_eq!(ys.len(), 2);
        assert!(ys[0] < 100.0 && ys[1] > 100.0, "NumericTextDown must roll downward: {ys:?}");
        assert!(!a.tick(0.2));
        let _ = a.render_scene(&s2);

        // ContentTransitionKind::Interpolate cross-fades old and new text.
        let s3 = make("Hello", 2, ContentTransitionKind::Interpolate);
        a.update(&s3);
        assert!(a.tick(0.2));
        let mid_i = a.render_scene(&s3);
        assert_eq!(mid_i.nodes.iter().filter(|n| n.text.is_some()).count(), 2, "Interpolate should cross-fade");
    }

    #[test]
    fn composite_affine_scale_and_rotation_animate() {
        let mut a = Animator::new();
        let make = |scale: f64, deg: f64, tok: u64| {
            let rad = deg.to_radians();
            let (s, c) = rad.sin_cos();
            let mut n = Node::new("orb#effect0", "visual_op");
            n.bounds = Rect { x: 100.0, y: 100.0, w: 96.0, h: 96.0 };
            n.composite = Some(Box::new(Composite::Effect(
                VisualOp::Affine { matrix: [scale * c, scale * s, -scale * s, scale * c, 0.0, 0.0], anchor: [0.5, 0.5] },
                None,
            )));
            n.anim = (tok > 0).then(|| Box::new(NodeAnim { animation: Some(linear(0.4, tok)), ..Default::default() }));
            make_scene(vec![n])
        };
        a.update(&make(0.8, 0.0, 0));
        let s2 = make(1.2, 90.0, 1);
        a.update(&s2);
        assert!(a.is_animating());
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let Some(Composite::Effect(VisualOp::Affine { matrix, .. }, _)) = mid.nodes[1].composite.as_deref() else {
            panic!("expected Affine composite");
        };
        let sx = matrix[0].hypot(matrix[1]);
        let ang = matrix[1].atan2(matrix[0]).to_degrees();
        assert!((sx - 1.0).abs() < 0.02, "halfway scale should be 1.0, got {sx}");
        assert!((ang - 45.0).abs() < 0.5, "halfway rotation should be 45 deg, got {ang}");
        assert!(!a.tick(0.2));
    }

    #[test]
    fn gradient_shadow_and_glass_tint_animate() {
        use crate::glass::{GlassSpec, GlassVariant};
        use onesim_protocol::{GradientColorSpace, GradientPaint, GradientStop};

        let mut a = Animator::new();
        let make = |r: f32, b: f32, sh_r: f64, tok: u64| {
            let mut n = Node::new("card", "shape");
            n.bounds = Rect { x: 16.0, y: 100.0, w: 300.0, h: 160.0 };
            let gp = GradientPaint::Linear {
                stops: vec![
                    GradientStop { color: onesim_protocol::Color::Rgba { r: f64::from(r), g: 0.0, b: f64::from(b), a: 1.0, dark: None }, location: 0.0 },
                    GradientStop { color: onesim_protocol::Color::Rgba { r: f64::from(b), g: 0.5, b: f64::from(r), a: 1.0, dark: None }, location: 1.0 },
                ],
                start: [0.0, 0.0],
                end: [1.0, 1.0],
                color_space: GradientColorSpace::Device,
                absolute: false,
                options: 0,
            };
            n.fill_paint = Some(Arc::new(PaintGradient::resolve(&gp, |c| match c {
                onesim_protocol::Color::Rgba { r, g, b, a, .. } => Color::rgba(*r as f32, *g as f32, *b as f32, *a as f32),
                _ => Color::rgba(0.0, 0.0, 0.0, 1.0),
            })));
            n.shadow = Some(Shadow { color: Color::rgba(r, 0.0, b, 0.4), radius: sh_r, dx: 0.0, dy: 8.0 });
            n.glass = Some(GlassSpec {
                variant: GlassVariant::Regular,
                tint: Some(Color::rgba(r, 0.0, b, 0.35)),
                interactive: false,
                blur: None,
                mix: None,
                shadow: None,
                flat: None,
            });
            n.anim = (tok > 0).then(|| Box::new(NodeAnim { animation: Some(linear(0.4, tok)), ..Default::default() }));
            make_scene(vec![n])
        };
        a.update(&make(1.0, 0.0, 8.0, 0));
        let s2 = make(0.0, 1.0, 18.0, 1);
        a.update(&s2);
        assert!(a.is_animating());
        assert!(a.tick(0.2));
        let mid = a.render_scene(&s2);
        let sh = mid.nodes[1].shadow.expect("shadow");
        assert!((sh.radius - 13.0).abs() < 0.1, "shadow radius halfway 8->18 = 13, got {}", sh.radius);
        assert!((sh.color.r - 0.5).abs() < 0.02 && (sh.color.b - 0.5).abs() < 0.02);
        let gt = mid.nodes[1].glass.as_ref().and_then(|g| g.tint).expect("glass tint");
        assert!((gt.r - 0.5).abs() < 0.02 && (gt.b - 0.5).abs() < 0.02);
        assert!(mid.nodes[1].fill_paint.as_ref().unwrap().differs(s2.nodes[1].fill_paint.as_ref().unwrap()));
        assert!(!a.tick(0.2));
    }
}
