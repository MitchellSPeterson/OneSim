use crate::scene::{Color, Glyph, Scene, Shadow, SymbolRun, VectorGeom, VectorShape, VectorStroke};
use crate::symbols;
use crate::text::{FontSpec, Fonts};
use onesim_protocol::{IosVersion, LineCap as ProtoLineCap, LineJoin as ProtoLineJoin, PathOp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, StrokeDash, Transform,
};

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = r * (1.0 - 0.552_284_8);
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - k, y, x + w, y + k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + k, y + h, x, y + h - k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + k, x + k, y, x + r, y);
    b.close();
    b.finish()
}

/// Build an uneven (and optionally Apple-continuous squircle) rounded rectangle path.
/// `radii` = `[top_left, top_right, bottom_right, bottom_left]` each `(rx, ry)`.
fn uneven_rrect(x: f32, y: f32, w: f32, h: f32, radii: [(f64, f64); 4], continuous: bool) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let mut r = radii.map(|(rx, ry)| ((rx as f32).max(0.0), (ry as f32).max(0.0)));
    // Proportionally clamp adjacent corners so edges never cross.
    let mut f = 1.0f32;
    let check = |sum: f32, len: f32, f: &mut f32| {
        if sum > len && sum > 0.0 {
            *f = (*f).min(len / sum);
        }
    };
    check(r[0].0 + r[1].0, w, &mut f);
    check(r[3].0 + r[2].0, w, &mut f);
    check(r[0].1 + r[3].1, h, &mut f);
    check(r[1].1 + r[2].1, h, &mut f);
    if f < 1.0 {
        for c in &mut r {
            c.0 *= f;
            c.1 *= f;
        }
    }
    let (tl, tr, br, bl) = (r[0], r[1], r[2], r[3]);
    let k = 0.552_284_8_f32;

    // Emit one corner transition from incoming straight edge to outgoing straight edge.
    // `corner` = sharp corner vertex, `u` = unit direction back along incoming edge, `v` = unit direction forward along outgoing edge,
    // `(ru, rv)` = corner radii along `u` and `v`, `(au, av)` = available edge budget along `u` and `v`.
    let corner = |b: &mut PathBuilder, cx: f32, cy: f32, (ux, uy): (f32, f32), (vx, vy): (f32, f32), (ru, rv): (f32, f32), (au, av): (f32, f32)| {
        let pt = |du: f32, dv: f32| (cx + ux * du + vx * dv, cy + uy * du + vy * dv);
        if ru <= 1e-4 || rv <= 1e-4 {
            let (px, py) = pt(0.0, 0.0);
            b.line_to(px, py);
            return;
        }
        // Apple continuous corner (measured from SwiftUICore.RoundedRectangle(style: .continuous)):
        // p = 1.52866, c1 = 1.08849, c2 = 0.868407, p1 = (0.631494, 0.0749114), c3 = (0.372824, 0.16906).
        const P: f32 = 1.528_66;
        if continuous && au >= ru * P - 1e-3 && av >= rv * P - 1e-3 {
            let (s0x, s0y) = pt(ru * P, 0.0);
            b.line_to(s0x, s0y);
            let (c1x, c1y) = pt(ru * 1.088_49, 0.0);
            let (c2x, c2y) = pt(ru * 0.868_407, 0.0);
            let (p1x, p1y) = pt(ru * 0.631_494, rv * 0.074_911_4);
            b.cubic_to(c1x, c1y, c2x, c2y, p1x, p1y);
            let (c3x, c3y) = pt(ru * 0.372_824, rv * 0.169_06);
            let (c4x, c4y) = pt(ru * 0.169_06, rv * 0.372_824);
            let (p2x, p2y) = pt(ru * 0.074_911_4, rv * 0.631_494);
            b.cubic_to(c3x, c3y, c4x, c4y, p2x, p2y);
            let (c5x, c5y) = pt(0.0, rv * 0.868_407);
            let (c6x, c6y) = pt(0.0, rv * 1.088_49);
            let (p3x, p3y) = pt(0.0, rv * P);
            b.cubic_to(c5x, c5y, c6x, c6y, p3x, p3y);
        } else {
            let (sx, sy) = pt(ru, 0.0);
            b.line_to(sx, sy);
            let (c1x, c1y) = pt(ru * (1.0 - k), 0.0);
            let (c2x, c2y) = pt(0.0, rv * (1.0 - k));
            let (ex, ey) = pt(0.0, rv);
            b.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
        }
    };

    // Half-edge budgets so two continuous corners on the same edge never overlap.
    let budget = |a: f32, b: f32, len: f32| -> (f32, f32) {
        if a + b <= 1e-6 {
            (len * 0.5, len * 0.5)
        } else {
            (len * a / (a + b), len * b / (a + b))
        }
    };
    let (top_l, top_r) = budget(tl.0, tr.0, w);
    let (bot_l, bot_r) = budget(bl.0, br.0, w);
    let (left_t, left_b) = budget(tl.1, bl.1, h);
    let (right_t, right_b) = budget(tr.1, br.1, h);

    let mut b = PathBuilder::new();
    // Start at midpoint of top edge so every corner can simply `line_to` its entry point.
    b.move_to(x + top_l, y);
    // Top-right: incoming (-1, 0), outgoing (0, 1)
    corner(&mut b, x + w, y, (-1.0, 0.0), (0.0, 1.0), (tr.0, tr.1), (top_r, right_t));
    // Bottom-right: incoming (0, -1), outgoing (-1, 0)
    corner(&mut b, x + w, y + h, (0.0, -1.0), (-1.0, 0.0), (br.1, br.0), (right_b, bot_r));
    // Bottom-left: incoming (1, 0), outgoing (0, -1)
    corner(&mut b, x, y + h, (1.0, 0.0), (0.0, -1.0), (bl.0, bl.1), (bot_l, left_b));
    // Top-left: incoming (0, 1), outgoing (1, 0)
    corner(&mut b, x, y, (0.0, 1.0), (1.0, 0.0), (tl.1, tl.0), (left_t, top_l));
    b.close();
    b.finish()
}

fn build_vector_path(geom: &VectorGeom, b: crate::scene::Rect, inset: f64) -> Option<Path> {
    let bx = b.x as f32;
    let by = b.y as f32;
    let bw = b.w as f32;
    let bh = b.h as f32;
    let ins = inset as f32;
    match geom {
        VectorGeom::Ellipse => {
            let (w, h) = (bw - 2.0 * ins, bh - 2.0 * ins);
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            Rect::from_xywh(bx + ins, by + ins, w, h).and_then(PathBuilder::from_oval)
        }
        VectorGeom::RoundedRect { radii, continuous } => {
            let (w, h) = (bw - 2.0 * ins, bh - 2.0 * ins);
            let inset_radii = radii.map(|(rx, ry)| ((rx - inset).max(0.0), (ry - inset).max(0.0)));
            uneven_rrect(bx + ins, by + ins, w, h, inset_radii, *continuous)
        }
        VectorGeom::Path { ops, mirror_x } => {
            let mut pb = PathBuilder::new();
            let px = |x: f64| if *mirror_x { bx + bw - x as f32 } else { bx + x as f32 };
            let py = |y: f64| by + y as f32;
            for op in ops {
                match *op {
                    PathOp::M([x, y]) => pb.move_to(px(x), py(y)),
                    PathOp::L([x, y]) => pb.line_to(px(x), py(y)),
                    PathOp::Q([cx, cy, x, y]) => pb.quad_to(px(cx), py(cy), px(x), py(y)),
                    PathOp::C([c1x, c1y, c2x, c2y, x, y]) => pb.cubic_to(px(c1x), py(c1y), px(c2x), py(c2y), px(x), py(y)),
                    PathOp::Z => pb.close(),
                }
            }
            pb.finish()
        }
        VectorGeom::ContainerRelative | VectorGeom::Concentric(_) => {
            uneven_rrect(bx + ins, by + ins, bw - 2.0 * ins, bh - 2.0 * ins, [(0.0, 0.0); 4], false)
        }
    }
}

fn to_skia_stroke(st: &VectorStroke) -> Stroke {
    let line_cap = match st.line_cap {
        ProtoLineCap::Butt => LineCap::Butt,
        ProtoLineCap::Round => LineCap::Round,
        ProtoLineCap::Square => LineCap::Square,
    };
    let line_join = match st.line_join {
        ProtoLineJoin::Miter => LineJoin::Miter,
        ProtoLineJoin::Round => LineJoin::Round,
        ProtoLineJoin::Bevel => LineJoin::Bevel,
    };
    let dash = if st.dash.is_empty() {
        None
    } else {
        let mut d: Vec<f32> = st.dash.iter().map(|&v| (v as f32).max(0.0)).collect();
        if d.len() % 2 == 1 {
            let copy = d.clone();
            d.extend(copy);
        }
        let sum: f32 = d.iter().sum();
        if sum > 0.0 {
            StrokeDash::new(d, st.dash_phase as f32)
        } else {
            None
        }
    };
    Stroke {
        width: st.line_width.max(0.0) as f32,
        miter_limit: st.miter_limit.max(1.0) as f32,
        line_cap,
        line_join,
        dash,
    }
}

fn draw_vector(pm: &mut Pixmap, v: &VectorShape, bounds: crate::scene::Rect, opacity: f32, xf: Transform, mask: Option<&Mask>) {
    if let Some(f) = v.fill {
        if let Some(p) = build_vector_path(&v.geom, bounds, 0.0) {
            let mut pt = paint(f.color, opacity);
            pt.anti_alias = f.antialiased;
            let rule = if f.eo_fill { FillRule::EvenOdd } else { FillRule::Winding };
            pm.fill_path(&p, &pt, rule, xf, mask);
        }
    }
    if let Some(st) = &v.stroke {
        if st.line_width > 0.0 {
            if let Some(p) = build_vector_path(&v.geom, bounds, st.inset) {
                let mut pt = paint(st.color, opacity);
                pt.anti_alias = st.antialiased;
                let sk = to_skia_stroke(st);
                pm.stroke_path(&p, &pt, &sk, xf, mask);
            }
        }
    }
}

