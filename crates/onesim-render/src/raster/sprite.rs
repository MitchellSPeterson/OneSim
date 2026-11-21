use super::glyphs::{draw_glyph, symbol_tile};
use super::vector::{build_vector_path, draw_painted_vector_at, draw_vector};
use super::{apply_text_op, paint_text, PaintClip, TextOp};
use crate::scene::{Color, Glyph, SymbolRun, TextRun, VectorGeom, VectorShape};
use crate::text::{FontSpec, Fonts, Span};
use onesim_protocol::PathOp;
use std::hash::{Hash, Hasher};
use tiny_skia::{Path, Pixmap, PixmapPaint, Transform};

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

fn hash_color<H: Hasher>(c: Color, h: &mut H) {
    [c.r, c.g, c.b, c.a].map(f32::to_bits).hash(h);
}

fn hash_vector<H: Hasher>(v: &VectorShape, h: &mut H) {
    match &v.geom {
        VectorGeom::Ellipse => 0u8.hash(h),
        VectorGeom::RoundedRect { radii, continuous } => {
            1u8.hash(h);
            for (rx, ry) in radii {
                rx.to_bits().hash(h);
                ry.to_bits().hash(h);
            }
            continuous.hash(h);
        }
        VectorGeom::Path { ops, mirror_x } => {
            2u8.hash(h);
            mirror_x.hash(h);
            ops.len().hash(h);
            for op in ops {
                match *op {
                    PathOp::M(p) => {
                        0u8.hash(h);
                        p.map(f64::to_bits).hash(h);
                    }
                    PathOp::L(p) => {
                        1u8.hash(h);
                        p.map(f64::to_bits).hash(h);
                    }
                    PathOp::Q(p) => {
                        2u8.hash(h);
                        p.map(f64::to_bits).hash(h);
                    }
                    PathOp::C(p) => {
                        3u8.hash(h);
                        p.map(f64::to_bits).hash(h);
                    }
                    PathOp::Z => 4u8.hash(h),
                }
            }
        }
        VectorGeom::ContainerRelative => 3u8.hash(h),
        VectorGeom::Concentric(c) => {
            4u8.hash(h);
            for corner in &c.corners {
                match corner {
                    onesim_protocol::CornerStyleSpec::Fixed(v) => {
                        0u8.hash(h);
                        v.to_bits().hash(h);
                    }
                    onesim_protocol::CornerStyleSpec::Concentric(m) => {
                        1u8.hash(h);
                        m.map(f64::to_bits).hash(h);
                    }
                }
            }
            c.uniform.map(|u| u as u8).hash(h);
        }
    }
    if let Some(f) = v.fill {
        true.hash(h);
        hash_color(f.color, h);
        f.eo_fill.hash(h);
        f.antialiased.hash(h);
    } else {
        false.hash(h);
    }
    if let Some(st) = &v.stroke {
        true.hash(h);
        hash_color(st.color, h);
        st.line_width.to_bits().hash(h);
        (st.line_cap as u8).hash(h);
        (st.line_join as u8).hash(h);
        st.miter_limit.to_bits().hash(h);
        st.dash.len().hash(h);
        for d in &st.dash {
            d.to_bits().hash(h);
        }
        st.dash_phase.to_bits().hash(h);
        st.antialiased.hash(h);
        st.inset.to_bits().hash(h);
    } else {
        false.hash(h);
    }
}

fn hash_glyph<H: Hasher>(g: Glyph, h: &mut H) {
    std::mem::discriminant(&g).hash(h);
    if let Glyph::Arc { from, to, width } = g {
        (from, to, width).hash(h);
    }
}

fn hash_symbol<H: Hasher>(sy: &SymbolRun, h: &mut H) {
    sy.spec.name.hash(h);
    sy.spec.point_size.to_bits().hash(h);
    sy.spec.weight.hash(h);
    (sy.spec.scale as u8).hash(h);
    sy.spec.mode.hash(h);
    sy.spec.variable.hash(h);
    sy.spec.gradient.hash(h);
    hash_color(sy.color, h);
    sy.size.0.to_bits().hash(h);
    sy.size.1.to_bits().hash(h);
}

