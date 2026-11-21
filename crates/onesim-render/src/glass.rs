//! Liquid Glass backdrop effect (docs/protocol.md section F).
//!
//! A glass node samples the pixels already painted beneath its shape and replaces them with:
//! blurred backdrop -> lens displacement near the rim -> saturation/brightness transform -> tint ->
//! specular rim + soft outer shadow. CPU-only, deterministic, no threads or SIMD (wasm32-wasip1 safe).
//! Parameters are fitted per iOS release: 26.5 (`tests/fixtures/showcase-ios/README.md`) and 27.2
//! (`tests/fixtures/ios27/showcase-ios/README.md`: no outer shadow, a dark hairline outside the edge,
//! highlights along the top and bottom instead of the diagonal rim, a more saturated backdrop).

pub use onesim_display::glass::*;
use onesim_protocol::IosVersion;
use tiny_skia::{Mask, Pixmap};

/// Backdrop copy (RGB, f32 0..255) taken before the node's own shadow is painted.
pub struct Snapshot {
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    rgb: [Vec<f32>; 3],
}

/// Copy the pixels around `shape` (padded by the blur radius and lens reach).
pub fn snapshot(pm: &Pixmap, shape: Shape, scale: f32, p: &Params) -> Snapshot {
    let (x0, y0, w, h) = snapshot_rect(shape, scale, p, pm.width(), pm.height());
    let mut rgb = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    // Mirror a page coordinate into [lo, hi) (flat materials see only their own backdrop).
    let fold = |v: i32, lo: f32, hi: f32| {
        let (lo, hi) = (lo.round() as i32, (hi.round() as i32).max(lo.round() as i32 + 1));
        if !p.mirror {
            return v;
        }
        let n = hi - lo;
        let m = (v - lo).rem_euclid(2 * n);
        lo + if m < n { m } else { 2 * n - 1 - m }
    };
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (fold(x0 + x as i32, shape.x, shape.x + shape.w), fold(y0 + y as i32, shape.y, shape.y + shape.h));
            if let Some(px) = pm.pixel(px as u32, py as u32) {
                let c = px.demultiply();
                rgb[0][y * w + x] = c.red() as f32;
                rgb[1][y * w + x] = c.green() as f32;
                rgb[2][y * w + x] = c.blue() as f32;
            }
        }
    }
    Snapshot { x0, y0, w, h, rgb }
}

/// Gaussian blur approximated by three box blurs (Kovesi's widths), edges replicated. Constant cost
/// per pixel whatever the sigma; within a few levels of the exact kernel on backdrops, far below
/// what the pixel-diff rule counts (24/255).
fn box_blur3(plane: &mut [f32], w: usize, h: usize, sigma: f32) {
    let mut tmp = vec![0f32; plane.len()];
    for r in box_radii(sigma) {
        for (src, dst) in plane.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
            box_line(src, dst, r);
        }
        box_columns(&tmp, plane, w, h, r);
    }
}

/// Vertical box blur of radius `r`: a running sum over whole rows (vectorizes; no transpose).
fn box_columns(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let row = |y: isize| &src[y.clamp(0, h as isize - 1) as usize * w..][..w];
    let (r, inv) = (r as isize, 1.0 / (2 * r + 1) as f32);
    let mut acc = vec![0f32; w];
    for y in -r..=r {
        acc.iter_mut().zip(row(y)).for_each(|(a, v)| *a += v);
    }
    for (y, out) in dst.chunks_exact_mut(w).enumerate() {
        out.iter_mut().zip(&acc).for_each(|(o, a)| *o = a * inv);
        let (add, sub) = (row(y as isize + r + 1), row(y as isize - r));
        acc.iter_mut().zip(add.iter().zip(sub)).for_each(|(a, (p, m))| *a += p - m);
    }
}

/// Mean over `[i - r, i + r]` (edge samples repeated), as a running sum.
fn box_line(src: &[f32], dst: &mut [f32], r: usize) {
    let n = src.len() as isize;
    let at = |i: isize| src[i.clamp(0, n - 1) as usize];
    let (r, inv) = (r as isize, 1.0 / (2 * r + 1) as f32);
    let mut acc: f32 = (-r..=r).map(at).sum();
    for (i, d) in dst.iter_mut().enumerate() {
        *d = acc * inv;
        acc += at(i as isize + r + 1) - at(i as isize - r);
    }
}

