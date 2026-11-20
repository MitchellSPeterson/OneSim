//! [`Builder`]: lowers a [`Scene`] to a [`DisplayList`] in the CPU rasterizer's paint order.
//!
//! The builder owns everything that needs fonts, symbol tables or the node tree: text, icon and symbol
//! sprites, bitmap resources and their cache, gradient endpoints, image transforms, shadow sigmas, device
//! pixel snapping, the ancestor clip chain and the order of a glass node's halo, glass, shadow and scroll
//! edge. A display-list executor needs nothing else (see `onesim-render-gpu`). No GPU or windowing
//! dependency, so it builds for `wasm32-wasip1` with the rest of the runtime.
//!
//! Resources are identified by stable ids: a sprite's id is its content hash (top bit clear), a bitmap's
//! a per-builder counter (top bit set). A resource is sent in `added` the first time a frame uses it and
//! kept by the executor until it appears in `dropped`; the builder drops resources unused for
//! [`CACHE_TTL_FRAMES`] frames and limits idle resources by count and RGBA bytes.

use std::collections::{hash_map::{DefaultHasher, Entry}, HashMap};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use tiny_skia::Pixmap;

use crate::display::{DisplayList, Op, RRect, Resource, Sampling};
use crate::raster::{clip_radius, needs_backdrop, node_sprite, sprite_key, text_sprite, PaintClip, SpriteLayer};
use crate::scene::{Color, Gradient, ImageRun, Node};
use crate::{Fonts, Scene};

/// Frames a cached sprite or bitmap may go unused before it is dropped.
pub const CACHE_TTL_FRAMES: u64 = 120;
/// Hard bound on cached sprites + bitmaps (least recently used dropped first; never one used this frame).
pub const CACHE_MAX_ENTRIES: usize = 4096;
/// RGBA bytes retained by the consumer, plus any working set that the current frame requires.
const CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const BITMAP_ID_BIT: u64 = 1 << 63;

struct SpriteEntry {
    tile: SpriteTile,
    last_used: u64,
}

enum SpriteTile {
    Empty,
    /// Ink bounds relative to the anchor. The executor has no resource for this sprite yet.
    Offscreen(PaintClip),
    /// Offset from the anchor and sprite size in device pixels.
    Uploaded(i32, i32, u32, u32),
}

impl SpriteTile {
    fn bytes(&self) -> usize {
        match self {
            Self::Uploaded(_, _, width, height) => *width as usize * *height as usize * 4,
            _ => 0,
        }
    }
}

struct BitmapEntry {
    /// Keeps the allocation alive so the pointer key cannot be reused.
    src: Arc<Pixmap>,
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
        self.sprites.values().filter(|e| matches!(e.tile, SpriteTile::Uploaded(..))).count() + self.bitmaps.len()
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
        let mut dl = DisplayList { width: pw, height: ph, scale: s, dark: scene.appearance == onesim_protocol::Appearance::Dark, ios: scene.ios, ..Default::default() };
        let nodes = &scene.nodes;
        // Ancestor clips of each node, with their conservative intersection and the chain open in `dl.ops`.
        let viewport = PaintClip::viewport(pw, ph);
        let root_chain = Rc::new(vec![]);
        let mut chains: Vec<(Rc<Vec<usize>>, PaintClip)> = Vec::with_capacity(nodes.len());
        let mut open: Vec<usize> = vec![];