/// `mask` (or the whole canvas) intersected with `path`, which is in points.
fn clip_mask(mask: Option<&Mask>, path: &Path, xf: Transform, pw: u32, ph: u32) -> Mask {
    let mut m = match mask {
        Some(m) => m.clone(),
        None => {
            let mut m = Mask::new(pw, ph).expect("mask");
            m.fill_path(&PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, pw as f32, ph as f32).unwrap()), FillRule::Winding, false, Transform::identity());
            m
        }
    };
    m.intersect_path(path, FillRule::Winding, true, xf);
    m
}

/// Linear gradient over the node's rounded bounds.
fn draw_gradient(pm: &mut Pixmap, n: &crate::scene::Node, g: &crate::scene::Gradient, s: f32, mask: Option<&Mask>) {
    let b = n.bounds;
    let Some(path) = rrect(b.x as f32, b.y as f32, b.w as f32, b.h as f32, n.radius as f32) else { return };
    let k = g.stops.len().saturating_sub(1).max(1) as f32;
    let stops: Vec<tiny_skia::GradientStop> = g
        .stops
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let at = g.locations.as_ref().and_then(|l| l.get(i)).copied().unwrap_or(i as f32 / k);
            tiny_skia::GradientStop::new(at, tiny_skia::Color::from_rgba(c.r, c.g, c.b, c.a * n.opacity).unwrap_or(tiny_skia::Color::BLACK))
        })
        .collect();
    let (from, to) = if let Some([x0, y0, x1, y1]) = g.points {
        // Unit-box points placed on the bounds; isolines stay perpendicular on screen (CAGradientLayer, measured on iOS 26.5).
        let at = |u: f32, v: f32| (b.x as f32 + u * b.w as f32, b.y as f32 + v * b.h as f32);
        (at(x0, y0), at(x1, y1))
    } else {
        let a = g.angle.to_radians();
        let (dx, dy) = (a.sin(), -a.cos());
        let (cx, cy) = ((b.x + b.w / 2.0) as f32, (b.y + b.h / 2.0) as f32);
        let half = (b.w as f32 * dx.abs() + b.h as f32 * dy.abs()) / 2.0;
        ((cx - dx * half, cy - dy * half), (cx + dx * half, cy + dy * half))
    };
    let Some(sh) = tiny_skia::LinearGradient::new(from.into(), to.into(), stops, tiny_skia::SpreadMode::Pad, Transform::identity()) else { return };
    let p = Paint { shader: sh, ..Paint::default() };
    pm.fill_path(&path, &p, FillRule::Winding, Transform::from_scale(s, s), mask);
}

/// Draw `im.dest` of the bitmap, clipped to the node's rounded bounds.
fn draw_image(pm: &mut Pixmap, n: &crate::scene::Node, im: &crate::scene::ImageRun, s: f32, mask: Option<&Mask>) {
    let (b, d) = (n.bounds, im.dest);
    let (iw, ih) = (im.pixmap.width() as f32, im.pixmap.height() as f32);
    let Some(clip) = rrect(b.x as f32, b.y as f32, b.w as f32, b.h as f32, n.radius as f32) else { return };
    let xf = Transform::from_scale(s, s);
    let m = clip_mask(mask, &clip, xf, pm.width(), pm.height());
    // A rotated bitmap may reach anywhere in the bounds.
    let d = if im.rotation != 0.0 { b } else { d };
    let (x0, y0) = (d.x.max(b.x) as f32, d.y.max(b.y) as f32);
    let (x1, y1) = ((d.x + d.w).min(b.x + b.w) as f32, (d.y + d.h).min(b.y + b.h) as f32);
    let Some(r) = Rect::from_ltrb(x0, y0, x1, y1) else { return };
    let mut p = Paint::default();
    let mut to_pt = Transform::from_row(d.w as f32 / iw, 0.0, 0.0, d.h as f32 / ih, d.x as f32, d.y as f32);
    if im.rotation != 0.0 {
        to_pt = to_pt.post_rotate_at(im.rotation, (d.x + d.w / 2.0) as f32, (d.y + d.h / 2.0) as f32);
    }
    let quality = match im.filter {
        0 => tiny_skia::FilterQuality::Nearest,
        1 => tiny_skia::FilterQuality::Bilinear,
        _ => tiny_skia::FilterQuality::Bicubic,
    };
    p.shader = tiny_skia::Pattern::new(im.pixmap.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, quality, n.opacity, to_pt);
    pm.fill_rect(r, &p, xf, Some(&m));
}

fn stroke_pts(pm: &mut Pixmap, pts: &[(f32, f32)], c: Color, w: f32, xf: Transform, mask: Option<&Mask>) {
    let mut b = PathBuilder::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        if i == 0 {
            b.move_to(*x, *y);
        } else {
            b.line_to(*x, *y);
        }
    }
    if let Some(p) = b.finish() {
        let st = Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        pm.stroke_path(&p, &paint(c, 1.0), &st, xf, mask);
    }
}

/// Spinner spokes as fractions of the box: inner radius, outer radius, width; alpha lost across the 8 spokes.
const SPINNER_SPOKE: (f32, f32, f32) = (0.24, 0.44, 0.115);
/// Large style (UIActivityIndicatorView `.large`, 33 pt box; measured on iOS 26.5).
const SPINNER_SPOKE_LARGE: (f32, f32, f32) = (0.242, 0.455, 0.142);
const SPINNER_FADE: f32 = 0.85;

