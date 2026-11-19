//! [`Builder`]: lowers a [`Scene`] to a [`DisplayList`] in the CPU rasterizer's paint order.
//!
//! The builder owns everything that needs fonts, symbol tables or the node tree: text, icon and symbol
//! sprites, bitmap resources and their cache, gradient endpoints, image transforms, shadow sigmas, device
//! pixel snapping, the ancestor clip chain and the order of a glass node's halo, glass, shadow and scroll
//! edge. A display-list executor needs nothing else (see `simcore-render-gpu`). No GPU or windowing
//! dependency, so it builds for `wasm32-wasip1` with the rest of the runtime.
//!
//! Resources are identified by stable ids: a sprite's id is its content hash (top bit clear), a bitmap's
//! a per-builder counter (top bit set). A resource is sent in `added` the first time a frame uses it and
//! kept by the executor until it appears in `dropped`; the builder drops resources unused for
//! [`CACHE_TTL_FRAMES`] frames and bounds the table at [`CACHE_MAX_ENTRIES`].

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use tiny_skia::Pixmap;

use crate::display::{DisplayList, Op, RRect, Resource, Sampling};
use crate::raster::{clip_radius, needs_backdrop, node_sprite, sprite_key, SpriteLayer};
use crate::scene::{Color, Gradient, ImageRun, Node};
use crate::{Fonts, Scene};

/// Frames a cached sprite or bitmap may go unused before it is dropped.
pub const CACHE_TTL_FRAMES: u64 = 120;
/// Hard bound on cached sprites + bitmaps (least recently used dropped first; never one used this frame).
pub const CACHE_MAX_ENTRIES: usize = 4096;
const BITMAP_ID_BIT: u64 = 1 << 63;

struct SpriteEntry {
    /// Offset of the sprite's top-left from the node's anchor (device px); `None` = nothing to draw (no resource).
    off: Option<(i32, i32)>,
    last_used: u64,
}

struct BitmapEntry {
    /// Keeps the allocation alive so the pointer key cannot be reused.
    _src: Arc<Pixmap>,
    id: u64,
    last_used: u64,
}

/// Scene to display list, with the resource table that makes successive frames cheap.
#[derive(Default)]
pub struct Builder {
    frame: u64,
    sprites: HashMap<u64, SpriteEntry>,
    bitmaps: HashMap<usize, BitmapEntry>,
    next_bitmap: u64,
    sprite_lookups: usize,
    sprite_misses: usize,
}

/// `x, y, w, h` (points, scene space) to a device-pixel rounded rect; `r` in points, clamped to half the size.
fn rr(x: f64, y: f64, w: f64, h: f64, r: f64, s: f32) -> RRect {
    let (w, h) = (w as f32 * s, h as f32 * s);
    RRect { x: x as f32 * s, y: y as f32 * s, w, h, r: (r as f32 * s).min(w / 2.0).min(h / 2.0).max(0.0) }
}

fn with_opacity(c: Color, opacity: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a * opacity)
}

/// 2D affine in kurbo's order `[a, b, c, d, e, f]`.
#[derive(Clone, Copy)]
struct Aff([f64; 6]);

