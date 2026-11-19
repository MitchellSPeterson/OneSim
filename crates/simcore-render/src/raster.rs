use crate::scene::{Color, Glyph, Scene, Shadow, SymbolRun};
use crate::symbols;
use crate::text::{FontSpec, Fonts};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform,
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
    p.shader = tiny_skia::Pattern::new(im.pixmap.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bicubic, n.opacity, to_pt);
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
fn draw_symbol(pm: &mut Pixmap, sy: &SymbolRun, b: (f32, f32, f32, f32), op: f32, s: f32, mask: Option<&Mask>) {
    let (wpx, hpx) = ((sy.size.0 as f32 * s).round().max(1.0) as u32, (sy.size.1 as f32 * s).round().max(1.0) as u32);
    let Some(m) = symbols::mask(&sy.spec, wpx, hpx, s as f64) else { return };
    let (bx, by) = ((b.0 * s).round() as i32, (b.1 * s).round() as i32);
    let (bw, bh) = ((b.2 * s).round() as i32, (b.3 * s).round() as i32);
    let (x0, y0) = (bx + (bw - wpx as i32) / 2, by + (bh - hpx as i32) / 2);
    let Some(mut tmp) = Pixmap::new(wpx, hpx) else { return };
    let c = sy.color;
    let k = c.a * op;
    let (r, g, bl) = (c.r * 255.0, c.g * 255.0, c.b * 255.0);
    for (i, px) in tmp.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (x0 + (i as u32 % wpx) as i32, y0 + (i as u32 / wpx) as i32);
        if x < bx || y < by || x >= bx + bw || y >= by + bh {
            continue;
        }
        let a = m.alpha[i] as f32 / 255.0 * k;
        let al = (a * 255.0).round() as u8;
        let ch = |v: f32| ((v * a).round() as u8).min(al);
        *px = tiny_skia::PremultipliedColorU8::from_rgba(ch(r), ch(g), ch(bl), al).unwrap_or(*px);
    }
    pm.draw_pixmap(x0, y0, tmp.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
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

/// SwiftUI `shadow(radius:)` is a gaussian with sigma = radius (fitted to simulator renders).
fn draw_shadow(pm: &mut Pixmap, n: &crate::scene::Node, sh: &Shadow, b: (f32, f32, f32, f32), s: f32, mask: Option<&Mask>) {
    let (id, op, radius) = (n.id.as_str(), n.opacity, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32);
    let sigma = (sh.radius as f32 * s).max(0.5);
    let pad = (sigma * 3.0).ceil() + 2.0;
    let (px, py) = (b.0 * s + sh.dx as f32 * s, b.1 * s + sh.dy as f32 * s);
    let (fx, fy) = (px - px.floor(), py - py.floor());
    let rgba = [sh.color.r, sh.color.g, sh.color.b, sh.color.a * op].map(|v| (v * 255.0).round() as u8);
    let key = (id.to_string(), [(b.2 * s).to_bits(), (b.3 * s).to_bits(), (radius * s).to_bits(), sigma.to_bits(), fx.to_bits(), fy.to_bits(), 0], rgba);
    let img = SHADOWS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 256 {
            c.clear();
        }
        c.entry(key)
            .or_insert_with(|| {
                let (w, h) = ((b.2 * s + 2.0 * pad).ceil() as u32 + 1, (b.3 * s + 2.0 * pad).ceil() as u32 + 1);
                let mut cov = Pixmap::new(w.max(1), h.max(1)).expect("shadow size");
                if let Some(p) = rrect(pad + fx, pad + fy, b.2 * s, b.3 * s, radius * s) {
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

/// Rasterize at `scale` device pixels per point.
pub fn render(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Pixmap {
    let pw = (scene.size.0 * scale).round().max(1.0) as u32;
    let ph = (scene.size.1 * scale).round().max(1.0) as u32;
    let mut pm = Pixmap::new(pw, ph).expect("pixmap size");
    let s = scale as f32;
    let xf = Transform::from_scale(s, s);
    // Clip mask in effect for the *children* of each node.
    let mut masks: Vec<Option<Rc<Mask>>> = Vec::with_capacity(scene.nodes.len());
    for n in &scene.nodes {
        let mask = n.parent.and_then(|p| masks[p].clone());
        let b = n.bounds;
        let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
        let sh = (bx, by, bw, bh);
        // Liquid Glass: snapshot the backdrop before our own halo lands on it.
        let glass = n.glass.as_ref().filter(|g| g.variant != crate::glass::GlassVariant::Identity).map(|g| {
            let dark = scene.appearance == simcore_protocol::Appearance::Dark;
            let p = crate::glass::spec_params(g, dark, scene.ios);
            let r = (n.radius as f32).min(bw / 2.0).min(bh / 2.0) * s;
            let shape = crate::glass::Shape { x: bx * s, y: by * s, w: bw * s, h: bh * s, r };
            (g, p, shape, crate::glass::snapshot(&pm, shape, s, &p), dark)
        });
        if let Some((_, p, ..)) = &glass {
            let sd = Shadow { color: Color::rgba(0.0, 0.0, 0.0, p.sh_a), radius: p.sh_sigma as f64, dx: 0.0, dy: p.sh_dy as f64 };
            if p.sh_a > 0.0 {
                draw_shadow(&mut pm, n, &sd, sh, s, mask.as_deref());
            }
        }
        if let Some((g, _, shape, snap, dark)) = glass {
            crate::glass::draw(&mut pm, snap, shape, g, s, dark,
                scene.ios, n.opacity, mask.as_deref());
        }
        if let Some(sd) = &n.shadow {
            draw_shadow(&mut pm, n, sd, sh, s, mask.as_deref());
        }
        if let Some(e) = &n.edge {
            crate::glass::draw_edge(&mut pm, e, s, scene.ios, mask.as_deref());
        }
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
                blend_rect(&mut pm, (x, y, w, h), [c[0], c[1], c[2], (c[3] as u32 * base_a / 255) as u8], mask.as_deref());
            });
            // Strikethrough / underline: per line, or per styled run (in the run's own color and size).
            for sp in spans {
                let run = sp.run.and_then(|i| t.spans.get(i));
                let (strike, under) = (t.strike || run.is_some_and(|r| r.strike), run.is_some_and(|r| r.underline));
                let size = run.map_or(spec.size, |r| spec.size * r.pt / t.font.pt);
                let col = run.map_or(rgba, |r| [(r.color.r * 255.0).round() as u8, (r.color.g * 255.0).round() as u8, (r.color.b * 255.0).round() as u8, (r.color.a * n.opacity * 255.0).round() as u8]);
                for (on, drop) in [(strike, 0.27f32), (under, -0.11)] {
                    if !on {
                        continue;
                    }
                    let th = (size * 0.065).max(1.0);
                    let y = oy + sp.baseline - size * drop - th / 2.0;
                    if let Some(r) = Rect::from_xywh(ox + sp.x0, y, (sp.x1 - sp.x0).max(0.0), th) {
                        let mut p = Paint::default();
                        p.set_color_rgba8(col[0], col[1], col[2], col[3]);
                        p.anti_alias = false;
                        pm.fill_rect(r, &p, Transform::identity(), mask.as_deref());
                    }
                }
            }
        }
        // A square clip covering the whole canvas clips nothing: skip the full-size mask (and the
        // slower masked blending under it). Full scrollers are exactly that.
        let covers = n.clip_radius == Some(0.0) && bx * s <= 0.0 && by * s <= 0.0 && (bx + bw) * s >= pw as f32 && (by + bh) * s >= ph as f32;
        let child_mask = match n.clip_radius.filter(|_| !covers).and_then(|r| rrect(bx, by, bw, bh, r as f32)) {
            Some(path) => Some(Rc::new(clip_mask(mask.as_deref(), &path, xf, pw, ph))),
            None => mask,
        };
        masks.push(child_mask);
    }
    pm
}

pub fn render_png(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Vec<u8>, String> {
    render(scene, fonts, scale).encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Node, Rect as SRect};
    use simcore_protocol::Appearance;

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
}