/// Icons drawn inside `(x, y, w, h)` (points).
fn draw_glyph(pm: &mut Pixmap, g: Glyph, c: Color, b: (f32, f32, f32, f32), op: f32, xf: Transform, mask: Option<&Mask>) {
    let (x, y, w, h) = b;
    // Status bar glyphs are drawn at their 17 Pro size (20x4, 16x12, 27x13) and scaled to the box.
    let k = match g {
        Glyph::SignalDots | Glyph::SignalDots27 => w / 20.0,
        Glyph::Wifi | Glyph::Wifi27 => h / 12.0,
        Glyph::AndroidWifi => h / 11.8,
        Glyph::Battery | Glyph::Battery27 => h / 13.0,
        // Emoji / dictation inside the SE's short bottom-row keys (the 17 Pro's sit in a 44pt box).
        Glyph::Emoji | Glyph::Mic if h < 44.0 => h / 60.0,
        _ => 1.0,
    };
    if (k - 1.0).abs() > 1e-4 {
        return draw_glyph(pm, g, c, (0.0, 0.0, w / k, h / k), op, xf.pre_translate(x, y).pre_scale(k, k), mask);
    }
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let c = Color { a: c.a * op, ..c };
    match g {
        Glyph::Chevron => {
            let k = w / 7.0; // grows with Dynamic Type
            let i = 0.9 * k;
            stroke_pts(pm, &[(x + i, y + i), (x + w - i, cy), (x + i, y + h - i)], c, 1.9 * k, xf, mask)
        }
        Glyph::BackChevron => {
            stroke_pts(pm, &[(cx + 3.1, cy - 10.3), (cx - 7.5, cy), (cx + 3.1, cy + 10.3)], Color { a: c.a * 0.9, ..c }, 2.7, xf, mask)
        }
        Glyph::Minus => stroke_pts(pm, &[(cx - 6.5, cy), (cx + 6.5, cy)], c, 2.0, xf, mask),
        Glyph::Plus => {
            stroke_pts(pm, &[(cx - 6.5, cy), (cx + 6.5, cy)], c, 2.0, xf, mask);
            stroke_pts(pm, &[(cx, cy - 6.5), (cx, cy + 6.5)], c, 2.0, xf, mask);
        }
        Glyph::SignalDots => {
            for i in 0..4 {
                if let Some(p) = PathBuilder::from_circle(x + 1.85 + i as f32 * 5.4, cy, 1.85) {
                    pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
                }
            }
        }
        Glyph::Wifi => {
            let (bx, by) = (cx, y + h - 0.5);
            let arc = |r: f32| -> Vec<(f32, f32)> {
                (0..=14)
                    .map(|i| {
                        let a = (-45.0 + 90.0 * i as f32 / 14.0).to_radians();
                        (bx + r * a.sin(), by - r * a.cos())
                    })
                    .collect()
            };
            let mut wedge = PathBuilder::new();
            wedge.move_to(bx, by);
            for (px, py) in arc(3.6) {
                wedge.line_to(px, py);
            }
            wedge.close();
            if let Some(p) = wedge.finish() {
                pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
            }
            for r in [7.2, 10.8] {
                stroke_pts(pm, &arc(r), c, 2.0, xf, mask);
            }
        }
        Glyph::BatteryRing => {
            // Two arcs (r 19.2, 2.75pt, round caps) from 12 to 136 degrees either side of the top, bolt in the gap.
            let r = 19.2;
            let green = Color::rgba(52.0 / 255.0, 199.0 / 255.0, 89.0 / 255.0, c.a);
            for side in [-1.0f32, 1.0] {
                let mut pb = PathBuilder::new();
                for i in 0..=24 {
                    let a = (12.0 + 124.0 * i as f32 / 24.0).to_radians() * side;
                    let (px, py) = (cx + r * a.sin(), cy - r * a.cos());
                    if i == 0 {
                        pb.move_to(px, py)
                    } else {
                        pb.line_to(px, py)
                    }
                }
                if let Some(p) = pb.finish() {
                    let st = Stroke {
                        width: 2.75,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    };
                    pm.stroke_path(&p, &paint(green, 1.0), &st, xf, mask);
                }
            }
            let mut bolt = PathBuilder::new();
            for (i, (bx, by)) in [
                (0.6, -4.4),
                (2.2, -4.4),
                (1.0, -0.6),
                (2.6, -0.4),
                (-0.8, 4.4),
                (-0.2, 0.6),
                (-2.4, 0.4),
            ]
            .iter()
            .enumerate()
            {
                let (px, py) = (cx + bx, cy - r + by);
                if i == 0 {
                    bolt.move_to(px, py)
                } else {
                    bolt.line_to(px, py)
                }
            }
            bolt.close();
            if let Some(p) = bolt.finish() {
                pm.fill_path(
                    &p,
                    &paint(Color::rgba(0.5, 0.55, 0.52, c.a), 1.0),
                    FillRule::Winding,
                    xf,
                    mask,
                );
            }
        }
        Glyph::DotsArc => {
            for (dx, dy) in [(2.4, 2.25), (8.6, 4.75), (15.5, 4.75), (21.9, 2.0)] {
                if let Some(p) = PathBuilder::from_circle(x + dx, y + dy, 1.8) {
                    pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
                }
            }
        }
        Glyph::SignalDots27 => {
            // Smaller dots on iOS 27 (3pt at the 18 Pro's 1.1 scale, 5.67pt apart), sitting lower.
            for i in 0..4 {
                if let Some(p) =
                    PathBuilder::from_circle(x + 2.7 + i as f32 * 5.15, cy + 0.75, 1.36)
                {
                    pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
                }
            }
        }
        Glyph::Wifi27 => {
            // Measured on the iOS 27.2 18 Pro (home.png): r 3.4 / 6.8 / 11.4 at its 1.1 scale, 2.4pt arcs, round caps.
            let (bx, by) = (cx + 1.2, y + h + 0.15);
            let arc = |r: f32| -> Vec<(f32, f32)> {
                (0..=16)
                    .map(|i| {
                        let a = (-43.0 + 86.0 * i as f32 / 16.0).to_radians();
                        (bx + r * a.sin(), by - r * a.cos())
                    })
                    .collect()
            };
            let mut wedge = PathBuilder::new();
            wedge.move_to(bx, by);
            for (px, py) in arc(3.4) {
                wedge.line_to(px, py);
            }
            wedge.close();
            if let Some(p) = wedge.finish() {
                pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
                let st = Stroke {
                    width: 0.8,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                pm.stroke_path(&p, &paint(c, 1.0), &st, xf, mask);
            }
            for r in [6.8, 11.2] {
                let mut pb = PathBuilder::new();
                for (i, (px, py)) in arc(r).into_iter().enumerate() {
                    if i == 0 {
                        pb.move_to(px, py)
                    } else {
                        pb.line_to(px, py)
                    }
                }
                if let Some(p) = pb.finish() {
                    let st = Stroke {
                        width: 2.4,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    };
                    pm.stroke_path(&p, &paint(c, 1.0), &st, xf, mask);
                }
            }
        }
        Glyph::Battery27 => {
            // Solid green body with a round nub and the charging bolt (iOS 27.2 home.png).
            let green = Color::rgba(52.0 / 255.0, 199.0 / 255.0, 89.0 / 255.0, c.a);
            if let Some(p) = rrect(x + 0.9, y - 0.1, 23.9, h - 0.6, 3.9) {
                pm.fill_path(&p, &paint(green, 1.0), FillRule::Winding, xf, mask);
            }
            if let Some(p) = rrect(x + 24.0, cy - 2.4, 2.9, 4.2, 1.3) {
                pm.fill_path(&p, &paint(green, 1.0), FillRule::Winding, xf, mask);
            }
            let mut bolt = PathBuilder::new();
            for (i, (bx, by)) in [
                (12.6, 0.7),
                (15.9, 0.7),
                (14.8, 4.6),
                (17.3, 5.3),
                (11.5, 12.6),
                (12.2, 8.3),
                (7.6, 7.3),
            ]
            .iter()
            .enumerate()
            {
                if i == 0 {
                    bolt.move_to(x + bx, y + by);
                } else {
                    bolt.line_to(x + bx, y + by);
                }
            }
            bolt.close();
            if let Some(p) = bolt.finish() {
                // Light: a gray-green bolt; dark (white glyph color): neutral gray (keyboard-dark.png).
                let fill = if c.r > 0.5 {
                    Color::rgba(0.5, 0.5, 0.5, c.a)
                } else {
                    Color::rgba(156.0 / 255.0, 178.0 / 255.0, 164.0 / 255.0, c.a)
                };
                pm.fill_path(&p, &paint(fill, 1.0), FillRule::Winding, xf, mask);
                let st = Stroke {
                    width: 0.9,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                pm.stroke_path(
                    &p,
                    &paint(
                        Color::rgba(205.0 / 255.0, 234.0 / 255.0, 216.0 / 255.0, c.a),
                        1.0,
                    ),
                    &st,
                    xf,
                    mask);
            }
        }
        Glyph::Shift | Glyph::ShiftFilled | Glyph::Backspace | Glyph::Return | Glyph::Emoji | Glyph::Mic => {
            crate::chrome::draw_glyph(pm, g, c, b, xf, mask);
        }
        Glyph::ArrowDown | Glyph::ArrowUp | Glyph::ArrowLeft => {
            // Popover arrow: a triangle with a softened tip filling the box, base on the card's edge.
            let mut pb = PathBuilder::new();
            match g {
                Glyph::ArrowDown => {
                    pb.move_to(x, y);
                    pb.line_to(x + w, y);
                    pb.line_to(cx + 1.5, y + h - 1.0);
                    pb.line_to(cx - 1.5, y + h - 1.0);
                }
                Glyph::ArrowUp => {
                    pb.move_to(x, y + h);
                    pb.line_to(x + w, y + h);
                    pb.line_to(cx + 1.5, y + 1.0);
                    pb.line_to(cx - 1.5, y + 1.0);
                }
                _ => {
                    pb.move_to(x + w, y);
                    pb.line_to(x + w, y + h);
                    pb.line_to(x + 1.0, cy + 1.5);
                    pb.line_to(x + 1.0, cy - 1.5);
                }
            }
            pb.close();
            if let Some(p) = pb.finish() {
                // The card's soft shadow follows the notch: a few widening faint strokes stand in for a blur.
                for k in 1..=5 {
                    let st = Stroke { width: k as f32 * 2.4, line_join: LineJoin::Round, ..Stroke::default() };
                    pm.stroke_path(&p, &paint(Color::rgba(0.0, 0.0, 0.0, 0.016), 1.0), &st, xf, mask);
                }
                pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
            }
        }
        Glyph::AndroidWifi => {
            // Measured on the Android 17 emulator (Pixel 10a): arcs of radius 4.9 and 9.45 dp, +-50 deg from
            // vertical, 1.7 dp round-capped strokes, around a 1.45 dp dot.
            let (ox, oy) = (x + w / 2.0, y + 10.5);
            for r in [4.9f32, 9.45] {
                let pts: Vec<(f32, f32)> = (0..=16)
                    .map(|i| (-50.0 + 100.0 * i as f32 / 16.0).to_radians())
                    .map(|a| (ox + r * a.sin(), oy - r * a.cos()))
                    .collect();
                stroke_pts(pm, &pts, c, 1.7, xf, mask);
            }
            if let Some(p) = PathBuilder::from_circle(ox, oy, 1.45) {
                pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
            }
        }
        Glyph::AndroidSpinner => {
            let size = w.min(h);
            let width = size / 10.0;
            let radius = (size - width) / 2.0;
            let points: Vec<_> = (0..=24)
                .map(|i| (270.0 * i as f32 / 24.0).to_radians())
                .map(|a| (cx + radius * a.sin(), cy - radius * a.cos()))
                .collect();
            stroke_pts(pm, &points, c, width, xf, mask);
        }
        Glyph::Arc { from, to, width } => {
            let sw = width as f32 / 100.0;
            let r = (w.min(h) - sw) / 2.0;
            let (a0, a1) = (from as f32, to as f32);
            let steps = (((a1 - a0).abs() / 3.0).ceil() as usize).max(2);
            let pts: Vec<_> = (0..=steps)
                .map(|i| (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians())
                .map(|a| (cx + r * a.sin(), cy - r * a.cos()))
                .collect();
            stroke_pts(pm, &pts, c, sw, xf, mask);
        }
        Glyph::Spinner | Glyph::SpinnerLarge => {
            // 8 rounded spokes, the leading one opaque, the rest fading (the indicator at rest).
            let s = w.min(h);
            let k = if g == Glyph::Spinner { SPINNER_SPOKE } else { SPINNER_SPOKE_LARGE };
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let (dx, dy) = (a.sin(), -a.cos());
                let (r0, r1) = (k.0 * s, k.1 * s);
                let alpha = 1.0 - (i as f32 / 8.0) * SPINNER_FADE;
                stroke_pts(pm, &[(cx + dx * r0, cy + dy * r0), (cx + dx * r1, cy + dy * r1)], Color { a: c.a * alpha, ..c }, k.2 * s, xf, mask);
            }
        }
        Glyph::Battery => {
            let col = |a: f32| Color { a: c.a * a, ..c };
            if let Some(p) = rrect(x + 0.5, y + 0.5, 24.0, h - 1.0, 3.8) {
                let st = Stroke { width: 1.0, ..Stroke::default() };
                pm.stroke_path(&p, &paint(col(0.35), 1.0), &st, xf, mask);
            }
            if let Some(p) = rrect(x + 2.0, y + 2.0, 21.0, h - 4.0, 2.4) {
                pm.fill_path(&p, &paint(Color::rgba(52.0 / 255.0, 199.0 / 255.0, 89.0 / 255.0, c.a), 1.0), FillRule::Winding, xf, mask);
            }
            if let Some(r) = Rect::from_xywh(x + 25.2, y + 4.3, 1.5, h - 8.6) {
                pm.fill_rect(r, &paint(col(0.4), 1.0), xf, mask);
            }
            // Charging bolt (iOS shows it on the simulator's battery): translucent fill, light outline.
            let mut bolt = PathBuilder::new();
            for (i, (bx, by)) in [(12.3, 0.2), (15.6, 0.2), (14.5, 4.2), (17.0, 4.9), (11.2, 12.8), (11.9, 8.1), (7.3, 7.0)].iter().enumerate() {
                if i == 0 {
                    bolt.move_to(x + bx, y + by);
                } else {
                    bolt.line_to(x + bx, y + by);
                }
            }
            bolt.close();
            if let Some(p) = bolt.finish() {
                pm.fill_path(&p, &paint(Color::rgba(0.58, 0.71, 0.62, c.a), 1.0), FillRule::Winding, xf, mask);
                let st = Stroke { width: 0.8, line_join: LineJoin::Round, ..Stroke::default() };
                pm.stroke_path(&p, &paint(Color::rgba(0.73, 0.91, 0.80, c.a), 1.0), &st, xf, mask);
            }
        }
    }
}

/// Tint an SF Symbol's alpha mask with `sy.color`, centred in `b` (points) and cropped to it.
/// Returns the tile and its device-pixel origin.
fn symbol_tile(sy: &SymbolRun, b: (f32, f32, f32, f32), op: f32, s: f32) -> Option<(i32, i32, Pixmap)> {
    let (wpx, hpx) = ((sy.size.0 as f32 * s).round().max(1.0) as u32, (sy.size.1 as f32 * s).round().max(1.0) as u32);
    let tint = [(sy.color.r * 255.0).round() as u8, (sy.color.g * 255.0).round() as u8, (sy.color.b * 255.0).round() as u8, (sy.color.a * 255.0).round() as u8];
    let m = symbols::mask(&sy.spec, wpx, hpx, s as f64, tint)?;
    let (bx, by) = ((b.0 * s).round() as i32, (b.1 * s).round() as i32);
    let (bw, bh) = ((b.2 * s).round() as i32, (b.3 * s).round() as i32);
    let (x0, y0) = (bx + (bw - wpx as i32) / 2, by + (bh - hpx as i32) / 2);
    let mut tmp = Pixmap::new(wpx, hpx)?;
    let c = sy.color;
    let k = c.a * op;
    let (r, g, bl) = (c.r * 255.0, c.g * 255.0, c.b * 255.0);
    let colored = !m.rgba.is_empty();
    for (i, px) in tmp.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (x0 + (i as u32 % wpx) as i32, y0 + (i as u32 / wpx) as i32);
        if x < bx || y < by || x >= bx + bw || y >= by + bh {
            continue;
        }
        if colored {
            let o = i * 4;
            let a0 = m.rgba[o + 3] as f32;
            let al = (a0 * k).round() as u8;
            let scale = if a0 == 0.0 { 0.0 } else { al as f32 / a0 };
            let ch = |c: u8| (c as f32 * scale).round() as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(ch(m.rgba[o]), ch(m.rgba[o + 1]), ch(m.rgba[o + 2]), al).unwrap_or(*px);
        } else {
            let a = m.alpha[i] as f32 / 255.0 * k;
            let al = (a * 255.0).round() as u8;
            let ch = |v: f32| ((v * a).round() as u8).min(al);
            *px = tiny_skia::PremultipliedColorU8::from_rgba(ch(r), ch(g), ch(bl), al).unwrap_or(*px);
        }
    }
    Some((x0, y0, tmp))
}

fn draw_symbol(pm: &mut Pixmap, sy: &SymbolRun, b: (f32, f32, f32, f32), op: f32, s: f32, mask: Option<&Mask>) {
    if let Some((x0, y0, tmp)) = symbol_tile(sy, b, op, s) {
        pm.draw_pixmap(x0, y0, tmp.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
    }
}

/// Separable gaussian blur of an `f32` coverage plane (exact kernel, truncated at 3 sigma).
pub(crate) fn gaussian(plane: &mut [f32], w: usize, h: usize, sigma: f32, replicate: bool) {
    let (cols, rows) = (0..w, 0..h);
    let r = (sigma * 3.0).ceil() as usize;
    let mut k: Vec<f32> = (-(r as isize)..=r as isize).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    // Output `i` of an `n`-sample line whose kernel overhangs an end: `replicate` clamps to the edge
    // sample (backdrops); otherwise outside counts as zero (coverage).
    let edge = |n: usize, i: usize, at: &dyn Fn(usize) -> f32| {
        let mut acc = 0.0f32;
        for (ki, kv) in k.iter().enumerate() {
            let j = (i + ki) as isize - r as isize;
            if replicate {
                acc += kv * at(j.clamp(0, n as isize - 1) as usize);
            } else if (0..n as isize).contains(&j) {
                acc += kv * at(j as usize);
            }
        }
        acc
    };
    // Outputs whose kernel fits inside the line.
    let inner = |n: usize, o: &Range<usize>| {
        let a = o.start.max(r);
        a..o.end.min(n.saturating_sub(r)).max(a)
    };
    // `out[i] = sum over taps of k * src[i + tap]`, taps outer so the inner loop vectorizes. Each
    // output still adds its taps in kernel order, so the result is bit-identical to a per-output loop.
    fn convolve<'a>(k: &[f32], out: &mut [f32], src: impl Fn(usize) -> &'a [f32]) {
        out.fill(0.0);
        for (ki, kv) in k.iter().enumerate() {
            for (o, v) in out.iter_mut().zip(src(ki)) {
                *o += kv * v;
            }
        }
    }
    // Rows, then columns.
    let mut tmp = vec![0f32; plane.len()];
    let hi = inner(w, &cols);
    for y in 0..h {
        let src = &plane[y * w..(y + 1) * w];
        let dst = &mut tmp[y * w..(y + 1) * w];
        for i in cols.clone().filter(|i| !hi.contains(i)) {
            dst[i] = edge(w, i, &|j| src[j]);
        }
        convolve(&k, &mut dst[hi.clone()], |ki| &src[hi.start + ki - r..hi.end + ki - r]);
    }
    let vi = inner(h, &rows);
    for i in rows {
        if vi.contains(&i) {
            let c = cols.clone();
            convolve(&k, &mut plane[i * w + c.start..i * w + c.end], |ki| &tmp[(i + ki - r) * w + c.start..(i + ki - r) * w + c.end]);
        } else {
            for c in cols.clone() {
                plane[i * w + c] = edge(h, i, &|j| tmp[j * w + c]);
            }
        }
    }
}

type ShadowKey = (String, [u32; 7], [u8; 4]);

thread_local! {
    /// Blurred shadow bitmaps keyed by node id, geometry (size, radius, sigma, scale, sub-pixel
    /// phase) and color, so repeated screenshots and unchanged nodes skip the blur.
    static SHADOWS: RefCell<HashMap<ShadowKey, Rc<Pixmap>>> = RefCell::new(HashMap::new());
}

/// A shadow in device pixels: the shape's `x, y, w, h`, its corner radius, gaussian sigma, offset and straight RGBA.
struct DevShadow {
    b: (f32, f32, f32, f32),
    radius: f32,
    sigma: f32,
    dx: f32,
    dy: f32,
    rgba: [u8; 4],
}

/// SwiftUI `shadow(radius:)` is a gaussian with sigma = radius (fitted to simulator renders).
fn draw_shadow(pm: &mut Pixmap, n: &crate::scene::Node, sh: &Shadow, b: (f32, f32, f32, f32), s: f32, mask: Option<&Mask>) {
    let (id, op, radius) = (n.id.as_str(), n.opacity, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32);
    let rgba = [sh.color.r, sh.color.g, sh.color.b, sh.color.a * op].map(|v| (v * 255.0).round() as u8);
    let dev = DevShadow { b: (b.0 * s, b.1 * s, b.2 * s, b.3 * s), radius: radius * s, sigma: (sh.radius as f32 * s).max(0.5), dx: sh.dx as f32 * s, dy: sh.dy as f32 * s, rgba };
    draw_shadow_dev(pm, id, &dev, mask);
}

fn draw_shadow_dev(pm: &mut Pixmap, id: &str, sh: &DevShadow, mask: Option<&Mask>) {
    let DevShadow { b, radius, sigma, rgba, .. } = *sh;
    let pad = (sigma * 3.0).ceil() + 2.0;
    let (px, py) = (b.0 + sh.dx, b.1 + sh.dy);
    let (fx, fy) = (px - px.floor(), py - py.floor());
    let key = (id.to_string(), [b.2.to_bits(), b.3.to_bits(), radius.to_bits(), sigma.to_bits(), fx.to_bits(), fy.to_bits(), 0], rgba);
    let img = SHADOWS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 256 {
            c.clear();
        }
        c.entry(key)
            .or_insert_with(|| {
                let (w, h) = ((b.2 + 2.0 * pad).ceil() as u32 + 1, (b.3 + 2.0 * pad).ceil() as u32 + 1);
                let mut cov = Pixmap::new(w.max(1), h.max(1)).expect("shadow size");
                if let Some(p) = rrect(pad + fx, pad + fy, b.2, b.3, radius) {
                    let mut white = Paint::default();
                    white.set_color_rgba8(255, 255, 255, 255);
                    cov.fill_path(&p, &white, FillRule::Winding, Transform::identity(), None);
                }
                let (w, h) = (cov.width() as usize, cov.height() as usize);
                let mut plane: Vec<f32> = cov.data().chunks(4).map(|px| px[3] as f32 / 255.0).collect();
                gaussian(&mut plane, w, h, sigma, false);
                let a = rgba[3] as f32 / 255.0;
                for (px, v) in cov.data_mut().chunks_mut(4).zip(&plane) {
                    let al = v * a;
                    px[0] = (rgba[0] as f32 * al).round() as u8;
                    px[1] = (rgba[1] as f32 * al).round() as u8;
                    px[2] = (rgba[2] as f32 * al).round() as u8;
                    px[3] = (al * 255.0).round() as u8;
                }
                Rc::new(cov)
            })
            .clone()
    });
    pm.draw_pixmap((px.floor() - pad) as i32, (py.floor() - pad) as i32, img.as_ref().as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
}