fn bilinear(pl: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x.clamp(0.0, w as f32 - 1.0), y.clamp(0.0, h as f32 - 1.0));
    let (xi, yi) = (x.floor() as usize, y.floor() as usize);
    let (xj, yj) = ((xi + 1).min(w - 1), (yi + 1).min(h - 1));
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let top = pl[yi * w + xi] * (1.0 - fx) + pl[yi * w + xj] * fx;
    let bot = pl[yj * w + xi] * (1.0 - fx) + pl[yj * w + xj] * fx;
    top * (1.0 - fy) + bot * fy
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Paint the glass for `spec` over `shape`. `snap` must come from [`snapshot`] with the same `shape`.
#[allow(clippy::too_many_arguments)]
pub fn draw(pm: &mut Pixmap, snap: Snapshot, shape: Shape, spec: &GlassSpec, scale: f32, dark: bool, ios: IosVersion, opacity: f32, mask: Option<&Mask>) {
    let p = spec_params(spec, dark, ios);
    if spec.variant == GlassVariant::Identity {
        return;
    }
    let Snapshot { x0, y0, w, h, mut rgb } = snap;
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let (lens_d, lens_w) = (p.lens_d * scale, (p.lens_w * scale).max(0.5));
    let ow = p.outline_w * scale;
    let grow = 1 + ow.ceil() as i32;
    let (sx0, sy0) = (shape.x.floor() as i32 - grow, shape.y.floor() as i32 - grow);
    let (sx1, sy1) = ((shape.x + shape.w).ceil() as i32 + grow, (shape.y + shape.h).ceil() as i32 + grow);
    for pl in rgb.iter_mut() {
        box_blur3(pl, w, h, p.sigma * scale);
    }
    let tint = spec.tint.map(|t| {
        let sh = p.tint_shift * 255.0;
        ([(t.r * 255.0 + sh).clamp(0.0, 255.0), (t.g * 255.0 + sh).clamp(0.0, 255.0), (t.b * 255.0 + sh).clamp(0.0, 255.0)], t.a)
    });
    let rim_w = (p.rim_w * scale).max(1.0);
    let mask_data = mask.map(|m| m.data());
    let data = pm.data_mut();
    for y in sy0.max(0)..sy1.min(ph) {
        for x in sx0.max(0)..sx1.min(pw) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (sd, nx, ny) = shape.sdf(fx, fy);
            let cov = (0.5 - sd).clamp(0.0, 1.0);
            // Dark hairline outside the outline (over whatever is painted there).
            if p.outline_a > 0.0 && sd > -0.5 && sd < ow + 0.5 {
                let k = (sd + 0.5).clamp(0.0, 1.0)
                    * (ow + 0.5 - sd).clamp(0.0, 1.0)
                    * p.outline_a
                    * (0.3 + 0.7 * nx.abs())
                    * opacity
                    * mask_data.map_or(1.0, |m| m[(y * pw + x) as usize] as f32 / 255.0);
                let i = ((y * pw + x) * 4) as usize;
                for ch in 0..3 {
                    data[i + ch] = mix(data[i + ch] as f32, 0.0, k).round() as u8;
                }
            }
            if cov <= 0.0 {
                continue;
            }
            let cov = cov * opacity * mask_data.map_or(1.0, |m| m[(y * pw + x) as usize] as f32 / 255.0);
            if cov <= 0.0 {
                continue;
            }
            let d = (-sd).max(0.0);
            let t = (1.0 - d / lens_w).max(0.0);
            let disp = lens_d * t * t;
            let (lx, ly) = (fx - nx * disp - x0 as f32 - 0.5, fy - ny * disp - y0 as f32 - 0.5);
            let mut c = [bilinear(&rgb[0], w, h, lx, ly), bilinear(&rgb[1], w, h, lx, ly), bilinear(&rgb[2], w, h, lx, ly)];
            let lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            for v in c.iter_mut() {
                *v = ((lum + (*v - lum) * p.sat) * p.mul + p.add * 255.0).clamp(0.0, 255.0);
            }
            if let Some((tc, ta)) = tint {
                for (v, t) in c.iter_mut().zip(tc) {
                    *v = mix(*v, t, ta);
                }
            }
            // Specular rim: brightest at the outline, favouring the top-left and bottom-right.
            if d < rim_w + 1.0 {
                let dir = if p.rim_vertical { ny } else { (nx + ny) * -std::f32::consts::FRAC_1_SQRT_2 }; // -1 (bottom-right) .. 1 (top-left)
                let k = (1.0 - d / rim_w).clamp(0.0, 1.0) * p.rim_a * (1.0 - p.rim_dir + p.rim_dir * dir.abs());
                for v in c.iter_mut() {
                    *v = mix(*v, 255.0, k);
                }
            }
            let i = ((y * pw + x) * 4) as usize;
            for (ch, v) in c.iter().enumerate() {
                let old = data[i + ch] as f32;
                data[i + ch] = mix(old, *v, cov).round().clamp(0.0, 255.0) as u8;
            }
            data[i + 3] = 255;
        }
    }
}

