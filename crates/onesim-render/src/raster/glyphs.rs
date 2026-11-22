use super::vector::{paint, rrect};
use crate::scene::{Color, Glyph, SymbolRun};
use crate::symbols;
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, PathBuilder, Pixmap, Rect, Stroke, Transform};

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
pub(super) fn draw_glyph(pm: &mut Pixmap, g: Glyph, c: Color, b: (f32, f32, f32, f32), op: f32, xf: Transform, mask: Option<&Mask>) {
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
            for i in 0..4 {
                if let Some(p) = PathBuilder::from_circle(x + 2.7 + i as f32 * 5.15, cy + 0.75, 1.36) {
                    pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
                }
            }
        }
        Glyph::Wifi27 => {
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
                let st = Stroke { width: 0.8, line_join: LineJoin::Round, ..Stroke::default() };
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
                    let st = Stroke { width: 2.4, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
                    pm.stroke_path(&p, &paint(c, 1.0), &st, xf, mask);
                }
            }
        }
        Glyph::Battery27 => {
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
                let fill = if c.r > 0.5 {
                    Color::rgba(0.5, 0.5, 0.5, c.a)
                } else {
                    Color::rgba(156.0 / 255.0, 178.0 / 255.0, 164.0 / 255.0, c.a)
                };
                pm.fill_path(&p, &paint(fill, 1.0), FillRule::Winding, xf, mask);
                let st = Stroke { width: 0.9, line_join: LineJoin::Round, ..Stroke::default() };
                pm.stroke_path(
                    &p,
                    &paint(Color::rgba(205.0 / 255.0, 234.0 / 255.0, 216.0 / 255.0, c.a), 1.0),
                    &st,
                    xf,
                    mask,
                );
            }
        }
        Glyph::Shift | Glyph::ShiftFilled | Glyph::Backspace | Glyph::Return | Glyph::Search | Glyph::Emoji | Glyph::Mic => {
            crate::chrome::draw_glyph(pm, g, c, b, xf, mask);
        }
        Glyph::ArrowDown | Glyph::ArrowUp | Glyph::ArrowLeft | Glyph::ArrowRight => {
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
                Glyph::ArrowRight => {
                    pb.move_to(x, y);
                    pb.line_to(x, y + h);
                    pb.line_to(x + w - 1.0, cy + 1.5);
                    pb.line_to(x + w - 1.0, cy - 1.5);
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
                for k in 1..=5 {
                    let st = Stroke { width: k as f32 * 2.4, line_join: LineJoin::Round, ..Stroke::default() };
                    pm.stroke_path(&p, &paint(Color::rgba(0.0, 0.0, 0.0, 0.016), 1.0), &st, xf, mask);
                }
                pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, xf, mask);
            }
        }
        Glyph::AndroidWifi => {
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
        Glyph::Spinner { step, .. } | Glyph::SpinnerLarge { step, .. } => {
            let s = w.min(h);
            let k = if matches!(g, Glyph::Spinner { .. }) { SPINNER_SPOKE } else { SPINNER_SPOKE_LARGE };
            // UIActivityIndicatorView turns clockwise: the brightest spoke leads (top-left at step 0), the trail fades
            // behind it counter-clockwise.
            let lead = (7 + u32::from(step)) % 8;
            for i in 0..8u32 {
                let a = (i as f32 * 45.0).to_radians();
                let (dx, dy) = (a.sin(), -a.cos());
                let (r0, r1) = (k.0 * s, k.1 * s);
                let alpha = 1.0 - ((lead + 8 - i) % 8) as f32 / 8.0 * SPINNER_FADE;
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
pub(super) fn symbol_tile(sy: &SymbolRun, b: (f32, f32, f32, f32), op: f32, s: f32) -> Option<(i32, i32, Pixmap)> {
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

/// Blit an SF Symbol directly onto `pm` without allocating a temporary `Pixmap`.
pub(super) fn draw_symbol(pm: &mut Pixmap, sy: &SymbolRun, b: (f32, f32, f32, f32), op: f32, s: f32, mask: Option<&Mask>) {
    let (wpx, hpx) = ((sy.size.0 as f32 * s).round().max(1.0) as u32, (sy.size.1 as f32 * s).round().max(1.0) as u32);
    let tint = [(sy.color.r * 255.0).round() as u8, (sy.color.g * 255.0).round() as u8, (sy.color.b * 255.0).round() as u8, (sy.color.a * 255.0).round() as u8];
    let Some(m) = symbols::mask(&sy.spec, wpx, hpx, s as f64, tint) else { return };
    let (bx, by) = ((b.0 * s).round() as i32, (b.1 * s).round() as i32);
    let (bw, bh) = ((b.2 * s).round() as i32, (b.3 * s).round() as i32);
    let (x0, y0) = (bx + (bw - wpx as i32) / 2, by + (bh - hpx as i32) / 2);
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let min_x = x0.max(bx).max(0);
    let min_y = y0.max(by).max(0);
    let max_x = (x0 + wpx as i32).min(bx + bw).min(pw);
    let max_y = (y0 + hpx as i32).min(by + bh).min(ph);
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    let c = sy.color;
    let k = c.a * op;
    let (r, g, bl) = (c.r * 255.0, c.g * 255.0, c.b * 255.0);
    let colored = !m.rgba.is_empty();
    let div255 = |v: u16| (v + 255) >> 8;
    let mask_data = mask.map(|mk| (mk.data(), mk.width() as i32));
    let data = pm.data_mut();
    for dy in min_y..max_y {
        let sy_i = (dy - y0) as usize;
        for dx in min_x..max_x {
            let sx_i = (dx - x0) as usize;
            let idx = sy_i * wpx as usize + sx_i;
            let mut src: [u16; 4] = if colored {
                let o = idx * 4;
                let a0 = m.rgba[o + 3] as f32;
                let al = (a0 * k).round() as u8;
                if al == 0 {
                    continue;
                }
                let scale = if a0 == 0.0 { 0.0 } else { al as f32 / a0 };
                let ch = |v: u8| ((v as f32 * scale).round() as u8).min(al) as u16;
                [ch(m.rgba[o]), ch(m.rgba[o + 1]), ch(m.rgba[o + 2]), al as u16]
            } else {
                let ma = m.alpha[idx];
                if ma == 0 {
                    continue;
                }
                let a = ma as f32 / 255.0 * k;
                let al = (a * 255.0).round() as u8;
                if al == 0 {
                    continue;
                }
                let ch = |v: f32| ((v * a).round() as u8).min(al) as u16;
                [ch(r), ch(g), ch(bl), al as u16]
            };
            if let Some((md, mw)) = mask_data {
                let mk = md[(dy * mw + dx) as usize] as u16;
                if mk == 0 {
                    continue;
                }
                if mk < 255 {
                    src = src.map(|v| div255(v * mk));
                }
            }
            let di = (dy * pw + dx) as usize * 4;
            let inv = 255 - src[3];
            for (ch, sv) in src.iter().enumerate() {
                data[di + ch] = (*sv + div255(data[di + ch] as u16 * inv)) as u8;
            }
        }
    }
}