/// Source-over a solid color onto an integer pixel rect, bit-identical to tiny-skia's non-AA
/// `fill_rect` (lowp pipeline: uniform_color, mask_u8, source_over) but without compiling a
/// pipeline per call: cosmic-text calls back once per glyph pixel, which made that the bulk of a frame.
fn blend_rect(pm: &mut Pixmap, (x, y, w, h): (i32, i32, u32, u32), c: [u8; 4], mask: Option<&Mask>) {
    if c[3] == 0 {
        return; // most of a glyph's box
    }
    let pc = tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]).premultiply();
    let src = [pc.red(), pc.green(), pc.blue(), pc.alpha()].map(|v| (v * 255.0 + 0.5) as u16);
    if src[3] == 0 {
        return;
    }
    let div255 = |v: u16| (v + 255) >> 8;
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w as i32).min(pw), (y + h as i32).min(ph));
    let data = pm.data_mut();
    for py in y0..y1 {
        for px in x0..x1 {
            let mut s = src;
            if let Some(m) = mask {
                let k = m.data()[(py * m.width() as i32 + px) as usize] as u16;
                s = s.map(|v| div255(v * k));
            }
            let i = (py * pw + px) as usize * 4;
            let inv = 255 - s[3];
            for (ch, sv) in s.iter().enumerate() {
                data[i + ch] = (sv + div255(data[i + ch] as u16 * inv)) as u8;
            }
        }
    }
}

