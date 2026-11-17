use crate::scene::{Color, Scene};
use crate::text::Fonts;
use std::rc::Rc;
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Transform};

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
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
        if let Some(c) = n.fill {
            if let Some(r) = Rect::from_xywh(bx, by, bw, bh) {
                pm.fill_rect(r, &paint(c, n.opacity), xf, mask.as_deref());
            }
        }
        if let Some(t) = &n.text {
            let rgba = [
                (t.color.r * 255.0).round() as u8,
                (t.color.g * 255.0).round() as u8,
                (t.color.b * 255.0).round() as u8,
                (t.color.a * n.opacity * 255.0).round() as u8,
            ];
            let (ox, oy) = (bx * s, by * s);
            // +2px slack so rounding differences vs layout never re-wrap.
            fonts.draw(&t.text, t.size * s, t.line_height * s, bw * s + 2.0, rgba, |x, y, w, h, c| {
                if let Some(r) = Rect::from_xywh(ox + x as f32, oy + y as f32, w as f32, h as f32) {
                    let mut p = Paint::default();
                    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
                    p.anti_alias = false;
                    pm.fill_rect(r, &p, Transform::identity(), mask.as_deref());
                }
            });
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
