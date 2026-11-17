use crate::scene::{Color, Glyph, Scene, Shadow, SymbolRun};
use crate::symbols;
use crate::text::{FontSpec, Fonts};
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

/// Icons drawn inside `(x, y, w, h)` (points).
fn draw_glyph(pm: &mut Pixmap, g: Glyph, c: Color, b: (f32, f32, f32, f32), op: f32, xf: Transform, mask: Option<&Mask>) {
    let (x, y, w, h) = b;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let c = Color { a: c.a * op, ..c };
    match g {
        Glyph::Chevron => stroke_pts(pm, &[(x + 0.9, y + 0.9), (x + w - 0.9, cy), (x + 0.9, y + h - 0.9)], c, 1.9, xf, mask),
        Glyph::BackChevron => {
            stroke_pts(pm, &[(cx + 4.5, cy - 8.8), (cx - 6.0, cy), (cx + 4.5, cy + 8.8)], c, 2.3, xf, mask)
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

/// In-place 3x box blur approximating a gaussian of `sigma` (device px) on RGBA8.
fn blur(data: &mut [u8], w: usize, h: usize, sigma: f32) {
    let d = ((sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize).max(1);
    let r = d / 2;
    let mut tmp = vec![0u8; data.len()];
    let pass = |src: &[u8], dst: &mut [u8], horiz: bool| {
        let (n, lines, stride, step) = if horiz { (w, h, 4, w * 4) } else { (h, w, w * 4, 4) };
        for l in 0..lines {
            let base = l * step;
            for ch in 0..4 {
                let at = |i: isize| -> u32 {
                    if i < 0 || i >= n as isize {
                        0
                    } else {
                        src[base + i as usize * stride + ch] as u32
                    }
                };
                let win = (2 * r + 1) as u32;
                let mut acc: u32 = (-(r as isize)..=r as isize).map(at).sum();
                for i in 0..n {
                    dst[base + i * stride + ch] = (acc / win) as u8;
                    acc = acc + at(i as isize + r as isize + 1) - at(i as isize - r as isize);
                }
            }
        }
    };
    for _ in 0..3 {
        pass(data, &mut tmp, true);
        pass(&tmp, data, false);
    }
}

fn draw_shadow(pm: &mut Pixmap, sh: &Shadow, b: (f32, f32, f32, f32), radius: f32, op: f32, s: f32, mask: Option<&Mask>) {
    let sigma = (sh.radius as f32 / 2.0 * s).max(0.5);
    let pad = (sigma * 3.0).ceil() + 2.0;
    let (w, h) = ((b.2 * s + 2.0 * pad).ceil() as u32, (b.3 * s + 2.0 * pad).ceil() as u32);
    let Some(mut tmp) = Pixmap::new(w.max(1), h.max(1)) else { return };
    if let Some(p) = rrect(pad, pad, b.2 * s, b.3 * s, radius * s) {
        tmp.fill_path(&p, &paint(Color { a: sh.color.a * op, ..sh.color }, 1.0), FillRule::Winding, Transform::identity(), None);
    }
    let (tw, th) = (tmp.width() as usize, tmp.height() as usize);
    blur(tmp.data_mut(), tw, th, sigma);
    let (x, y) = (b.0 * s - pad + sh.dx as f32 * s, b.1 * s - pad + sh.dy as f32 * s);
    pm.draw_pixmap(x.round() as i32, y.round() as i32, tmp.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
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
        if let Some(sd) = &n.shadow {
            draw_shadow(&mut pm, sd, sh, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32, n.opacity, s, mask.as_deref());
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
        if let Some(t) = &n.text {
            let rgba = [
                (t.color.r * 255.0).round() as u8,
                (t.color.g * 255.0).round() as u8,
                (t.color.b * 255.0).round() as u8,
                (t.color.a * n.opacity * 255.0).round() as u8,
            ];
            // Wrapped text uses a larger pitch than the single-line box; keep line 1 where it is.
            let (ox, oy) = ((bx + t.offset_x) * s, (by - (t.font.pitch - t.font.line_height) / 2.0) * s);
            let spec = FontSpec { size: t.font.size * s, line_height: t.font.line_height * s, pitch: t.font.pitch * s, ..t.font };
            // +2px slack so rounding differences vs layout never re-wrap.
            let base_a = rgba[3] as u32;
            let spans = fonts.draw(&t.text, &spec, bw * s + 2.0, rgba, |x, y, w, h, c| {
                if let Some(r) = Rect::from_xywh(ox + x as f32, oy + y as f32, w as f32, h as f32) {
                    let mut p = Paint::default();
                    // cosmic-text replaces the colour alpha with glyph coverage; reapply ours.
                    p.set_color_rgba8(c[0], c[1], c[2], (c[3] as u32 * base_a / 255) as u8);
                    p.anti_alias = false;
                    pm.fill_rect(r, &p, Transform::identity(), mask.as_deref());
                }
            });
            if t.strike {
                let th = (spec.size * 0.065).max(1.0);
                for sp in spans {
                    let y = oy + sp.baseline - spec.size * 0.27 - th / 2.0;
                    if let Some(r) = Rect::from_xywh(ox + sp.x0, y, (sp.x1 - sp.x0).max(0.0), th) {
                        let mut p = Paint::default();
                        p.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                        p.anti_alias = false;
                        pm.fill_rect(r, &p, Transform::identity(), mask.as_deref());
                    }
                }
            }
        }
        let child_mask = match n.clip_radius.and_then(|r| rrect(bx, by, bw, bh, r as f32)) {
            Some(path) => {
                let mut m = match &mask {
                    Some(m) => (**m).clone(),
                    None => {
                        let mut m = Mask::new(pw, ph).expect("mask");
                        m.fill_path(&tiny_skia::PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, pw as f32, ph as f32).unwrap()), FillRule::Winding, false, Transform::identity());
                        m
                    }
                };
                m.intersect_path(&path, FillRule::Winding, true, xf);
                Some(Rc::new(m))
            }
            None => mask,
        };
        masks.push(child_mask);
    }
    pm
}

pub fn render_png(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Vec<u8>, String> {
    render(scene, fonts, scale).encode_png().map_err(|e| e.to_string())
}