fn paint(c: Color, opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
        (c.a * opacity * 255.0).round() as u8,
    );
    p
}

/// One paint operation of a text node, in device pixels.
enum TextOp {
    /// Glyph pixel run blended with straight-alpha `c`.
    Blend(i32, i32, u32, u32, [u8; 4]),
    /// Underline / strikethrough rule (non-antialiased).
    Rule(f32, f32, f32, f32, [u8; 4]),
}

fn apply_text_op(pm: &mut Pixmap, op: &TextOp, (dx, dy): (i32, i32), mask: Option<&Mask>) {
    match *op {
        TextOp::Blend(x, y, w, h, c) => blend_rect(pm, (x - dx, y - dy, w, h), c, mask),
        TextOp::Rule(x, y, w, h, col) => {
            if let Some(r) = Rect::from_xywh(x - dx as f32, y - dy as f32, w, h) {
                let mut p = Paint::default();
                p.set_color_rgba8(col[0], col[1], col[2], col[3]);
                p.anti_alias = false;
                pm.fill_rect(r, &p, Transform::identity(), mask);
            }
        }
    }
}

/// Solid, dotted, dashed, dash-dot, or dash-dot-dot rule. Lengths are device pixels.
fn emit_rule(emit: &mut impl FnMut(TextOp), x: f32, y: f32, w: f32, th: f32, col: [u8; 4], pat: u8) {
    if w <= 0.0 {
        return;
    }
    if pat == 0 {
        emit(TextOp::Rule(x, y, w, th, col));
        return;
    }
    let (dash, gap, dot) = (th * 3.0, th * 2.0, th.max(1.0));
    let unit: &[f32] = match pat {
        1 => &[dot, gap],
        2 => &[dash, gap],
        3 => &[dash, gap, dot, gap],
        _ => &[dash, gap, dot, gap, dot, gap],
    };
    let mut at = x;
    let end = x + w;
    let mut i = 0;
    while at < end - 0.25 {
        let mark = i % 2 == 0;
        let len = unit[i % unit.len()].min(end - at);
        if mark && len > 0.0 {
            emit(TextOp::Rule(at, y, len, th, col));
        }
        at += len;
        i += 1;
        if i > 4096 {
            break;
        }
    }
}

