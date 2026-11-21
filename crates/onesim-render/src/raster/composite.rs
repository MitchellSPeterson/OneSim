//! Bounded per-subtree CPU rasterization and GPU-native outer-modifier peeling for composite nodes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use onesim_protocol::{BlendMode, VisualOp};
use tiny_skia::{Path, Pixmap};

use super::sprite::{crop, hash_color, hash_glyph, hash_symbol, hash_text, hash_vector};
use super::vector::{build_vector_path, to_skia_stroke};
use super::{render_subtree_into, text_geometry, PaintClip, Sprite};
use crate::effects::Composite;
use crate::glass::{box_radii, spec_params, GlassVariant};
use crate::scene::{Node, Rect, Scene};
use crate::text::Fonts;

/// True when the scene has a top-level non-Normal `Blend` composite that reads the outer framebuffer.
/// A `Blend` nested inside another composite (e.g. `Group`, `Canvas`, `Blur`, `Mask`, `Affine`)
/// renders inside that composite's isolated bounded pixmap and does not require a whole-frame fallback.
pub(crate) fn needs_whole_frame_blend(nodes: &[Node]) -> bool {
    let mut in_composite = vec![false; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if n.parent.is_some_and(|p| in_composite[p]) {
            in_composite[i] = true;
            continue;
        }
        if let Some(comp) = n.composite.as_deref() {
            in_composite[i] = true;
            if matches!(comp, Composite::Effect(VisualOp::Blend { mode }, _) if *mode != BlendMode::Normal)
                && n.opacity > 0.0
            {
                return true;
            }
        }
    }
    false
}

/// Root indices of the inner subtree remaining after peeling GPU-native outer modifiers.
pub(crate) enum CompositeRoots {
    Single(usize),
    ChildrenOf(usize),
}

impl CompositeRoots {
    pub(crate) fn slice<'a>(&'a self, children: &'a [Vec<usize>]) -> &'a [usize] {
        match self {
            Self::Single(idx) => std::slice::from_ref(idx),
            Self::ChildrenOf(parent) => children[*parent].as_slice(),
        }
    }
}

/// Result of peeling GPU-native outer modifiers (`Affine`, `Opacity`, `Group { opaque: false }`,
/// `Blur { radius <= 0 }`, `Blend { Normal }`) from a composite chain.
pub(crate) struct CompositePlan {
    pub roots: CompositeRoots,
    /// Device-pixel affine `[a, b, c, d, e, f]`: `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
    pub affine: [f64; 6],
    pub opacity: f32,
    pub had_affine: bool,
}

