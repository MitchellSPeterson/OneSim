//! Isolated SwiftUI layers for view and Canvas effects.

use onesim_protocol::{BlendMode, VisualOp};
use tiny_skia::{Mask, Pixmap, PixmapPaint, Transform};

use crate::{Color, Rect};

#[derive(Debug, Clone)]
pub(crate) enum Composite {
    Effect(VisualOp, Option<Color>),
    Mask { inverted: bool },
}

fn blend_mode(mode: BlendMode) -> tiny_skia::BlendMode {
    use tiny_skia::BlendMode as Skia;
    match mode {
        BlendMode::Normal => Skia::SourceOver,
        BlendMode::Multiply => Skia::Multiply,
        BlendMode::Screen => Skia::Screen,
        BlendMode::Overlay => Skia::Overlay,
        BlendMode::Darken => Skia::Darken,
        BlendMode::Lighten => Skia::Lighten,
        BlendMode::ColorDodge => Skia::ColorDodge,
        BlendMode::ColorBurn => Skia::ColorBurn,
        BlendMode::SoftLight => Skia::SoftLight,
        BlendMode::HardLight => Skia::HardLight,
        BlendMode::Difference => Skia::Difference,
        BlendMode::Exclusion => Skia::Exclusion,
        BlendMode::Hue => Skia::Hue,
        BlendMode::Saturation => Skia::Saturation,
        BlendMode::Color => Skia::Color,
        BlendMode::Luminosity => Skia::Luminosity,
        BlendMode::SourceAtop => Skia::SourceAtop,
        BlendMode::DestinationOver => Skia::DestinationOver,
        BlendMode::DestinationOut => Skia::DestinationOut,
        BlendMode::PlusDarker | BlendMode::PlusLighter => Skia::Plus,
        BlendMode::Clear => Skia::Clear,
        BlendMode::Copy => Skia::Source,
        BlendMode::SourceIn => Skia::SourceIn,
        BlendMode::SourceOut => Skia::SourceOut,
        BlendMode::DestinationIn => Skia::DestinationIn,
        BlendMode::DestinationAtop => Skia::DestinationAtop,
        BlendMode::Xor => Skia::Xor,
    }
}

pub(crate) fn needs_blend_coverage(mode: BlendMode) -> bool {
    matches!(
        mode,
        BlendMode::Clear
            | BlendMode::Copy
            | BlendMode::SourceIn
            | BlendMode::SourceOut
            | BlendMode::DestinationIn
            | BlendMode::DestinationAtop
    )
}

pub(crate) fn apply_mask(layer: &mut Pixmap, source: &Pixmap, inverted: bool) {
    for (dst, src) in layer
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(source.pixels())
    {
        let a = if inverted {
            255 - src.alpha()
        } else {
            src.alpha()
        };
        for c in dst {
            *c = ((u16::from(*c) * u16::from(a) + 127) / 255) as u8;
        }
    }
}

pub(crate) fn composite(
    parent: &mut Pixmap,
    mut layer: Pixmap,
    effect: Option<(VisualOp, Option<Color>)>,
    bounds: Rect,
    scale: f32,
    opacity: f32,
    clip: Option<&Mask>,
) {
    let mut config = PixmapPaint {
        opacity,
        ..Default::default()
    };
    let mut affine = Transform::identity();
    if let Some((effect, color)) = effect {
        match effect {
            VisualOp::Affine {
                matrix: [a, b, c, d, e, f],
                anchor: [u, v],
            } => {
                let (x, y) = (bounds.x + u * bounds.w, bounds.y + v * bounds.h);
                let (tx, ty) = (e + x - a * x - c * y, f + y - b * x - d * y);
                affine = Transform::from_row(
                    a as f32,
                    b as f32,
                    c as f32,
                    d as f32,
                    tx as f32 * scale,
                    ty as f32 * scale,
                );
                if affine.invert().is_none() {
                    return;
                }
                config.quality = tiny_skia::FilterQuality::Bilinear;
            }
            VisualOp::Projection { matrix } => {
                let Some(projected) = project(&layer, matrix, bounds, scale) else {
                    return;
                };
                layer = projected;
            }
            VisualOp::Blend { mode } => {
                if mode == BlendMode::PlusDarker {
                    plus_darker(parent, &layer, opacity, clip);
                    return;
                }
                if needs_blend_coverage(mode) {
                    if let Some(coverage) = clip {
                        blend_with_coverage(parent, &layer, coverage, mode, opacity);
                    }
                    return;
                }
                config.blend_mode = blend_mode(mode);
            }
            VisualOp::Blur { radius, opaque } if radius > 0.0 => {
                blur(&mut layer, radius * f64::from(scale), opaque, bounds, scale);
            }
            VisualOp::Opacity { amount } => config.opacity *= amount.clamp(0.0, 1.0) as f32,
            VisualOp::Group { opaque } => {
                if opaque {
                    force_opaque(&mut layer, bounds, scale);
                }
            }
            VisualOp::Blur { .. } => {}
            effect => filter(&mut layer, effect, color),
        }
    }
    parent.draw_pixmap(0, 0, layer.as_ref(), &config, affine, clip);
}

