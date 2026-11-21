use crate::scene::{Color, VectorGeom, VectorShape, VectorStroke};
use onesim_protocol::{LineCap as ProtoLineCap, LineJoin as ProtoLineJoin, PathOp};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform,
};

pub(super) fn paint(c: Color, opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
        (c.a * opacity * 255.0).round() as u8,
    );
    p
}

pub(super) fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
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
    let corner = |b: &mut PathBuilder, cx: f32, cy: f32, (ux, uy): (f32, f32), (vx, vy): (f32, f32), (ru, rv): (f32, f32), (au, av): (f32, f32)| {
        let pt = |du: f32, dv: f32| (cx + ux * du + vx * dv, cy + uy * du + vy * dv);
        if ru <= 1e-4 || rv <= 1e-4 {
            let (px, py) = pt(0.0, 0.0);
            b.line_to(px, py);
            return;
        }
        // Apple continuous corner (measured from SwiftUICore.RoundedRectangle(style: .continuous)):
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
    b.move_to(x + top_l, y);
    corner(&mut b, x + w, y, (-1.0, 0.0), (0.0, 1.0), (tr.0, tr.1), (top_r, right_t));
    corner(&mut b, x + w, y + h, (0.0, -1.0), (-1.0, 0.0), (br.1, br.0), (right_b, bot_r));
    corner(&mut b, x, y + h, (1.0, 0.0), (0.0, -1.0), (bl.0, bl.1), (bot_l, left_b));
    corner(&mut b, x, y, (0.0, 1.0), (1.0, 0.0), (tl.1, tl.0), (left_t, top_l));
    b.close();
    b.finish()
}

