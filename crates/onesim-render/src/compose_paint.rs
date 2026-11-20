use base64::Engine as _;
use onesim_protocol::{ComposeBrush, ComposeBrushKind, ComposeTextShadow};

pub(crate) struct BrushSampler<'a> {
    brush: &'a ComposeBrush,
    coord: [f32; 4],
    pixels: Option<Vec<u8>>,
}

impl<'a> BrushSampler<'a> {
    pub fn new(brush: &'a ComposeBrush, width: f32, height: f32) -> Self {
        let at = |i, fallback| brush.positions.get(i).copied().flatten().unwrap_or(fallback);
        let coord = match brush.kind {
            ComposeBrushKind::Linear => [at(0, 0.0), at(1, 0.0), at(2, width), at(3, height)],
            _ => [at(0, width / 2.0), at(1, height / 2.0), at(2, width.min(height) / 2.0), 0.0],
        };
        let pixels = brush.image.as_ref().and_then(|image| {
            let len = usize::try_from(u64::from(image.width) * u64::from(image.height)).ok()?.checked_mul(4)?;
            if len == 0 || image.rgba.len() != len.div_ceil(3).checked_mul(4)? {
                return None;
            }
            let pixels = base64::engine::general_purpose::STANDARD.decode(&image.rgba).ok()?;
            (pixels.len() == len).then_some(pixels)
        });
        Self { brush, coord, pixels }
    }

    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        let [x0, y0, a, b] = self.coord;
        let d = (x - x0, y - y0);
        let t = match self.brush.kind {
            ComposeBrushKind::Solid => 0.0,
            ComposeBrushKind::Linear => {
                let (dx, dy) = (a - x0, b - y0);
                (d.0 * dx + d.1 * dy) / (dx * dx + dy * dy).max(f32::MIN_POSITIVE)
            }
            ComposeBrushKind::Radial => d.0.hypot(d.1) / a.max(f32::MIN_POSITIVE),
            ComposeBrushKind::Sweep => (d.1.atan2(d.0) / std::f32::consts::TAU).rem_euclid(1.0),
            ComposeBrushKind::Image => return self.sample_image(x, y).unwrap_or([0.0; 4]),
        };
        let t = match self.brush.tile {
            1 => t.rem_euclid(1.0),
            2 => 1.0 - (t.rem_euclid(2.0) - 1.0).abs(),
            3 if !(0.0..=1.0).contains(&t) => return [0.0; 4],
            _ => t.clamp(0.0, 1.0),
        };
        let stops = &self.brush.stops;
        let i = stops.partition_point(|(at, _)| *at <= t);
        match (i.checked_sub(1).and_then(|i| stops.get(i)), stops.get(i)) {
            (Some(&(lo, left)), Some(&(hi, right))) => {
                let f = ((t - lo) / (hi - lo)).clamp(0.0, 1.0);
                std::array::from_fn(|c| left[c] + (right[c] - left[c]) * f)
            }
            (Some(&(_, color)), None) | (None, Some(&(_, color))) => color,
            (None, None) => [0.0; 4],
        }
    }

    fn sample_image(&self, x: f32, y: f32) -> Option<[f32; 4]> {
        let image = self.brush.image.as_ref()?;
        let pixels = self.pixels.as_ref()?;
        let x = tile_pixel(x, image.width, image.tile_x)?;
        let y = tile_pixel(y, image.height, image.tile_y)?;
        let start = (y * image.width as usize + x) * 4;
        Some(std::array::from_fn(|i| f32::from(pixels[start + i]) / 255.0))
    }
}

fn tile_pixel(position: f32, size: u32, tile: u8) -> Option<usize> {
    let size = i64::from(size);
    let pixel = position.floor() as i64;
    let pixel = match tile {
        1 => pixel.rem_euclid(size),
        2 => {
            let pixel = pixel.rem_euclid(2 * size);
            pixel.min(2 * size - 1 - pixel)
        }
        3 if !(0..size).contains(&pixel) => return None,
        _ => pixel.clamp(0, size - 1),
    };
    Some(pixel as usize)
}