fn blend_with_coverage(
    target: &mut Pixmap,
    source: &Pixmap,
    coverage: &Mask,
    mode: BlendMode,
    opacity: f32,
) {
    let pixels = target.data_mut().as_chunks_mut::<4>().0;
    let source = source.data().as_chunks::<4>().0;
    for ((dst, src), &coverage) in pixels.iter_mut().zip(source).zip(coverage.data()) {
        if coverage == 0 {
            continue;
        }
        let coverage = f32::from(coverage) / 255.0 * opacity;
        let sa = f32::from(src[3]) / 255.0;
        let da = f32::from(dst[3]) / 255.0;
        let (s, d) = match mode {
            BlendMode::Clear => (0.0, 0.0),
            BlendMode::Copy => (1.0, 0.0),
            BlendMode::SourceIn => (da, 0.0),
            BlendMode::SourceOut => (1.0 - da, 0.0),
            BlendMode::DestinationIn => (0.0, sa),
            BlendMode::DestinationAtop => (1.0 - da, sa),
            _ => unreachable!(),
        };
        let (s, d) = (s * coverage, 1.0 - coverage + d * coverage);
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst = (f32::from(src) * s + f32::from(*dst) * d).round() as u8;
        }
    }
}

fn plus_darker(parent: &mut Pixmap, layer: &Pixmap, opacity: f32, clip: Option<&Mask>) {
    for (i, (dst, src)) in parent
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(layer.data().as_chunks::<4>().0.iter())
        .enumerate()
    {
        let sa = f32::from(src[3]) / 255.0
            * opacity
            * clip.map_or(1.0, |m| f32::from(m.data()[i]) / 255.0);
        let da = f32::from(dst[3]) / 255.0;
        if sa == 0.0 {
            continue;
        }
        for c in 0..3 {
            let s = f32::from(src[c]) / f32::from(src[3]);
            let d = if da > 0.0 {
                f32::from(dst[c]) / f32::from(dst[3])
            } else {
                0.0
            };
            let blended = (s + d - 1.0).max(0.0);
            dst[c] = ((s * sa * (1.0 - da) + d * da * (1.0 - sa) + blended * sa * da) * 255.0)
                .round() as u8;
        }
        dst[3] = ((sa + da - sa * da) * 255.0).round() as u8;
    }
}

fn force_opaque(layer: &mut Pixmap, bounds: Rect, scale: f32) {
    if let Some(area) = crate::paint::Tile::new(layer, bounds, bounds, scale) {
        let stride = layer.width() as usize * 4;
        for row in layer
            .data_mut()
            .chunks_exact_mut(stride)
            .skip(area.y as usize)
            .take(area.h as usize)
        {
            for p in row[area.x as usize * 4..(area.x as usize + area.w as usize) * 4]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
            {
                p[3] = 255;
            }
        }
    }
}