impl Aff {
    fn scale(sx: f64, sy: f64) -> Self {
        Aff([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }
    fn translate(x: f64, y: f64) -> Self {
        Aff([1.0, 0.0, 0.0, 1.0, x, y])
    }
    fn rotate(th: f64) -> Self {
        let (s, c) = th.sin_cos();
        Aff([c, s, -s, c, 0.0, 0.0])
    }
    /// `self` first, then `o`.
    fn then(self, o: Aff) -> Aff {
        let (m, n) = (o.0, self.0);
        Aff([
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ])
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sprites and bitmaps currently held (what the executor holds after applying the last list).
    pub fn cache_len(&self) -> usize {
        self.sprites.values().filter(|e| e.off.is_some()).count() + self.bitmaps.len()
    }

    /// Sprite cache hits and misses while building the last display list.
    pub fn sprite_cache_counts(&self) -> (usize, usize) {
        (self.sprite_lookups - self.sprite_misses, self.sprite_misses)
    }

    /// Forget every resource, as after the executor lost its device: the next list re-sends what it uses.
    /// The executor must clear its table too.
    pub fn reset(&mut self) {
        self.sprites.clear();
        self.bitmaps.clear();
    }

    /// Lower `scene` at `scale` device pixels per point.
    pub fn build(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> DisplayList {
        self.frame += 1;
        self.sprite_lookups = 0;
        self.sprite_misses = 0;
        let s = scale as f32;
        let sd = scale;
        let (pw, ph) = ((scene.size.0 * scale).round().max(1.0) as u32, (scene.size.1 * scale).round().max(1.0) as u32);
        let mut dl = DisplayList { width: pw, height: ph, scale: s, dark: scene.appearance == simcore_protocol::Appearance::Dark, ..Default::default() };
        let nodes = &scene.nodes;
        // Clipping ancestors of each node (outermost first), and the clips currently open in `dl.ops`.
        let mut chains: Vec<Rc<Vec<usize>>> = Vec::with_capacity(nodes.len());
        let mut open: Vec<usize> = vec![];

        for n in nodes {
            let chain = match n.parent {
                None => Rc::new(vec![]),
                Some(p) => {
                    if clip_radius(&nodes[p], s, pw, ph).is_some() {
                        let mut c = (*chains[p]).clone();
                        c.push(p);
                        Rc::new(c)
                    } else {
                        chains[p].clone()
                    }
                }
            };
            chains.push(chain.clone());

            let common = open.iter().zip(chain.iter()).take_while(|(a, b)| a == b).count();
            for _ in common..open.len() {
                dl.ops.push(Op::PopClip);
            }
            open.truncate(common);
            for &i in &chain[common..] {
                let (b, r) = (nodes[i].bounds, nodes[i].clip_radius.unwrap_or(0.0));
                dl.ops.push(Op::PushClip(rr(b.x, b.y, b.w, b.h, r, s)));
                open.push(i);
            }

            self.lower_node(n, fonts, sd, &mut dl);
        }
        dl.ops.extend(open.iter().map(|_| Op::PopClip));
        self.evict(&mut dl.dropped);
        dl.dropped.sort_unstable();
        dl
    }

    /// The ops of one node, in the order of `raster::render`: halo/glass/shadow/edge, fill, gradient,
    /// stroke, icons, image, text.
    fn lower_node(&mut self, n: &Node, fonts: &mut Fonts, scale: f64, dl: &mut DisplayList) {
        let s = scale as f32;
        let b = n.bounds;
        let op = n.opacity;
        let backdrop = needs_backdrop(n);

        // Backdrop effects, in the CPU order: halo + glass (one op: the glass samples the backdrop before its halo
        // lands), the node's own shadow, the scroll edge.
        if backdrop {
            if let Some(g) = n.glass.as_ref().filter(|g| g.variant != crate::glass::GlassVariant::Identity) {
                dl.ops.push(Op::Glass { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), spec: *g, opacity: op });
            }
        }
        if let Some(sh) = &n.shadow {
            let radius = n.radius.max(n.clip_radius.unwrap_or(0.0));
            // The CPU always draws an own shadow; the GPU's Vello path only where it cannot sample the backdrop.
            let shape = RRect { x: b.x as f32 * s, y: b.y as f32 * s, w: b.w as f32 * s, h: b.h as f32 * s, r: radius as f32 * s };
            let (color, sigma, dx, dy) = (with_opacity(sh.color, op), (sh.radius as f32 * s).max(0.5), sh.dx as f32 * s, sh.dy as f32 * s);
            if backdrop {
                dl.ops.push(Op::BackdropShadow { shape, color, sigma, dx, dy });
            } else if shape.w > 0.0 && shape.h > 0.0 {
                dl.ops.push(Op::Shadow { shape, color, sigma, dx, dy });
            }
        }
        if let Some(e) = &n.edge {
            dl.ops.push(Op::Edge(*e));
        }

        if let Some(c) = n.fill {
            if n.radius > 0.0 {
                if b.w > 0.0 && b.h > 0.0 {
                    dl.ops.push(Op::Fill { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), color: with_opacity(c, op) });
                }
            } else {
                // Snap to device pixels like UIKit (keeps hairlines crisp).
                let (x0, y0) = ((b.x as f32 * s).round(), (b.y as f32 * s).round());
                let x1 = (((b.x + b.w) as f32 * s).round()).max(x0 + 1.0);
                let y1 = (((b.y + b.h) as f32 * s).round()).max(y0 + 1.0);
                dl.ops.push(Op::Fill { shape: RRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0, r: 0.0 }, color: with_opacity(c, op) });
            }
        }
        if let Some(g) = &n.gradient {
            if let Some(o) = gradient(n, g, scale) {
                dl.ops.push(o);
            }
        }
        if let Some((c, lw)) = n.stroke {
            if b.w >= 0.0 && b.h >= 0.0 {
                dl.ops.push(Op::Stroke { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), width: lw as f32 * s, color: with_opacity(c, op) });
            }
        }
        if let Some(o) = self.sprite(n, fonts, scale, SpriteLayer::Icons, &mut dl.added) {
            dl.ops.push(o);
        }
        if let Some(im) = &n.image {
            self.image(n, im, scale, dl);
        }
        if let Some(o) = self.sprite(n, fonts, scale, SpriteLayer::Text, &mut dl.added) {
            dl.ops.push(o);
        }
    }

