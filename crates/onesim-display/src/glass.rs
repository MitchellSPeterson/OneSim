//! Liquid Glass and scroll-edge backdrop parameters shared by CPU and GPU backends.

use crate::Color;
pub use onesim_protocol::GlassVariant;
use onesim_protocol::IosVersion;

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
    /// A plain material (UIVisualEffectView blur) with this saturation: backdrop blur and colour transform only, no
    /// lens, rim or shadow, and the backdrop is the shape's own pixels mirrored at its edges.
    pub flat: Option<f32>,
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
    /// Rim direction: false = top-left / bottom-right diagonals (iOS 26), true = top and bottom edges (iOS 27).
    pub rim_vertical: bool,
    /// Dark hairline just outside the outline: mix toward black at its strongest (on vertical edges;
    /// 30% of that on horizontal ones) and width (pt). iOS 27 only.
    pub outline_a: f32,
    pub outline_w: f32,
    /// Outer shadow: alpha, sigma (pt), dy (pt).
    pub sh_a: f32,
    pub sh_sigma: f32,
    pub sh_dy: f32,
    /// Backdrop limited to the shape, mirrored beyond its edges (flat materials).
    pub mirror: bool,
}

pub fn params(variant: GlassVariant, dark: bool, ios: IosVersion) -> Params {
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
        rim_vertical: false,
        outline_a: 0.0,
        outline_w: 0.0,
        mirror: false,
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
    if ios >= IosVersion::Ios27 {
        // No outer shadow; a dark hairline outside and highlights along the top and bottom instead.
        (p.rim_w, p.rim_dir, p.rim_vertical, p.sh_a) = (3.0, 1.0, true, 0.0);
        (p.outline_a, p.outline_w) = (0.4, 0.67);
        match (variant, dark) {
            // Least-squares fit (glass.png, back buttons): gray (242,242,247) -> (246,246,250), purple (203,48,224)
            // -> (255,133,255), pink (255,45,85) -> (255,131,169), blue (0,136,255) -> (84,204,255); the
            // red-tinted circle (255,56,60) -> (235,34,36).
            (GlassVariant::Regular, false) => {
                (p.sat, p.mul, p.add, p.tint_shift, p.rim_a) =
                    (1.85, 0.48, 0.51, -21.0 / 255.0, 0.24)
            }
            // glass-dark.png, back buttons: black -> 32, purple (219,52,242) -> (245,61,255), pink (255,55,95)
            // -> (255,63,109), blue (0,145,255) -> (0,152,255).
            (GlassVariant::Regular, true) => {
                (p.sat, p.mul, p.add, p.rim_a) = (1.37, 0.804, 0.127, 0.15)
            }
            // purple -> (237,73,255) light, (253,76,255) dark.
            (GlassVariant::Clear, _) => (p.sat, p.add) = (1.06, 0.109),
            _ => {}
        }
    }
    p
}

/// Parameters of one glass node: the variant's, with the spec's overrides.
pub fn spec_params(spec: &GlassSpec, dark: bool, ios: IosVersion) -> Params {
    let mut p = params(spec.variant, dark, ios);
    p.sigma = spec.blur.unwrap_or(p.sigma);
    if let Some((mul, add)) = spec.mix {
        (p.mul, p.add) = (mul, add);
    }
    if let (Some((a, sg, dy)), false) = (spec.shadow, dark) {
        (p.sh_a, p.sh_sigma, p.sh_dy) = (a, sg, dy);
    }
    if let Some(sat) = spec.flat {
        (p.lens_d, p.rim_a, p.sh_a, p.sat, p.tint_shift, p.mirror, p.outline_a) = (0.0, 0.0, 0.0, sat, 0.0, true, 0.0);
    }
    p
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
    pub fn sdf(&self, px: f32, py: f32) -> (f32, f32, f32) {
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

/// Page rectangle `(x, y, w, h)` (device px) that a backdrop snapshot copies for `shape` on a `pw` x `ph` canvas:
/// the shape padded by the blur radius and lens reach, clipped to the canvas.
pub fn snapshot_rect(shape: Shape, scale: f32, p: &Params, pw: u32, ph: u32) -> (i32, i32, usize, usize) {
    let pad = (p.sigma * scale * 3.0 + p.lens_d * scale + 2.0).ceil() as i32;
    let x0 = ((shape.x.floor() as i32) - pad).max(0);
    let y0 = ((shape.y.floor() as i32) - pad).max(0);
    let x1 = (((shape.x + shape.w).ceil() as i32) + pad).min(pw as i32);
    let y1 = (((shape.y + shape.h).ceil() as i32) + pad).min(ph as i32);
    (x0, y0, (x1 - x0).max(1) as usize, (y1 - y0).max(1) as usize)
}

/// Radii of the three box blurs (Kovesi's widths) that approximate a gaussian of `sigma` pixels.
pub fn box_radii(sigma: f32) -> [usize; 3] {
    let wi = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let wl = (wi.floor() as usize).max(1);
    let wl = if wl.is_multiple_of(2) { wl - 1 } else { wl };
    let m = ((12.0 * sigma * sigma - 3.0 * (wl * wl) as f32 - 12.0 * wl as f32 - 9.0) / (-4.0 * wl as f32 - 4.0)).round() as usize;
    std::array::from_fn(|i| if i < m { wl / 2 } else { wl / 2 + 1 })
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
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
pub const EDGE_SIGMA: f32 = 1.5;
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

/// iOS 27: the content under the bar is veiled almost opaquely right down to the bar's bottom edge
/// and not at all below it (typography-scrolled(-dark).png, ios27: glyph stems at ~0.88 just above the edge).
const EDGE27_ALPHA: f32 = 0.88;

fn veil_at_27(pt: f32, bar: f32, scale: f32) -> f32 {
    let below = ((pt - bar) * scale + 0.5).clamp(0.0, 1.0);
    mix(mix(EDGE27_ALPHA, EDGE_TOP_ALPHA, ((bar - pt) / bar).clamp(0.0, 1.0)), 0.0, below)
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

/// Rows (from the top of the canvas) the scroll edge touches on a `ph`-tall canvas.
pub fn edge_rows(e: &ScrollEdge, scale: f32, ph: usize) -> usize {
    (((e.bar + 32.0) * scale).ceil() as usize).min(ph)
}

/// Per-row scroll-edge terms: veil alpha, blur weight, tint, and signed hairline alpha.
/// A positive hairline alpha mixes toward white and a negative one toward black.
pub fn edge_row(e: &ScrollEdge, y: usize, scale: f32, ios: IosVersion) -> [f32; 4] {
    let pt = (y as f32 + 0.5) / scale;
    if ios == IosVersion::Ios26 {
        return [veil_at(pt, e.bar), blur_at(pt, e.bar), e.tint * ((e.bar + 12.0 - pt) / 44.0).clamp(0.0, 1.0), 0.0];
    }
    let dark = e.color.r < 0.5;
    let blur = if pt < e.bar { blur_at(pt, e.bar) } else { 0.0 };
    let tint = if dark && pt < e.bar { -8.0 / 255.0 } else { 0.0 };
    let hair = if y + 1 != (e.bar * scale).round() as usize { 0.0 } else if dark { 0.13 } else { -0.1 };
    [veil_at_27(pt, e.bar, scale), blur, tint, hair]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_edge_veil_ramp() {
        assert!((veil(-20.0) - 0.41).abs() < 1e-3 && (veil(0.0) - 0.24).abs() < 1e-6);
        assert!(veil(12.0) > veil(20.0) && veil(32.0) == 0.0 && veil(100.0) == 0.0);
    }
}
