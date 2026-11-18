//! Liquid Glass backdrop effect (docs/protocol.md section F).
//!
//! A glass node samples the pixels already painted beneath its shape and replaces them with:
//! blurred backdrop -> lens displacement near the rim -> saturation/brightness transform -> tint ->
//! specular rim + soft outer shadow. CPU-only, deterministic, no threads or SIMD (wasm32-wasip1 safe).
//! Parameters are fitted to the iOS 26.5 Simulator fixtures (`tests/fixtures/showcase-ios/README.md`).

use crate::scene::Color;
pub use simcore_protocol::GlassVariant;
use tiny_skia::{Mask, Pixmap};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassSpec {
    pub variant: GlassVariant,
    pub tint: Option<Color>,
    pub interactive: bool,
    /// Backdrop blur sigma (pt) overriding the variant's (alert cards blur more).
    pub blur: Option<f32>,
    /// Color transform (mul, add) replacing the variant's (floating sheets and popovers are lighter).
    pub mix: Option<(f32, f32)>,
    /// Outer shadow (alpha, sigma pt, dy pt) replacing the variant's, in light appearance.
    pub shadow: Option<(f32, f32, f32)>,
}

/// Fitted parameters. Lengths are points, colors/mixes are 0...1.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Backdrop blur sigma.
    pub sigma: f32,
    /// Lens: samples within `lens_w` of the rim are pulled `lens_d * (1 - d/lens_w)^2` from further inside.
    pub lens_d: f32,
    pub lens_w: f32,
    /// Color transform of the blurred backdrop: saturation, then `c * mul + add`.
    pub sat: f32,
    pub mul: f32,
    pub add: f32,
    /// Opaque tint color is `tint + tint_shift`.
    pub tint_shift: f32,
    /// Specular rim: white mix at its brightest, width (pt) and how much it favours the top-left / bottom-right.
    pub rim_a: f32,
    pub rim_w: f32,
    pub rim_dir: f32,
    /// Outer shadow: alpha, sigma (pt), dy (pt).
    pub sh_a: f32,
    pub sh_sigma: f32,
    pub sh_dy: f32,
}

pub fn params(variant: GlassVariant, dark: bool) -> Params {
    let mut p = Params {
        sigma: 3.0,
        lens_d: 30.0,
        lens_w: 3.0,
        sat: 1.0,
        mul: 0.43,
        add: 0.575,
        tint_shift: -17.0 / 255.0,
        rim_a: 0.38,
        rim_w: 1.2,
        rim_dir: 0.4,
        sh_a: 0.13,
        sh_sigma: 12.0,
        sh_dy: 5.0,
    };
    match (variant, dark) {
        (GlassVariant::Regular, true) => {
            p.mul = 0.57;
            p.add = 18.0 / 255.0;
            p.rim_a = 0.15;
            p.tint_shift = 8.0 / 255.0;
            p.sh_a = 0.0;
        }
        (GlassVariant::Clear, false) => {
            p.sigma = 1.0;
            p.sat = 1.03;
            p.mul = 1.0;
            p.add = 0.09;
        }
        (GlassVariant::Clear, true) => {
            p.sigma = 1.0;
            p.sat = 1.03;
            p.mul = 1.0;
            p.add = 0.09;
            p.tint_shift = 8.0 / 255.0;
            p.sh_a = 0.0;
        }
        _ => {}
    }
    p
}

/// Parameters of one glass node: the variant's, with the spec's overrides.
pub fn spec_params(spec: &GlassSpec, dark: bool) -> Params {
    let mut p = params(spec.variant, dark);
    p.sigma = spec.blur.unwrap_or(p.sigma);
    if let Some((mul, add)) = spec.mix {
        (p.mul, p.add) = (mul, add);
    }
    if let (Some((a, sg, dy)), false) = (spec.shadow, dark) {
        (p.sh_a, p.sh_sigma, p.sh_dy) = (a, sg, dy);
    }
    p
}

/// Backdrop copy (RGB, f32 0..255) taken before the node's own shadow is painted.
pub struct Snapshot {
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    rgb: [Vec<f32>; 3],
}

/// Geometry of the shape in device pixels.
#[derive(Clone, Copy)]
pub struct Shape {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
}