pub fn draw_edge(pm: &mut Pixmap, e: &ScrollEdge, scale: f32, ios: IosVersion, mask: Option<&Mask>) {
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let rows = edge_rows(e, scale, ph);
    let col = [e.color.r * 255.0, e.color.g * 255.0, e.color.b * 255.0];
    // Blurred copy of the band, computed at ~1 px per point (it is soft anyway) and sampled back up.
    let f = (scale.round() as usize).max(1);
    let (bw, bh) = (pw.div_ceil(f), rows.div_ceil(f));
    let mut planes = [vec![0f32; bw * bh], vec![0f32; bw * bh], vec![0f32; bw * bh]];
    {
        let data = pm.data();
        for by in 0..bh {
            for bx in 0..bw {
                let (mut acc, mut n) = ([0f32; 3], 0f32);
                for y in by * f..((by + 1) * f).min(rows) {
                    for x in bx * f..((bx + 1) * f).min(pw) {
                        let i = (y * pw + x) * 4;
                        (0..3).for_each(|c| acc[c] += data[i + c] as f32);
                        n += 1.0;
                    }
                }
                (0..3).for_each(|c| planes[c][by * bw + bx] = acc[c] / n.max(1.0));
            }
        }
    }
    let sigma = EDGE_SIGMA * scale / f as f32;
    for pl in planes.iter_mut() {
        box_blur3(pl, bw, bh, sigma);
    }
    // Bilinear upsampling, separated: one horizontal mapping for all rows, one vertical lerp per row.
    let xmap: Vec<(usize, usize, f32)> = (0..pw)
        .map(|x| {
            let sx = ((x as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, bw as f32 - 1.0);
            let xi = sx.floor() as usize;
            (xi, (xi + 1).min(bw - 1), sx - xi as f32)
        })
        .collect();
    let mut row = [vec![0f32; bw], vec![0f32; bw], vec![0f32; bw]];
    let mask_data = mask.map(|m| m.data());
    let data = pm.data_mut();
    for y in 0..rows {
        let [a, bwt, t, hair] = edge_row(e, y, scale, ios);
        if bwt > 0.0 {
            let sy = ((y as f32 + 0.5) / f as f32 - 0.5).clamp(0.0, bh as f32 - 1.0);
            let (yi, fy) = (sy.floor() as usize, sy - sy.floor());
            let yj = (yi + 1).min(bh - 1);
            for (r, pl) in row.iter_mut().zip(&planes) {
                for (bx, v) in r.iter_mut().enumerate() {
                    *v = pl[yi * bw + bx] * (1.0 - fy) + pl[yj * bw + bx] * fy;
                }
            }
        }
        let px = |d: f32, c: usize, m: f32| {
            let v = mix(d, col[c], a) - t * 255.0;
            let v = mix(d, v.clamp(0.0, 255.0), m);
            mix(v, if hair > 0.0 { 255.0 } else { 0.0 }, hair.abs() * m)
        };
        // The veil is constant along the row: unmasked pixels come from a table.
        let lut: [[u8; 256]; 3] = std::array::from_fn(|c| std::array::from_fn(|d| px(d as f32, c, 1.0).round() as u8));
        for (x, &(xi, xj, fx)) in xmap.iter().enumerate() {
            let mb = mask_data.map_or(255, |m| m[y * pw + x]);
            let i = (y * pw + x) * 4;
            for c in 0..3 {
                let d = data[i + c] as f32;
                let d = if bwt > 0.0 { mix(d, row[c][xi] * (1.0 - fx) + row[c][xj] * fx, bwt) } else { d };
                data[i + c] = if mb == 255 { lut[c][d.round() as usize] } else { px(d, c, mb as f32 / 255.0).round() as u8 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::render;
    use crate::scene::{Color, Node, Rect};
    use crate::text::Fonts;
    use onesim_protocol::Appearance;

    fn scene(dark: bool, variant: GlassVariant, tint: Option<Color>) -> crate::Scene {
        let mut bg = Node::new("bg", "screen");
        bg.bounds = Rect { x: 0.0, y: 0.0, w: 200.0, h: 200.0 };
        bg.fill = Some(Color::rgba(203.0 / 255.0, 48.0 / 255.0, 224.0 / 255.0, 1.0));
        let mut g = Node::new("g", "glass");
        g.bounds = Rect { x: 50.0, y: 50.0, w: 100.0, h: 100.0 };
        g.radius = 20.0;
        g.glass = Some(GlassSpec { variant, tint, interactive: false, blur: None, mix: None, shadow: None, flat: None });
        let appearance = if dark { Appearance::Dark } else { Appearance::Light };
        crate::Scene { size: (200.0, 200.0), appearance,
            ios: IosVersion::Ios26, nodes: vec![bg, g], unsupported_symbols: vec![], notes: vec![] }
    }

    fn center(s: &crate::Scene) -> [u8; 3] {
        let pm = render(s, &mut Fonts::new(), 3.0);
        let p = pm.pixel(300, 300).unwrap();
        [p.red(), p.green(), p.blue()]
    }

    #[test]
    fn interior_matches_fitted_transforms() {
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 2);
        // fixtures: purple (203,48,224) -> light (234,168,243); dark is 0.57 * raw + 17
        assert!(near(center(&scene(false, GlassVariant::Regular, None)), [234, 168, 243]));
        assert!(near(center(&scene(true, GlassVariant::Regular, None)), [133, 44, 145]));
        // clear adds ~24/255; identity paints nothing
        assert!(near(center(&scene(false, GlassVariant::Clear, None)), [229, 69, 250]));
        assert!(near(center(&scene(false, GlassVariant::Identity, None)), [203, 48, 224]));
        // an opaque tint hides the backdrop: red (255,56,60) -> (239,38,41)
        let red = Color::rgba(1.0, 56.0 / 255.0, 60.0 / 255.0, 1.0);
        assert!(near(center(&scene(false, GlassVariant::Regular, Some(red))), [239, 39, 43]));
    }

    #[test]
    fn ios27_interior_and_outline() {
        let near = |a: [u8; 3], b: [u8; 3]| {
            a.iter()
                .zip(b)
                .all(|(x, y)| (*x as i32 - y as i32).abs() <= 3)
        };
        let mut s = scene(false, GlassVariant::Regular, None);
        s.ios = IosVersion::Ios27;
        // fixtures (ios27 glass.png): purple (203,48,224) -> (255,133,255)
        assert!(near(center(&s), [255, 133, 255]), "{:?}", center(&s));
        let pm = render(&s, &mut Fonts::new(), 3.0);
        let edge = pm.pixel(149, 300).unwrap(); // just left of the shape (x = 50pt = 150px)
        assert!(edge.green() < 40, "outline {edge:?}");
        let far = pm.pixel(140, 300).unwrap(); // no outer shadow
        assert_eq!((far.red(), far.green(), far.blue()), (203, 48, 224));
    }

    #[test]
    fn box_blur_tracks_the_gaussian() {
        let (w, h) = (90, 70);
        // A hard-edged backdrop: worst case for the approximation.
        let src: Vec<f32> = (0..w * h).map(|i| if (i % w) / 15 % 2 == (i / w) / 12 % 2 { 255.0 } else { 0.0 }).collect();
        for sigma in [3.0f32, 9.0] {
            let (mut exact, mut boxed) = (src.clone(), src.clone());
            crate::raster::gaussian(&mut exact, w, h, sigma, true);
            box_blur3(&mut boxed, w, h, sigma);
            let worst = exact.iter().zip(&boxed).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(worst < 12.0, "sigma {sigma}: off by {worst} of 255");
        }
    }
}