/// Lay out and paint a text node as device-pixel operations (`emit` receives them in paint order).
fn paint_text(n: &crate::scene::Node, t: &crate::scene::TextRun, fonts: &mut Fonts, s: f32, mut emit: impl FnMut(TextOp)) {
    let b = n.bounds;
    let (bx, by, bw) = (b.x as f32, b.y as f32, b.w as f32);
    let rgba = [
        (t.color.r * 255.0).round() as u8,
        (t.color.g * 255.0).round() as u8,
        (t.color.b * 255.0).round() as u8,
        (t.color.a * n.opacity * 255.0).round() as u8,
    ];
    // Wrapped text uses a larger pitch than the single-line box; keep line 1 where it is.
    // Accessibility sizes: iOS sets the glyphs ~0.01 em higher in the line box (measured).
    let (lift, ldx) = match (t.font.size > 40.0, fonts.is_sf()) {
        (false, _) => (0.0, 0.0),
        (true, true) => (0.0105 * t.font.size, 0.0),
        // Inter fallback at accessibility sizes: baseline and side bearing differ from SF.
        (true, false) => (-0.03 * t.font.size, -0.775),
    };
    let (ox, oy) = ((bx + t.offset_x + ldx) * s, (by - (t.font.pitch - t.font.line_height).max(0.0) / 2.0 - lift) * s);
    let spec = FontSpec { size: t.font.size * s, line_height: t.font.line_height * s, pitch: t.font.pitch * s, ..t.font };
    // +2px slack so rounding differences vs layout never re-wrap.
    // Styled runs carry their own color (alpha included); only the node opacity is applied on top.
    let base_a = if t.spans.is_empty() { rgba[3] as u32 } else { (n.opacity * 255.0).round() as u32 };
    let spans = fonts.draw(&t.text, &spec, bw * s + 2.0, rgba, (ox, oy), &t.spans, |x, y, w, h, c| {
        // cosmic-text replaces the colour alpha with glyph coverage; reapply ours.
        emit(TextOp::Blend(x, y, w, h, [c[0], c[1], c[2], (c[3] as u32 * base_a / 255) as u8]));
    });
    // Strikethrough / underline: per line, or per styled run (in the run's own color and size).
    for sp in spans {
        let run = sp.run.and_then(|i| t.spans.get(i));
        let strike = t.strike || run.is_some_and(|r| r.strike);
        let under = t.font.underline || run.is_some_and(|r| r.underline);
        let size = run.map_or(spec.size, |r| spec.size * r.pt / t.font.pt.max(0.01));
        let col = run.map_or(rgba, |r| [(r.color.r * 255.0).round() as u8, (r.color.g * 255.0).round() as u8, (r.color.b * 255.0).round() as u8, (r.color.a * n.opacity * 255.0).round() as u8]);
        let paint_col = |over: Option<crate::scene::Color>| {
            over.map_or(col, |c| [(c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8, (c.a * n.opacity * 255.0).round() as u8])
        };
        let rules = [
            (strike, 0.27f32, run.map_or(t.font.strike_pat, |r| r.strike_pat), run.and_then(|r| r.strike_color).or(t.font.strike_color)),
            (under, -0.11, run.map_or(t.font.line_pat, |r| r.line_pat), run.and_then(|r| r.under_color).or(t.font.under_color)),
        ];
        for (on, drop, pat, over) in rules {
            if !on {
                continue;
            }
            let th = (size * 0.065).max(1.0);
            // `Fonts::draw` already folded baselineOffset into `sp.baseline`.
            let y = oy + sp.baseline - size * drop - th / 2.0;
            emit_rule(&mut emit, ox + sp.x0, y, (sp.x1 - sp.x0).max(0.0), th, paint_col(over), pat);
        }
    }
}

/// Liquid Glass halo + glass, own shadow and scroll-edge effect of `n`: everything in the paint order
/// that reads or blends against the backdrop already in `pm`.
fn paint_backdrop(pm: &mut Pixmap, n: &crate::scene::Node, dark: bool, ios: IosVersion, s: f32, mask: Option<&Mask>) {
    let b = n.bounds;
    let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    let sh = (bx, by, bw, bh);
    // Liquid Glass: snapshot the backdrop before our own halo lands on it.
    let glass = n.glass.as_ref().filter(|g| g.variant != crate::glass::GlassVariant::Identity).map(|g| {
        let p = crate::glass::spec_params(g, dark, ios);
        let r = (n.radius as f32).min(bw / 2.0).min(bh / 2.0) * s;
        let shape = crate::glass::Shape { x: bx * s, y: by * s, w: bw * s, h: bh * s, r };
        (g, p, shape, crate::glass::snapshot(pm, shape, s, &p))
    });
    if let Some((_, p, ..)) = &glass {
        let sd = Shadow { color: Color::rgba(0.0, 0.0, 0.0, p.sh_a), radius: p.sh_sigma as f64, dx: 0.0, dy: p.sh_dy as f64 };
        if p.sh_a > 0.0 {
            draw_shadow(pm, n, &sd, sh, s, mask);
        }
    }
    if let Some((g, _, shape, snap)) = glass {
        crate::glass::draw(pm, snap, shape, g, s, dark, ios, n.opacity, mask);
    }
    if let Some(sd) = &n.shadow {
        draw_shadow(pm, n, sd, sh, s, mask);
    }
    if let Some(e) = &n.edge {
        crate::glass::draw_edge(pm, e, s, ios, mask);
    }
}

/// The clip mask `n` imposes on its descendants, on top of `mask` (its own).
/// A square clip covering the whole canvas clips nothing: skip the full-size mask (and the
/// slower masked blending under it). Full scrollers are exactly that.
fn child_mask(n: &crate::scene::Node, mask: Option<Rc<Mask>>, s: f32, pw: u32, ph: u32) -> Option<Rc<Mask>> {
    let b = n.bounds;
    let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    match clip_radius(n, s, pw, ph).and_then(|r| rrect(bx, by, bw, bh, r as f32)) {
        Some(path) => Some(Rc::new(clip_mask(mask.as_deref(), &path, Transform::from_scale(s, s), pw, ph))),
        None => mask,
    }
}

/// Corner radius (points) of the clip `n` applies to its descendants on a `pw` x `ph` canvas at
/// `scale`; `None` when it clips nothing (no clip, or a square clip covering the whole canvas).
pub fn clip_radius(n: &crate::scene::Node, s: f32, pw: u32, ph: u32) -> Option<f64> {
    let b = n.bounds;
    let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    let covers = n.clip_radius == Some(0.0) && bx * s <= 0.0 && by * s <= 0.0 && (bx + bw) * s >= pw as f32 && (by + bh) * s >= ph as f32;
    n.clip_radius.filter(|_| !covers)
}

/// The clip rounded rects (device px `[x, y, w, h, radius]`, outermost first) that apply to node `idx`: its ancestors'
/// clips, exactly those [`apply_backdrop`] intersects into its mask.
pub fn clip_chain(scene: &Scene, idx: usize, scale: f64) -> Vec<[f32; 5]> {
    let s = scale as f32;
    let (pw, ph) = ((scene.size.0 * scale).round().max(1.0) as u32, (scene.size.1 * scale).round().max(1.0) as u32);
    let mut chain = vec![];
    let mut at = scene.nodes[idx].parent;
    while let Some(i) = at {
        let n = &scene.nodes[i];
        if let Some(r) = clip_radius(n, s, pw, ph) {
            let b = n.bounds;
            let (w, h) = (b.w as f32 * s, b.h as f32 * s);
            if rrect(b.x as f32, b.y as f32, b.w as f32, b.h as f32, r as f32).is_some() {
                chain.push([b.x as f32 * s, b.y as f32 * s, w, h, (r as f32 * s).min(w / 2.0).min(h / 2.0).max(0.0)]);
            }
        }
        at = n.parent;
    }
    chain.reverse();
    chain
}

/// True when `n` paints by reading or blending against what is already in the frame (Liquid Glass, scroll edge),
/// so a GPU backend must composite it on the CPU.
pub fn needs_backdrop(n: &crate::scene::Node) -> bool {
    n.glass.as_ref().is_some_and(|g| g.variant != crate::glass::GlassVariant::Identity) || n.edge.is_some()
}

/// Paint the backdrop-dependent parts of node `idx` (see [`needs_backdrop`]: halo, glass, own shadow,
/// scroll edge, in that order) into `pm`, which holds every earlier node, clipped by its ancestors exactly as [`render`] does.
pub fn apply_backdrop(pm: &mut Pixmap, scene: &Scene, idx: usize, scale: f64) {
    let s = scale as f32;
    let (pw, ph) = (pm.width(), pm.height());
    let mut chain = vec![];
    let mut at = scene.nodes[idx].parent;
    while let Some(i) = at {
        chain.push(i);
        at = scene.nodes[i].parent;
    }
    let mask = chain.iter().rev().fold(None, |m, &i| child_mask(&scene.nodes[i], m, s, pw, ph));
    let dark = scene.appearance == onesim_protocol::Appearance::Dark;
    paint_backdrop(pm, &scene.nodes[idx], dark, scene.ios, s, mask.as_deref());
}

/// Paint the backdrop display-list op `op` ([`crate::display::Op::is_backdrop`]) into `pm`, which holds everything
/// painted before it, clipped by `clips` (outermost first). Everything is in device pixels at `scale` px per point.
/// This is the CPU reference for the GPU executor's backdrop passes and its fallback.
pub fn apply_backdrop_op(pm: &mut Pixmap, op: &crate::display::Op, clips: &[crate::display::RRect], scale: f32, dark: bool, ios: IosVersion) {
    use crate::display::Op;
    let (pw, ph) = (pm.width(), pm.height());
    let mask = clips.iter().fold(None, |m: Option<Mask>, c| match rrect(c.x, c.y, c.w, c.h, c.r) {
        Some(p) => Some(clip_mask(m.as_ref(), &p, Transform::identity(), pw, ph)),
        None => m,
    });
    let mask = mask.as_ref();
    let rgba = |c: Color| [c.r, c.g, c.b, c.a].map(|v| (v * 255.0).round() as u8);
    match op {
        Op::Glass { shape, spec, opacity, halo_radius } => {
            if spec.variant == crate::glass::GlassVariant::Identity {
                return;
            }
            let p = crate::glass::spec_params(spec, dark, ios);
            let r = shape.r.min(shape.w / 2.0).min(shape.h / 2.0).max(0.0);
            let sh = crate::glass::Shape { x: shape.x, y: shape.y, w: shape.w, h: shape.h, r };
            // Snapshot the backdrop before our own halo lands on it.
            let snap = crate::glass::snapshot(pm, sh, scale, &p);
            if p.sh_a > 0.0 {
                let halo = Color::rgba(0.0, 0.0, 0.0, p.sh_a * opacity);
                let dev = DevShadow { b: (sh.x, sh.y, sh.w, sh.h), radius: *halo_radius, sigma: (p.sh_sigma * scale).max(0.5), dx: 0.0, dy: p.sh_dy * scale, rgba: rgba(halo) };
                draw_shadow_dev(pm, "", &dev, mask);
            }
            crate::glass::draw(pm, snap, sh, spec, scale, dark, ios, *opacity, mask);
        }
        Op::BackdropShadow { shape, color, sigma, dx, dy } => {
            let dev = DevShadow { b: (shape.x, shape.y, shape.w, shape.h), radius: shape.r, sigma: sigma.max(0.5), dx: *dx, dy: *dy, rgba: rgba(*color) };
            draw_shadow_dev(pm, "", &dev, mask);
        }
        Op::Edge(e) => crate::glass::draw_edge(pm, e, scale, ios, mask),
        _ => {}
    }
}

/// Which part of a node's content [`node_sprite`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteLayer {
    /// Rich vector shape (painted alongside fill/stroke, before icons/image/text).
    Shape,
    /// Vector glyph icon and SF Symbol (painted before the node's image).
    Icons,
    /// Text with underline / strikethrough (painted last).
    Text,
}

/// A CPU-drawn piece of one node, unclipped, premultiplied, at device-pixel position `(x, y)`.
pub struct Sprite {
    pub x: i32,
    pub y: i32,
    pub pixmap: Pixmap,
}

/// Cache identity of a node's sprite: equal `hash` and the same `anchor`-relative offsets mean
/// equal pixels, so a sprite from [`node_sprite`] can be reused after the node moves by whole device pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteKey {
    pub hash: u64,
    /// Device-pixel origin the sprite position is relative to.
    pub anchor: (i32, i32),
}

/// Identity of the `layer` sprite of `n` at `scale`, or `None` if the node has none.
pub fn sprite_key(n: &crate::scene::Node, fonts: &Fonts, scale: f64, layer: SpriteLayer) -> Option<SpriteKey> {
    use std::hash::{Hash, Hasher};
    let has = match layer {
        SpriteLayer::Shape => n.vector.is_some(),
        SpriteLayer::Icons => n.glyph.is_some() || n.symbol.is_some(),
        SpriteLayer::Text => n.text.is_some(),
    };
    if !has {
        return None;
    }
    let s = scale as f32;
    let (px, py) = (n.bounds.x as f32 * s, n.bounds.y as f32 * s);
    let (ax, ay) = (px.floor(), py.floor());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let content = match layer {
        SpriteLayer::Shape => format!("{:?}", n.vector),
        SpriteLayer::Icons => format!("{:?}", (&n.glyph, &n.symbol)),
        SpriteLayer::Text => format!("{:?}", n.text),
    };
    (layer as u8, content, fonts.is_sf(), s.to_bits(), n.opacity.to_bits()).hash(&mut h);
    [n.bounds.w as f32, n.bounds.h as f32, px - ax, py - ay].map(f32::to_bits).hash(&mut h);
    Some(SpriteKey { hash: h.finish(), anchor: (ax as i32, ay as i32) })
}

/// Largest sprite edge in device pixels; anything bigger is not drawn.
const MAX_SPRITE: i32 = 8192;