fn dirty_rect(pixmap: &Pixmap) -> Option<(usize, usize, usize, usize)> {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let data = pixmap.data();
    let row_live = |r: usize| {
        let base = r * w * 4;
        (0..w).any(|c| data[base + c * 4 + 3] != 0)
    };
    let y0 = (0..h).find(|&r| row_live(r))?;
    let y1 = (0..h).rev().find(|&r| row_live(r))? + 1;
    let col_live = |c: usize| (y0..y1).any(|r| data[(r * w + c) * 4 + 3] != 0);
    let x0 = (0..w).find(|&c| col_live(c))?;
    let x1 = (0..w).rev().find(|&c| col_live(c))? + 1;
    Some((x0, y0, x1, y1))
}

fn project(source: &Pixmap, matrix: [f64; 9], bounds: Rect, scale: f32) -> Option<Pixmap> {
    let [a, b, c, d, e, f, g, h, i] = matrix;
    let inverse = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let det = a * inverse[0] + b * inverse[3] + c * inverse[6];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let mut output = Pixmap::new(source.width(), source.height())?;
    let Some((sx0, sy0, sx1, sy1)) = dirty_rect(source) else {
        return Some(output);
    };
    let (sw, sh) = (source.width() as usize, source.height() as usize);
    let scale_f = f64::from(scale);
    // Project the 4 corners of the source dirty rect forward to find a conservative destination rect.
    let corners = [
        (sx0 as f64 / scale_f - bounds.x, sy0 as f64 / scale_f - bounds.y),
        (sx1 as f64 / scale_f - bounds.x, sy0 as f64 / scale_f - bounds.y),
        (sx0 as f64 / scale_f - bounds.x, sy1 as f64 / scale_f - bounds.y),
        (sx1 as f64 / scale_f - bounds.x, sy1 as f64 / scale_f - bounds.y),
    ];
    let (mut dx0, mut dy0, mut dx1, mut dy1) = (0usize, 0usize, sw, sh);
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut valid_bounds = true;
    for (cx, cy) in corners {
        let w = cx * c + cy * f + i;
        if w <= 1e-6 {
            valid_bounds = false;
            break;
        }
        let px = ((cx * a + cy * d + g) / w + bounds.x) * scale_f;
        let py = ((cx * b + cy * e + h) / w + bounds.y) * scale_f;
        if !px.is_finite() || !py.is_finite() {
            valid_bounds = false;
            break;
        }
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    if valid_bounds {
        dx0 = ((min_x.floor() as isize) - 1).clamp(0, sw as isize) as usize;
        dy0 = ((min_y.floor() as isize) - 1).clamp(0, sh as isize) as usize;
        dx1 = ((max_x.ceil() as isize) + 1).clamp(0, sw as isize) as usize;
        dy1 = ((max_y.ceil() as isize) + 1).clamp(0, sh as isize) as usize;
    }
    if dx0 >= dx1 || dy0 >= dy1 {
        return Some(output);
    }
    let [ia, ib, ic, id, ie, if_, ig, ih, ii] = inverse;
    let src_data = source.data();
    let sw_f = sw as f64;
    let sh_f = sh as f64;
    let inv_scale = 1.0 / scale_f;
    for (y, row) in output
        .data_mut()
        .chunks_exact_mut(sw * 4)
        .enumerate()
        .skip(dy0)
        .take(dy1 - dy0)
    {
        let py = (y as f64 + 0.5) * inv_scale - bounds.y;
        let row_pixels = &mut row.as_chunks_mut::<4>().0[dx0..dx1];
        for (offset, dst) in row_pixels.iter_mut().enumerate() {
            let x = dx0 + offset;
            let px = (x as f64 + 0.5) * inv_scale - bounds.x;
            let w = px * ic + py * if_ + ii;
            if w == 0.0 {
                continue;
            }
            let sx = (((px * ia + py * id + ig) / w + bounds.x) * scale_f).floor();
            let sy = (((px * ib + py * ie + ih) / w + bounds.y) * scale_f).floor();
            if sx >= 0.0 && sy >= 0.0 && sx < sw_f && sy < sh_f {
                let idx = (sy as usize * sw + sx as usize) * 4;
                dst.copy_from_slice(&src_data[idx..idx + 4]);
            }
        }
    }
    Some(output)
}

fn filter(layer: &mut Pixmap, effect: VisualOp, color: Option<Color>) {
    let pixels = layer.data_mut().as_chunks_mut::<4>().0;
    let apply_matrix = |pixels: &mut [[u8; 4]], m: [[f32; 3]; 3], bias: [f32; 3]| {
        for px in pixels {
            let a_u8 = px[3];
            if a_u8 == 0 {
                continue;
            }
            let a_f = f32::from(a_u8);
            let inv_a = 1.0 / a_f;
            let r = f32::from(px[0]) * inv_a;
            let g = f32::from(px[1]) * inv_a;
            let b = f32::from(px[2]) * inv_a;
            for (c, (row, &b_c)) in m.iter().zip(&bias).enumerate() {
                let v = (row[0] * r + row[1] * g + row[2] * b + b_c).clamp(0.0, 1.0);
                px[c] = (v * a_f).round() as u8;
            }
        }
    };
    match effect {
        VisualOp::Brightness { amount } => {
            let b = amount as f32;
            apply_matrix(pixels, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [b, b, b]);
        }
        VisualOp::Contrast { amount } => {
            let k = amount as f32;
            let b = 0.5 - 0.5 * k;
            apply_matrix(pixels, [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]], [b, b, b]);
        }
        VisualOp::Grayscale { amount } => {
            let t = amount.clamp(0.0, 1.0) as f32;
            let inv = 1.0 - t;
            let (lr, lg, lb) = (0.2126 * t, 0.7152 * t, 0.0722 * t);
            apply_matrix(
                pixels,
                [[inv + lr, lg, lb], [lr, inv + lg, lb], [lr, lg, inv + lb]],
                [0.0; 3],
            );
        }
        VisualOp::Saturation { amount } => {
            let s = amount as f32;
            let t = 1.0 - s;
            let (lr, lg, lb) = (0.2126 * t, 0.7152 * t, 0.0722 * t);
            apply_matrix(
                pixels,
                [[s + lr, lg, lb], [lr, s + lg, lb], [lr, lg, s + lb]],
                [0.0; 3],
            );
        }
        VisualOp::ColorInvert { amount } => {
            let t = amount.clamp(0.0, 1.0) as f32;
            let k = 1.0 - 2.0 * t;
            apply_matrix(pixels, [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, k]], [t, t, t]);
        }
        VisualOp::HueRotation { radians } => {
            let (s, c) = radians.sin_cos();
            let (s, c) = (s as f32, c as f32);
            apply_matrix(
                pixels,
                [
                    [0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928],
                    [0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.140, 0.072 - c * 0.072 - s * 0.283],
                    [0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072],
                ],
                [0.0; 3],
            );
        }
        VisualOp::ColorMultiply { .. } => {
            let color = color.expect("resolved multiply color");
            let (cr, cg, cb, ca) = (color.r, color.g, color.b, color.a);
            for px in pixels {
                if px[3] == 0 {
                    continue;
                }
                let a_f = f32::from(px[3]);
                let alpha = (a_f / 255.0 * ca).clamp(0.0, 1.0) * 255.0;
                let inv_a = 1.0 / a_f;
                px[0] = ((f32::from(px[0]) * inv_a * cr).clamp(0.0, 1.0) * alpha).round() as u8;
                px[1] = ((f32::from(px[1]) * inv_a * cg).clamp(0.0, 1.0) * alpha).round() as u8;
                px[2] = ((f32::from(px[2]) * inv_a * cb).clamp(0.0, 1.0) * alpha).round() as u8;
                px[3] = alpha.round() as u8;
            }
        }
        VisualOp::AlphaThreshold { min, max, .. } => {
            let color = color.expect("resolved threshold color");
            let alpha_on = color.a.clamp(0.0, 1.0) * 255.0;
            let on_px = [
                (color.r.clamp(0.0, 1.0) * alpha_on).round() as u8,
                (color.g.clamp(0.0, 1.0) * alpha_on).round() as u8,
                (color.b.clamp(0.0, 1.0) * alpha_on).round() as u8,
                alpha_on.round() as u8,
            ];
            let (min_f, max_f) = (min as f32, max as f32);
            for px in pixels {
                if px[3] == 0 {
                    continue;
                }
                let a = f32::from(px[3]) / 255.0;
                *px = if (min_f..=max_f).contains(&a) { on_px } else { [0; 4] };
            }
        }
        VisualOp::LuminanceToAlpha => {
            for px in pixels {
                if px[3] == 0 {
                    continue;
                }
                let lum = 0.2126 * f32::from(px[0]) + 0.7152 * f32::from(px[1]) + 0.0722 * f32::from(px[2]);
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                px[3] = lum.clamp(0.0, 255.0).round() as u8;
            }
        }
        _ => {}
    }
}