fn hash_font_spec<H: Hasher>(f: &FontSpec, h: &mut H) {
    [
        f.size,
        f.pt,
        f.line_height,
        f.pitch,
        f.tracking,
        f.extra,
        f.baseline,
        f.min_scale,
    ]
    .map(f32::to_bits)
    .hash(h);
    (f.weight, f.align as u8, f.mono, f.lines, f.fixed_pitch, f.inter_point_tracking, f.android, f.design, f.width).hash(h);
    (f.digits, f.trunc, f.case_fold, f.tighten, f.underline, f.italic, f.line_pat, f.strike_pat, f.reserve).hash(h);
    if let Some(c) = f.under_color {
        true.hash(h);
        hash_color(c, h);
    } else {
        false.hash(h);
    }
    if let Some(c) = f.strike_color {
        true.hash(h);
        hash_color(c, h);
    } else {
        false.hash(h);
    }
}

fn hash_span<H: Hasher>(sp: &Span, h: &mut H) {
    sp.text.hash(h);
    [sp.pt, sp.extra, sp.baseline].map(f32::to_bits).hash(h);
    hash_color(sp.color, h);
    (
        sp.weight,
        sp.italic,
        sp.underline,
        sp.strike,
        sp.mono,
        sp.design,
        sp.width,
        sp.digits,
        sp.line_pat,
        sp.strike_pat,
        sp.origin,
    )
        .hash(h);
    if let Some(c) = sp.under_color {
        true.hash(h);
        hash_color(c, h);
    } else {
        false.hash(h);
    }
    if let Some(c) = sp.strike_color {
        true.hash(h);
        hash_color(c, h);
    } else {
        false.hash(h);
    }
}

fn hash_text<H: Hasher>(t: &TextRun, h: &mut H) {
    t.text.hash(h);
    hash_font_spec(&t.font, h);
    hash_color(t.color, h);
    t.strike.hash(h);
    t.offset_x.to_bits().hash(h);
    t.spans.len().hash(h);
    for sp in &t.spans {
        hash_span(sp, h);
    }
}

/// Identity of the `layer` sprite of `n` at `scale`, or `None` if the node has none.
pub fn sprite_key(n: &crate::scene::Node, fonts: &Fonts, scale: f64, layer: SpriteLayer) -> Option<SpriteKey> {
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
    (layer as u8).hash(&mut h);
    match layer {
        SpriteLayer::Shape => {
            if let Some(v) = n.vector.as_deref() {
                hash_vector(v, &mut h);
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
        }
        SpriteLayer::Icons => {
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
        }
        SpriteLayer::Text => {
            if let Some(t) = n.text.as_ref() {
                hash_text(t, &mut h);
            }
            if let Some(g) = n.content_paint.as_ref() {
                true.hash(&mut h);
                g.hash_key(&mut h);
            } else {
                false.hash(&mut h);
            }
        }
    }
    (fonts.is_sf(), s.to_bits(), n.opacity.to_bits(), n.text_squash.to_bits()).hash(&mut h);
    [n.bounds.w as f32, n.bounds.h as f32, px - ax, py - ay].map(f32::to_bits).hash(&mut h);
    Some(SpriteKey { hash: h.finish(), anchor: (ax as i32, ay as i32) })
}

/// Largest sprite edge in device pixels; anything bigger is not drawn.
const MAX_SPRITE: i32 = 8192;

/// Draw the `layer` content of `n` with the exact CPU code of [`super::render`], unclipped, into a tight pixmap.
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
            if n.fill_paint.is_some() || n.stroke_paint.is_some() {
                draw_painted_vector_at(&mut pm, n, v, s, (ox, oy), None);
            } else {
                let xf = Transform::from_scale(s, s).post_translate(-ox as f32, -oy as f32);
                draw_vector(&mut pm, v, b, n.opacity, xf, None, None);
            }
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
        SpriteLayer::Text => text_sprite(n, fonts, s, None).unwrap_or_default(),
    }
}

