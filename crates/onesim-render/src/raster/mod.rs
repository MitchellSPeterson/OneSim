pub(crate) mod composite;
mod glyphs;
mod shadow;
mod sprite;
mod vector;

pub(crate) use shadow::gaussian;
pub(crate) use sprite::text_sprite;
pub(crate) use vector::{build_vector_path, to_skia_stroke};
pub use sprite::{node_sprite, sprite_key, Sprite, SpriteKey, SpriteLayer};

use crate::scene::{Color, Scene, Shadow};
use crate::text::{FontSpec, Fonts};
use glyphs::{draw_glyph, draw_symbol};
use onesim_protocol::IosVersion;
use shadow::{draw_shadow, draw_shadow_dev, DevShadow};
use std::rc::Rc;
use tiny_skia::{FillRule, Mask, Paint, Pixmap, PixmapPaint, Rect, Stroke, Transform};
use vector::{clip_mask, draw_gradient, draw_image, draw_painted_vector, draw_vector, paint, rrect};

/// Conservative device-pixel coverage. Actual clipping still uses the rounded paths.
#[derive(Clone, Copy)]
pub(crate) struct PaintClip {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl PaintClip {
    pub(crate) fn viewport(width: u32, height: u32) -> Self {
        Self { left: 0.0, top: 0.0, right: width as f32, bottom: height as f32 }
    }