fn blur(layer: &mut Pixmap, sigma: f64, opaque: bool, bounds: Rect, scale: f32) {
    // When the blur is opaque, extend the color at the view's edges and keep its original extent.
    let mut viewport = None;
    if opaque {
        let Some(area) = crate::paint::Tile::new(layer, bounds, bounds, scale) else {
            layer.fill(tiny_skia::Color::TRANSPARENT);
            return;
        };
        let rect = tiny_skia::IntRect::from_xywh(area.x, area.y, area.w, area.h).unwrap();
        let Some(mut tile) = layer.clone_rect(rect) else {
            return;
        };
        force_opaque(
            &mut tile,
            Rect {
                x: 0.0,
                y: 0.0,
                w: f64::from(area.w),
                h: f64::from(area.h),
            },
            1.0,
        );
        std::mem::swap(layer, &mut tile);
        viewport = Some((tile, area.x, area.y));
    } else if let Some((x0, y0, x1, y1)) = dirty_rect(layer) {
        let (lw, lh) = (layer.width() as usize, layer.height() as usize);
        let pad = if sigma < 1_000_000.0 {
            crate::glass::box_radii(sigma as f32).iter().sum::<usize>()
        } else {
            lw.max(lh)
        };
        let rx0 = x0.saturating_sub(pad);
        let ry0 = y0.saturating_sub(pad);
        let rx1 = (x1 + pad).min(lw);
        let ry1 = (y1 + pad).min(lh);
        if rx0 > 0 || ry0 > 0 || rx1 < lw || ry1 < lh {
            if let Some(rect) = tiny_skia::IntRect::from_xywh(rx0 as i32, ry0 as i32, (rx1 - rx0) as u32, (ry1 - ry0) as u32) {
                if let Some(mut tile) = layer.clone_rect(rect) {
                    std::mem::swap(layer, &mut tile);
                    viewport = Some((tile, rx0 as i32, ry0 as i32));
                }
            }
        }
    } else {
        return;
    }

    let (w, h) = (layer.width() as usize, layer.height() as usize);
    let mut plane = vec![0.0_f32; w * h];
    let mut tmp = vec![0.0_f32; w * h];
    let mut acc = vec![0.0_f32; w];
    let large = sigma >= 1_000_000.0;
    let radii_usize = if large { [0; 3] } else { crate::glass::box_radii(sigma as f32) };
    let huge_r = sigma.min(f64::MAX / 4.0);

    for channel in 0..4 {
        for (out, px) in plane.iter_mut().zip(layer.data().as_chunks::<4>().0.iter()) {
            *out = f32::from(px[channel]);
        }
        if !large {
            for r in radii_usize {
                if r == 0 {
                    continue;
                }
                for (src, dst) in plane.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
                    box_line_f32(src, dst, r, opaque);
                }
                box_columns_f32(&tmp, &mut plane, &mut acc, w, h, r, opaque);
            }
        } else {
            let mut plane64: Vec<f64> = plane.iter().map(|&v| f64::from(v)).collect();
            let mut tmp64 = vec![0.0_f64; w * h];
            for _ in 0..3 {
                box_pass_f64(&plane64, &mut tmp64, w, h, huge_r, opaque);
                box_pass_f64(&tmp64, &mut plane64, h, w, huge_r, opaque);
            }
            for (dst, &src) in plane.iter_mut().zip(&plane64) {
                *dst = src as f32;
            }
        }
        for (out, px) in plane
            .iter()
            .zip(layer.data_mut().as_chunks_mut::<4>().0.iter_mut())
        {
            px[channel] = out.round().clamp(0.0, 255.0) as u8;
        }
    }
    if let Some((mut parent, x, y)) = viewport {
        parent.fill(tiny_skia::Color::TRANSPARENT);
        parent.draw_pixmap(
            x,
            y,
            layer.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        *layer = parent;
    }
}