/// Draw the `layer` content of `n` with the exact CPU code of [`render`], unclipped, into a tight pixmap.
pub fn node_sprite(n: &crate::scene::Node, fonts: &mut Fonts, scale: f64, layer: SpriteLayer) -> Option<Sprite> {
    let s = scale as f32;
    let b = n.bounds;
    let sh = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    match layer {
        SpriteLayer::Shape => {
            let v = n.vector.as_ref()?;
            // Tightly bound the rasterized path (expanded for stroke width / miter / caps).
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
                        let pad = (st.line_width as f32 * (st.miter_limit as f32).clamp(1.0, 10.0) * 0.5 + 2.0).max(st.line_width as f32 + 2.0);
                        grow(&p, pad);
                    }
                }
            }
            if !min_x.is_finite() || !min_y.is_finite() || max_x <= min_x || max_y <= min_y {
                return None;
            }
            let ox = (min_x * s).floor() as i32;
            let oy = (min_y * s).floor() as i32;
            let w = (max_x * s).ceil() as i32 - ox + 1;
            let h = (max_y * s).ceil() as i32 - oy + 1;
            if w <= 0 || h <= 0 || w > MAX_SPRITE || h > MAX_SPRITE {
                return None;
            }
            let mut pm = Pixmap::new(w as u32, h as u32)?;
            let xf = Transform::from_scale(s, s).post_translate(-ox as f32, -oy as f32);
            draw_vector(&mut pm, v, b, n.opacity, xf, None);
            crop(ox, oy, pm).map(|(x, y, pixmap)| Sprite { x, y, pixmap })
        }
        SpriteLayer::Icons => {
            let mut parts: Vec<(i32, i32, Pixmap)> = vec![];
            if let Some((g, c)) = n.glyph {
                // Strokes and halos reach past the box (popover arrow shadow strokes: up to ~6 pt).
                let pad = (12.0 * s).ceil() as i32;
                let (ox, oy) = ((sh.0 * s).floor() as i32 - pad, (sh.1 * s).floor() as i32 - pad);
                let (w, h) = ((sh.2 * s).ceil() as i32 + 2 * pad + 1, (sh.3 * s).ceil() as i32 + 2 * pad + 1);
                if w <= MAX_SPRITE && h <= MAX_SPRITE {
                    if let Some(mut pm) = Pixmap::new(w as u32, h as u32) {
                        let xf = Transform::from_scale(s, s).post_translate(-ox as f32, -oy as f32);
                        draw_glyph(&mut pm, g, c, sh, n.opacity, xf, None);
                        parts.push((ox, oy, pm));
                    }
                }
            }
            if let Some(sy) = &n.symbol {
                parts.extend(symbol_tile(sy, sh, n.opacity, s));
            }
            combine(parts)
        }
        SpriteLayer::Text => {
            let t = n.text.as_ref()?;
            let mut ops = vec![];
            paint_text(n, t, fonts, s, |op| match op {
                TextOp::Blend(_, _, _, _, c) if c[3] == 0 => {}
                op => ops.push(op),
            });
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for op in &ops {
                let (a, b, c, d) = match *op {
                    TextOp::Blend(x, y, w, h, _) => (x, y, x + w as i32, y + h as i32),
                    TextOp::Rule(x, y, w, h, _) => (x.floor() as i32, y.floor() as i32, (x + w).ceil() as i32 + 1, (y + h).ceil() as i32 + 1),
                };
                (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
            }
            if x1 <= x0 || y1 <= y0 || x1 - x0 > MAX_SPRITE || y1 - y0 > MAX_SPRITE {
                return None;
            }
            let mut pm = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
            for op in &ops {
                apply_text_op(&mut pm, op, (x0, y0), None);
            }
            Some(Sprite { x: x0, y: y0, pixmap: pm })
        }
    }
}