pub(super) fn build_vector_path(geom: &VectorGeom, b: crate::scene::Rect, inset: f64) -> Option<Path> {
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

pub(super) fn to_skia_stroke(st: &VectorStroke) -> Stroke {
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

pub(super) fn draw_vector(
    pm: &mut Pixmap,
    v: &VectorShape,
    bounds: crate::scene::Rect,
    opacity: f32,
    xf: Transform,
    mask: Option<&Mask>,
    color: Option<Color>,
) {
    if let Some(f) = v.fill {
        if let Some(p) = build_vector_path(&v.geom, bounds, 0.0) {
            let mut pt = paint(color.unwrap_or(f.color), opacity);
            pt.anti_alias = f.antialiased;
            let rule = if f.eo_fill { FillRule::EvenOdd } else { FillRule::Winding };
            pm.fill_path(&p, &pt, rule, xf, mask);
        }
    }
    if let Some(st) = &v.stroke {
        if st.line_width > 0.0 {
            if let Some(p) = build_vector_path(&v.geom, bounds, st.inset) {
                let mut pt = paint(color.unwrap_or(st.color), opacity);
                pt.anti_alias = st.antialiased;
                let sk = to_skia_stroke(st);
                pm.stroke_path(&p, &pt, &sk, xf, mask);
            }
        }
    }
}

pub(super) fn draw_painted_vector(pm: &mut Pixmap, node: &crate::Node, vector: &VectorShape, scale: f32, mask: Option<&Mask>) {
    draw_painted_vector_at(pm, node, vector, scale, (0, 0), mask);
}

pub(super) fn draw_painted_vector_at(
    pm: &mut Pixmap,
    node: &crate::Node,
    vector: &VectorShape,
    scale: f32,
    origin: (i32, i32),
    mask: Option<&Mask>,
) {
    use crate::paint::Tile;

    let (ox, oy) = origin;
    let bounds = crate::Rect {
        x: node.bounds.x - f64::from(ox) / f64::from(scale),
        y: node.bounds.y - f64::from(oy) / f64::from(scale),
        w: node.bounds.w,
        h: node.bounds.h,
    };
    let path_tile = |pm: &Pixmap, path: &Path, margin: f64| {
        let b = path.bounds();
        let limits = crate::Rect {
            x: f64::from(b.x()) - margin,
            y: f64::from(b.y()) - margin,
            w: f64::from(b.width()) + 2.0 * margin,
            h: f64::from(b.height()) + 2.0 * margin,
        };
        Tile::new(pm, bounds, limits, scale)
    };
    let xf = Transform::from_scale(scale, scale);
    if let Some(fill) = vector.fill {
        if let Some(path) = build_vector_path(&vector.geom, bounds, 0.0) {
            let rule = if fill.eo_fill { FillRule::EvenOdd } else { FillRule::Winding };
            let mut pt = paint(fill.color, node.opacity);
            pt.anti_alias = fill.antialiased;
            if let Some(gradient) = &node.fill_paint {
                if let Some(area) = path_tile(pm, &path, 1.0) {
                    if let Some(mut coverage) = Pixmap::new(area.w, area.h) {
                        coverage.fill_path(&path, &pt, rule, area.transform(), None);
                        gradient.composite(pm, area, &coverage, 1.0, mask);
                    }
                }
            } else {
                pm.fill_path(&path, &pt, rule, xf, mask);
            }
        }
    }
    if let Some(stroke) = vector.stroke.as_ref().filter(|st| st.line_width > 0.0) {
        if let Some(path) = build_vector_path(&vector.geom, bounds, stroke.inset) {
            let mut pt = paint(stroke.color, node.opacity);
            pt.anti_alias = stroke.antialiased;
            let sk = to_skia_stroke(stroke);
            if let Some(gradient) = &node.stroke_paint {
                let margin = stroke.line_width * 0.5 * stroke.miter_limit.max(2.0) + 1.0;
                if let Some(area) = path_tile(pm, &path, margin) {
                    if let Some(mut coverage) = Pixmap::new(area.w, area.h) {
                        coverage.stroke_path(&path, &pt, &sk, area.transform(), None);
                        gradient.composite(pm, area, &coverage, 1.0, mask);
                    }
                }
            } else {
                pm.stroke_path(&path, &pt, &sk, xf, mask);
            }
        }
    }
}

/// `mask` (or the whole canvas) intersected with `path`, which is in points.
pub(super) fn clip_mask(mask: Option<&Mask>, path: &Path, xf: Transform, pw: u32, ph: u32) -> Mask {
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
pub(super) fn draw_gradient(pm: &mut Pixmap, n: &crate::scene::Node, g: &crate::scene::Gradient, s: f32, mask: Option<&Mask>) {
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
pub(super) fn draw_image(pm: &mut Pixmap, n: &crate::scene::Node, im: &crate::scene::ImageRun, s: f32, mask: Option<&Mask>) {
    let (b, d) = (n.bounds, im.dest);
    let (iw, ih) = (im.pixmap.width() as f32, im.pixmap.height() as f32);
    let rotated = im.rotation != 0.0;
    // A rotated bitmap may reach anywhere in the bounds.
    let d = if rotated { b } else { d };
    let (x0, y0) = (d.x.max(b.x) as f32, d.y.max(b.y) as f32);
    let (x1, y1) = ((d.x + d.w).min(b.x + b.w) as f32, (d.y + d.h).min(b.y + b.h) as f32);
    let Some(r) = Rect::from_ltrb(x0, y0, x1, y1) else { return };
    let xf = Transform::from_scale(s, s);
    let clipped = n.radius > 0.0 || (!rotated && (d.x < b.x || d.y < b.y || d.x + d.w > b.x + b.w || d.y + d.h > b.y + b.h));
    let owned_mask = if clipped {
        let Some(clip) = rrect(b.x as f32, b.y as f32, b.w as f32, b.h as f32, n.radius as f32) else { return };
        Some(clip_mask(mask, &clip, xf, pm.width(), pm.height()))
    } else {
        None
    };
    let mut p = Paint::default();
    let mut to_pt = Transform::from_row(d.w as f32 / iw, 0.0, 0.0, d.h as f32 / ih, d.x as f32, d.y as f32);
    if rotated {
        to_pt = to_pt.post_rotate_at(im.rotation, (d.x + d.w / 2.0) as f32, (d.y + d.h / 2.0) as f32);
    }
    let quality = match im.filter {
        0 => tiny_skia::FilterQuality::Nearest,
        1 => tiny_skia::FilterQuality::Bilinear,
        _ => tiny_skia::FilterQuality::Bicubic,
    };
    p.shader = tiny_skia::Pattern::new(im.pixmap.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, quality, n.opacity, to_pt);
    pm.fill_rect(r, &p, xf, owned_mask.as_ref().or(mask));
}
