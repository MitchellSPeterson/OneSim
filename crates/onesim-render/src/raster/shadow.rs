use super::vector::rrect;
use crate::scene::Shadow;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use tiny_skia::{FillRule, FilterQuality, Mask, Paint, Pixmap, PixmapPaint, Transform};

/// Separable gaussian blur of an `f32` coverage plane (exact kernel, truncated at 3 sigma).
pub(crate) fn gaussian(plane: &mut [f32], w: usize, h: usize, sigma: f32, replicate: bool) {
    let (cols, rows) = (0..w, 0..h);
    let r = (sigma * 3.0).ceil() as usize;
    let mut k: Vec<f32> = (-(r as isize)..=r as isize).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    // Output `i` of an `n`-sample line whose kernel overhangs an end: `replicate` clamps to the edge
    // sample (backdrops); otherwise outside counts as zero (coverage).
    let edge = |n: usize, i: usize, at: &dyn Fn(usize) -> f32| {
        let mut acc = 0.0f32;
        for (ki, kv) in k.iter().enumerate() {
            let j = (i + ki) as isize - r as isize;
            if replicate {
                acc += kv * at(j.clamp(0, n as isize - 1) as usize);
            } else if (0..n as isize).contains(&j) {
                acc += kv * at(j as usize);
            }
        }
        acc
    };
    // Outputs whose kernel fits inside the line.
    let inner = |n: usize, o: &Range<usize>| {
        let a = o.start.max(r);
        a..o.end.min(n.saturating_sub(r)).max(a)
    };
    // `out[i] = sum over taps of k * src[i + tap]`, taps outer so the inner loop vectorizes. Each
    // output still adds its taps in kernel order, so the result is bit-identical to a per-output loop.
    fn convolve<'a>(k: &[f32], out: &mut [f32], src: impl Fn(usize) -> &'a [f32]) {
        out.fill(0.0);
        for (ki, kv) in k.iter().enumerate() {
            for (o, v) in out.iter_mut().zip(src(ki)) {
                *o += kv * v;
            }
        }
    }
    // Rows, then columns.
    let mut tmp = vec![0f32; plane.len()];
    let hi = inner(w, &cols);
    for y in 0..h {
        let src = &plane[y * w..(y + 1) * w];
        let dst = &mut tmp[y * w..(y + 1) * w];
        for i in cols.clone().filter(|i| !hi.contains(i)) {
            dst[i] = edge(w, i, &|j| src[j]);
        }
        convolve(&k, &mut dst[hi.clone()], |ki| &src[hi.start + ki - r..hi.end + ki - r]);
    }
    let vi = inner(h, &rows);
    for i in rows {
        if vi.contains(&i) {
            let c = cols.clone();
            convolve(&k, &mut plane[i * w + c.start..i * w + c.end], |ki| &tmp[(i + ki - r) * w + c.start..(i + ki - r) * w + c.end]);
        } else {
            for c in cols.clone() {
                plane[i * w + c] = edge(h, i, &|j| tmp[j * w + c]);
            }
        }
    }
}

const SHADOW_CACHE_MAX: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ShadowGeom {
    geom: [u32; 7],
    rgba: [u8; 4],
}

#[derive(PartialEq, Eq, Hash)]
struct ShadowKey {
    id: String,
    meta: ShadowGeom,
}

trait ShadowLookup {
    fn key(&self) -> (&str, ShadowGeom);
}

impl ShadowLookup for ShadowKey {
    fn key(&self) -> (&str, ShadowGeom) {
        (&self.id, self.meta)
    }
}

impl ShadowLookup for (&str, ShadowGeom) {
    fn key(&self) -> (&str, ShadowGeom) {
        *self
    }
}

impl<'a> Borrow<dyn ShadowLookup + 'a> for ShadowKey {
    fn borrow(&self) -> &(dyn ShadowLookup + 'a) {
        self
    }
}

impl PartialEq for dyn ShadowLookup + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for dyn ShadowLookup + '_ {}

impl Hash for dyn ShadowLookup + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (id, meta) = self.key();
        id.hash(state);
        meta.hash(state);
    }
}

#[derive(Default)]
struct ShadowCache {
    tick: u64,
    entries: HashMap<ShadowKey, (u64, Rc<Pixmap>, u32)>,
}

thread_local! {
    /// Blurred shadow bitmaps keyed by node id, geometry (size, radius, sigma, scale, sub-pixel
    /// phase) and color, so repeated screenshots and unchanged nodes skip the blur.
    static SHADOWS: RefCell<ShadowCache> = RefCell::new(ShadowCache::default());
}

/// A shadow in device pixels: the shape's `x, y, w, h`, its corner radius, gaussian sigma, offset and straight RGBA.
pub(super) struct DevShadow {
    pub b: (f32, f32, f32, f32),
    pub radius: f32,
    pub sigma: f32,
    pub dx: f32,
    pub dy: f32,
    pub rgba: [u8; 4],
}