/// Composite device-positioned tiles into one pixmap cropped to their non-empty extent.
fn combine(parts: Vec<(i32, i32, Pixmap)>) -> Option<Sprite> {
    let mut all: Vec<(i32, i32, Pixmap)> = parts.into_iter().filter_map(|(x, y, p)| crop(x, y, p)).collect();
    match all.len() {
        0 => None,
        1 => all.pop().map(|(x, y, pixmap)| Sprite { x, y, pixmap }),
        _ => {
            let x0 = all.iter().map(|p| p.0).min()?;
            let y0 = all.iter().map(|p| p.1).min()?;
            let x1 = all.iter().map(|p| p.0 + p.2.width() as i32).max()?;
            let y1 = all.iter().map(|p| p.1 + p.2.height() as i32).max()?;
            let mut pm = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
            for (x, y, p) in &all {
                pm.draw_pixmap(x - x0, y - y0, p.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            }
            Some(Sprite { x: x0, y: y0, pixmap: pm })
        }
    }
}

/// Crop a tile to its non-transparent pixels (`None` if fully transparent).
fn crop(x: i32, y: i32, p: Pixmap) -> Option<(i32, i32, Pixmap)> {
    let (w, h) = (p.width() as usize, p.height() as usize);
    let d = p.data();
    let live = |i: usize| d[i * 4 + 3] != 0;
    let rows: Vec<usize> = (0..h).filter(|&r| (0..w).any(|c| live(r * w + c))).collect();
    let (r0, r1) = (*rows.first()?, *rows.last()?);
    let cols = |c: usize| (r0..=r1).any(|r| live(r * w + c));
    let c0 = (0..w).find(|&c| cols(c))?;
    let c1 = (0..w).rev().find(|&c| cols(c))?;
    if (c0, r0, c1 + 1, r1 + 1) == (0, 0, w, h) {
        return Some((x, y, p));
    }
    let rect = tiny_skia::IntRect::from_xywh(c0 as i32, r0 as i32, (c1 - c0 + 1) as u32, (r1 - r0 + 1) as u32)?;
    Some((x + c0 as i32, y + r0 as i32, p.clone_rect(rect)?))
}

/// Rasterize at `scale` device pixels per point.
pub fn render(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Pixmap {
    let pw = (scene.size.0 * scale).round().max(1.0) as u32;
    let ph = (scene.size.1 * scale).round().max(1.0) as u32;
    let mut pm = Pixmap::new(pw, ph).expect("pixmap size");
    let s = scale as f32;
    let xf = Transform::from_scale(s, s);
    let dark = scene.appearance == onesim_protocol::Appearance::Dark;
    // Clip mask in effect for the *children* of each node.
    let mut masks: Vec<Option<Rc<Mask>>> = Vec::with_capacity(scene.nodes.len());
    for n in &scene.nodes {
        let mask = n.parent.and_then(|p| masks[p].clone());
        let b = n.bounds;
        let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
        let sh = (bx, by, bw, bh);
        paint_backdrop(&mut pm, n, dark, scene.ios, s, mask.as_deref());
        if let Some(c) = n.fill {
            if n.radius > 0.0 {
                if let Some(p) = rrect(bx, by, bw, bh, n.radius as f32) {
                    pm.fill_path(&p, &paint(c, n.opacity), FillRule::Winding, xf, mask.as_deref());
                }
            } else {
                // Snap to device pixels like UIKit (keeps hairlines crisp).
                let (x0, y0) = ((bx * s).round(), (by * s).round());
                let (x1, y1) = (((bx + bw) * s).round().max(x0 + 1.0), ((by + bh) * s).round().max(y0 + 1.0));
                if let Some(r) = Rect::from_ltrb(x0, y0, x1, y1) {
                    pm.fill_rect(r, &paint(c, n.opacity), Transform::identity(), mask.as_deref());
                }
            }
        }
        if let Some(g) = &n.gradient {
            draw_gradient(&mut pm, n, g, s, mask.as_deref());
        }
        if let Some((c, lw)) = n.stroke {
            if let Some(p) = rrect(bx, by, bw, bh, n.radius as f32) {
                let st = Stroke { width: lw as f32, ..Stroke::default() };
                pm.stroke_path(&p, &paint(c, n.opacity), &st, xf, mask.as_deref());
            }
        }
        if let Some(v) = &n.vector {
            draw_vector(&mut pm, v, b, n.opacity, xf, mask.as_deref());
        }
        if let Some((g, c)) = n.glyph {
            draw_glyph(&mut pm, g, c, sh, n.opacity, xf, mask.as_deref());
        }
        if let Some(sy) = &n.symbol {
            draw_symbol(&mut pm, sy, sh, n.opacity, s, mask.as_deref());
        }
        if let Some(im) = &n.image {
            draw_image(&mut pm, n, im, s, mask.as_deref());
        }
        if let Some(t) = &n.text {
            paint_text(n, t, fonts, s, |op| apply_text_op(&mut pm, &op, (0, 0), mask.as_deref()));
        }
        let cm = child_mask(n, mask, s, pw, ph);
        masks.push(cm);
    }
    pm
}

pub fn render_png(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Vec<u8>, String> {
    render(scene, fonts, scale).encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Node, Rect as SRect, VectorFill};
    use onesim_protocol::Appearance;

    #[test]
    fn shadow_radius_6_is_gaussian_sigma_6() {
        let mut n = Node::new("card", "shadow");
        n.bounds = SRect { x: 40.0, y: 40.0, w: 100.0, h: 60.0 };
        n.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.33), radius: 6.0, dx: 0.0, dy: 0.0 });
        let scene = Scene { size: (180.0, 140.0), appearance: Appearance::Light,
            ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] };
        let pm = render(&scene, &mut Fonts::new(), 3.0);
        // alpha at distance d above the top edge: 0.33 * (1 - Phi(d / 6))
        let a = |d: f32| pm.pixel(270, ((40.0 - d) * 3.0) as u32).unwrap().alpha() as f32 / 255.0;
        for (d, want) in [(1.0, 0.143), (6.0, 0.052), (12.0, 0.0075)] {
            assert!((a(d) - want).abs() < 0.012, "d={d}: {} vs {want}", a(d));
        }
    }

    /// Deterministic pseudo-random bytes (xorshift).
    fn noise(n: usize, mut x: u32) -> Vec<u8> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    #[test]
    fn blend_rect_matches_tiny_skia_fill_rect() {
        let (w, h) = (40u32, 30u32);
        let mut mask = Mask::new(w, h).unwrap();
        mask.data_mut().copy_from_slice(&noise((w * h) as usize, 7));
        let bg = noise((w * h * 4) as usize, 3);
        let mut base = Pixmap::new(w, h).unwrap();
        for (px, c) in base.pixels_mut().iter_mut().zip(bg.chunks(4)) {
            *px = tiny_skia::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
        }
        for (i, c) in noise(4 * 64, 11).chunks(4).enumerate() {
            let c = [c[0], c[1], c[2], if i % 8 == 0 { 255 } else { c[3] }];
            // On-canvas rects, some clipped at the right/bottom edge. (Wholly off-canvas rects at
            // negative coordinates differ: tiny-skia still paints pixel 0 there; glyphs never land there.)
            let (x, y) = (i as i32 * 7 % 40, i as i32 * 5 % 30);
            for m in [None, Some(&mask)] {
                let (mut a, mut b) = (base.clone(), base.clone());
                blend_rect(&mut a, (x, y, 3, 2), c, m);
                let mut p = Paint::default();
                p.set_color_rgba8(c[0], c[1], c[2], c[3]);
                p.anti_alias = false;
                b.fill_rect(Rect::from_xywh(x as f32, y as f32, 3.0, 2.0).unwrap(), &p, Transform::identity(), m);
                assert_eq!(a.data(), b.data(), "color {c:?} at ({x},{y}) mask {}", m.is_some());
            }
        }
    }

    #[test]
    fn vectorized_blur_is_bit_identical_to_per_output_loop() {
        let (w, h, sigma) = (37, 29, 2.3f32);
        let src: Vec<f32> = noise(w * h, 5).iter().map(|&b| b as f32 / 7.0).collect();
        // The straightforward blur: each output sums its taps in kernel order.
        let naive = |replicate: bool| {
            let r = (sigma * 3.0).ceil() as isize;
            let mut k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
            let sum: f32 = k.iter().sum();
            k.iter_mut().for_each(|v| *v /= sum);
            let pass = |from: &[f32], n: usize, lines: usize, stride: usize, step: usize| {
                let mut to = from.to_vec();
                for l in 0..lines {
                    for i in 0..n as isize {
                        let mut acc = 0.0f32;
                        for (ki, kv) in k.iter().enumerate() {
                            let j = i + ki as isize - r;
                            if replicate {
                                acc += kv * from[l * step + j.clamp(0, n as isize - 1) as usize * stride];
                            } else if (0..n as isize).contains(&j) {
                                acc += kv * from[l * step + j as usize * stride];
                            }
                        }
                        to[l * step + i as usize * stride] = acc;
                    }
                }
                to
            };
            pass(&pass(&src, w, h, 1, w), h, w, w, 1)
        };
        for replicate in [false, true] {
            let want = naive(replicate);
            let mut full = src.clone();
            gaussian(&mut full, w, h, sigma, replicate);
            assert!(full.iter().zip(&want).all(|(a, b)| a.to_bits() == b.to_bits()), "replicate={replicate}");
        }
    }

    #[test]
    fn ellipse_is_true_oval_not_capsule() {
        let mut n = Node::new("e", "shape");
        n.bounds = SRect { x: 10.0, y: 10.0, w: 120.0, h: 40.0 };
        n.vector = Some(Box::new(VectorShape {
            geom: VectorGeom::Ellipse,
            fill: Some(VectorFill { color: Color::rgba(1.0, 0.0, 0.0, 1.0), eo_fill: false, antialiased: true }),
            stroke: None,
        }));
        let scene = Scene { size: (140.0, 60.0), appearance: Appearance::Light,
            ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] };
        let pm = render(&scene, &mut Fonts::new(), 1.0);
        // Center (70, 30) is inside the ellipse.
        assert!(pm.pixel(70, 30).unwrap().alpha() > 240);
        // (32, 12) is inside a capsule (straight top edge starts at x=30) but OUTSIDE a 120x40 ellipse:
        // ((32-70)/60)^2 + ((12-30)/20)^2 = 0.401 + 0.81 = 1.21 > 1.
        assert_eq!(pm.pixel(32, 12).unwrap().alpha(), 0);
    }

    #[test]
    fn uneven_rrect_and_eo_fill_and_dashed_stroke_and_sprite_parity() {
        // 1. Even-odd fill cuts a hole where two same-winding squares overlap; winding fill does not.
        let ops = vec![
            PathOp::M([0.0, 0.0]), PathOp::L([40.0, 0.0]), PathOp::L([40.0, 40.0]), PathOp::L([0.0, 40.0]), PathOp::Z,
            PathOp::M([10.0, 10.0]), PathOp::L([30.0, 10.0]), PathOp::L([30.0, 30.0]), PathOp::L([10.0, 30.0]), PathOp::Z,
        ];
        for (eo, want_center_a) in [(true, 0u8), (false, 255u8)] {
            let mut n = Node::new("p", "shape");
            n.bounds = SRect { x: 0.0, y: 0.0, w: 40.0, h: 40.0 };
            n.vector = Some(Box::new(VectorShape {
                geom: VectorGeom::Path { ops: ops.clone(), mirror_x: false },
                fill: Some(VectorFill { color: Color::rgba(0.0, 0.5, 1.0, 1.0), eo_fill: eo, antialiased: true }),
                stroke: None,
            }));
            let scene = Scene { size: (40.0, 40.0), appearance: Appearance::Light,
                ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] };
            let pm = render(&scene, &mut Fonts::new(), 1.0);
            assert_eq!(pm.pixel(20, 20).unwrap().alpha(), want_center_a, "eo={eo}");
            assert_eq!(pm.pixel(5, 5).unwrap().alpha(), 255);
        }

        // 2. Inset stroke (strokeBorder) stays strictly inside bounds; dash creates gaps.
        let mut n = Node::new("s", "shape");
        n.bounds = SRect { x: 10.0, y: 10.0, w: 60.0, h: 40.0 };
        n.vector = Some(Box::new(VectorShape {
            geom: VectorGeom::RoundedRect { radii: [(16.0, 16.0), (0.0, 0.0), (16.0, 16.0), (0.0, 0.0)], continuous: false },
            fill: None,
            stroke: Some(VectorStroke {
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                line_width: 6.0,
                line_cap: ProtoLineCap::Butt,
                line_join: ProtoLineJoin::Miter,
                miter_limit: 10.0,
                dash: vec![10.0, 10.0],
                dash_phase: 0.0,
                antialiased: true,
                inset: 3.0,
            }),
        }));
        let scene = Scene { size: (80.0, 60.0), appearance: Appearance::Light,
            ios: Default::default(), nodes: vec![n.clone()], unsupported_symbols: vec![], notes: vec![] };
        let pm = render(&scene, &mut Fonts::new(), 1.0);
        // Outside the bounds (x=9) must be empty because inset = lineWidth/2 = 3.
        assert_eq!(pm.pixel(9, 30).unwrap().alpha(), 0);
        // Sprite rendering matches CPU render pixel-for-pixel.
        let sp = node_sprite(&n, &mut Fonts::new(), 1.0, SpriteLayer::Shape).expect("shape sprite");
        let mut via_sprite = Pixmap::new(80, 60).unwrap();
        via_sprite.draw_pixmap(sp.x, sp.y, sp.pixmap.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        assert_eq!(pm.data(), via_sprite.data());
    }

    #[test]
    fn container_relative_and_concentric_resolve_against_container_shape() {
        let tree: onesim_protocol::ViewNode = serde_json::from_value(serde_json::json!({
            "kind": "vstack", "id": "0", "spacing": 0.0, "alignment": "center",
            "modifiers": [
                {"padding": {"top": 10.0, "leading": 10.0, "bottom": 10.0, "trailing": 10.0}},
                {"frame": {"width": 120.0, "height": 80.0}},
                {"containerShape": {"shape": "roundedRectangle", "cornerRadius": 28.0, "cornerStyle": "continuous"}}
            ],
            "children": [
                {"kind": "shape", "id": "0/0", "shape": "containerRelative", "fill": {"r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0}},
                {"kind": "shape", "id": "0/1", "shape": "concentricRectangle",
                 "concentric": {"corners": [{"concentric": null}, {"concentric": null}, {"concentric": 6.0}, {"fixed": 4.0}]}},
                {"kind": "shape", "id": "0/2", "shape": "path", "custom": true}
            ]
        })).unwrap();
        let params = crate::SceneParams {
            viewport: (402.0, 874.0),
            device: crate::device(crate::DEFAULT_DEVICE).unwrap(),
            scale: 3.0,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            ios: Default::default(),
            scroll: &Default::default(),
            sheets: &Default::default(),
            focus: None,
            overscroll: false,
            status_bar: false,
            chrome: Default::default(),
        };
        let scene = crate::build_scene(&tree, &params, &mut Fonts::new());
        // ContainerRelativeShape at top of container (d_top=10, d_left=10, d_right=10, d_bottom=50)
        // resolves top corners to 28-10=18 and bottom corners to max(28-50, 0)=0.
        let rel = scene.nodes.iter().find(|n| n.id == "0/0").unwrap();
        let VectorGeom::RoundedRect { radii, .. } = &rel.vector.as_ref().unwrap().geom else { panic!("expected resolved rrect") };
        assert!((radii[0].0 - 18.0).abs() < 0.1 && (radii[1].0 - 18.0).abs() < 0.1, "top radii: {radii:?}");
        assert!(radii[2].0 < 0.1 && radii[3].0 < 0.1, "bottom radii: {radii:?}");
        // ConcentricRectangle in middle: bottomTrailing has minimum 6, bottomLeading is fixed 4.
        let conc = scene.nodes.iter().find(|n| n.id == "0/1").unwrap();
        let VectorGeom::RoundedRect { radii: cr, .. } = &conc.vector.as_ref().unwrap().geom else { panic!("expected resolved rrect") };
        assert!((cr[2].0 - 6.0).abs() < 0.1 && (cr[3].0 - 4.0).abs() < 0.1, "concentric radii: {cr:?}");
        // Custom shape without pathSize appears in pending_shape_frames.
        let pending = scene.pending_shape_frames();
        assert!(pending.contains_key("0/2"), "pending: {pending:?}");

        // Stroke with omitted color inherits foregroundColor from environment.
        let fg_stroke_tree: onesim_protocol::ViewNode = serde_json::from_value(serde_json::json!({
            "kind": "shape", "id": "fg", "shape": "circle",
            "stroke": {"lineWidth": 4.0, "inset": 2.0},
            "modifiers": [{"foregroundColor": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}}, {"frame": {"width": 40.0, "height": 40.0}}]
        })).unwrap();
        let fg_scene = crate::build_scene(&fg_stroke_tree, &params, &mut Fonts::new());
        let fg_node = fg_scene.nodes.iter().find(|n| n.id == "fg").unwrap();
        let st_col = fg_node.vector.as_ref().unwrap().stroke.as_ref().unwrap().color;
        assert_eq!(st_col, Color::rgba(1.0, 0.0, 0.0, 1.0));
    }
}