    /// The sprite op for `layer` of `n`, rasterizing and queueing the resource on first use.
    fn sprite(&mut self, n: &Node, fonts: &mut Fonts, scale: f64, layer: SpriteLayer, added: &mut Vec<Resource>) -> Option<Op> {
        let key = sprite_key(n, fonts, scale, layer)?;
        let id = key.hash & !BITMAP_ID_BIT;
        let now = self.frame;
        self.sprite_lookups += 1;
        let e = self.sprites.entry(id).or_insert_with(|| {
            self.sprite_misses += 1;
            let off = node_sprite(n, fonts, scale, layer).map(|sp| {
                let off = (sp.x - key.anchor.0, sp.y - key.anchor.1);
                let (width, height) = (sp.pixmap.width(), sp.pixmap.height());
                added.push(Resource { id, width, height, rgba: sp.pixmap.take() });
                off
            });
            SpriteEntry { off, last_used: now }
        });
        e.last_used = now;
        e.off.map(|(dx, dy)| Op::Sprite { id, x: key.anchor.0 + dx, y: key.anchor.1 + dy })
    }

    /// `im.dest` of the bitmap, clipped to the node's rounded bounds (mirrors `raster::draw_image`).
    fn image(&mut self, n: &Node, im: &ImageRun, scale: f64, dl: &mut DisplayList) {
        let s = scale as f32;
        let (b, d) = (n.bounds, im.dest);
        let (iw, ih) = (im.pixmap.width() as f64, im.pixmap.height() as f64);
        let rotated = im.rotation != 0.0;
        // A rotated bitmap is mapped to the whole bounds.
        let d = if rotated { b } else { d };
        let (x0, y0) = (d.x.max(b.x), d.y.max(b.y));
        let (x1, y1) = ((d.x + d.w).min(b.x + b.w), (d.y + d.h).min(b.y + b.h));
        if x1 <= x0 || y1 <= y0 || b.w <= 0.0 || b.h <= 0.0 || iw <= 0.0 || ih <= 0.0 {
            return;
        }
        let mut to_pt = Aff::scale(d.w / iw, d.h / ih).then(Aff::translate(d.x, d.y));
        if rotated {
            let (cx, cy) = (d.x + d.w / 2.0, d.y + d.h / 2.0);
            to_pt = to_pt.then(Aff::translate(-cx, -cy)).then(Aff::rotate((im.rotation as f64).to_radians())).then(Aff::translate(cx, cy));
        }
        let m = to_pt.then(Aff::scale(scale, scale)).0.map(|v| v as f32);
        let id = self.bitmap(&im.pixmap, &mut dl.added);
        let clipped = n.radius > 0.0 || rotated || (d.x < b.x || d.y < b.y || d.x + d.w > b.x + b.w || d.y + d.h > b.y + b.h);
        if clipped {
            dl.ops.push(Op::PushClip(rr(b.x, b.y, b.w, b.h, n.radius, s)));
        }
        dl.ops.push(Op::Image { id, transform: m, sampling: Sampling::Bicubic, opacity: n.opacity });
        if clipped {
            dl.ops.push(Op::PopClip);
        }
    }