impl Shape {
    /// Signed distance (negative inside) and outward normal at `(px, py)`.
    fn sdf(&self, px: f32, py: f32) -> (f32, f32, f32) {
        let r = self.r.min(self.w / 2.0).min(self.h / 2.0).max(0.0);
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let (dx, dy) = (px - cx, py - cy);
        let (qx, qy) = (dx.abs() - (self.w / 2.0 - r), dy.abs() - (self.h / 2.0 - r));
        let (sx, sy) = (dx.signum(), dy.signum());
        let (mx, my) = (qx.max(0.0), qy.max(0.0));
        let ml = (mx * mx + my * my).sqrt();
        let sd = ml + qx.max(qy).min(0.0) - r;
        let (nx, ny) = if ml > 1e-4 {
            (sx * mx / ml, sy * my / ml)
        } else if qx > qy {
            (sx, 0.0)
        } else {
            (0.0, sy)
        };
        (sd, nx, ny)
    }
}

/// Copy the pixels around `shape` (padded by the blur radius and lens reach).
pub fn snapshot(pm: &Pixmap, shape: Shape, scale: f32, p: &Params) -> Snapshot {
    let pad = (p.sigma * scale * 3.0 + p.lens_d * scale + 2.0).ceil() as i32;
    let x0 = ((shape.x.floor() as i32) - pad).max(0);
    let y0 = ((shape.y.floor() as i32) - pad).max(0);
    let x1 = (((shape.x + shape.w).ceil() as i32) + pad).min(pm.width() as i32);
    let y1 = (((shape.y + shape.h).ceil() as i32) + pad).min(pm.height() as i32);
    let (w, h) = ((x1 - x0).max(1) as usize, (y1 - y0).max(1) as usize);
    let mut rgb = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    for y in 0..h {
        for x in 0..w {
            if let Some(px) = pm.pixel((x0 as usize + x) as u32, (y0 as usize + y) as u32) {
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
    let wi = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let wl = (wi.floor() as usize).max(1);
    let wl = if wl.is_multiple_of(2) { wl - 1 } else { wl };
    let m = ((12.0 * sigma * sigma - 3.0 * (wl * wl) as f32 - 12.0 * wl as f32 - 9.0) / (-4.0 * wl as f32 - 4.0)).round() as usize;
    let radii: [usize; 3] = std::array::from_fn(|i| if i < m { wl / 2 } else { wl / 2 + 1 });
    let mut tmp = vec![0f32; plane.len()];
    for r in radii {
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
pub fn draw(pm: &mut Pixmap, snap: Snapshot, shape: Shape, spec: &GlassSpec, scale: f32, dark: bool, opacity: f32, mask: Option<&Mask>) {
    let p = spec_params(spec, dark);
    if spec.variant == GlassVariant::Identity {
        return;
    }
    let Snapshot { x0, y0, w, h, mut rgb } = snap;
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let (lens_d, lens_w) = (p.lens_d * scale, (p.lens_w * scale).max(0.5));
    let (sx0, sy0) = (shape.x.floor() as i32 - 1, shape.y.floor() as i32 - 1);
    let (sx1, sy1) = ((shape.x + shape.w).ceil() as i32 + 1, (shape.y + shape.h).ceil() as i32 + 1);
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
                let dir = (nx + ny) * -std::f32::consts::FRAC_1_SQRT_2; // -1 (bottom-right) .. 1 (top-left)
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

/// Scroll-edge effect: content under the bars is veiled toward the background color. `bar` is the
/// bottom of the navigation bar (pt); the veil is ~0.42 up there and decays below it (fitted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollEdge {
    pub bar: f32,
    pub color: Color,
    /// Light bars tint the background slightly (252 vs 255) from the top down to ~`bar` + 12.
    pub tint: f32,
}

/// (pt relative to the bar's bottom edge, veil alpha), piecewise linear; measured on the scrolled
/// Typography fixtures (alpha = 1 - content ink ratio along a glyph stem).
const VEIL: [(f32, f32); 9] =
    [(-1e9, 0.42), (-8.0, 0.41), (-4.0, 0.31), (0.0, 0.24), (4.0, 0.17), (8.0, 0.13), (12.0, 0.095), (20.0, 0.04), (28.0, 0.012)];

fn veil(rel: f32) -> f32 {
    if rel >= 32.0 {
        return 0.0;
    }
    if rel >= VEIL[8].0 {
        return mix(VEIL[8].1, 0.0, (rel - VEIL[8].0) / (32.0 - VEIL[8].0));
    }
    for w in VEIL.windows(2) {
        if rel < w[1].0 {
            return mix(w[0].1, w[1].1, ((rel - w[0].0) / (w[1].0 - w[0].0)).clamp(0.0, 1.0));
        }
    }
    0.0
}

// Scroll-edge effect above the bar, fitted to iOS 26.5 captures (typography-scrolled at 40pt,
// Typography at 337pt, home at AX5 206pt): the veil keeps its measured ramp up to EDGE_KNEE above the
// bar's bottom, then rises to EDGE_TOP_ALPHA at the top of the screen; a blur (EDGE_SIGMA) is mixed in
// at EDGE_BLUR_TOP at the top, EDGE_BLUR_BAR at the bar's bottom, fading out EDGE_BLUR_TAIL below it.
const EDGE_TOP_ALPHA: f32 = 0.95;
const EDGE_KNEE: f32 = 8.0;
const EDGE_SIGMA: f32 = 1.5;
const EDGE_BLUR_TOP: f32 = 1.0;
const EDGE_BLUR_BAR: f32 = 0.55;
const EDGE_BLUR_TAIL: f32 = 7.75;

/// Veil alpha at `pt` (screen y, points) for a bar ending at `bar`: the measured ramp around the
/// bar's bottom edge, rising toward the top of the screen above it.
fn veil_at(pt: f32, bar: f32) -> f32 {
    let (rel, knee, top) = (pt - bar, -EDGE_KNEE, EDGE_TOP_ALPHA);
    if rel >= knee {
        veil(rel)
    } else {
        mix(veil(knee), top, ((knee - rel) / (knee + bar)).clamp(0.0, 1.0))
    }
}

/// Weight of the blurred backdrop at `pt`: strongest at the top, fading out below the bar.
fn blur_at(pt: f32, bar: f32) -> f32 {
    let (rel, top, at_bar, tail) = (pt - bar, EDGE_BLUR_TOP, EDGE_BLUR_BAR, EDGE_BLUR_TAIL);
    if rel >= tail {
        0.0
    } else if rel >= 0.0 {
        mix(at_bar, 0.0, rel / tail)
    } else {
        mix(at_bar, top, (-rel / bar).clamp(0.0, 1.0))
    }
}

pub fn draw_edge(pm: &mut Pixmap, e: &ScrollEdge, scale: f32, mask: Option<&Mask>) {
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let rows = (((e.bar + 32.0) * scale).ceil() as usize).min(ph);
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
        let pt = (y as f32 + 0.5) / scale;
        let a = veil_at(pt, e.bar);
        let bwt = blur_at(pt, e.bar);
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
        // Tint: full above bar-32, gone by bar+12.
        let t = e.tint * ((e.bar + 12.0 - pt) / 44.0).clamp(0.0, 1.0);
        let px = |d: f32, c: usize, m: f32| {
            let v = mix(d, col[c], a) - t * 255.0;
            mix(d, v.clamp(0.0, 255.0), m)
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
    use crate::scene::{Node, Rect};
    use crate::text::Fonts;
    use simcore_protocol::Appearance;

    fn scene(dark: bool, variant: GlassVariant, tint: Option<Color>) -> crate::Scene {
        let mut bg = Node::new("bg", "screen");
        bg.bounds = Rect { x: 0.0, y: 0.0, w: 200.0, h: 200.0 };
        bg.fill = Some(Color::rgba(203.0 / 255.0, 48.0 / 255.0, 224.0 / 255.0, 1.0));
        let mut g = Node::new("g", "glass");
        g.bounds = Rect { x: 50.0, y: 50.0, w: 100.0, h: 100.0 };
        g.radius = 20.0;
        g.glass = Some(GlassSpec { variant, tint, interactive: false, blur: None, mix: None, shadow: None });
        let appearance = if dark { Appearance::Dark } else { Appearance::Light };
        crate::Scene { size: (200.0, 200.0), appearance, nodes: vec![bg, g], unsupported_symbols: vec![], notes: vec![] }
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
    fn scroll_edge_veil_ramp() {
        assert!((veil(-20.0) - 0.41).abs() < 1e-3 && (veil(0.0) - 0.24).abs() < 1e-6);
        assert!(veil(12.0) > veil(20.0) && veil(32.0) == 0.0 && veil(100.0) == 0.0);
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