/// Build an unclipped sprite, or return the ink bounds of a wholly offscreen run.
pub(crate) fn text_sprite(n: &crate::scene::Node, fonts: &mut Fonts, s: f32, clip: Option<PaintClip>) -> Result<Option<Sprite>, PaintClip> {
    let Some(t) = n.text.as_ref() else { return Ok(None) };
    let mut ops = vec![];
    paint_text(n, t, fonts, s, clip, |op| match op {
        TextOp::Blend(_, _, _, _, c) if c[3] == 0 => {}
        op => ops.push(op),
    })?;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for op in &ops {
        let (a, b, c, d) = match *op {
            TextOp::Blend(x, y, w, h, _) => (x, y, x + w as i32, y + h as i32),
            TextOp::Rule(x, y, w, h, _) => (x.floor() as i32, y.floor() as i32, (x + w).ceil() as i32 + 1, (y + h).ceil() as i32 + 1),
        };
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(d));
    }
    if x1 <= x0 || y1 <= y0 || x1 - x0 > MAX_SPRITE || y1 - y0 > MAX_SPRITE {
        return Ok(None);
    }
    let Some(mut pm) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return Ok(None) };
    for op in &ops {
        apply_text_op(&mut pm, op, (x0, y0), None);
    }
    let k = n.text_squash;
    let mut sprite = if (k - 1.0).abs() < 1e-4 || k <= 0.0 {
        Sprite { x: x0, y: y0, pixmap: pm }
    } else {
        // Squash vertically about the node's centre: same width, rows resampled.
        let cy = (n.bounds.y + n.bounds.h / 2.0) as f32 * s;
        let top = cy + (y0 as f32 - cy) * k;
        let ny0 = top.floor() as i32;
        let nh = (((y1 - y0) as f32 * k + (top - ny0 as f32)).ceil() as u32).max(1);
        let Some(mut out) = Pixmap::new(pm.width(), nh) else { return Ok(None) };
        let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..PixmapPaint::default() };
        out.draw_pixmap(0, 0, pm.as_ref(), &paint, Transform::from_row(1.0, 0.0, 0.0, k, 0.0, top - ny0 as f32), None);
        Sprite { x: x0, y: ny0, pixmap: out }
    };
    if let Some(gradient) = &n.content_paint {
        let Some(mut painted) = Pixmap::new(sprite.pixmap.width(), sprite.pixmap.height()) else { return Ok(None) };
        let bounds = crate::Rect {
            x: n.bounds.x - f64::from(sprite.x) / f64::from(s),
            y: n.bounds.y - f64::from(sprite.y) / f64::from(s),
            w: n.bounds.w,
            h: n.bounds.h,
        };
        let area = crate::paint::Tile {
            x: 0,
            y: 0,
            w: sprite.pixmap.width(),
            h: sprite.pixmap.height(),
            bounds,
            scale: s,
        };
        gradient.composite(&mut painted, area, &sprite.pixmap, 1.0, None);
        sprite.pixmap = painted;
    }
    Ok(Some(sprite))
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
    let row_live = |r: usize| (0..w).any(|c| live(r * w + c));
    let r0 = (0..h).find(|&r| row_live(r))?;
    let r1 = (0..h).rev().find(|&r| row_live(r))?;
    let cols = |c: usize| (r0..=r1).any(|r| live(r * w + c));
    let c0 = (0..w).find(|&c| cols(c))?;
    let c1 = (0..w).rev().find(|&c| cols(c))?;
    if (c0, r0, c1 + 1, r1 + 1) == (0, 0, w, h) {
        return Some((x, y, p));
    }
    let rect = tiny_skia::IntRect::from_xywh(c0 as i32, r0 as i32, (c1 - c0 + 1) as u32, (r1 - r0 + 1) as u32)?;
    Some((x + c0 as i32, y + r0 as i32, p.clone_rect(rect)?))
}