pub(crate) fn tint(
    pixmap: &mut tiny_skia::Pixmap,
    brush: &ComposeBrush,
    (x, y): (f32, f32),
    (width, height): (f32, f32),
) {
    let row = pixmap.width() as usize;
    let sampler = BrushSampler::new(brush, width, height);
    for (i, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
        if pixel.alpha() == 0 {
            continue;
        }
        let [r, g, b, a] = sampler.sample(x + (i % row) as f32 + 0.5, y + (i / row) as f32 + 0.5);
        let a = f32::from(pixel.alpha()) * a.clamp(0.0, 1.0);
        let channel = |value: f32| (a * value.clamp(0.0, 1.0)).round() as u8;
        if let Some(color) = tiny_skia::PremultipliedColorU8::from_rgba(channel(r), channel(g), channel(b), a.round() as u8) {
            *pixel = color;
        }
    }
}

pub(crate) type GlyphPixel = (i32, i32, [u8; 4]);

pub(crate) fn shadow(
    shadow: ComposeTextShadow,
    pixels: &[GlyphPixel],
    mut emit: impl FnMut(i32, i32, [u8; 4]),
) {
    let ComposeTextShadow([r, g, b, alpha], dx, dy, radius) = shadow;
    if alpha <= 0.0 || pixels.is_empty() {
        return;
    }
    let rgb = [r, g, b].map(|v| (v * 255.0).round() as u8);
    let dx = dx.round() as i32;
    let dy = dy.round() as i32;
    if radius <= 0.0 {
        for &(x, y, color) in pixels {
            let coverage = (f32::from(color[3]) * alpha).round() as u8;
            emit(x.saturating_add(dx), y.saturating_add(dy), [rgb[0], rgb[1], rgb[2], coverage]);
        }
        return;
    }
    // Skia's blur radius is expressed in pixels and converted to a gaussian sigma.
    let sigma = (radius * 0.57735 + 0.5).min(64.0);
    let padding = (3.0 * sigma).ceil() as i32;
    let (left, top, right, bottom) = pixels.iter().fold(
        (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
        |(l, t, r, b), &(x, y, _)| (l.min(x), t.min(y), r.max(x), b.max(y)),
    );
    let x0 = left.saturating_sub(padding);
    let y0 = top.saturating_sub(padding);
    let w = (right.saturating_add(padding).saturating_sub(x0) as usize).saturating_add(1);
    let h = (bottom.saturating_add(padding).saturating_sub(y0) as usize).saturating_add(1);
    let area = w.saturating_mul(h);
    if area > 8_388_608 {
        return;
    }
    let mut coverage = vec![0.0; area];
    for &(x, y, color) in pixels {
        let at = (y - y0) as usize * w + (x - x0) as usize;
        let a = f32::from(color[3]) / 255.0;
        coverage[at] += a * (1.0 - coverage[at]);
    }
    crate::raster::gaussian(&mut coverage, w, h, sigma, false);
    for (i, a) in coverage.into_iter().enumerate() {
        let a = (a * alpha * 255.0).round() as u8;
        if a > 0 {
            let x = x0.saturating_add(dx).saturating_add((i % w) as i32);
            let y = y0.saturating_add(dy).saturating_add((i / w) as i32);
            emit(x, y, [rgb[0], rgb[1], rgb[2], a]);
        }
    }
}

/// Apply the horizontal affine transform around the glyph baseline before compositing the ink.
pub(crate) fn transform_glyph(
    pixels: &[GlyphPixel],
    [scale, skew]: [f32; 2],
    mut emit: impl FnMut(i32, i32, [u8; 4]),
) {
    use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};
    let Some((min_x, min_y, max_x, max_y)) =
        pixels.iter().map(|&(x, y, _)| (x, y, x, y)).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    else { return };
    let (source_w, source_h) = ((max_x - min_x + 1) as u32, (max_y - min_y + 1) as u32);
    let Some(mut source) = Pixmap::new(source_w, source_h) else { return };
    for &(x, y, [r, g, b, alpha]) in pixels {
        let premul = |value: u8| ((u32::from(value) * u32::from(alpha) + 127) / 255) as u8;
        let at = (y - min_y) as usize * source_w as usize + (x - min_x) as usize;
        source.pixels_mut()[at] = PremultipliedColorU8::from_rgba(premul(r), premul(g), premul(b), alpha)
            .expect("premultiplied glyph color");
    }
    let (scale, skew) = (scale.clamp(-32.0, 32.0), skew.clamp(-32.0, 32.0));
    let xs = [(min_x, min_y), (min_x, max_y + 1), (max_x + 1, min_y), (max_x + 1, max_y + 1)]
        .map(|(x, y)| scale * x as f32 + skew * y as f32);
    let left = xs.into_iter().fold(f32::INFINITY, f32::min).floor();
    let right = xs.into_iter().fold(f32::NEG_INFINITY, f32::max).ceil();
    let width = (right - left) as u32;
    if width == 0 || width > 8192 || u64::from(width) * u64::from(source_h) > 4_194_304 {
        return;
    }
    let Some(mut destination) = Pixmap::new(width, source_h) else { return };
    let transform = Transform::from_row(scale, 0.0, skew, 1.0, scale * min_x as f32 + skew * min_y as f32 - left, 0.0);
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() };
    destination.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
    for (i, &pixel) in destination.pixels().iter().enumerate() {
        if pixel.alpha() == 0 { continue; }
        let color = pixel.demultiply();
        emit(left as i32 + (i as u32 % width) as i32, min_y + (i as u32 / width) as i32,
            [color.red(), color.green(), color.blue(), color.alpha()]);
    }
}