    pub(crate) fn empty() -> Self {
        Self { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    pub(crate) fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    pub(crate) fn left(self) -> f32 {
        self.left
    }

    pub(crate) fn top(self) -> f32 {
        self.top
    }

    pub(crate) fn right(self) -> f32 {
        self.right
    }

    pub(crate) fn bottom(self) -> f32 {
        self.bottom
    }

    pub(crate) fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    fn bounds(b: crate::scene::Rect, scale: f32) -> Option<Self> {
        let (x, y, w, h) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
        let (left, top) = (x * scale, y * scale);
        // Include both CPU (add before scaling) and display-list (add after scaling) edges.
        let right = ((x + w) * scale).max(left + w * scale).max((b.x + b.w) as f32 * scale);
        let bottom = ((y + h) * scale).max(top + h * scale).max((b.y + b.h) as f32 * scale);
        (w >= 0.0 && h >= 0.0 && [left, top, right, bottom].iter().all(|x| x.is_finite()))
            .then_some(Self { left: left.floor(), top: top.floor(), right: right.ceil(), bottom: bottom.ceil() })
    }

    pub(crate) fn intersect_node(self, b: crate::scene::Rect, scale: f32) -> Self {
        if b.w <= 0.0 || b.h <= 0.0 {
            return self;
        }
        let Some(b) = Self::bounds(b, scale) else { return self };
        // Round each clip outward before intersecting so fractional clips sharing an AA pixel survive.
        Self { left: self.left.max(b.left), top: self.top.max(b.top), right: self.right.min(b.right), bottom: self.bottom.min(b.bottom) }
    }

    pub(crate) fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    pub(crate) fn pixels(x: i32, y: i32, width: u32, height: u32) -> Self {
        let (x, y) = (x as f32, y as f32);
        Self { left: x, top: y, right: x + width as f32, bottom: y + height as f32 }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self { left: self.left.min(other.left), top: self.top.min(other.top), right: self.right.max(other.right), bottom: self.bottom.max(other.bottom) }
    }

    pub(crate) fn translated(self, x: f32, y: f32) -> Self {
        Self { left: self.left + x, top: self.top + y, right: self.right + x, bottom: self.bottom + y }
    }

    pub(crate) fn outset(self, pad: f32) -> Self {
        Self { left: self.left - pad, top: self.top - pad, right: self.right + pad, bottom: self.bottom + pad }
    }

    pub(crate) fn overlaps(self, other: Self) -> bool {
        other.left < self.right && other.top < self.bottom && other.right > self.left && other.bottom > self.top
    }

    pub(crate) fn intersects_node(self, b: crate::scene::Rect, scale: f32) -> bool {
        // Allow for snapping, including square fills forced to at least one device pixel.
        Self::bounds(b, scale).is_none_or(|b| self.overlaps(b.outset(1.0)))
    }
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

/// Two pixels of slack avoid re-wrapping because of subpixel rounding.
pub(crate) fn text_width(width: f32, scale: f32) -> f32 {
    width * scale + 2.0
}

/// Lay out and paint a text node as device-pixel operations (`emit` receives them in paint order).
fn paint_compose_text(n: &crate::scene::Node, t: &crate::scene::TextRun, fonts: &mut Fonts, s: f32, mut emit: impl FnMut(TextOp)) {
    let b = n.bounds;
    let (bx, by, bw) = (b.x as f32, b.y as f32, b.w as f32);
    let rgba = [
        (t.color.r * 255.0).round() as u8,
        (t.color.g * 255.0).round() as u8,
        (t.color.b * 255.0).round() as u8,
        (t.color.a * n.opacity * 255.0).round() as u8,
    ];
    let (ox, oy) = ((bx + t.offset_x) * s, (by - (t.font.pitch - t.font.line_height).max(0.0) / 2.0) * s);
    let spec = t.font.scaled(s);
    let clip = spec.no_wrap && matches!(spec.compose_overflow, Some(crate::text::ComposeOverflow::Clip | crate::text::ComposeOverflow::StartEllipsis | crate::text::ComposeOverflow::MiddleEllipsis));
    if t.spans.iter().any(|span| span.compose.as_ref().is_some_and(|style| style.background.is_some())) {
        let buffer = fonts.buffer(&t.text, &spec, Some(text_width(bw, s)), &t.spans);
        for line in buffer.layout_runs().take(spec.drawn_lines()) {
            let inset = crate::text::line_x(&buffer, &line);
            let mut backgrounds: Vec<(f32, f32, Color)> = Vec::new();
            for glyph in line.glyphs {
                let Some(color) = t.spans.get(glyph.metadata).and_then(|span| span.compose.as_ref()?.background) else {
                    continue;
                };
                let left = inset + glyph.x;
                let right = left + glyph.w;
                if let Some(last) = backgrounds.last_mut().filter(|last| last.2 == color && left <= last.1 + 0.001) {
                    last.0 = last.0.min(left);
                    last.1 = last.1.max(right);
                } else {
                    backgrounds.push((left, right, color));
                }
            }
            for (left, right, color) in backgrounds {
                let rgba = [(color.r * 255.0).round() as u8, (color.g * 255.0).round() as u8, (color.b * 255.0).round() as u8,
                    (color.a * n.opacity * 255.0).round() as u8];
                let (left, right) = if clip { (left.clamp(0.0, bw * s), right.clamp(0.0, bw * s)) } else { (left, right) };
                emit(TextOp::Rule(ox + left, oy + line.line_top, right - left, line.line_height, rgba));
            }
        }
    }
    if let Some(layout) = &n.compose_text {
        if let Some(color) = layout.selection_color {
            let rgba = [
                (color.r * 255.0).round() as u8, (color.g * 255.0).round() as u8, (color.b * 255.0).round() as u8,
                (color.a * n.opacity * 255.0).round() as u8,
            ];
            for rect in &layout.highlight {
                emit(TextOp::Rule((bx + rect.x as f32) * s, (by + rect.y as f32) * s, rect.w as f32 * s, rect.h as f32 * s, rgba));
            }
        }
    }
    // Styled runs carry their own color (alpha included); only the node opacity is applied on top.
    let base_a = if t.spans.is_empty() { rgba[3] as u32 } else { (n.opacity * 255.0).round() as u32 };
    let brushes: Vec<_> = t.spans.iter().map(|span| {
        span.compose.as_ref()?.brush.as_ref().map(|brush| crate::compose_paint::BrushSampler::new(brush, bw * s, b.h as f32 * s))
    }).collect();
    let has_shadow = t.spans.iter().any(|span| span.compose.as_ref().and_then(|style| style.shadow).is_some_and(|sh| sh.0[3] > 0.0));
    let mut foreground = Vec::new();
    let mut shadows: Vec<Vec<crate::compose_paint::GlyphPixel>> = if has_shadow { vec![vec![]; t.spans.len()] } else { vec![] };
    let spans = fonts.draw_compose(&t.text, &spec, text_width(bw, s), rgba, (ox, oy), &t.spans, |x, y, index, mut c| {
        if clip && (x < ox.floor() as i32 || x >= (ox + bw * s).ceil() as i32) {
            return;
        }
        let style = t.spans.get(index).and_then(|span| span.compose.as_ref());
        let mut opacity = style.and_then(|style| style.paint_alpha).unwrap_or(1.0).clamp(0.0, 1.0);
        if let Some(brush) = brushes.get(index).and_then(Option::as_ref) {
            let [r, g, b, a] = brush.sample(x as f32 + 0.5 - ox, y as f32 + 0.5 - oy);
            c[..3].copy_from_slice(&[r, g, b].map(|v| (v * 255.0).round() as u8));
            opacity *= a;
        }
        // cosmic-text replaces the colour alpha with glyph coverage; reapply ours.
        c[3] = (f32::from(c[3]) * opacity * base_a as f32 / 255.0) as u8;
        let pixel = TextOp::Blend(x, y, 1, 1, c);
        if has_shadow {
            if style.and_then(|style| style.shadow).is_some_and(|sh| sh.0[3] > 0.0) && c[3] > 0 {
                shadows[index].push((x, y, c));
            }
            foreground.push(pixel);
        } else {
            emit(pixel);
        }
    });
    for (span, pixels) in t.spans.iter().zip(shadows) {
        if let Some(shadow) = span.compose.as_ref().and_then(|style| style.shadow) {
            crate::compose_paint::shadow(shadow, &pixels, |x, y, c| emit(TextOp::Blend(x, y, 1, 1, c)));
        }
    }
    foreground.into_iter().for_each(&mut emit);
    // Strikethrough / underline: per line, or per styled run (in the run's own color and size).
    for sp in spans {
        let run = sp.run.and_then(|i| t.spans.get(i));
        let (strike, under) = (t.strike || run.is_some_and(|r| r.strike), run.is_some_and(|r| r.underline));
        let size = run.map_or(spec.size, |r| spec.size * r.pt / t.font.pt);
        let alpha = run.and_then(|r| r.compose.as_ref()?.paint_alpha).unwrap_or(1.0).clamp(0.0, 1.0);
        let col = run.map_or(rgba, |r| [(r.color.r * 255.0).round() as u8, (r.color.g * 255.0).round() as u8, (r.color.b * 255.0).round() as u8, (r.color.a * n.opacity * alpha * 255.0).round() as u8]);
        for (on, drop) in [(strike, 0.27f32), (under, -0.11)] {
            if !on {
                continue;
            }
            let th = (size * 0.065).max(1.0);
            let y = oy + sp.baseline - size * drop - th / 2.0;
            let (left, right) = if clip {
                (sp.x0.clamp(0.0, bw * s), sp.x1.clamp(0.0, bw * s))
            } else {
                (sp.x0, sp.x1)
            };
            if let Some(brush) = sp.run.and_then(|i| brushes.get(i)?.as_ref()) {
                let bottom = y + th;
                for py in y.floor() as i32..bottom.ceil() as i32 {
                    let coverage = (bottom.min(py as f32 + 1.0) - y.max(py as f32)).max(0.0);
                    for px in (ox + left).round() as i32..(ox + right).round() as i32 {
                        let mut c = brush.sample(px as f32 + 0.5 - ox, py as f32 + 0.5 - oy);
                        c[3] *= n.opacity * alpha * coverage;
                        emit(TextOp::Blend(px, py, 1, 1, c.map(|v| (v * 255.0).round() as u8)));
                    }
                }
            } else {
                emit(TextOp::Rule(ox + left, y, (right - left).max(0.0), th, col));
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

pub(crate) fn text_geometry(n: &crate::scene::Node, t: &crate::scene::TextRun, fonts: &Fonts, s: f32) -> (FontSpec, f32, (f32, f32)) {
    let b = n.bounds;
    let (bx, by, bw) = (b.x as f32, b.y as f32, b.w as f32);
    // Wrapped text uses a larger pitch than the single-line box; keep line 1 where it is.
    // Accessibility sizes: iOS sets the glyphs ~0.01 em higher in the line box (measured).
    let (lift, ldx) = match (t.font.size > 40.0 && t.font.compose_overflow.is_none(), fonts.is_sf()) {
        (false, _) => (0.0, 0.0),
        (true, true) => (0.0105 * t.font.size, 0.0),
        // Inter fallback at accessibility sizes: baseline and side bearing differ from SF.
        (true, false) => (-0.03 * t.font.size, -0.775),
    };
    let origin = ((bx + t.offset_x + ldx) * s, (by - (t.font.pitch - t.font.line_height).max(0.0) / 2.0 - lift) * s);
    let spec = FontSpec { size: t.font.size * s, line_height: t.font.line_height * s, pitch: t.font.pitch * s, ..t.font };
    // +2px slack so rounding differences vs layout never re-wrap.
    (spec, text_width(bw, s), origin)
}

/// Lay out and paint a text node as device-pixel operations (`emit` receives them in paint order).
fn paint_text(n: &crate::scene::Node, t: &crate::scene::TextRun, fonts: &mut Fonts, s: f32, clip: Option<PaintClip>, mut emit: impl FnMut(TextOp)) -> Result<(), PaintClip> {
    if t.font.compose_overflow.is_some() {
        paint_compose_text(n, t, fonts, s, emit);
        return Ok(());
    }
    let (spec, width, (ox, oy)) = text_geometry(n, t, fonts, s);
    let rgba = [
        (t.color.r * 255.0).round() as u8,
        (t.color.g * 255.0).round() as u8,
        (t.color.b * 255.0).round() as u8,
        (t.color.a * n.opacity * 255.0).round() as u8,
    ];
    // Styled runs carry their own color (alpha included); only the node opacity is applied on top.
    let base_a = if t.spans.is_empty() { rgba[3] as u32 } else { (n.opacity * 255.0).round() as u32 };
    // Decorations have their own bounds. Keep the full run when they are present.
    let clip = clip.filter(|_| !t.strike && !t.font.underline && !t.spans.iter().any(|span| span.strike || span.underline));
    let spans = fonts.draw_if_visible(&t.text, &spec, width, rgba, (ox, oy), &t.spans, clip, |x, y, w, h, c| {
        // cosmic-text replaces the colour alpha with glyph coverage; reapply ours.
        emit(TextOp::Blend(x, y, w, h, [c[0], c[1], c[2], (c[3] as u32 * base_a / 255) as u8]));
    })?;
    // Strikethrough / underline: per line, or per styled run (in the run's own color and size).
    for sp in spans {
        let run = sp.run.and_then(|i| t.spans.get(i));
        let strike = t.strike || run.is_some_and(|r| r.strike);
        let under = t.font.underline || run.is_some_and(|r| r.underline);
        let size = run.map_or(spec.size, |r| spec.size * r.pt / t.font.pt.max(0.01));
        let col = run.map_or(rgba, |r| [
            (r.color.r * 255.0).round() as u8,
            (r.color.g * 255.0).round() as u8,
            (r.color.b * 255.0).round() as u8,
            (r.color.a * n.opacity * 255.0).round() as u8,
        ]);
        let paint_col = |over: Option<crate::scene::Color>| {
            over.map_or(col, |c| [
                (c.r * 255.0).round() as u8,
                (c.g * 255.0).round() as u8,
                (c.b * 255.0).round() as u8,
                (c.a * n.opacity * 255.0).round() as u8,
            ])
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
    Ok(())
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
fn child_mask(n: &crate::scene::Node, radius: Option<f64>, mask: Option<Rc<Mask>>, s: f32, pw: u32, ph: u32) -> Option<Rc<Mask>> {
    let b = n.bounds;
    let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    match radius.and_then(|r| rrect(bx, by, bw, bh, r as f32)) {
        Some(path) => Some(Rc::new(clip_mask(mask.as_deref(), &path, Transform::from_scale(s, s), pw, ph))),
        None => mask,
    }
}

/// Shared clip propagation for `n`'s descendants.
fn descendant_clip(n: &crate::scene::Node, (clip, mask): Clip, s: f32, pw: u32, ph: u32) -> Clip {
    let cr = clip_radius(n, s, pw, ph);
    let child_clip = if cr.is_some() { clip.intersect_node(n.bounds, s) } else { clip };
    let mask = if child_clip.is_empty() { None } else { child_mask(n, cr, mask, s, pw, ph) };
    (child_clip, mask)
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
    let mask = chain.iter().rev().fold(None, |m, &i| {
        let n = &scene.nodes[i];
        child_mask(n, clip_radius(n, s, pw, ph), m, s, pw, ph)
    });
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

/// Rasterize at `scale` device pixels per point.
pub fn render(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Pixmap {
    render_reusing(scene, fonts, scale, Vec::new())
}

/// Rasterize into an existing RGBA allocation when it fits the new frame.
pub fn render_reusing(scene: &Scene, fonts: &mut Fonts, scale: f64, mut pixels: Vec<u8>) -> Pixmap {
    let pw = (scene.size.0 * scale).round().max(1.0) as u32;
    let ph = (scene.size.1 * scale).round().max(1.0) as u32;
    let bytes = u64::from(pw) * u64::from(ph);
    let mut pm = if bytes.checked_mul(4) == Some(pixels.len() as u64) {
        pixels.fill(0);
        let size = tiny_skia::IntSize::from_wh(pw, ph).expect("pixmap size");
        Pixmap::from_vec(pixels, size).expect("pixmap data")
    } else {
        drop(pixels);
        Pixmap::new(pw, ph).expect("pixmap size")
    };
    let mut painter = ScenePainter {
        fonts,
        scale: scale as f32,
        ios: scene.ios,
        dark: scene.appearance == onesim_protocol::Appearance::Dark,
    };
    painter.paint_scene(&mut pm, &scene.nodes);
    pm
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_subtree_into(
    pm: &mut Pixmap,
    roots: &[usize],
    nodes: &[crate::Node],
    children: &[Vec<usize>],
    fonts: &mut Fonts,
    scale: f32,
    dark: bool,
    ios: onesim_protocol::IosVersion,
) {
    let mut painter = ScenePainter { fonts, scale, ios, dark };
    painter.subtree(pm, (PaintClip::viewport(pm.width(), pm.height()), None), roots, nodes, children);
}

struct ScenePainter<'a> {
    fonts: &'a mut Fonts,
    scale: f32,
    ios: onesim_protocol::IosVersion,
    dark: bool,
}

type Clip = (PaintClip, Option<Rc<Mask>>);

impl ScenePainter<'_> {
    fn node(&mut self, pm: &mut Pixmap, n: &crate::scene::Node, (clip, mask): (PaintClip, Option<&Mask>)) {
        let Self { fonts, scale: s, ios, dark } = self;
        let (s, ios, dark) = (*s, *ios, *dark);
        let xf = Transform::from_scale(s, s);
        let b = n.bounds;
        let bounds_visible = clip.intersects_node(b, s);
        let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
        let sh = (bx, by, bw, bh);
        paint_backdrop(pm, n, dark, ios, s, mask);
        if let Some(c) = n.fill.filter(|_| bounds_visible) {
            if n.radius > 0.0 {
                if let Some(p) = rrect(bx, by, bw, bh, n.radius as f32) {
                    pm.fill_path(&p, &paint(c, n.opacity), FillRule::Winding, xf, mask);
                }
            } else {
                // Snap to device pixels like UIKit (keeps hairlines crisp).
                let (x0, y0) = ((bx * s).round(), (by * s).round());
                let (x1, y1) = (((bx + bw) * s).round().max(x0 + 1.0), ((by + bh) * s).round().max(y0 + 1.0));
                if let Some(r) = Rect::from_ltrb(x0, y0, x1, y1) {
                    pm.fill_rect(r, &paint(c, n.opacity), Transform::identity(), mask);
                }
            }
        }
        if let Some(g) = n.gradient.as_ref().filter(|_| bounds_visible) {
            draw_gradient(pm, n, g, s, mask);
        }
        if let Some((c, lw)) = n.stroke {
            if let Some(p) = rrect(bx, by, bw, bh, n.radius as f32) {
                let st = Stroke { width: lw as f32, ..Stroke::default() };
                pm.stroke_path(&p, &paint(c, n.opacity), &st, xf, mask);
            }
        }
        if let Some(v) = &n.vector {
            if n.vector_brush.is_some() {
                if let Some(sprite) = node_sprite(n, fonts, f64::from(s), SpriteLayer::Shape) {
                    pm.draw_pixmap(sprite.x, sprite.y, sprite.pixmap.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
                }
            } else if n.fill_paint.is_some() || n.stroke_paint.is_some() {
                draw_painted_vector(pm, n, v, s, mask);
            } else {
                draw_vector(pm, v, b, n.opacity, xf, mask, None);
            }
        }
        if let Some((g, c)) = n.glyph {
            draw_glyph(pm, g, c, sh, n.opacity, xf, mask);
        }
        if let Some(sy) = &n.symbol {
            draw_symbol(pm, sy, sh, n.opacity, s, mask);
        }
        if let Some(im) = n.image.as_ref().filter(|_| bounds_visible) {
            draw_image(pm, n, im, s, mask);
        }
        if let Some(t) = &n.text {
            let squashed = (n.text_squash - 1.0).abs() >= 1e-4;
            if n.content_paint.is_some() || squashed {
                // Wheel text is scaled about its center and clipped when its sprite is composited.
                if let Ok(Some(sprite)) = text_sprite(n, fonts, s, (!squashed).then_some(clip)) {
                    pm.draw_pixmap(sprite.x, sprite.y, sprite.pixmap.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
                }
            } else {
                let _ = paint_text(n, t, fonts, s, (!bounds_visible).then_some(clip), |op| apply_text_op(pm, &op, (0, 0), mask));
            }
        }
    }

    fn paint_scene(&mut self, pm: &mut Pixmap, nodes: &[crate::Node]) {
        let mut children = vec![Vec::new(); nodes.len() + 1];
        for (i, node) in nodes.iter().enumerate() {
            children[node.parent.unwrap_or(nodes.len())].push(i);
        }
        let viewport = PaintClip::viewport(pm.width(), pm.height());
        self.subtree(pm, (viewport, None), &children[nodes.len()], nodes, &children);
    }

    fn subtree(&mut self, pm: &mut Pixmap, inherited: Clip, indices: &[usize], nodes: &[crate::Node], children: &[Vec<usize>]) {
        // Ordinary nodes stay iterative even on an effect-heavy scene.
        let s = self.scale;
        let (pw, ph) = (pm.width(), pm.height());
        let mut pending: Vec<_> = indices.iter().rev().map(|&i| (i, inherited.clone())).collect();
        while let Some((i, (clip, mask))) = pending.pop() {
            if clip.is_empty() {
                continue;
            }
            let n = &nodes[i];
            if n.composite.is_some() {
                self.isolated(pm, i, nodes, children, mask.as_deref());
            } else {
                self.node(pm, n, (clip, mask.as_deref()));
                if children[i].is_empty() {
                    continue;
                }
                let (child_clip, child_mask) = descendant_clip(n, (clip, mask), s, pw, ph);
                if !child_clip.is_empty() {
                    pending.extend(children[i].iter().rev().map(|&child| (child, (child_clip, child_mask.clone()))));
                }
            }
        }
    }

    fn isolated(&mut self, pm: &mut Pixmap, first: usize, nodes: &[crate::Node], children: &[Vec<usize>], clip: Option<&Mask>) {
        use crate::effects::{self, Composite};

        // A modifier chain can render from the deepest layer outwards. Keeping one source and
        // destination at a time makes its memory independent of the number of modifiers.
        let mut chain = vec![first];
        let mut cursor = first;
        while matches!(nodes[cursor].composite.as_deref(), Some(Composite::Effect(..))) && !coverage_blend(&nodes[cursor]) {
            let [child] = children[cursor].as_slice() else { break };
            if nodes[*child].composite.is_none() {
                break;
            }
            cursor = *child;
            chain.push(cursor);
        }
        let (w, h) = (pm.width(), pm.height());
        let Some(mut layer) = Pixmap::new(w, h) else { return };
        let viewport = (PaintClip::viewport(w, h), None);
        let mut coverage = None;
        match nodes[cursor].composite.as_deref() {
            Some(Composite::Effect(..)) => {
                let mut content = children[cursor].as_slice();
                if coverage_blend(&nodes[cursor]) {
                    while let [i] = content {
                        let Some(Composite::Mask { inverted }) = nodes[*i].composite.as_deref() else { break };
                        if nodes[*i].opacity != 1.0 {
                            break;
                        }
                        let Some((&source, rest)) = children[*i].split_last() else { break };
                        let Some(mut alpha) = Pixmap::new(w, h) else { return };
                        self.subtree(&mut alpha, viewport.clone(), &[source], nodes, children);
                        let mut mask = Mask::from_pixmap(alpha.as_ref(), tiny_skia::MaskType::Alpha);
                        if *inverted {
                            for value in mask.data_mut() {
                                *value = 255 - *value;
                            }
                        }
                        coverage = Some(intersect_coverage(mask, coverage.as_ref()));
                        content = rest;
                    }
                }
                self.subtree(&mut layer, viewport, content, nodes, children);
                if coverage_blend(&nodes[cursor]) {
                    let shape = self
                        .vector_coverage(&layer, content, nodes, children)
                        .unwrap_or_else(|| Mask::from_pixmap(layer.as_ref(), tiny_skia::MaskType::Alpha));
                    remove_coverage(&mut layer, &shape);
                    coverage = Some(intersect_coverage(shape, coverage.as_ref()));
                }
            }
            Some(Composite::Mask { inverted }) => {
                let Some((&source, content)) = children[cursor].split_last() else { return };
                self.subtree(&mut layer, viewport.clone(), content, nodes, children);
                let Some(mut alpha) = Pixmap::new(w, h) else { return };
                self.subtree(&mut alpha, viewport, &[source], nodes, children);
                effects::apply_mask(&mut layer, &alpha, *inverted);
            }
            None => unreachable!(),
        }

        for i in chain.into_iter().rev() {
            let node = &nodes[i];
            let effect = match node.composite.as_deref() {
                Some(Composite::Effect(op, color)) => Some((op.clone(), *color)),
                _ => None,
            };
            let local_clip = if i == cursor { coverage.take() } else { None };
            if i == first {
                let local_clip = local_clip.map(|mask| intersect_coverage(mask, clip));
                effects::composite(pm, layer, effect, node.bounds, self.scale, node.opacity, local_clip.as_ref().or(clip));
                return;
            }
            let Some(mut dest) = Pixmap::new(w, h) else { return };
            effects::composite(&mut dest, layer, effect, node.bounds, self.scale, node.opacity, local_clip.as_ref());
            layer = dest;
        }
    }

    fn vector_coverage(&self, target: &Pixmap, content: &[usize], nodes: &[crate::Node], children: &[Vec<usize>]) -> Option<Mask> {
        let mut content = content;
        use onesim_protocol::VisualOp;

        let mut transforms = Vec::new();
        let (node, vector) = loop {
            let [i] = content else { return None };
            let node = &nodes[*i];
            if let Some(vector) = node.vector.as_deref() {
                break (node, vector);
            }
            match node.composite.as_deref() {
                Some(crate::effects::Composite::Effect(op @ (VisualOp::Affine { .. } | VisualOp::Projection { .. } | VisualOp::Blur { .. }), _)) => {
                    transforms.push((node.bounds, op.clone()));
                }
                Some(crate::effects::Composite::Mask { .. }) => return None,
                _ => {}
            }
            content = &children[*i];
        };
        let (w, h) = (target.width(), target.height());
        let mut shape = Pixmap::new(w, h)?;
        let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
        let xf = Transform::from_scale(self.scale, self.scale);
        draw_vector(&mut shape, vector, node.bounds, 1.0, xf, None, Some(white));
        for (bounds, op) in transforms.into_iter().rev() {
            let mut transformed = Pixmap::new(w, h)?;
            crate::effects::composite(&mut transformed, shape, Some((op, None)), bounds, self.scale, 1.0, None);
            shape = transformed;
        }
        Some(Mask::from_pixmap(shape.as_ref(), tiny_skia::MaskType::Alpha))
    }
}

fn coverage_blend(node: &crate::Node) -> bool {
    match node.composite.as_deref() {
        Some(crate::effects::Composite::Effect(onesim_protocol::VisualOp::Blend { mode }, _)) => crate::effects::needs_blend_coverage(*mode),
        _ => false,
    }
}

fn intersect_coverage(mut first: Mask, second: Option<&Mask>) -> Mask {
    if let Some(second) = second {
        for (a, b) in first.data_mut().iter_mut().zip(second.data()) {
            *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
        }
    }
    first
}

fn remove_coverage(layer: &mut Pixmap, mask: &Mask) {
    for (pixel, &coverage) in layer.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(mask.data()) {
        match coverage {
            0 => *pixel = [0; 4],
            255 => {}
            _ => {
                let coverage = u16::from(coverage);
                for channel in pixel {
                    *channel = ((u16::from(*channel) * 255 + coverage / 2) / coverage).min(255) as u8;
                }
            }
        }
    }
}

pub fn render_png(scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Vec<u8>, String> {
    render(scene, fonts, scale).encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Node, Rect as SRect, VectorFill, VectorGeom, VectorShape, VectorStroke};
    use onesim_protocol::{Appearance, LineCap as ProtoLineCap, LineJoin as ProtoLineJoin, PathOp};

    #[test]
    fn paint_culling_keeps_text_overflow_and_rejects_a_clipped_subtree() {
        use crate::{FontSpec, TextAlign, TextRun};

        let mut text = Node::new("text", "text");
        text.bounds = SRect { x: 10.0, y: 5.0, w: 80.0, h: 20.0 };
        text.text = Some(TextRun {
            text: "Overflow".into(),
            font: FontSpec::new(16.0, 20.0, 400, TextAlign::Leading),
            color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            strike: false,
            offset_x: 0.0,
            spans: vec![],
        });
        let mut scene = Scene { size: (100.0, 50.0), appearance: Appearance::Light,
            ios: Default::default(), nodes: vec![text], unsupported_symbols: vec![], notes: vec![] };
        let mut fonts = Fonts::with_fallback(true);
        let expected = render(&scene, &mut fonts, 2.625);
        assert!(expected.data().iter().any(|&v| v != 0));

        scene.nodes[0].bounds.x -= 100.0;
        scene.nodes[0].text.as_mut().unwrap().offset_x = 100.0;
        let mut clip = Node::new("clip", "clip");
        clip.bounds = SRect { x: 0.0, y: 100.0, w: 100.0, h: 50.0 };
        clip.clip_radius = Some(5.0);
        scene.nodes.push(clip);
        let mut child = Node::new("hidden", "fill");
        child.bounds = SRect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 };
        child.parent = Some(1);
        child.fill = Some(Color::rgba(1.0, 0.0, 0.0, 1.0));
        scene.nodes.push(child);
        assert_eq!(render(&scene, &mut fonts, 2.625).data(), expected.data());
    }

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
        assert!(pm.pixel(70, 30).unwrap().alpha() > 240);
        assert_eq!(pm.pixel(32, 12).unwrap().alpha(), 0);
    }

    #[test]
    fn uneven_rrect_and_eo_fill_and_dashed_stroke_and_sprite_parity() {
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
                compose_effect: None,
                antialiased: true,
                inset: 3.0,
            }),
        }));
        let scene = Scene { size: (80.0, 60.0), appearance: Appearance::Light,
            ios: Default::default(), nodes: vec![n.clone()], unsupported_symbols: vec![], notes: vec![] };
        let pm = render(&scene, &mut Fonts::new(), 1.0);
        assert_eq!(pm.pixel(9, 30).unwrap().alpha(), 0);
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
        let rel = scene.nodes.iter().find(|n| n.id == "0/0").unwrap();
        let VectorGeom::RoundedRect { radii, .. } = &rel.vector.as_ref().unwrap().geom else { panic!("expected resolved rrect") };
        assert!((radii[0].0 - 18.0).abs() < 0.1 && (radii[1].0 - 18.0).abs() < 0.1, "top radii: {radii:?}");
        assert!(radii[2].0 < 0.1 && radii[3].0 < 0.1, "bottom radii: {radii:?}");
        let conc = scene.nodes.iter().find(|n| n.id == "0/1").unwrap();
        let VectorGeom::RoundedRect { radii: cr, .. } = &conc.vector.as_ref().unwrap().geom else { panic!("expected resolved rrect") };
        assert!((cr[2].0 - 6.0).abs() < 0.1 && (cr[3].0 - 4.0).abs() < 0.1, "concentric radii: {cr:?}");
        let pending = scene.pending_shape_frames();
        assert!(pending.contains_key("0/2"), "pending: {pending:?}");

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