/// SwiftUI `shadow(radius:)` is a gaussian with sigma = radius (fitted to simulator renders).
pub(super) fn draw_shadow(pm: &mut Pixmap, n: &crate::scene::Node, sh: &Shadow, b: (f32, f32, f32, f32), s: f32, mask: Option<&Mask>) {
    let (id, op, radius) = (n.id.as_str(), n.opacity, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32);
    let rgba = [sh.color.r, sh.color.g, sh.color.b, sh.color.a * op].map(|v| (v * 255.0).round() as u8);
    let dev = DevShadow {
        b: (b.0 * s, b.1 * s, b.2 * s, b.3 * s),
        radius: radius * s,
        sigma: (sh.radius as f32 * s).max(0.5),
        dx: sh.dx as f32 * s,
        dy: sh.dy as f32 * s,
        rgba,
    };
    draw_shadow_dev(pm, id, &dev, mask);
}

pub(super) fn draw_shadow_dev(pm: &mut Pixmap, id: &str, sh: &DevShadow, mask: Option<&Mask>) {
    let DevShadow { b, radius, sigma, rgba, .. } = *sh;
    let pad = (sigma * 3.0).ceil() + 2.0;
    let (px, py) = (b.0 + sh.dx, b.1 + sh.dy);
    let (fx, fy) = (px - px.floor(), py - py.floor());
    let meta = ShadowGeom {
        geom: [b.2.to_bits(), b.3.to_bits(), radius.to_bits(), sigma.to_bits(), fx.to_bits(), fy.to_bits(), 0],
        rgba,
    };
    let (img, factor) = SHADOWS.with(|c| {
        let mut cache = c.borrow_mut();
        let lookup: (&str, ShadowGeom) = (id, meta);
        let next_tick = cache.tick + 1;
        if let Some(entry) = cache.entries.get_mut(&lookup as &dyn ShadowLookup) {
            entry.0 = next_tick;
            let res = (entry.1.clone(), entry.2);
            cache.tick = next_tick;
            return res;
        }
        // Match GPU `ShadowPlan`: downsample large-sigma shadows by `f = floor(sigma / 4)`.
        let f = ((sigma / 4.0).floor() as u32).max(1);
        let (w_full, h_full) = ((b.2 + 2.0 * pad).ceil() as u32 + 1, (b.3 + 2.0 * pad).ceil() as u32 + 1);
        let (w, h) = (w_full.div_ceil(f).max(1), h_full.div_ceil(f).max(1));
        let mut cov = Pixmap::new(w, h).expect("shadow size");
        if let Some(p) = rrect(pad + fx, pad + fy, b.2, b.3, radius) {
            let mut white = Paint::default();
            white.set_color_rgba8(255, 255, 255, 255);
            let xf = if f > 1 {
                Transform::from_scale(1.0 / f as f32, 1.0 / f as f32)
            } else {
                Transform::identity()
            };
            cov.fill_path(&p, &white, FillRule::Winding, xf, None);
        }
        let (wu, hu) = (cov.width() as usize, cov.height() as usize);
        let mut plane: Vec<f32> = cov.data().chunks(4).map(|px| px[3] as f32 / 255.0).collect();
        gaussian(&mut plane, wu, hu, sigma / f as f32, false);
        let a = rgba[3] as f32 / 255.0;
        for (px, v) in cov.data_mut().chunks_mut(4).zip(&plane) {
            let al = v * a;
            px[0] = (rgba[0] as f32 * al).round() as u8;
            px[1] = (rgba[1] as f32 * al).round() as u8;
            px[2] = (rgba[2] as f32 * al).round() as u8;
            px[3] = (al * 255.0).round() as u8;
        }
        let img = Rc::new(cov);
        if cache.entries.len() >= SHADOW_CACHE_MAX {
            let mut ticks: Vec<u64> = cache.entries.values().map(|(t, _, _)| *t).collect();
            let mid = ticks.len() / 2;
            let (_, &mut cutoff, _) = ticks.select_nth_unstable(mid);
            cache.entries.retain(|_, (t, _, _)| *t > cutoff);
        }
        cache.tick += 1;
        let tick = cache.tick;
        cache.entries.insert(ShadowKey { id: id.to_string(), meta }, (tick, img.clone(), f));
        (img, f)
    });
    let (ox, oy) = ((px.floor() - pad) as i32, (py.floor() - pad) as i32);
    if factor > 1 {
        let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
        let xf = Transform::from_row(factor as f32, 0.0, 0.0, factor as f32, ox as f32, oy as f32);
        pm.draw_pixmap(0, 0, img.as_ref().as_ref(), &paint, xf, mask);
    } else {
        pm.draw_pixmap(ox, oy, img.as_ref().as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
    }
}