    fn bitmap(&mut self, px: &Arc<Pixmap>, added: &mut Vec<Resource>) -> u64 {
        let key = Arc::as_ptr(px) as usize;
        let now = self.frame;
        let next = &mut self.next_bitmap;
        let e = self.bitmaps.entry(key).or_insert_with(|| {
            let id = BITMAP_ID_BIT | *next;
            *next += 1;
            added.push(Resource { id, width: px.width(), height: px.height(), rgba: px.data().to_vec() });
            BitmapEntry { _src: px.clone(), id, last_used: now }
        });
        e.last_used = now;
        e.id
    }

    /// Drop what has gone unused for [`CACHE_TTL_FRAMES`], then the least recently used beyond
    /// [`CACHE_MAX_ENTRIES`] (never anything this frame used), listing resource ids in `dropped`.
    fn evict(&mut self, dropped: &mut Vec<u64>) {
        let now = self.frame;
        self.sprites.retain(|&id, e| {
            let keep = now.saturating_sub(e.last_used) <= CACHE_TTL_FRAMES;
            if !keep && e.off.is_some() {
                dropped.push(id);
            }
            keep
        });
        self.bitmaps.retain(|_, e| {
            let keep = now.saturating_sub(e.last_used) <= CACHE_TTL_FRAMES;
            if !keep {
                dropped.push(e.id);
            }
            keep
        });
        let over = (self.sprites.len() + self.bitmaps.len()).saturating_sub(CACHE_MAX_ENTRIES);
        if over > 0 {
            // (last used, sprite id | bitmap pointer key, is bitmap)
            let mut old: Vec<(u64, u64, bool)> = self.sprites.iter().filter(|(_, e)| e.last_used < now).map(|(&k, e)| (e.last_used, k, false)).collect();
            old.extend(self.bitmaps.iter().filter(|(_, e)| e.last_used < now).map(|(&k, e)| (e.last_used, k as u64, true)));
            old.sort_unstable();
            for &(_, k, bitmap) in old.iter().take(over) {
                if bitmap {
                    if let Some(e) = self.bitmaps.remove(&(k as usize)) {
                        dropped.push(e.id);
                    }
                } else if let Some(e) = self.sprites.remove(&k) {
                    if e.off.is_some() {
                        dropped.push(k);
                    }
                }
            }
        }
    }
}

/// Linear gradient over the node's rounded bounds (mirrors `raster::draw_gradient`).
fn gradient(n: &Node, g: &Gradient, scale: f64) -> Option<Op> {
    let b = n.bounds;
    let s = scale as f32;
    if g.stops.is_empty() || b.w <= 0.0 || b.h <= 0.0 {
        return None;
    }
    let k = g.stops.len().saturating_sub(1).max(1) as f32;
    let stops: Vec<(f32, Color)> = g
        .stops
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let at = g.locations.as_ref().and_then(|l| l.get(i)).copied().unwrap_or(i as f32 / k);
            (at, with_opacity(*c, n.opacity))
        })
        .collect();
    let (from, to) = if let Some([x0, y0, x1, y1]) = g.points {
        // Unit-box points placed on the bounds (isolines stay perpendicular on screen).
        let at = |u: f32, v: f32| (b.x + u as f64 * b.w, b.y + v as f64 * b.h);
        (at(x0, y0), at(x1, y1))
    } else {
        let a = (g.angle as f64).to_radians();
        let (dx, dy) = (a.sin(), -a.cos());
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let half = (b.w * dx.abs() + b.h * dy.abs()) / 2.0;
        ((cx - dx * half, cy - dy * half), (cx + dx * half, cy + dy * half))
    };
    if from == to {
        return None;
    }
    let dev = |p: (f64, f64)| (p.0 as f32 * s, p.1 as f32 * s);
    Some(Op::Gradient { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), p0: dev(from), p1: dev(to), stops })
}