pub(crate) fn stroke(stroke: &onesim_protocol::ComposeStroke, color: crate::Color, density: f32) -> crate::scene::VectorStroke {
    let dp = |pixels: f32| f64::from(pixels / density);
    crate::scene::VectorStroke {
        color, line_width: dp(stroke.width.max(1.0)), line_cap: stroke.cap, line_join: stroke.join,
        miter_limit: f64::from(stroke.miter), dash: stroke.dash.iter().copied().map(dp).collect(),
        dash_phase: dp(stroke.phase), antialiased: true, inset: 0.0,
        compose_effect: stroke.path_effect.clone().map(|effect| (effect, 1.0 / density)),
    }
}

pub(crate) fn raster_outline(
    commands: &[zeno::Command],
    [scale, skew]: [f32; 2],
    outline: Option<&onesim_protocol::ComposeStroke>,
    effect: Option<&onesim_protocol::ComposePathEffect>,
    color: crate::Color,
    mut emit: impl FnMut(i32, i32, [u8; 4]),
) {
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
    use zeno::Command;
    let mut path = PathBuilder::new();
    let point = |p: zeno::Point| (p.x * scale.clamp(-32.0, 32.0) - p.y * skew.clamp(-32.0, 32.0), -p.y);
    for command in commands {
        match *command {
            Command::MoveTo(p) => {
                let (x, y) = point(p);
                path.move_to(x, y);
            }
            Command::LineTo(p) => {
                let (x, y) = point(p);
                path.line_to(x, y);
            }
            Command::QuadTo(a, b) => {
                let ((x1, y1), (x2, y2)) = (point(a), point(b));
                path.quad_to(x1, y1, x2, y2);
            }
            Command::CurveTo(a, b, c) => {
                let ((x1, y1), (x2, y2), (x3, y3)) = (point(a), point(b), point(c));
                path.cubic_to(x1, y1, x2, y2, x3, y3);
            }
            Command::Close => path.close(),
        }
    }
    let Some(mut path) = path.finish() else { return };
    let mut fill_rule = FillRule::Winding;
    let mut st = outline.map(|outline| crate::raster::to_skia_stroke(&stroke(outline, color, 1.0)));
    if let Some(effect) = effect {
        let geometry = crate::compose_path_effect::StrokePath { path, stroke: st.take(), fill_rule };
        let Some(geometry) = geometry.apply(effect, 1.0) else { return };
        path = geometry.path;
        st = geometry.stroke;
        fill_rule = geometry.fill_rule;
    }
    let pad = st.as_ref().map_or(1.0, |st| st.width * st.miter_limit.max(1.0) / 2.0 + 1.0);
    let bounds = path.bounds();
    let x = (bounds.left() - pad).floor();
    let y = (bounds.top() - pad).floor();
    let w = ((bounds.right() + pad).ceil() - x) as u32;
    let h = ((bounds.bottom() + pad).ceil() - y) as u32;
    if w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > 4_194_304 { return; }
    let Some(mut pm) = Pixmap::new(w, h) else { return };
    let mut ink = Paint::default();
    ink.set_color_rgba8(255, 255, 255, 255);
    let transform = Transform::from_translate(-x, -y);
    if let Some(st) = st {
        pm.stroke_path(&path, &ink, &st, transform, None);
    } else {
        pm.fill_path(&path, &ink, fill_rule, transform, None);
    }
    let rgb = [color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8);
    for (i, pixel) in pm.pixels().iter().enumerate() {
        let a = (f32::from(pixel.alpha()) * color.a).round() as u8;
        if a > 0 {
            emit(x as i32 + (i as u32 % w) as i32, y as i32 + (i as u32 / w) as i32, [rgb[0], rgb[1], rgb[2], a]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_and_tile_modes_sample_in_the_painted_bounds() {
        let mut brush = ComposeBrush {
            kind: ComposeBrushKind::Linear,
            positions: vec![Some(0.0), Some(0.0), None, Some(0.0)],
            stops: vec![(0.0, [1.0, 0.0, 0.0, 0.5]), (1.0, [0.0, 0.0, 1.0, 1.0])],
            tile: 2,
            image: None,
        };
        let sampler = BrushSampler::new(&brush, 100.0, 50.0);
        assert_eq!(sampler.sample(150.0, 17.0), [0.5, 0.0, 0.5, 0.75]);
        brush.tile = 3;
        assert_eq!(BrushSampler::new(&brush, 100.0, 50.0).sample(101.0, 0.0), [0.0; 4]);
    }

    #[test]
    fn image_shader_tiles_each_axis_and_preserves_alpha() {
        let brush = ComposeBrush {
            kind: ComposeBrushKind::Image, stops: vec![], positions: vec![], tile: 0,
            image: Some(onesim_protocol::ComposeImageShader {
                width: 2, height: 2, tile_x: 1, tile_y: 2,
                rgba: base64::engine::general_purpose::STANDARD.encode([
                    255, 0, 0, 255, 0, 255, 0, 255,
                    255, 255, 0, 128, 0, 0, 255, 255,
                ]),
            }),
        };
        let sampler = BrushSampler::new(&brush, 10.0, 10.0);
        assert_eq!(sampler.sample(-0.5, 0.5), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(sampler.sample(2.5, 2.5), [1.0, 1.0, 0.0, 128.0 / 255.0]);
        assert_eq!(sampler.sample(3.5, 3.5), [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn shadow_blurs_and_offsets_the_alpha_mask() {
        let mut pixels = Vec::new();
        shadow(ComposeTextShadow([0.0, 0.0, 1.0, 1.0], 5.0, 8.0, 2.5),
            &[(1, 2, [255, 255, 255, 255])], |x, y, c| pixels.push((x, y, c)));
        let &(x, y, color) = pixels.iter().max_by_key(|p| p.2[3]).unwrap();
        assert_eq!((x, y), (6, 10));
        assert_eq!(color[..3], [0, 0, 255]);
        assert!(pixels.len() > 3 && color[3] < 255);
    }
}
