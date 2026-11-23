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
use crate::raster::composite::{
    aligned_origin, anchored_origin, composite_key, needs_whole_frame_blend, plan_composite,
    render_composite_sprite, subtree_bounds, transform_clip,
};
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
    /// Keeps referenced `ImageRun` pixmaps alive while this cached sprite key depends on their pointers.
    #[allow(dead_code)]
    pinned: Vec<Arc<Pixmap>>,
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
    composed_frame: Option<u64>,
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
        self.composed_frame = None;
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
        dl.dropped.extend(self.composed_frame.take());
        let has_composite = scene.nodes.iter().any(|n| n.composite.is_some());
        if has_composite && needs_whole_frame_blend(&scene.nodes) {
            // Top-level non-Normal Blend reads the outer framebuffer; resolve on CPU.
            let rgba = crate::render(scene, fonts, scale).take();
            let id = BITMAP_ID_BIT | self.next_bitmap;
            self.next_bitmap += 1;
            self.composed_frame = Some(id);
            dl.added.push(Resource { id, width: pw, height: ph, rgba });
            dl.ops.push(Op::Image {
                id, transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                sampling: Sampling::Nearest, opacity: 1.0, shape: None,
            });
            self.evict(&mut dl.dropped);
            dl.dropped.sort_unstable();
            return dl;
        }
        let nodes = &scene.nodes;
        let children: Vec<Vec<usize>> = if has_composite {
            let mut ch = vec![Vec::new(); nodes.len()];
            for (i, n) in nodes.iter().enumerate() {
                if let Some(p) = n.parent {
                    ch[p].push(i);
                }
            }
            ch
        } else {
            Vec::new()
        };
        let mut skipped = if has_composite { vec![false; nodes.len()] } else { Vec::new() };
        // Ancestor clips of each node, with their conservative intersection and the chain open in `dl.ops`.
        let viewport = PaintClip::viewport(pw, ph);
        let root_chain = Rc::new(vec![]);
        let mut chains: Vec<(Rc<Vec<usize>>, PaintClip)> = Vec::with_capacity(nodes.len());
        let mut open: Vec<usize> = vec![];

        for (idx, n) in nodes.iter().enumerate() {
            if has_composite && n.parent.is_some_and(|p| skipped[p]) {
                skipped[idx] = true;
                chains.push((root_chain.clone(), PaintClip::viewport(0, 0)));
                continue;
            }
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
                if has_composite && n.composite.is_some() {
                    skipped[idx] = true;
                }
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

            if n.composite.is_some() {
                skipped[idx] = true;
                self.lower_composite(scene, idx, &children, fonts, sd, (pw, ph), clip, &mut dl);
            } else {
                self.lower_node(n, fonts, sd, clip, &mut dl);
            }
        }
        dl.ops.extend(open.iter().map(|_| Op::PopClip));
        self.evict(&mut dl.dropped);
        dl.dropped.sort_unstable();
        dl
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_composite(
        &mut self,
        scene: &Scene,
        idx: usize,
        children: &[Vec<usize>],
        fonts: &mut Fonts,
        scale: f64,
        (pw, ph): (u32, u32),
        clip: PaintClip,
        dl: &mut DisplayList,
    ) {
        let Some(plan) = plan_composite(idx, &scene.nodes, children, scale) else {
            return;
        };
        let roots = plan.roots.slice(children);
        let bounds = subtree_bounds(scene, roots, children, fonts, scale, pw, ph);
        if bounds.work.is_empty() || bounds.output.is_empty() {
            return;
        }
        let out_clip = transform_clip(bounds.output, plan.affine, if plan.had_affine { 2.0 } else { 0.0 });
        if !clip.overlaps(out_clip) {
            return;
        }
        let (ox, oy, origin_x_px, origin_y_px, dx_pt, dy_pt, rw, rh) = if plan.had_affine
            && !bounds.clipped_by_viewport
            && !bounds.has_scroll_edge
            && !roots.is_empty()
        {
            let anchor = scene.nodes[roots[0]].bounds;
            if let Some((ox_px, oy_px, dx_pt, dy_pt, rw, rh)) =
                anchored_origin(bounds.work, anchor.x, anchor.y, scale, pw, ph)
            {
                (0, 0, ox_px, oy_px, dx_pt, dy_pt, rw, rh)
            } else {
                let Some((ox, oy, dx_pt, dy_pt, rw, rh)) =
                    aligned_origin(bounds.work, scale, bounds.has_scroll_edge, pw, ph)
                else {
                    return;
                };
                (ox, oy, ox as f64, oy as f64, dx_pt, dy_pt, rw, rh)
            }
        } else {
            let Some((ox, oy, dx_pt, dy_pt, rw, rh)) =
                aligned_origin(bounds.work, scale, bounds.has_scroll_edge, pw, ph)
            else {
                return;
            };
            (ox, oy, ox as f64, oy as f64, dx_pt, dy_pt, rw, rh)
        };
        let padded = plan.had_affine;
        let viewport_clip = (bounds.clipped_by_viewport || bounds.has_scroll_edge).then_some((ox, oy, rw, rh));
        let (hash, pinned) = composite_key(scene, roots, children, fonts, scale, dx_pt, dy_pt, padded, viewport_clip);
        let id = hash & !BITMAP_ID_BIT;
        let now = self.frame;
        self.sprite_lookups += 1;

        let entry = match self.sprites.entry(id) {
            Entry::Occupied(entry) => {
                let entry = entry.into_mut();
                entry.last_used = now;
                entry
            }
            Entry::Vacant(entry) => {
                self.sprite_misses += 1;
                let tile = match render_composite_sprite(
                    scene, roots, children, fonts, scale, ox, oy, dx_pt, dy_pt, rw, rh, padded,
                ) {
                    Some(sp) => {
                        let (width, height) = (sp.pixmap.width(), sp.pixmap.height());
                        dl.added.push(Resource { id, width, height, rgba: sp.pixmap.take() });
                        SpriteTile::Uploaded(sp.x - ox, sp.y - oy, width, height)
                    }
                    None => SpriteTile::Empty,
                };
                entry.insert(SpriteEntry { tile, last_used: now, pinned })
            }
        };

        if let SpriteTile::Uploaded(dx, dy, width, height) = entry.tile {
            let (sx, sy) = (origin_x_px + dx as f64, origin_y_px + dy as f64);
            let sprite_clip = transform_clip(
                PaintClip::from_ltrb(
                    sx.floor() as f32,
                    sy.floor() as f32,
                    (sx + width as f64).ceil() as f32,
                    (sy + height as f64).ceil() as f32,
                ),
                plan.affine,
                if plan.had_affine { 1.0 } else { 0.0 },
            );
            if !clip.overlaps(sprite_clip) {
                return;
            }
            let [a, b, c, d, e, f] = plan.affine;
            let int_pos = (sx - sx.round()).abs() < 1e-4 && (sy - sy.round()).abs() < 1e-4;
            let pure_int_translate = int_pos
                && (a - 1.0).abs() < 1e-6
                && b.abs() < 1e-6
                && c.abs() < 1e-6
                && (d - 1.0).abs() < 1e-6
                && (e - e.round()).abs() < 1e-4
                && (f - f.round()).abs() < 1e-4;
            if pure_int_translate && plan.opacity >= 1.0 - 1e-6 {
                dl.ops.push(Op::Sprite { id, x: (sx + e).round() as i32, y: (sy + f).round() as i32 });
            } else {
                let m = Aff::translate(sx, sy).then(Aff(plan.affine)).0.map(|v| v as f32);
                let sampling = if plan.had_affine || !int_pos { Sampling::Bilinear } else { Sampling::Nearest };
                dl.ops.push(Op::Image { id, transform: m, sampling, opacity: plan.opacity, shape: None });
            }
        }
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
        if n.content_paint.is_some() {
            return false;
        }
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
                SpriteEntry { tile: SpriteTile::Uploaded(0, 0, g.width, g.height), last_used: self.frame, pinned: Vec::new() }
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
            Entry::Vacant(entry) => entry.insert(SpriteEntry { tile: build(), last_used: now, pinned: Vec::new() }),
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
        let sampling = match im.filter {
            0 => Sampling::Nearest,
            1 => Sampling::Bilinear,
            _ => Sampling::Bicubic,
        };
        dl.ops.push(Op::Image { id, transform: m, sampling, opacity: n.opacity, shape: rotated.then_some(coverage) });
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
            builder.sprites.insert(id, SpriteEntry { tile: SpriteTile::Uploaded(0, 0, 2048, 2048), last_used: id.min(2), pinned: Vec::new() });
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

    fn test_scene(w: f64, h: f64, nodes: Vec<Node>) -> Scene {
        Scene {
            size: (w, h),
            appearance: onesim_protocol::Appearance::Light,
            ios: Default::default(),
            nodes,
            unsupported_symbols: vec![],
            notes: vec![],
        }
    }

    fn effect_node(id: &str, parent: Option<usize>, bounds: Rect, op: onesim_protocol::VisualOp) -> Node {
        let mut n = Node::new(id, "effect");
        n.parent = parent;
        n.bounds = bounds;
        n.composite = Some(Box::new(crate::effects::Composite::Effect(op, None)));
        n
    }

    fn fill_node(id: &str, parent: Option<usize>, bounds: Rect, color: Color, radius: f64) -> Node {
        let mut n = Node::new(id, "rect");
        n.parent = parent;
        n.bounds = bounds;
        n.fill = Some(color);
        n.radius = radius;
        n
    }

    #[test]
    fn animated_transforms_on_realistic_scene_stay_native_and_hit_cache() {
        use onesim_protocol::VisualOp;

        let mut bg = fill_node("bg", None, Rect { x: 0.0, y: 0.0, w: 402.0, h: 874.0 }, Color::rgba(0.95, 0.96, 0.98, 1.0), 0.0);
        bg.kind = "bg";

        let card = fill_node("card", None, Rect { x: 20.0, y: 120.0, w: 362.0, h: 200.0 }, Color::rgba(1.0, 1.0, 1.0, 1.0), 20.0);

        let mut glass = Node::new("glass", "glass");
        glass.bounds = Rect { x: 20.0, y: 340.0, w: 362.0, h: 80.0 };
        glass.radius = 24.0;
        glass.glass = Some(crate::glass::GlassSpec {
            variant: crate::glass::GlassVariant::Regular,
            tint: None,
            interactive: false,
            blur: None,
            mix: None,
            shadow: None,
            flat: None,
        });

        let mut label = Node::new("label", "text");
        label.bounds = Rect { x: 36.0, y: 140.0, w: 200.0, h: 24.0 };
        label.text = Some(TextRun {
            text: "Hello World".into(),
            font: FontSpec::new(17.0, 22.0, 600, TextAlign::Leading),
            color: Color::rgba(0.1, 0.1, 0.1, 1.0),
            strike: false,
            offset_x: 0.0,
            spans: vec![],
        });

        // Blurred orb wrapped in animated scaleEffect / rotationEffect / offset / opacity.
        let orb_bounds = Rect { x: 140.0, y: 500.0, w: 80.0, h: 80.0 };
        let orb_opacity = effect_node("orb_opacity", None, orb_bounds, VisualOp::Opacity { amount: 0.9 });
        let orb_affine = effect_node(
            "orb_affine",
            Some(4),
            orb_bounds,
            VisualOp::Affine { matrix: [1.15, 0.0, 0.0, 1.15, 0.0, 0.0], anchor: [0.5, 0.5] },
        );
        let orb_blur = effect_node("orb_blur", Some(5), orb_bounds, VisualOp::Blur { radius: 8.0, opaque: false });
        let orb_fill = fill_node("orb_fill", Some(6), orb_bounds, Color::rgba(0.2, 0.5, 1.0, 0.8), 40.0);

        let mut scene = test_scene(402.0, 874.0, vec![bg, card, glass, label, orb_opacity, orb_affine, orb_blur, orb_fill]);
        let mut fonts = Fonts::with_fallback(true);
        let mut builder = Builder::new();

        let frame1 = builder.build(&scene, &mut fonts, 3.0);
        assert_eq!((frame1.width, frame1.height), (1206, 2622));
        assert!(frame1.ops.iter().any(|op| matches!(op, Op::Fill { .. })), "background & card stay native Op::Fill");
        assert!(frame1.ops.iter().any(|op| matches!(op, Op::Glass { .. })), "glass node stays native Op::Glass");
        assert!(
            frame1.added.iter().all(|r| r.width < 1206 && r.height < 2622),
            "must not upload a full-frame 1206x2622 resource"
        );
        let total_added_bytes: usize = frame1.added.iter().map(|r| r.rgba.len()).sum();
        assert!(total_added_bytes < 1_000_000, "orb sprite upload should be small (got {total_added_bytes} bytes)");

        // Frames 2..10: animate scale (including 1.0 and back to 1.15), rotation, offset, and opacity.
        let anim_steps: &[(f64, f64, f64, f64, f64)] = &[
            (1.10, 0.0, 0.0, 0.0, 0.85),
            (1.00, 0.0, 0.0, 0.0, 1.00),
            (1.15, 0.0, 0.0, 0.0, 0.90),
            (1.05, 0.25, 0.0, 0.0, 0.80),
            (1.00, 0.50, 4.0, -3.0, 0.75),
            (0.95, -0.30, -6.0, 5.0, 0.60),
            (1.20, 0.10, 10.0, 10.0, 0.95),
            (1.00, 0.0, 12.0, -8.0, 1.00),
            (1.15, 0.0, 0.0, 0.0, 0.50),
        ];

        for (step, &(sc, rot, tx, ty, op)) in anim_steps.iter().enumerate() {
            let (s, c) = rot.sin_cos();
            scene.nodes[4].composite = Some(Box::new(crate::effects::Composite::Effect(
                VisualOp::Opacity { amount: op },
                None,
            )));
            scene.nodes[5].composite = Some(Box::new(crate::effects::Composite::Effect(
                VisualOp::Affine {
                    matrix: [c * sc, s * sc, -s * sc, c * sc, tx, ty],
                    anchor: [0.5, 0.5],
                },
                None,
            )));
            let dl = builder.build(&scene, &mut fonts, 3.0);
            assert!(dl.added.is_empty(), "frame {} should upload zero resources", step + 2);
            assert!(dl.dropped.is_empty(), "frame {} should drop zero resources", step + 2);
            let (hits, misses) = builder.sprite_cache_counts();
            assert!(hits >= 1 && misses == 0, "frame {} should hit sprite cache (hits={hits}, misses={misses})", step + 2);
            let encoded = dl.encode();
            assert!(encoded.len() < 4096, "frame {} encoded payload too large: {} bytes", step + 2, encoded.len());
        }
    }

    #[test]
    fn chained_outer_modifiers_peel_and_handle_edge_inputs() {
        use onesim_protocol::{BlendMode, VisualOp};

        let mut fonts = Fonts::with_fallback(true);
        let b = Rect { x: 20.0, y: 20.0, w: 40.0, h: 40.0 };

        // 1. Chained Affine + Affine + Opacity + Group(false) + Blur(0) + Blend(Normal) -> single Op::Image
        let scene = test_scene(120.0, 120.0, vec![
            effect_node("a1", None, b, VisualOp::Affine { matrix: [1.1, 0.0, 0.0, 1.1, 2.0, 3.0], anchor: [0.5, 0.5] }),
            effect_node("a2", Some(0), b, VisualOp::Affine { matrix: [1.0, 0.0, 0.0, 1.0, 1.0, -1.0], anchor: [0.0, 0.0] }),
            effect_node("op", Some(1), b, VisualOp::Opacity { amount: 0.7 }),
            effect_node("grp", Some(2), b, VisualOp::Group { opaque: false }),
            effect_node("blur0", Some(3), b, VisualOp::Blur { radius: 0.0, opaque: false }),
            effect_node("blendn", Some(4), b, VisualOp::Blend { mode: BlendMode::Normal }),
            fill_node("child", Some(5), b, Color::rgba(0.9, 0.2, 0.2, 1.0), 6.0),
        ]);
        let mut builder = Builder::new();
        let dl = builder.build(&scene, &mut fonts, 2.0);
        assert_eq!(dl.ops.len(), 1);
        assert!(matches!(dl.ops[0], Op::Image { sampling: Sampling::Bilinear, opacity, .. } if (opacity - 0.7).abs() < 1e-5));

        // Pure integer translation + opacity 1.0 -> Op::Sprite
        let scene_int = test_scene(120.0, 120.0, vec![
            effect_node("a1", None, b, VisualOp::Affine { matrix: [1.0, 0.0, 0.0, 1.0, 5.0, -3.0], anchor: [0.0, 0.0] }),
            effect_node("op", Some(0), b, VisualOp::Opacity { amount: 1.0 }),
            effect_node("grp", Some(1), b, VisualOp::Group { opaque: false }),
            effect_node("blur0", Some(2), b, VisualOp::Blur { radius: 0.0, opaque: false }),
            effect_node("blendn", Some(3), b, VisualOp::Blend { mode: BlendMode::Normal }),
            fill_node("child", Some(4), b, Color::rgba(0.2, 0.8, 0.2, 1.0), 4.0),
        ]);
        let dl_int = Builder::new().build(&scene_int, &mut fonts, 2.0);
        assert_eq!(dl_int.ops.len(), 1);
        assert!(matches!(dl_int.ops[0], Op::Sprite { .. }), "pure integer translate with opacity 1.0 should emit Op::Sprite");

        // 2. Negative scale (mirror: x: -1.0, y: 1.15)
        let scene_mirror = test_scene(120.0, 120.0, vec![
            effect_node("mirror", None, b, VisualOp::Affine { matrix: [-1.0, 0.0, 0.0, 1.15, 0.0, 0.0], anchor: [0.5, 0.5] }),
            fill_node("child", Some(0), b, Color::rgba(0.2, 0.4, 0.9, 1.0), 4.0),
        ]);
        let dl_mirror = Builder::new().build(&scene_mirror, &mut fonts, 2.0);
        assert_eq!(dl_mirror.ops.len(), 1);
        assert!(matches!(dl_mirror.ops[0], Op::Image { transform, .. } if transform[0] < 0.0 && transform[3] > 1.0));

        // 3. Singular, NaN, and Inf affines -> no ops, no resources, no panic
        for matrix in [
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0],
            [f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0],
        ] {
            let s = test_scene(100.0, 100.0, vec![
                effect_node("bad_aff", None, b, VisualOp::Affine { matrix, anchor: [0.5, 0.5] }),
                fill_node("child", Some(0), b, Color::rgba(1.0, 0.0, 0.0, 1.0), 0.0),
            ]);
            let dl = Builder::new().build(&s, &mut fonts, 2.0);
            assert!(dl.ops.is_empty(), "invalid affine {matrix:?} should emit no ops");
            assert!(dl.added.is_empty(), "invalid affine {matrix:?} should upload no resources");
        }

        // 4. Zero, negative, and NaN opacity -> no ops, no resources
        for amount in [0.0, -0.5, f64::NAN] {
            let s = test_scene(100.0, 100.0, vec![
                effect_node("bad_op", None, b, VisualOp::Opacity { amount }),
                fill_node("child", Some(0), b, Color::rgba(1.0, 0.0, 0.0, 1.0), 0.0),
            ]);
            let dl = Builder::new().build(&s, &mut fonts, 2.0);
            assert!(dl.ops.is_empty(), "opacity {amount:?} should emit no ops");
            assert!(dl.added.is_empty(), "opacity {amount:?} should upload no resources");
        }

        // 5. Empty composite (0 children) and transparent child -> no ops, no resources
        let s_empty = test_scene(100.0, 100.0, vec![effect_node("empty", None, b, VisualOp::Group { opaque: false })]);
        let dl_empty = Builder::new().build(&s_empty, &mut fonts, 2.0);
        assert!(dl_empty.ops.is_empty() && dl_empty.added.is_empty());

        let s_trans = test_scene(100.0, 100.0, vec![
            effect_node("wrap", None, b, VisualOp::Blur { radius: 2.0, opaque: false }),
            fill_node("clear", Some(0), b, Color::rgba(1.0, 0.0, 0.0, 0.0), 0.0),
        ]);
        let dl_trans = Builder::new().build(&s_trans, &mut fonts, 2.0);
        assert!(dl_trans.ops.is_empty() && dl_trans.added.is_empty());

        // 6. Zero-height hairline fill (h = 0.0, radius = 0.0) inside a composite is preserved
        let hairline_bounds = Rect { x: 10.0, y: 20.0, w: 60.0, h: 0.0 };
        let s_hair = test_scene(100.0, 100.0, vec![
            effect_node("op", None, hairline_bounds, VisualOp::Opacity { amount: 0.5 }),
            fill_node("line", Some(0), hairline_bounds, Color::rgba(0.0, 0.0, 0.0, 1.0), 0.0),
        ]);
        let dl_hair = Builder::new().build(&s_hair, &mut fonts, 2.0);
        assert_eq!(dl_hair.ops.len(), 1, "zero-height hairline inside composite must be preserved");
        assert_eq!(dl_hair.added.len(), 1);
    }

    #[test]
    fn non_peelable_effects_lower_cleanly() {
        use onesim_protocol::{BlendMode, VisualOp};

        let mut fonts = Fonts::with_fallback(true);
        let b = Rect { x: 20.0, y: 20.0, w: 40.0, h: 40.0 };

        // 1. Blur (non-opaque, opaque, huge radius 1e7)
        for (radius, opaque) in [(4.0, false), (4.0, true)] {
            let s = test_scene(100.0, 100.0, vec![
                effect_node("blur", None, b, VisualOp::Blur { radius, opaque }),
                fill_node("c", Some(0), b, Color::rgba(0.2, 0.6, 0.9, 1.0), 4.0),
            ]);
            let dl = Builder::new().build(&s, &mut fonts, 2.0);
            assert_eq!(dl.ops.len(), 1, "blur({radius}, {opaque}) should emit 1 op");
            assert_eq!(dl.added.len(), 1);
        }
        // Huge blur radius: no overflow/panic (may dilute alpha to 0 and emit empty sprite)
        let s_huge_blur = test_scene(100.0, 100.0, vec![
            effect_node("blur_huge", None, b, VisualOp::Blur { radius: 1e7, opaque: false }),
            fill_node("c", Some(0), b, Color::rgba(0.2, 0.6, 0.9, 1.0), 4.0),
        ]);
        let dl_huge_blur = Builder::new().build(&s_huge_blur, &mut fonts, 2.0);
        assert!(dl_huge_blur.ops.len() <= 1 && dl_huge_blur.added.len() <= 1);

        // 2. Projection: valid perspective + behind-camera (w <= 0) fallback to viewport bounds
        let valid_proj = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.001, 0.0, 1.0];
        let behind_cam = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0];
        for matrix in [valid_proj, behind_cam] {
            let s = test_scene(100.0, 100.0, vec![
                effect_node("proj", None, b, VisualOp::Projection { matrix }),
                fill_node("c", Some(0), b, Color::rgba(0.8, 0.3, 0.2, 1.0), 4.0),
            ]);
            let dl = Builder::new().build(&s, &mut fonts, 2.0);
            assert!(dl.added.len() <= 1);
        }

        // 3. Mask (normal, inverted, plus 0-child and 1-child degenerate masks)
        for inverted in [false, true] {
            let mut m = Node::new("mask", "mask");
            m.bounds = b;
            m.composite = Some(Box::new(crate::effects::Composite::Mask { inverted }));
            let content = fill_node("content", Some(0), b, Color::rgba(1.0, 0.0, 0.0, 1.0), 0.0);
            let mask_shape = fill_node("shape", Some(0), Rect { x: 25.0, y: 25.0, w: 20.0, h: 20.0 }, Color::rgba(0.0, 0.0, 0.0, 1.0), 10.0);
            let dl = Builder::new().build(&test_scene(100.0, 100.0, vec![m, content, mask_shape]), &mut fonts, 2.0);
            assert_eq!(dl.ops.len(), 1, "mask(inverted={inverted}) should emit 1 op");
            assert_eq!(dl.added.len(), 1);
        }
        // Degenerate 0-child and 1-child masks
        let mut m0 = Node::new("m0", "mask");
        m0.bounds = b;
        m0.composite = Some(Box::new(crate::effects::Composite::Mask { inverted: false }));
        let dl_m0 = Builder::new().build(&test_scene(100.0, 100.0, vec![m0]), &mut fonts, 2.0);
        assert!(dl_m0.ops.is_empty() && dl_m0.added.is_empty());

        let mut m1 = Node::new("m1", "mask");
        m1.bounds = b;
        m1.composite = Some(Box::new(crate::effects::Composite::Mask { inverted: false }));
        let m1_child = fill_node("c", Some(0), b, Color::rgba(1.0, 0.0, 0.0, 1.0), 0.0);
        let dl_m1 = Builder::new().build(&test_scene(100.0, 100.0, vec![m1, m1_child]), &mut fonts, 2.0);
        assert!(dl_m1.ops.is_empty() && dl_m1.added.is_empty());

        // 4. Group(opaque: true/false) containing nested Blend(Multiply) does NOT trigger whole-frame fallback
        for opaque in [true, false] {
            let sibling = fill_node("sib", None, Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }, Color::rgba(0.9, 0.9, 0.9, 1.0), 0.0);
            let grp = effect_node("grp", None, b, VisualOp::Group { opaque });
            let base = fill_node("base", Some(1), b, Color::rgba(0.2, 0.5, 0.9, 1.0), 0.0);
            let blend = effect_node("blend", Some(1), b, VisualOp::Blend { mode: BlendMode::Multiply });
            let top = fill_node("top", Some(3), b, Color::rgba(0.9, 0.4, 0.2, 1.0), 0.0);
            let dl = Builder::new().build(&test_scene(100.0, 100.0, vec![sibling, grp, base, blend, top]), &mut fonts, 2.0);
            assert!(dl.ops.iter().any(|op| matches!(op, Op::Fill { .. })), "sibling stays native Op::Fill for group(opaque={opaque})");
            assert!(dl.added.iter().all(|r| r.width < 200 && r.height < 200), "group(opaque={opaque}) emits bounded sprite, not full-frame");
        }

        // 5. All color filters
        let filters: Vec<(VisualOp, Option<Color>)> = vec![
            (VisualOp::Brightness { amount: 0.2 }, None),
            (VisualOp::Contrast { amount: 1.5 }, None),
            (VisualOp::Saturation { amount: 0.5 }, None),
            (VisualOp::Grayscale { amount: 1.0 }, None),
            (VisualOp::HueRotation { radians: 1.0 }, None),
            (VisualOp::ColorInvert { amount: 1.0 }, None),
            (
                VisualOp::ColorMultiply { color: onesim_protocol::Color::Rgba { r: 0.5, g: 0.8, b: 1.0, a: 1.0, dark: None } },
                Some(Color::rgba(0.5, 0.8, 1.0, 1.0)),
            ),
            (
                VisualOp::AlphaThreshold { min: 0.5, max: 1.0, color: onesim_protocol::Color::Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0, dark: None } },
                Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            ),
            (VisualOp::LuminanceToAlpha, None),
        ];
        for (filter, resolved_color) in filters {
            let mut cf = effect_node("cf", None, b, filter.clone());
            cf.composite = Some(Box::new(crate::effects::Composite::Effect(filter.clone(), resolved_color)));
            let s = test_scene(100.0, 100.0, vec![
                cf,
                fill_node("c", Some(0), b, Color::rgba(0.4, 0.6, 0.8, 1.0), 0.0),
            ]);
            let dl = Builder::new().build(&s, &mut fonts, 2.0);
            assert_eq!(dl.ops.len(), 1, "color filter {filter:?} should emit 1 op");
            assert_eq!(dl.added.len(), 1);
        }
    }

    #[test]
    fn top_level_blend_fallback_vs_normal() {
        use onesim_protocol::{BlendMode, VisualOp};

        let mut fonts = Fonts::with_fallback(true);
        let mut builder = Builder::new();
        let b = Rect { x: 10.0, y: 10.0, w: 40.0, h: 40.0 };

        // Top-level Blend(Multiply) falls back to full-frame Op::Image matching crate::render
        let bg = fill_node("bg", None, Rect { x: 0.0, y: 0.0, w: 60.0, h: 60.0 }, Color::rgba(0.8, 0.8, 0.8, 1.0), 0.0);
        let blend = effect_node("blend", None, b, VisualOp::Blend { mode: BlendMode::Multiply });
        let child = fill_node("c", Some(1), b, Color::rgba(0.2, 0.5, 0.9, 1.0), 0.0);
        let scene_mul = test_scene(60.0, 60.0, vec![bg.clone(), blend, child]);

        let dl_mul = builder.build(&scene_mul, &mut fonts, 2.0);
        assert_eq!(dl_mul.ops.len(), 1);
        assert_eq!(dl_mul.added.len(), 1);
        let composed_id = dl_mul.added[0].id;
        assert_eq!((dl_mul.added[0].width, dl_mul.added[0].height), (120, 120));
        let expected_rgba = crate::render(&scene_mul, &mut fonts, 2.0).take();
        assert_eq!(dl_mul.added[0].rgba, expected_rgba);

        // Next frame without blend drops the composed_frame resource
        let dl_next = builder.build(&test_scene(60.0, 60.0, vec![bg.clone()]), &mut fonts, 2.0);
        assert!(dl_next.dropped.contains(&composed_id), "composed_frame must be dropped on next frame");
        assert!(dl_next.ops.iter().any(|op| matches!(op, Op::Fill { .. })));

        // Top-level Blend(Normal) does NOT trigger whole-frame fallback
        let blend_norm = effect_node("blend_n", None, b, VisualOp::Blend { mode: BlendMode::Normal });
        let child_n = fill_node("cn", Some(1), b, Color::rgba(0.2, 0.5, 0.9, 1.0), 0.0);
        let dl_norm = Builder::new().build(&test_scene(60.0, 60.0, vec![bg, blend_norm, child_n]), &mut fonts, 2.0);
        assert!(dl_norm.ops.iter().any(|op| matches!(op, Op::Fill { .. })), "Blend::Normal must not trigger whole-frame fallback");
        assert!(dl_norm.added.iter().all(|r| r.width < 120 && r.height < 120));
    }

    #[test]
    fn composite_clips_culling_scrolling_viewport_edge_and_cache_lifecycle() {
        use onesim_protocol::VisualOp;

        let mut fonts = Fonts::with_fallback(true);
        let mut builder = Builder::new();

        // 1. Ancestor clip_radius emits PushClip / PopClip around composite op, and offscreen composite is culled
        let mut clip_parent = Node::new("clip", "clip");
        clip_parent.bounds = Rect { x: 10.0, y: 10.0, w: 80.0, h: 50.0 };
        clip_parent.clip_radius = Some(8.0);
        let comp = effect_node("comp", Some(0), Rect { x: 20.0, y: 20.0, w: 30.0, h: 20.0 }, VisualOp::Blur { radius: 2.0, opaque: false });
        let child = fill_node("child", Some(1), Rect { x: 20.0, y: 20.0, w: 30.0, h: 20.0 }, Color::rgba(0.1, 0.4, 0.8, 1.0), 4.0);
        let mut scene = test_scene(120.0, 120.0, vec![clip_parent, comp, child]);

        let dl_in = builder.build(&scene, &mut fonts, 3.0);
        assert!(dl_in.ops.iter().any(|op| matches!(op, Op::PushClip(_))));
        assert!(dl_in.ops.iter().any(|op| matches!(op, Op::Sprite { .. } | Op::Image { .. })));
        assert!(dl_in.ops.iter().any(|op| matches!(op, Op::PopClip)));
        assert_eq!(dl_in.added.len(), 1);

        // Scroll completely outside ancestor clip -> culled (zero composite ops, zero added resources)
        scene.nodes[1].bounds.y = 90.0;
        scene.nodes[2].bounds.y = 90.0;
        let dl_out = builder.build(&scene, &mut fonts, 3.0);
        assert!(dl_out.added.is_empty());
        assert!(dl_out.ops.iter().all(|op| !matches!(op, Op::Sprite { .. } | Op::Image { .. })));

        // 2. Scrolling a fully-in-viewport composite by whole points hits the sprite cache
        let mut scroll_builder = Builder::new();
        let mut scroll_scene = test_scene(200.0, 200.0, vec![
            effect_node("orb", None, Rect { x: 40.0, y: 40.0, w: 40.0, h: 40.0 }, VisualOp::Blur { radius: 3.0, opaque: false }),
            fill_node("c", Some(0), Rect { x: 40.0, y: 40.0, w: 40.0, h: 40.0 }, Color::rgba(0.9, 0.3, 0.1, 1.0), 20.0),
        ]);
        let s1 = scroll_builder.build(&scroll_scene, &mut fonts, 3.0);
        assert_eq!(s1.added.len(), 1);
        scroll_scene.nodes[0].bounds.y += 15.0;
        scroll_scene.nodes[1].bounds.y += 15.0;
        let s2 = scroll_builder.build(&scroll_scene, &mut fonts, 3.0);
        assert!(s2.added.is_empty(), "whole-point scroll inside viewport should hit sprite cache");
        assert_eq!(scroll_builder.sprite_cache_counts(), (1, 0));

        // 3. Moving a composite partially clipped at viewport edge does NOT produce a stale cache hit
        scroll_scene.nodes[0].bounds.x = -10.0;
        scroll_scene.nodes[1].bounds.x = -10.0;
        let edge1 = scroll_builder.build(&scroll_scene, &mut fonts, 3.0);
        assert_eq!(edge1.added.len(), 1, "partially clipped composite must re-rasterize");
        scroll_scene.nodes[0].bounds.x = -20.0;
        scroll_scene.nodes[1].bounds.x = -20.0;
        let edge2 = scroll_builder.build(&scroll_scene, &mut fonts, 3.0);
        assert_eq!(edge2.added.len(), 1, "moving partially clipped composite must not reuse stale clipped sprite");

        // 4. ImageRun inside a composite pins Arc<Pixmap> in SpriteEntry, survives reuse, and drops on reset & TTL
        let mut px = Pixmap::new(16, 16).unwrap();
        px.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        let arc_px = Arc::new(px);
        let mut img_node = Node::new("img", "image");
        img_node.parent = Some(0);
        img_node.bounds = Rect { x: 20.0, y: 20.0, w: 32.0, h: 32.0 };
        img_node.image = Some(ImageRun {
            pixmap: arc_px.clone(),
            dest: img_node.bounds,
            rotation: 0.0,
            filter: 1,
            frames: None,
        });
        let img_scene = test_scene(100.0, 100.0, vec![
            effect_node("wrap", None, img_node.bounds, VisualOp::Opacity { amount: 0.8 }),
            img_node,
        ]);
        let mut img_builder = Builder::new();
        let im1 = img_builder.build(&img_scene, &mut fonts, 2.0);
        assert_eq!(im1.added.len(), 1);
        let sprite_id = im1.added[0].id;
        assert!(Arc::strong_count(&arc_px) >= 3, "SpriteEntry must pin Arc<Pixmap> (local + scene + cache)");
        let im2 = img_builder.build(&img_scene, &mut fonts, 2.0);
        assert!(im2.added.is_empty());

        // TTL eviction drops the composite sprite cleanly
        let empty_scene = test_scene(100.0, 100.0, vec![]);
        let mut dropped_it = false;
        for _ in 0..=CACHE_TTL_FRAMES + 1 {
            let dl = img_builder.build(&empty_scene, &mut fonts, 2.0);
            if dl.dropped.contains(&sprite_id) {
                dropped_it = true;
                break;
            }
        }
        assert!(dropped_it, "composite sprite must be evicted after CACHE_TTL_FRAMES");

        // 5. ScrollEdge inside a composite re-rasterizes when viewport width changes (no stale width cache hit)
        let mut edge_bg = Node::new("edge-bg", "rect");
        edge_bg.parent = Some(0);
        edge_bg.bounds = Rect { x: 0.0, y: 0.0, w: 140.0, h: 30.0 };
        edge_bg.fill = Some(Color::rgba(0.2, 0.4, 0.8, 1.0));
        let mut edge_node = Node::new("edge", "edge");
        edge_node.parent = Some(0);
        edge_node.bounds = Rect { x: 0.0, y: 0.0, w: 100.0, h: 30.0 };
        edge_node.edge = Some(crate::glass::ScrollEdge {
            bar: 20.0,
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            tint: 0.2,
        });
        let mut edge_scene = test_scene(100.0, 100.0, vec![
            effect_node("wrap", None, edge_node.bounds, VisualOp::Opacity { amount: 0.9 }),
            edge_bg,
            edge_node,
        ]);
        let mut edge_builder = Builder::new();
        let e1 = edge_builder.build(&edge_scene, &mut fonts, 1.0);
        assert_eq!(e1.added.len(), 1);
        assert_eq!(e1.added[0].width, 100);
        edge_scene.size.0 = 140.0;
        let e2 = edge_builder.build(&edge_scene, &mut fonts, 1.0);
        assert_eq!(e2.added.len(), 1, "viewport width resize must re-rasterize ScrollEdge composite");
        assert_eq!(e2.added[0].width, 140);

        // 6. Gradient fill/stroke/content paints hash by content (not Arc pointer), avoiding stale cache hits
        let make_grad = |r: f32| {
            Arc::new(crate::paint::Gradient::resolve(
                &onesim_protocol::GradientPaint::Linear {
                    stops: vec![
                        onesim_protocol::GradientStop {
                            color: onesim_protocol::Color::Rgba { r: f64::from(r), g: 0.2, b: 0.8, a: 1.0, dark: None },
                            location: 0.0,
                        },
                        onesim_protocol::GradientStop {
                            color: onesim_protocol::Color::Rgba { r: 0.1, g: 0.9, b: 0.3, a: 1.0, dark: None },
                            location: 1.0,
                        },
                    ],
                    start: [0.0, 0.0],
                    end: [1.0, 0.0],
                    color_space: onesim_protocol::GradientColorSpace::Device,
                    absolute: false,
                    options: 0,
                },
                |c| match c {
                    onesim_protocol::Color::Rgba { r, g, b, a, .. } => Color::rgba(*r as f32, *g as f32, *b as f32, *a as f32),
                    _ => Color::rgba(0.0, 0.0, 0.0, 1.0),
                },
            ))
        };
        let make_grad_scene = |r: f32| {
            let mut shape = Node::new("grad-shape", "shape");
            shape.parent = Some(0);
            shape.bounds = Rect { x: 10.0, y: 10.0, w: 40.0, h: 40.0 };
            shape.vector = Some(Box::new(crate::scene::VectorShape {
                geom: crate::scene::VectorGeom::Ellipse,
                fill: Some(crate::scene::VectorFill {
                    color: Color::rgba(1.0, 1.0, 1.0, 1.0),
                    eo_fill: false,
                    antialiased: true,
                }),
                stroke: None,
            }));
            shape.fill_paint = Some(make_grad(r));
            test_scene(80.0, 80.0, vec![
                effect_node("wrap", None, shape.bounds, VisualOp::Opacity { amount: 0.9 }),
                shape,
            ])
        };
        let mut grad_builder = Builder::new();
        let g1 = grad_builder.build(&make_grad_scene(0.9), &mut fonts, 1.0);
        assert_eq!(g1.added.len(), 1);
        // Rebuilt identical gradient (fresh Arc allocation) hits cache cleanly.
        let g1_again = grad_builder.build(&make_grad_scene(0.9), &mut fonts, 1.0);
        assert!(g1_again.added.is_empty(), "identical rebuilt gradient should hit cache");
        // Different gradient color (even after dropping the first scene/Arc) produces a distinct key.
        let g2 = grad_builder.build(&make_grad_scene(0.1), &mut fonts, 1.0);
        assert_eq!(g2.added.len(), 1, "changed gradient content must not hit stale sprite key");
        assert_ne!(g1.added[0].id, g2.added[0].id);

        // 7. Acute stroked path with miter_limit > 10 keeps the full extended miter tip inside a composite
        let mut miter_shape = Node::new("acute", "shape");
        miter_shape.parent = Some(0);
        miter_shape.bounds = Rect { x: 20.0, y: 20.0, w: 40.0, h: 40.0 };
        miter_shape.vector = Some(Box::new(crate::scene::VectorShape {
            geom: crate::scene::VectorGeom::Path {
                ops: vec![
                    onesim_protocol::PathOp::M([40.0, 0.0]),
                    onesim_protocol::PathOp::L([40.0, 40.0]),
                    onesim_protocol::PathOp::L([44.0, 0.0]),
                ],
                mirror_x: false,
            },
            fill: None,
            stroke: Some(crate::scene::VectorStroke {
                color: Color::rgba(1.0, 0.0, 0.0, 1.0),
                line_width: 10.0,
                line_cap: onesim_protocol::LineCap::Butt,
                line_join: onesim_protocol::LineJoin::Miter,
                miter_limit: 30.0,
                dash: vec![],
                dash_phase: 0.0,
                antialiased: true,
                inset: 0.0,
            }),
        }));
        let miter_scene = test_scene(160.0, 220.0, vec![
            effect_node("wrap", None, miter_shape.bounds, VisualOp::Opacity { amount: 0.9 }),
            miter_shape,
        ]);
        let cpu_miter = crate::render(&miter_scene, &mut fonts, 1.0);
        let mut miter_builder = Builder::new();
        let dl_miter = miter_builder.build(&miter_scene, &mut fonts, 1.0);
        assert_eq!(dl_miter.added.len(), 1);
        // The acute miter tip extends well past 60 + 52 = 112px downward; verify sprite height captures it.
        let cpu_max_y = (0..cpu_miter.height())
            .rev()
            .find(|&y| (0..cpu_miter.width()).any(|x| cpu_miter.pixel(x, y).unwrap().alpha() > 0))
            .expect("miter stroke drew pixels");
        assert!(cpu_max_y > 120, "acute miter tip should extend past 120px, got {cpu_max_y}");
        let Op::Image { transform, .. } = dl_miter.ops[0] else { panic!("expected Op::Image for 0.9 opacity") };
        let sprite_bottom = transform[5] as u32 + dl_miter.added[0].height - 1;
        assert_eq!(sprite_bottom, cpu_max_y, "bounded composite sprite must preserve the full miter tip");

        // Builder::reset() clears cached composite sprites and re-uploads on next build
        let im3 = img_builder.build(&img_scene, &mut fonts, 2.0);
        assert_eq!(im3.added.len(), 1);
        assert!(img_builder.cache_len() >= 1);
        img_builder.reset();
        assert_eq!(img_builder.cache_len(), 0);
        let im4 = img_builder.build(&img_scene, &mut fonts, 2.0);
        assert_eq!(im4.added.len(), 1, "after reset, composite sprite must be re-uploaded");
    }

    #[test]
    fn translated_affine_composite_hits_sprite_cache() {
        use onesim_protocol::VisualOp;
        let mut fonts = Fonts::with_fallback(true);
        let make_orb = |tx: f64, ty: f64| {
            let bounds = Rect { x: 150.0 + tx, y: 300.0 + ty, w: 96.0, h: 96.0 };
            let mut orb = Node::new("orb", "circle");
            orb.parent = Some(0);
            orb.bounds = bounds;
            orb.radius = 48.0;
            orb.fill = Some(Color::rgba(1.0, 0.37, 0.38, 1.0));
            orb.shadow = Some(crate::scene::Shadow {
                color: Color::rgba(1.0, 0.37, 0.38, 0.65),
                radius: 20.0,
                dx: 0.0,
                dy: 0.0,
            });
            test_scene(402.0, 874.0, vec![
                effect_node(
                    "scale",
                    None,
                    bounds,
                    VisualOp::Affine { matrix: [1.08, 0.0, 0.0, 1.08, 0.0, 0.0], anchor: [0.5, 0.5] },
                ),
                orb,
            ])
        };

        for scale in [2.5, 3.0] {
            let mut builder = Builder::new();
            let first = builder.build(&make_orb(0.0, 0.0), &mut fonts, scale);
            assert_eq!(first.added.len(), 1, "initial frame uploads the composite sprite");
            let Op::Image { id: first_id, transform: t0, .. } = first.ops[0] else {
                panic!("expected Op::Image for scaled composite");
            };

            for (tx, ty) in [(0.3, -0.7), (12.45, 27.81), (-33.19, 19.62), (48.01, -41.33)] {
                let dl = builder.build(&make_orb(tx, ty), &mut fonts, scale);
                assert!(
                    dl.added.is_empty(),
                    "translated affine composite at scale {scale} offset ({tx}, {ty}) should hit sprite cache"
                );
                let Op::Image { id, transform, .. } = dl.ops[0] else {
                    panic!("expected Op::Image for scaled composite");
                };
                assert_eq!(id, first_id, "should reuse the exact cached sprite id");
                let expected_e = t0[4] + (tx * scale) as f32;
                let expected_f = t0[5] + (ty * scale) as f32;
                assert!(
                    (transform[4] - expected_e).abs() < 0.05 && (transform[5] - expected_f).abs() < 0.05,
                    "transform should translate by ({}, {}), got ({}, {}) vs ({expected_e}, {expected_f})",
                    tx * scale,
                    ty * scale,
                    transform[4],
                    transform[5]
                );
            }
        }
    }
}