        for n in nodes {
            let (chain, clip) = match n.parent {
                None => (root_chain.clone(), viewport),
                Some(p) => {
                    let (ancestors, clip) = &chains[p];
                    if !clip.is_empty() && clip_radius(&nodes[p], s, pw, ph).is_some() {
                        let clip = clip.intersect_node(nodes[p].bounds, s);
                        let mut c = (**ancestors).clone();
                        c.push(p);
                        (Rc::new(c), clip)
                    } else {
                        (ancestors.clone(), *clip)
                    }
                }
            };
            chains.push((chain.clone(), clip));
            if clip.is_empty() {
                continue;
            }

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

            self.lower_node(n, fonts, sd, clip, &mut dl);
        }
        dl.ops.extend(open.iter().map(|_| Op::PopClip));
        self.evict(&mut dl.dropped);
        dl.dropped.sort_unstable();
        dl
    }

    /// The ops of one node, in the order of `raster::render`: halo/glass/shadow/edge, fill, gradient,
    /// stroke, icons, image, text.
    fn lower_node(&mut self, n: &Node, fonts: &mut Fonts, scale: f64, clip: PaintClip, dl: &mut DisplayList) {
        let s = scale as f32;
        let b = n.bounds;
        let op = n.opacity;
        let bounds_visible = clip.intersects_node(b, s);
        let backdrop = needs_backdrop(n);
        let shadow_radius = n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32 * s;

        // Backdrop effects, in the CPU order: halo + glass (one op: the glass samples the backdrop before its halo
        // lands), the node's own shadow, the scroll edge.
        if backdrop {
            if let Some(g) = n.glass.as_ref().filter(|g| g.variant != crate::glass::GlassVariant::Identity) {
                dl.ops.push(Op::Glass { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), spec: *g, opacity: op, halo_radius: shadow_radius });
            }
        }
        if let Some(sh) = &n.shadow {
            // The CPU always draws an own shadow; the GPU's Vello path only where it cannot sample the backdrop.
            let shape = RRect { x: b.x as f32 * s, y: b.y as f32 * s, w: b.w as f32 * s, h: b.h as f32 * s, r: shadow_radius };
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

        if let Some(c) = n.fill.filter(|_| bounds_visible) {
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
        if let Some(g) = n.gradient.as_ref().filter(|_| bounds_visible) {
            if let Some(o) = gradient(n, g, scale) {
                dl.ops.push(o);
            }
        }
        if let Some((c, lw)) = n.stroke {
            if b.w >= 0.0 && b.h >= 0.0 {
                dl.ops.push(Op::Stroke { shape: rr(b.x, b.y, b.w, b.h, n.radius, s), width: lw as f32 * s, color: with_opacity(c, op) });
            }
        }
        if let Some(o) = self.sprite(n, fonts, scale, SpriteLayer::Shape, clip, &mut dl.added) {
            dl.ops.push(o);
        }
        if let Some(o) = self.sprite(n, fonts, scale, SpriteLayer::Icons, clip, &mut dl.added) {
            dl.ops.push(o);
        }
        if let Some(im) = n.image.as_ref().filter(|_| bounds_visible) {
            self.image(n, im, scale, dl);
        }
        if !(bounds_visible && self.glyph_sprites(n, fonts, s, clip, dl)) {
            dl.ops.extend(self.sprite(n, fonts, scale, SpriteLayer::Text, clip, &mut dl.added));
        }
    }

    /// Use shared glyph-mask resources for short, opaque, undecorated grayscale labels.
    fn glyph_sprites(&mut self, n: &Node, fonts: &mut Fonts, scale: f32, clip: PaintClip, dl: &mut DisplayList) -> bool {
        let Some(t) = n.text.as_ref() else { return false };
        let c = t.color;
        if n.opacity != 1.0 || c.a != 1.0 || c.r != c.g || c.g != c.b || t.strike || !t.spans.is_empty() || t.font.size > 40.0 || t.text.len() > 256 {
            return false;
        }
        let (spec, width, origin) = crate::raster::text_geometry(n, t, fonts, scale);
        let Some(glyphs) = fonts.mask_glyphs(&t.text, &spec, width, origin, clip) else { return false };
        let tint = (c.r * 255.0).round() as u8;
        for g in glyphs {
            let mut hash = DefaultHasher::new();
            ("glyph", g.key, tint).hash(&mut hash);
            let id = hash.finish() & !BITMAP_ID_BIT;
            self.sprite_lookups += 1;
            let entry = self.sprites.entry(id).or_insert_with(|| {
                self.sprite_misses += 1;
                let rgba = fonts.glyph_mask(g.key).iter().flat_map(|&alpha| {
                    let value = ((u16::from(tint) * u16::from(alpha) + 127) / 255) as u8;
                    [value, value, value, alpha]
                }).collect();
                dl.added.push(Resource { id, width: g.width, height: g.height, rgba });
                SpriteEntry { tile: SpriteTile::Uploaded(0, 0, g.width, g.height), last_used: self.frame }
            });
            entry.last_used = self.frame;
            dl.ops.push(Op::Sprite { id, x: g.x, y: g.y });
        }
        true
    }

    /// The sprite op for `layer` of `n`, rasterizing and queueing the resource on first use.
    fn sprite(&mut self, n: &Node, fonts: &mut Fonts, scale: f64, layer: SpriteLayer, clip: PaintClip, added: &mut Vec<Resource>) -> Option<Op> {
        let key = sprite_key(n, fonts, scale, layer)?;
        let id = key.hash & !BITMAP_ID_BIT;
        let now = self.frame;
        self.sprite_lookups += 1;
        let (ax, ay) = key.anchor;
        let mut build = || {
            self.sprite_misses += 1;
            let sprite = if layer == SpriteLayer::Text {
                let text_clip = (!clip.intersects_node(n.bounds, scale as f32)).then_some(clip);
                text_sprite(n, fonts, scale as f32, text_clip)
            } else {
                Ok(node_sprite(n, fonts, scale, layer))
            };
            match sprite {
                Ok(None) => SpriteTile::Empty,
                Err(bounds) => SpriteTile::Offscreen(bounds.translated(-(ax as f32), -(ay as f32))),
                Ok(Some(sp)) => {
                    let (width, height) = (sp.pixmap.width(), sp.pixmap.height());
                    let bounds = PaintClip::pixels(sp.x, sp.y, width, height);
                    if !clip.overlaps(bounds) {
                        return SpriteTile::Offscreen(bounds.translated(-(ax as f32), -(ay as f32)));
                    }
                    added.push(Resource { id, width, height, rgba: sp.pixmap.take() });
                    SpriteTile::Uploaded(sp.x - ax, sp.y - ay, width, height)
                }
            }
        };
        let entry = match self.sprites.entry(id) {
            Entry::Occupied(entry) => {
                let entry = entry.into_mut();
                if let SpriteTile::Offscreen(bounds) = entry.tile {
                    // A translated run can snap differently at a half-pixel boundary.
                    if clip.overlaps(bounds.translated(ax as f32, ay as f32).outset(1.0)) {
                        entry.tile = build();
                    }
                }
                entry
            }
            Entry::Vacant(entry) => entry.insert(SpriteEntry { tile: build(), last_used: now }),
        };
        match entry.tile {
            SpriteTile::Empty => entry.last_used = now,
            // Keep the bounds warm, but let the cache cap evict metadata that didn't paint.
            SpriteTile::Offscreen(_) => entry.last_used = now - 1,
            SpriteTile::Uploaded(dx, dy, width, height) => {
                let (x, y) = (ax + dx, ay + dy);
                if clip.overlaps(PaintClip::pixels(x, y, width, height)) {
                    entry.last_used = now;
                    return Some(Op::Sprite { id, x, y });
                }
            }
        }
        None
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
        let coverage = rr(b.x, b.y, b.w, b.h, n.radius, s);
        let clipped = !rotated && (n.radius > 0.0 || d.x < b.x || d.y < b.y || d.x + d.w > b.x + b.w || d.y + d.h > b.y + b.h);
        if clipped {
            dl.ops.push(Op::PushClip(coverage));
        }
        dl.ops.push(Op::Image { id, transform: m, sampling: Sampling::Bicubic, opacity: n.opacity, shape: rotated.then_some(coverage) });
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
            BitmapEntry { src: px.clone(), id, last_used: now }
        });
        e.last_used = now;
        e.id
    }

    /// Drop idle entries past their TTL, then trim the oldest idle entries to fit both cache budgets.
    fn evict(&mut self, dropped: &mut Vec<u64>) {
        let now = self.frame;
        let mut bytes = 0;
        self.sprites.retain(|&id, e| {
            let keep = now.saturating_sub(e.last_used) <= CACHE_TTL_FRAMES;
            let size = e.tile.bytes();
            if keep {
                bytes += size;
            } else if size > 0 {
                dropped.push(id);
            }
            keep
        });
        self.bitmaps.retain(|_, e| {
            let keep = now.saturating_sub(e.last_used) <= CACHE_TTL_FRAMES;
            if keep {
                bytes += e.src.data().len();
            } else {
                dropped.push(e.id);
            }
            keep
        });
        let mut entries = self.sprites.len() + self.bitmaps.len();
        if entries > CACHE_MAX_ENTRIES || bytes > CACHE_MAX_BYTES {
            // (last used, sprite id | bitmap pointer key, is bitmap)
            let mut old: Vec<(u64, u64, bool)> = self.sprites.iter().filter(|(_, e)| e.last_used < now).map(|(&k, e)| (e.last_used, k, false)).collect();
            old.extend(self.bitmaps.iter().filter(|(_, e)| e.last_used < now).map(|(&k, e)| (e.last_used, k as u64, true)));
            old.sort_unstable();
            for (_, k, bitmap) in old {
                if entries <= CACHE_MAX_ENTRIES && bytes <= CACHE_MAX_BYTES {
                    break;
                }
                if bitmap {
                    if let Some(e) = self.bitmaps.remove(&(k as usize)) {
                        entries -= 1;
                        bytes -= e.src.data().len();
                        dropped.push(e.id);
                    }
                } else if let Some(e) = self.sprites.remove(&k) {
                    entries -= 1;
                    let size = e.tile.bytes();
                    bytes -= size;
                    if size > 0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontSpec, Rect, TextAlign, TextRun};

    #[test]
    fn short_labels_share_glyph_resources() {
        let row = |text: &str, y| {
            let mut n = Node::new(text, "text");
            n.bounds = Rect { x: 0.0, y, w: 100.0, h: 20.0 };
            n.text = Some(TextRun {
                text: text.into(),
                font: FontSpec::new(16.0, 20.0, 400, TextAlign::Leading),
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                strike: false,
                offset_x: 0.0,
                spans: vec![],
            });
            n
        };
        let scene = Scene { size: (100.0, 100.0), appearance: onesim_protocol::Appearance::Light,
            ios: Default::default(), nodes: vec![row("1 2", 10.0), row("1 3", 40.0)], unsupported_symbols: vec![], notes: vec![] };
        let mut fonts = Fonts::with_fallback(true);
        let mut builder = Builder::new();
        let first = builder.build(&scene, &mut fonts, 3.0);
        let ids: Vec<_> = first.ops.iter().filter_map(|op| match op { Op::Sprite { id, .. } => Some(*id), _ => None }).collect();
        assert_eq!(first.added.len(), 3);
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[0], ids[2]);
        let next = builder.build(&scene, &mut fonts, 3.0);
        assert!(next.added.is_empty());
        assert_eq!(first.ops, next.ops);
    }

    #[test]
    fn sprite_budget_keeps_the_current_frames_resources() {
        let mut builder = Builder::new();
        builder.frame = 2;
        for id in 1..=3 {
            builder.sprites.insert(id, SpriteEntry { tile: SpriteTile::Uploaded(0, 0, 2048, 2048), last_used: id.min(2) });
        }
        let mut dropped = vec![];
        builder.evict(&mut dropped);
        assert_eq!(dropped, vec![1]);
        assert_eq!(builder.sprites.len(), 2);
    }

    #[test]
    fn offscreen_text_uploads_only_when_it_enters_the_clip() {
        let mut parent = Node::new("clip", "clip");
        parent.bounds = Rect { x: 0.0, y: 10.0, w: 100.0, h: 40.0 };
        parent.clip_radius = Some(4.0);
        let mut row = Node::new("row", "text");
        row.parent = Some(0);
        row.bounds = Rect { x: 0.0, y: 60.0, w: 100.0, h: 20.0 };
        row.text = Some(TextRun {
            text: "Row".into(),
            font: FontSpec::new(16.0, 20.0, 400, TextAlign::Leading),
            color: Color::rgba(0.0, 0.0, 0.25, 1.0),
            strike: false,
            offset_x: 0.0,
            spans: vec![],
        });
        let mut scene = Scene { size: (100.0, 100.0), appearance: onesim_protocol::Appearance::Light,
            ios: Default::default(), nodes: vec![parent, row], unsupported_symbols: vec![], notes: vec![] };
        let mut builder = Builder::new();
        let mut fonts = Fonts::with_fallback(true);

        assert!(builder.build(&scene, &mut fonts, 3.0).added.is_empty());
        scene.nodes[1].bounds.y = 65.0;
        assert!(builder.build(&scene, &mut fonts, 3.0).added.is_empty());
        assert_eq!(builder.sprite_cache_counts(), (1, 0), "offscreen paint bounds are reused");

        scene.nodes[1].bounds.y = 20.0;
        let visible = builder.build(&scene, &mut fonts, 3.0);
        assert_eq!(visible.added.len(), 1);
        assert!(visible.ops.iter().any(|op| matches!(op, Op::Sprite { .. })));

        scene.nodes[1].bounds.y = 65.0;
        let hidden = builder.build(&scene, &mut fonts, 3.0);
        assert!(hidden.added.is_empty());
        assert!(hidden.ops.iter().all(|op| !matches!(op, Op::Sprite { .. })));

        scene.nodes[1].bounds.y = 20.0;
        let visible_again = builder.build(&scene, &mut fonts, 3.0);
        assert!(visible_again.added.is_empty());
        assert_eq!(visible_again.ops, visible.ops);
    }
}
