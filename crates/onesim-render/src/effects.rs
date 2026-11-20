//! Isolated SwiftUI layers for view and Canvas effects.

use onesim_protocol::{BlendMode, VisualOp};
use tiny_skia::{Mask, Pixmap, PixmapPaint, Transform};

use crate::{Color, Rect};

#[derive(Debug, Clone, Copy)]
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
        BlendMode::Clear => Skia::DestinationOut,
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
        BlendMode::Copy
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
                config.blend_mode = blend_mode(mode);
                if needs_blend_coverage(mode) {
                    let Some(area) = crate::paint::Tile::new(parent, bounds, bounds, scale) else {
                        return;
                    };
                    let Some(rect) = tiny_skia::IntRect::from_xywh(area.x, area.y, area.w, area.h)
                    else {
                        return;
                    };
                    if let Some(tile) = layer.clone_rect(rect) {
                        parent.draw_pixmap(area.x, area.y, tile.as_ref(), &config, affine, clip);
                    }
                    return;
                }
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
    let [a, b, c, d, e, f, g, h, i] = inverse;
    let mut output = Pixmap::new(source.width(), source.height())?;
    let scale = f64::from(scale);
    for (y, row) in output
        .data_mut()
        .chunks_exact_mut(source.width() as usize * 4)
        .enumerate()
    {
        for (x, dst) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (
                (x as f64 + 0.5) / scale - bounds.x,
                (y as f64 + 0.5) / scale - bounds.y,
            );
            let w = x * c + y * f + i;
            if w == 0.0 {
                continue;
            }
            let (sx, sy) = (
                (((x * a + y * d + g) / w + bounds.x) * scale).floor(),
                (((x * b + y * e + h) / w + bounds.y) * scale).floor(),
            );
            if sx >= 0.0
                && sy >= 0.0
                && sx < f64::from(source.width())
                && sy < f64::from(source.height())
            {
                let idx = (sy as usize * source.width() as usize + sx as usize) * 4;
                dst.copy_from_slice(&source.data()[idx..idx + 4]);
            }
        }
    }
    Some(output)
}

fn filter(layer: &mut Pixmap, effect: VisualOp, color: Option<Color>) {
    let gray = |[r, g, b]: [f64; 3]| 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let hue_components = match effect {
        VisualOp::HueRotation { radians } => radians.sin_cos(),
        _ => (0.0, 1.0),
    };
    for px in layer.data_mut().as_chunks_mut::<4>().0 {
        let mut alpha = f64::from(px[3]) / 255.0;
        if alpha == 0.0 {
            continue;
        }
        let [r, g, b] = [px[0], px[1], px[2]].map(|c| f64::from(c) / f64::from(px[3]));
        let rgb = [r, g, b];
        let channels = match effect {
            VisualOp::Brightness { amount } => rgb.map(|c| c + amount),
            VisualOp::Contrast { amount } => rgb.map(|c| (c - 0.5) * amount + 0.5),
            VisualOp::Grayscale { amount } => {
                rgb.map(|c| c + (gray(rgb) - c) * amount.clamp(0.0, 1.0))
            }
            VisualOp::Saturation { amount } => rgb.map(|c| gray(rgb) + (c - gray(rgb)) * amount),
            VisualOp::ColorInvert { amount } => {
                rgb.map(|c| c + (1.0 - 2.0 * c) * amount.clamp(0.0, 1.0))
            }
            VisualOp::ColorMultiply { .. } => {
                let color = color.expect("resolved multiply color");
                alpha *= f64::from(color.a);
                [
                    r * f64::from(color.r),
                    g * f64::from(color.g),
                    b * f64::from(color.b),
                ]
            }
            VisualOp::AlphaThreshold { min, max, .. } => {
                let color = color.expect("resolved threshold color");
                alpha = if (min..=max).contains(&alpha) {
                    f64::from(color.a)
                } else {
                    0.0
                };
                [color.r, color.g, color.b].map(f64::from)
            }
            VisualOp::HueRotation { .. } => {
                let (s, c) = hue_components;
                [
                    (0.213 + c * 0.787 - s * 0.213) * r
                        + (0.715 - c * 0.715 - s * 0.715) * g
                        + (0.072 - c * 0.072 + s * 0.928) * b,
                    (0.213 - c * 0.213 + s * 0.143) * r
                        + (0.715 + c * 0.285 + s * 0.140) * g
                        + (0.072 - c * 0.072 - s * 0.283) * b,
                    (0.213 - c * 0.213 - s * 0.787) * r
                        + (0.715 - c * 0.715 + s * 0.715) * g
                        + (0.072 + c * 0.928 + s * 0.072) * b,
                ]
            }
            VisualOp::LuminanceToAlpha => {
                alpha *= gray(rgb);
                [0.0; 3]
            }
            _ => rgb,
        };
        let alpha = alpha.clamp(0.0, 1.0) * 255.0;
        for (out, value) in px.iter_mut().zip(channels) {
            *out = (value.clamp(0.0, 1.0) * alpha).round() as u8;
        }
        px[3] = alpha.round() as u8;
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
    }
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    let mut plane = vec![0.0; w * h];
    let mut temporary = vec![0.0; w * h];
    // Three separable boxes approximate a Gaussian. Beyond the exact radii's integer
    // range, use the continuous approximation and keep running sums in floating point.
    let radii = if sigma < 1_000_000.0 {
        crate::glass::box_radii(sigma as f32).map(|n| n as f64)
    } else {
        [sigma.min(f64::MAX / 4.0); 3]
    };
    for channel in 0..4 {
        for (out, px) in plane.iter_mut().zip(layer.data().as_chunks::<4>().0.iter()) {
            *out = f64::from(px[channel]);
        }
        for radius in radii {
            box_pass(&plane, &mut temporary, w, h, radius, opaque);
            box_pass(&temporary, &mut plane, h, w, radius, opaque);
        }
        for (out, px) in plane
            .iter()
            .zip(layer.data_mut().as_chunks_mut::<4>().0.iter_mut())
        {
            px[channel] = out.round() as u8;
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

fn box_pass(source: &[f64], output: &mut [f64], w: usize, h: usize, radius: f64, pad: bool) {
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