/// Apply `inner` first, then `outer`.
fn compose_affine(inner: [f64; 6], outer: [f64; 6]) -> [f64; 6] {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

/// Iteratively peel GPU-native outer modifiers starting at `first`.
pub(crate) fn plan_composite(
    first: usize,
    nodes: &[Node],
    children: &[Vec<usize>],
    scale: f64,
) -> Option<CompositePlan> {
    let mut cursor = first;
    let mut total_affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut total_opacity = 1.0_f32;
    let mut had_affine = false;

    loop {
        let node = &nodes[cursor];
        match node.composite.as_deref() {
            Some(Composite::Effect(
                VisualOp::Affine {
                    matrix: [a, b, c, d, e, f],
                    anchor: [u, v],
                },
                _,
            )) => {
                let bounds = node.bounds;
                let (x, y) = (bounds.x + u * bounds.w, bounds.y + v * bounds.h);
                let (tx, ty) = (e + x - a * x - c * y, f + y - b * x - d * y);
                let m = [*a, *b, *c, *d, tx * scale, ty * scale];
                let det = a * d - b * c;
                if det == 0.0 || !det.is_finite() || !m.iter().all(|v| v.is_finite()) {
                    return None;
                }
                total_affine = compose_affine(m, total_affine);
                total_opacity *= node.opacity;
                had_affine = true;
            }
            Some(Composite::Effect(VisualOp::Opacity { amount }, _)) => {
                total_opacity *= node.opacity * (amount.clamp(0.0, 1.0) as f32);
            }
            Some(Composite::Effect(VisualOp::Group { opaque: false }, _))
            | Some(Composite::Effect(VisualOp::Blend { mode: BlendMode::Normal }, _)) => {
                total_opacity *= node.opacity;
            }
            Some(Composite::Effect(VisualOp::Blur { radius, .. }, _)) if *radius <= 0.0 => {
                total_opacity *= node.opacity;
            }
            _ => {
                if !total_opacity.is_finite() || total_opacity <= 0.0 || !total_affine.iter().all(|v| v.is_finite()) {
                    return None;
                }
                return Some(CompositePlan {
                    roots: CompositeRoots::Single(cursor),
                    affine: total_affine,
                    opacity: total_opacity,
                    had_affine,
                });
            }
        }

        if !total_opacity.is_finite() || total_opacity <= 0.0 || !total_affine.iter().all(|v| v.is_finite()) {
            return None;
        }
        match children[cursor].as_slice() {
            [] => return None,
            [child] if nodes[*child].composite.is_some() => {
                cursor = *child;
            }
            _ => {
                return Some(CompositePlan {
                    roots: CompositeRoots::ChildrenOf(cursor),
                    affine: total_affine,
                    opacity: total_opacity,
                    had_affine,
                });
            }
        }
    }
}

/// Conservative device-pixel bounds for a composite subtree.
pub(crate) struct CompositeBounds {
    /// Working pixmap bounds clamped to the viewport `0..pw, 0..ph`.
    pub work: PaintClip,
    /// Conservative output bounds in inner space (before `plan.affine`), clamped to viewport.
    pub output: PaintClip,
    pub clipped_by_viewport: bool,
    pub has_scroll_edge: bool,
}

fn union_opt(a: Option<PaintClip>, b: Option<PaintClip>) -> Option<PaintClip> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

fn rect_clip(b: Rect, s: f32) -> Option<PaintClip> {
    if b.w < 0.0 || b.h < 0.0 {
        return None;
    }
    let left = (b.x as f32 * s).floor();
    let top = (b.y as f32 * s).floor();
    let right = ((b.x + b.w) as f32 * s).max(b.x as f32 * s + b.w as f32 * s).ceil().max(left + 1.0);
    let bottom = ((b.y + b.h) as f32 * s).max(b.y as f32 * s + b.h as f32 * s).ceil().max(top + 1.0);
    if [left, top, right, bottom].iter().all(|v| v.is_finite()) {
        Some(PaintClip::from_ltrb(left, top, right, bottom))
    } else {
        None
    }
}

/// Transform the 4 corners of `clip` through `affine`, take AABB, and outset by `pad`.
pub(crate) fn transform_clip(clip: PaintClip, affine: [f64; 6], pad: f32) -> PaintClip {
    if clip.is_empty() {
        return PaintClip::empty();
    }
    let [a, b, c, d, e, f] = affine;
    let corners = [
        (clip.left() as f64, clip.top() as f64),
        (clip.right() as f64, clip.top() as f64),
        (clip.left() as f64, clip.bottom() as f64),
        (clip.right() as f64, clip.bottom() as f64),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in corners {
        let tx = a * x + c * y + e;
        let ty = b * x + d * y + f;
        if !tx.is_finite() || !ty.is_finite() {
            return PaintClip::empty();
        }
        min_x = min_x.min(tx);
        min_y = min_y.min(ty);
        max_x = max_x.max(tx);
        max_y = max_y.max(ty);
    }
    PaintClip::from_ltrb(
        min_x.floor() as f32 - pad,
        min_y.floor() as f32 - pad,
        max_x.ceil() as f32 + pad,
        max_y.ceil() as f32 + pad,
    )
}

fn node_own_bounds(
    n: &Node,
    fonts: &Fonts,
    s: f32,
    dark: bool,
    ios: onesim_protocol::IosVersion,
    viewport: PaintClip,
    has_scroll_edge: &mut bool,
) -> Option<PaintClip> {
    let mut acc: Option<PaintClip> = None;
    let b = n.bounds;
    let base = rect_clip(b, s);

    if n.fill.is_some() || n.gradient.is_some() || n.image.is_some() {
        if let Some(r) = base {
            acc = union_opt(acc, Some(r.outset(1.0)));
        }
    }
    if let Some((_, lw)) = n.stroke {
        if let Some(r) = base {
            acc = union_opt(acc, Some(r.outset((lw as f32 * 2.0 + 2.0) * s)));
        }
    }
    if let Some(sh) = &n.shadow {
        if let Some(r) = base {
            let shifted = r.translated(sh.dx as f32 * s, sh.dy as f32 * s);
            acc = union_opt(acc, Some(shifted.outset((sh.radius as f32 * 3.0 + 2.0) * s)));
        }
    }
    if let Some(g) = n.glass.as_ref().filter(|g| g.variant != GlassVariant::Identity) {
        if let Some(r) = base {
            let p = spec_params(g, dark, ios);
            acc = union_opt(acc, Some(r.outset((p.sh_sigma * 3.0 + p.sh_dy.abs() + 8.0) * s)));
        }
    }
    if let Some(e) = &n.edge {
        *has_scroll_edge = true;
        acc = union_opt(acc, Some(PaintClip::from_ltrb(0.0, 0.0, viewport.right(), e.bar * s + 4.0)));
    }
    if let Some(v) = n.vector.as_deref() {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut grow = |p: &Path, pad: f32| {
            let pb = p.bounds();
            min_x = min_x.min(pb.left() - pad);
            min_y = min_y.min(pb.top() - pad);
            max_x = max_x.max(pb.right() + pad);
            max_y = max_y.max(pb.bottom() + pad);
        };
        if v.fill.is_some() {
            if let Some(p) = build_vector_path(&v.geom, b, 0.0) {
                grow(&p, 2.0);
            }
        }
        if let Some(st) = &v.stroke {
            if st.line_width > 0.0 {
                if let Some(p) = build_vector_path(&v.geom, b, st.inset) {
                    let sk = to_skia_stroke(st);
                    if let Some(stroked) = p.stroke(&sk, 1.0) {
                        grow(&stroked, 2.0);
                    } else {
                        let miter = match st.line_join {
                            onesim_protocol::LineJoin::Miter => st.miter_limit.max(1.0),
                            _ => 1.0,
                        };
                        let pad = (st.line_width * miter * 0.5 + 2.0).max(st.line_width + 2.0) as f32;
                        if pad.is_finite() {
                            grow(&p, pad);
                        } else {
                            acc = union_opt(acc, Some(viewport));
                        }
                    }
                }
            }
        }
        let edges = [(min_x * s).floor(), (min_y * s).floor(), (max_x * s).ceil() + 1.0, (max_y * s).ceil() + 1.0];
        if edges.iter().all(|v| v.is_finite()) && edges[2] > edges[0] && edges[3] > edges[1] {
            acc = union_opt(acc, Some(PaintClip::from_ltrb(edges[0], edges[1], edges[2], edges[3])));
        } else if min_x != f32::INFINITY {
            acc = union_opt(acc, Some(viewport));
        }
    }
    if n.glyph.is_some() {
        if let Some(r) = base {
            acc = union_opt(acc, Some(r.outset((12.0 * s).ceil())));
        }
    }
    if let Some(sy) = n.symbol.as_ref() {
        let pad = (sy.size.0.max(sy.size.1) as f32 * 0.25 + 4.0) * s;
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let sym_rect = Rect {
            x: cx - sy.size.0 / 2.0,
            y: cy - sy.size.1 / 2.0,
            w: sy.size.0,
            h: sy.size.1,
        };
        if let Some(r) = union_opt(base, rect_clip(sym_rect, s)) {
            acc = union_opt(acc, Some(r.outset(pad)));
        }
    }
    if let Some(t) = n.text.as_ref() {
        let (spec, width, (ox, oy)) = text_geometry(n, t, fonts, s);
        let left = ox - spec.size;
        let right = (ox + width + spec.size).max((b.x + b.w) as f32 * s + spec.size);
        let mut top = oy - spec.size * 0.5;
        let mut bottom = oy + (b.h as f32 * s).max(spec.line_height) + spec.size * 1.5;
        if n.text_squash > 1.0 {
            let cy = (b.y + b.h / 2.0) as f32 * s;
            top = cy + (top - cy) * n.text_squash;
            bottom = cy + (bottom - cy) * n.text_squash;
        }
        if [left, top, right, bottom].iter().all(|v| v.is_finite()) && right > left && bottom > top {
            acc = union_opt(acc, Some(PaintClip::from_ltrb(left.floor(), top.floor(), right.ceil(), bottom.ceil())));
        }
    }
    acc
}

/// Iteratively compute conservative `(work, output)` device-pixel bounds for `roots`.
pub(crate) fn subtree_bounds(
    scene: &Scene,
    roots: &[usize],
    children: &[Vec<usize>],
    fonts: &Fonts,
    scale: f64,
    pw: u32,
    ph: u32,
) -> CompositeBounds {
    let s = scale as f32;
    let dark = scene.appearance == onesim_protocol::Appearance::Dark;
    let viewport = PaintClip::viewport(pw, ph);
    let mut has_scroll_edge = false;

    // Post-order traversal using an explicit stack so deep chains never overflow the call stack.
    let mut order: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = roots.iter().rev().copied().collect();
    while let Some(idx) = stack.pop() {
        order.push(idx);
        for &c in children[idx].iter().rev() {
            stack.push(c);
        }
    }

    let mut work_map: std::collections::HashMap<usize, Option<PaintClip>> =
        std::collections::HashMap::with_capacity(order.len());
    let mut out_map: std::collections::HashMap<usize, Option<PaintClip>> =
        std::collections::HashMap::with_capacity(order.len());

    for &idx in order.iter().rev() {
        let n = &scene.nodes[idx];
        let kids = &children[idx];
        if let Some(comp) = n.composite.as_deref() {
            let mut child_work: Option<PaintClip> = None;
            let mut child_output: Option<PaintClip> = None;
            match comp {
                Composite::Mask { .. } => {
                    for &c in kids {
                        child_work = union_opt(child_work, work_map.get(&c).copied().flatten());
                    }
                    if let Some((_, content)) = kids.split_last() {
                        for &c in content {
                            child_output = union_opt(child_output, out_map.get(&c).copied().flatten());
                        }
                    }
                    work_map.insert(idx, child_work);
                    out_map.insert(idx, child_output);
                }
                Composite::Effect(op, _) => {
                    for &c in kids {
                        child_work = union_opt(child_work, work_map.get(&c).copied().flatten());
                        child_output = union_opt(child_output, out_map.get(&c).copied().flatten());
                    }
                    let out = match op {
                        VisualOp::Affine {
                            matrix: [a, b, c, d, e, f],
                            anchor: [u, v],
                        } => {
                            let bounds = n.bounds;
                            let (x, y) = (bounds.x + u * bounds.w, bounds.y + v * bounds.h);
                            let (tx, ty) = (e + x - a * x - c * y, f + y - b * x - d * y);
                            let m = [*a, *b, *c, *d, tx * scale, ty * scale];
                            child_output.map(|co| transform_clip(co, m, 2.0))
                        }
                        VisualOp::Projection { matrix } => {
                            let [a, b, c, d, e, f, g, h, i] = *matrix;
                            child_output.map(|co| {
                                let corners = [
                                    (co.left() as f64 / scale - n.bounds.x, co.top() as f64 / scale - n.bounds.y),
                                    (co.right() as f64 / scale - n.bounds.x, co.top() as f64 / scale - n.bounds.y),
                                    (co.left() as f64 / scale - n.bounds.x, co.bottom() as f64 / scale - n.bounds.y),
                                    (co.right() as f64 / scale - n.bounds.x, co.bottom() as f64 / scale - n.bounds.y),
                                ];
                                let mut min_x = f64::INFINITY;
                                let mut min_y = f64::INFINITY;
                                let mut max_x = f64::NEG_INFINITY;
                                let mut max_y = f64::NEG_INFINITY;
                                let mut valid = true;
                                for (cx, cy) in corners {
                                    let w = cx * c + cy * f + i;
                                    if w <= 1e-6 {
                                        valid = false;
                                        break;
                                    }
                                    let px = ((cx * a + cy * d + g) / w + n.bounds.x) * scale;
                                    let py = ((cx * b + cy * e + h) / w + n.bounds.y) * scale;
                                    if !px.is_finite() || !py.is_finite() {
                                        valid = false;
                                        break;
                                    }
                                    min_x = min_x.min(px);
                                    min_y = min_y.min(py);
                                    max_x = max_x.max(px);
                                    max_y = max_y.max(py);
                                }
                                if valid {
                                    PaintClip::from_ltrb(
                                        min_x.floor() as f32 - 2.0,
                                        min_y.floor() as f32 - 2.0,
                                        max_x.ceil() as f32 + 2.0,
                                        max_y.ceil() as f32 + 2.0,
                                    )
                                } else {
                                    viewport
                                }
                            })
                        }
                        VisualOp::Blur { radius, opaque } => {
                            if *opaque {
                                rect_clip(n.bounds, s)
                            } else if *radius > 0.0 {
                                let sigma = *radius * scale;
                                if sigma >= 1_000_000.0 || !sigma.is_finite() {
                                    Some(viewport)
                                } else {
                                    let pad = box_radii(sigma as f32).iter().sum::<usize>() as f32 + 1.0;
                                    child_output.map(|co| co.outset(pad))
                                }
                            } else {
                                child_output
                            }
                        }
                        VisualOp::Group { opaque: true } => {
                            union_opt(child_output, rect_clip(n.bounds, s))
                        }
                        _ => child_output,
                    };
                    let work = union_opt(child_work, out);
                    work_map.insert(idx, work);
                    out_map.insert(idx, out);
                }
            }
        } else {
            let own = node_own_bounds(n, fonts, s, dark, scene.ios, viewport, &mut has_scroll_edge);
            let mut work = own;
            let mut out = own;
            for &c in kids {
                work = union_opt(work, work_map.get(&c).copied().flatten());
                out = union_opt(out, out_map.get(&c).copied().flatten());
            }
            if n.clip_radius.is_some() && n.bounds.w > 0.0 && n.bounds.h > 0.0 {
                if let Some(clip_rect) = rect_clip(n.bounds, s).map(|r| r.outset(1.0)) {
                    out = out.map(|o| o.intersect(clip_rect)).filter(|o| !o.is_empty());
                }
            }
            work_map.insert(idx, work);
            out_map.insert(idx, out);
        }
    }

    let mut total_work: Option<PaintClip> = None;
    let mut total_output: Option<PaintClip> = None;
    for &r in roots {
        total_work = union_opt(total_work, work_map.get(&r).copied().flatten());
        total_output = union_opt(total_output, out_map.get(&r).copied().flatten());
    }

    let raw_work = total_work.unwrap_or_else(PaintClip::empty);
    let raw_output = total_output.unwrap_or_else(PaintClip::empty);
    let clipped_by_viewport = !raw_work.is_empty()
        && (raw_work.left() < 0.0
            || raw_work.top() < 0.0
            || raw_work.right() > pw as f32
            || raw_work.bottom() > ph as f32);

    CompositeBounds {
        work: raw_work.intersect(viewport),
        output: raw_output.intersect(viewport),
        clipped_by_viewport,
        has_scroll_edge,
    }
}

/// Compute exact point-aligned tile origin `(ox, oy, dx_pt, dy_pt, rw, rh)` for `work`.
pub(crate) fn aligned_origin(
    work: PaintClip,
    scale: f64,
    has_scroll_edge: bool,
    pw: u32,
    ph: u32,
) -> Option<(i32, i32, f64, f64, u32, u32)> {
    if work.is_empty() {
        return None;
    }
    let (px_step, pt_step) = if scale.fract() == 0.0 && scale >= 1.0 {
        (scale as i32, 1.0)
    } else if (scale * 8.0).fract() == 0.0 && scale > 0.0 {
        ((scale * 8.0).round() as i32, 8.0)
    } else {
        (1, 1.0 / scale)
    };

    let ox = ((work.left().floor() as i32).max(0) / px_step) * px_step;
    let dx_pt = (ox / px_step) as f64 * pt_step;
    let (oy, dy_pt) = if has_scroll_edge {
        (0, 0.0)
    } else {
        let oy = ((work.top().floor() as i32).max(0) / px_step) * px_step;
        (oy, (oy / px_step) as f64 * pt_step)
    };
    let x1 = (work.right().ceil() as i32).clamp(ox, pw as i32);
    let y1 = (work.bottom().ceil() as i32).clamp(oy, ph as i32);
    let rw = (x1 - ox) as u32;
    let rh = (y1 - oy) as u32;
    if rw == 0 || rh == 0 {
        return None;
    }
    Some((ox, oy, dx_pt, dy_pt, rw, rh))
}

fn hash_proto_color<H: Hasher>(c: &onesim_protocol::Color, h: &mut H) {
    match c {
        onesim_protocol::Color::Hierarchical { semantic, hierarchical } => {
            0u8.hash(h);
            std::mem::discriminant(semantic).hash(h);
            hierarchical.hash(h);
        }
        onesim_protocol::Color::Semantic { semantic } => {
            1u8.hash(h);
            std::mem::discriminant(semantic).hash(h);
        }
        onesim_protocol::Color::Rgba { r, g, b, a, dark } => {
            2u8.hash(h);
            [*r, *g, *b, *a].map(f64::to_bits).hash(h);
            dark.as_deref().map(|d| d.map(f64::to_bits)).hash(h);
        }
    }
}

fn hash_visual_op<H: Hasher>(op: &VisualOp, h: &mut H) {
    match op {
        VisualOp::Affine { matrix, anchor } => {
            0u8.hash(h);
            matrix.map(f64::to_bits).hash(h);
            anchor.map(f64::to_bits).hash(h);
        }
        VisualOp::Projection { matrix } => {
            1u8.hash(h);
            matrix.map(f64::to_bits).hash(h);
        }
        VisualOp::Blur { radius, opaque } => {
            2u8.hash(h);
            radius.to_bits().hash(h);
            opaque.hash(h);
        }
        VisualOp::Blend { mode } => {
            3u8.hash(h);
            (*mode as u8).hash(h);
        }
        VisualOp::Brightness { amount } => {
            4u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::Contrast { amount } => {
            5u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::Saturation { amount } => {
            6u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::Grayscale { amount } => {
            7u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::HueRotation { radians } => {
            8u8.hash(h);
            radians.to_bits().hash(h);
        }
        VisualOp::ColorMultiply { color } => {
            9u8.hash(h);
            hash_proto_color(color, h);
        }
        VisualOp::ColorInvert { amount } => {
            10u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::AlphaThreshold { min, max, color } => {
            11u8.hash(h);
            min.to_bits().hash(h);
            max.to_bits().hash(h);
            hash_proto_color(color, h);
        }
        VisualOp::LuminanceToAlpha => {
            12u8.hash(h);
        }
        VisualOp::Opacity { amount } => {
            13u8.hash(h);
            amount.to_bits().hash(h);
        }
        VisualOp::Group { opaque } => {
            14u8.hash(h);
            opaque.hash(h);
        }
    }
}

/// Content-addressed hash for a composite subtree, returning `(hash, pinned_bitmaps)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_key(
    scene: &Scene,
    roots: &[usize],
    children: &[Vec<usize>],
    fonts: &Fonts,
    scale: f64,
    dx_pt: f64,
    dy_pt: f64,
    padded: bool,
    viewport_clip: Option<(i32, i32, u32, u32)>,
) -> (u64, Vec<Arc<Pixmap>>) {
    let mut h = DefaultHasher::new();
    0xC0u8.hash(&mut h);
    fonts.is_sf().hash(&mut h);
    (scale as f32).to_bits().hash(&mut h);
    (scene.appearance as u8).hash(&mut h);
    (scene.ios as u8).hash(&mut h);
    padded.hash(&mut h);
    viewport_clip.hash(&mut h);

    let mut pinned = Vec::new();
    roots.len().hash(&mut h);
    let mut stack: Vec<usize> = roots.iter().rev().copied().collect();
    while let Some(idx) = stack.pop() {
        let n = &scene.nodes[idx];
        let kids = &children[idx];
        kids.len().hash(&mut h);
        for &c in kids.iter().rev() {
            stack.push(c);
        }

        let b = n.bounds;
        ((b.x - dx_pt).to_bits(), (b.y - dy_pt).to_bits(), b.w.to_bits(), b.h.to_bits()).hash(&mut h);
        n.radius.to_bits().hash(&mut h);
        n.clip_radius.map(f64::to_bits).hash(&mut h);
        n.opacity.to_bits().hash(&mut h);
        n.text_squash.to_bits().hash(&mut h);

        if let Some(c) = n.fill {
            true.hash(&mut h);
            hash_color(c, &mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some((c, lw)) = n.stroke {
            true.hash(&mut h);
            hash_color(c, &mut h);
            lw.to_bits().hash(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(g) = n.gradient.as_ref() {
            true.hash(&mut h);
            g.angle.to_bits().hash(&mut h);
            g.stops.len().hash(&mut h);
            for &c in &g.stops {
                hash_color(c, &mut h);
            }
            g.points.map(|p| p.map(f32::to_bits)).hash(&mut h);
            if let Some(locs) = &g.locations {
                true.hash(&mut h);
                locs.len().hash(&mut h);
                for &l in locs {
                    l.to_bits().hash(&mut h);
                }
            } else {
                false.hash(&mut h);
            }
        } else {
            false.hash(&mut h);
        }
        if let Some(sh) = &n.shadow {
            true.hash(&mut h);
            hash_color(sh.color, &mut h);
            [sh.radius, sh.dx, sh.dy].map(f64::to_bits).hash(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(g) = &n.glass {
            true.hash(&mut h);
            (g.variant as u8).hash(&mut h);
            if let Some(c) = g.tint {
                true.hash(&mut h);
                hash_color(c, &mut h);
            } else {
                false.hash(&mut h);
            }
            g.interactive.hash(&mut h);
            g.blur.map(f32::to_bits).hash(&mut h);
            g.mix.map(|(a, b)| (a.to_bits(), b.to_bits())).hash(&mut h);
            g.shadow.map(|(a, b, c)| (a.to_bits(), b.to_bits(), c.to_bits())).hash(&mut h);
            g.flat.map(f32::to_bits).hash(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(e) = &n.edge {
            true.hash(&mut h);
            e.bar.to_bits().hash(&mut h);
            hash_color(e.color, &mut h);
            e.tint.to_bits().hash(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(v) = n.vector.as_deref() {
            true.hash(&mut h);
            hash_vector(v, &mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(g) = n.fill_paint.as_ref() {
            true.hash(&mut h);
            g.hash_key(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(g) = n.stroke_paint.as_ref() {
            true.hash(&mut h);
            g.hash_key(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(g) = n.content_paint.as_ref() {
            true.hash(&mut h);
            g.hash_key(&mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some((g, c)) = n.glyph {
            true.hash(&mut h);
            hash_glyph(g, &mut h);
            hash_color(c, &mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(sy) = n.symbol.as_ref() {
            true.hash(&mut h);
            hash_symbol(sy, &mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(im) = n.image.as_ref() {
            true.hash(&mut h);
            let d = im.dest;
            ((d.x - dx_pt).to_bits(), (d.y - dy_pt).to_bits(), d.w.to_bits(), d.h.to_bits()).hash(&mut h);
            im.rotation.to_bits().hash(&mut h);
            im.filter.hash(&mut h);
            (Arc::as_ptr(&im.pixmap) as usize).hash(&mut h);
            pinned.push(im.pixmap.clone());
        } else {
            false.hash(&mut h);
        }
        if let Some(t) = n.text.as_ref() {
            true.hash(&mut h);
            hash_text(t, &mut h);
        } else {
            false.hash(&mut h);
        }
        if let Some(comp) = n.composite.as_deref() {
            true.hash(&mut h);
            match comp {
                Composite::Effect(op, col) => {
                    0u8.hash(&mut h);
                    hash_visual_op(op, &mut h);
                    if let Some(c) = col {
                        true.hash(&mut h);
                        hash_color(*c, &mut h);
                    } else {
                        false.hash(&mut h);
                    }
                }
                Composite::Mask { inverted } => {
                    1u8.hash(&mut h);
                    inverted.hash(&mut h);
                }
            }
        } else {
            false.hash(&mut h);
        }
    }

    (h.finish(), pinned)
}

/// Rasterize a composite subtree into a bounded, cropped (and optionally 1px-padded) `Sprite`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_composite_sprite(
    scene: &Scene,
    roots: &[usize],
    children: &[Vec<usize>],
    fonts: &mut Fonts,
    scale: f64,
    ox: i32,
    oy: i32,
    dx_pt: f64,
    dy_pt: f64,
    rw: u32,
    rh: u32,
    padded: bool,
) -> Option<Sprite> {
    let mut local_nodes: Vec<Node> = Vec::new();
    let mut local_children: Vec<Vec<usize>> = Vec::new();
    let mut local_roots: Vec<usize> = Vec::with_capacity(roots.len());

    // Iteratively clone reachable nodes with remapped parent indices and shifted coordinates.
    let mut stack: Vec<(usize, Option<usize>)> = roots.iter().rev().map(|&r| (r, None)).collect();
    while let Some((orig, new_parent)) = stack.pop() {
        let new_idx = local_nodes.len();
        let mut node = scene.nodes[orig].clone();
        node.parent = new_parent;
        node.bounds.x -= dx_pt;
        node.bounds.y -= dy_pt;
        if let Some(im) = node.image.as_mut() {
            im.dest.x -= dx_pt;
            im.dest.y -= dy_pt;
        }
        local_nodes.push(node);
        local_children.push(Vec::new());
        if let Some(p) = new_parent {
            local_children[p].push(new_idx);
        } else {
            local_roots.push(new_idx);
        }
        for &c in children[orig].iter().rev() {
            stack.push((c, Some(new_idx)));
        }
    }

    let mut pm = Pixmap::new(rw, rh)?;
    let dark = scene.appearance == onesim_protocol::Appearance::Dark;
    render_subtree_into(
        &mut pm,
        &local_roots,
        &local_nodes,
        &local_children,
        fonts,
        scale as f32,
        dark,
        scene.ios,
    );

    let (cx, cy, cropped) = crop(ox, oy, pm)?;
    if padded {
        let cw = cropped.width();
        let ch = cropped.height();
        let mut out = Pixmap::new(cw + 2, ch + 2)?;
        let src_stride = cw as usize * 4;
        let dst_stride = (cw + 2) as usize * 4;
        let src = cropped.data();
        let dst = out.data_mut();
        for row in 0..ch as usize {
            let s0 = row * src_stride;
            let d0 = (row + 1) * dst_stride + 4;
            dst[d0..d0 + src_stride].copy_from_slice(&src[s0..s0 + src_stride]);
        }
        Some(Sprite { x: cx - 1, y: cy - 1, pixmap: out })
    } else {
        Some(Sprite { x: cx, y: cy, pixmap: cropped })
    }
}