/// Fast row-vectorized horizontal box blur (matches `glass::box_line`, with optional edge replication).
fn box_line_f32(src: &[f32], dst: &mut [f32], r: usize, pad: bool) {
    let n = src.len() as isize;
    let (ri, inv) = (r as isize, 1.0 / (2 * r + 1) as f32);
    if pad {
        let at = |i: isize| src[i.clamp(0, n - 1) as usize];
        let mut acc: f32 = (-ri..=ri).map(at).sum();
        for (i, d) in dst.iter_mut().enumerate() {
            *d = acc * inv;
            acc += at(i as isize + ri + 1) - at(i as isize - ri);
        }
    } else {
        let reach = r.min(src.len().saturating_sub(1));
        let mut acc: f32 = src[..=reach].iter().sum();
        for (i, d) in dst.iter_mut().enumerate() {
            *d = acc * inv;
            let add_i = i + r + 1;
            if add_i < src.len() {
                acc += src[add_i];
            }
            if i >= r {
                acc -= src[i - r];
            }
        }
    }
}

/// Fast row-vectorized vertical box blur (matches `glass::box_columns`, with optional edge replication).
fn box_columns_f32(src: &[f32], dst: &mut [f32], acc: &mut [f32], w: usize, h: usize, r: usize, pad: bool) {
    let (ri, inv) = (r as isize, 1.0 / (2 * r + 1) as f32);
    acc.fill(0.0);
    if pad {
        let row = |y: isize| &src[y.clamp(0, h as isize - 1) as usize * w..][..w];
        for y in -ri..=ri {
            acc.iter_mut().zip(row(y)).for_each(|(a, &v)| *a += v);
        }
        for (y, out) in dst.chunks_exact_mut(w).enumerate() {
            out.iter_mut().zip(&*acc).for_each(|(o, &a)| *o = a * inv);
            let (add, sub) = (row(y as isize + ri + 1), row(y as isize - ri));
            acc.iter_mut().zip(add.iter().zip(sub)).for_each(|(a, (&p, &m))| *a += p - m);
        }
    } else {
        let reach = r.min(h.saturating_sub(1));
        for y in 0..=reach {
            let row = &src[y * w..][..w];
            acc.iter_mut().zip(row).for_each(|(a, &v)| *a += v);
        }
        for (y, out) in dst.chunks_exact_mut(w).enumerate() {
            out.iter_mut().zip(&*acc).for_each(|(o, &a)| *o = a * inv);
            let add_y = y + r + 1;
            if add_y < h {
                let add = &src[add_y * w..][..w];
                acc.iter_mut().zip(add).for_each(|(a, &p)| *a += p);
            }
            if y >= r {
                let sub = &src[(y - r) * w..][..w];
                acc.iter_mut().zip(sub).for_each(|(a, &m)| *a -= m);
            }
        }
    }
}

fn box_pass_f64(source: &[f64], output: &mut [f64], w: usize, h: usize, radius: f64, pad: bool) {
    let reach = radius.min(w as f64) as isize;
    let inv = 1.0 / (2.0 * radius + 1.0);
    for (y, line) in source.chunks_exact(w).enumerate().take(h) {
        let at = |x: isize| {
            if pad {
                line[x.clamp(0, w as isize - 1) as usize]
            } else {
                line.get(x as usize).copied().unwrap_or(0.0)
            }
        };
        let mut sum = line.iter().take(reach as usize + 1).sum::<f64>() * inv;
        if pad {
            sum +=
                (radius * inv) * line[0] + ((radius - (w - 1) as f64).max(0.0) * inv) * line[w - 1];
        }
        for x in 0..w {
            output[x * h + y] = sum;
            if radius < w as f64 {
                let x = x as isize;
                sum += (at(x + reach + 1) - at(x - reach)) * inv;
            } else if pad {
                sum += (line[w - 1] - line[0]) * inv;
            }
        }
    }
}
