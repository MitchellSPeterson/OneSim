//! GPU backend (wgpu + Vello) behind [`simcore_render::Renderer`].
//!
//! Vector content (fills, strokes, gradients, shadows, bitmaps, clips) is drawn by Vello into an
//! `Rgba8Unorm` storage texture and read back once per frame. Text, icon glyphs and SF Symbols are
//! drawn by the CPU rasterizer's own code into small cached "sprites" and composited at integer
//! device pixels, so they match the CPU reference exactly. Liquid Glass and scroll-edge effects read
//! the backdrop, which Vello cannot: the frame is split there. Each segment is rendered by Vello into the
//! target, copied into an accumulation texture, composited by compute passes (see [`glass`]), and the next
//! segment starts by drawing the accumulation texture (registered with Vello) as a full-frame image. Only the
//! final frame is read back. Where a node or the device cannot do this, that node takes the old path:
//! read back, `apply_backdrop` on the CPU, upload.

mod glass;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use simcore_render::{
    apply_backdrop, clip_radius, needs_backdrop, node_sprite, sprite_key, BackendKind, Diagnostics, Fonts, Frame, FrameTimings, Node,
    RenderError, Renderer, Scene, SpriteKey, SpriteLayer,
};
use tiny_skia::Pixmap;
use vello::kurbo::{Affine, Join, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{
    color::AlphaColor, Blob, Color, Extend, Fill, Gradient, ImageAlphaType,InterpolationAlphaSpace, ImageBrush, ImageData, ImageFormat, ImageQuality,
};
use vello::wgpu;
use glass::{GlassGpu, Surfaces};
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

/// Frames a cached sprite or bitmap may go unused before it is dropped.
const CACHE_TTL_FRAMES: u64 = 120;
/// Hard bound on cached sprites + bitmaps (oldest dropped first).
const CACHE_MAX_ENTRIES: usize = 4096;

/// Whether a GPU adapter can be acquired at all (cheap probe; creates no device).
pub fn adapter_available() -> bool {
    let instance = wgpu::Instance::default();
    pollster::block_on(instance.request_adapter(&adapter_options())).is_ok()
}

fn adapter_options() -> wgpu::RequestAdapterOptions<'static, 'static> {
    wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false, ..Default::default() }
}

struct Target {
    w: u32,
    h: u32,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Everything painted so far in the frame (straight alpha), registered with Vello to start the next segment.
struct Acc {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    img: ImageData,
}

struct Staging {
    size: u64,
    buf: wgpu::Buffer,
}

struct CachedSprite {
    /// Offset from the node's anchor (device px) and the premultiplied image; `None` = nothing to draw.
    image: Option<(i32, i32, ImageData)>,
    last_used: u64,
}

struct CachedBitmap {
    /// Keeps the allocation alive so the pointer key cannot be reused.
    _src: Arc<Pixmap>,
    image: ImageData,
    last_used: u64,
}

#[derive(Default)]
struct Errors {
    lost: Option<String>,
    other: Option<String>,
}

/// Headless (render-to-texture + readback) GPU renderer.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    vello: vello::Renderer,
    scene: vello::Scene,
    /// 1x1 transparent image drawn first in every pass (see [`GpuRenderer::begin_scene`]).
    anchor: ImageData,
    adapter: String,
    errors: Arc<Mutex<Errors>>,
    target: Option<Target>,
    acc: Option<Acc>,
    /// Bumped whenever `target` / `acc` are re-created.
    gen: u64,
    /// GPU glass pipelines, or `None` (reason in `glass_note`).
    glass: Option<GlassGpu>,
    glass_note: String,
    /// Backdrop nodes composited on the GPU / on the CPU so far.
    backdrops: (u64, u64),
    staging: Option<Staging>,
    sprites: HashMap<u64, CachedSprite>,
    bitmaps: HashMap<usize, CachedBitmap>,
    frames: u64,
    last: FrameTimings,
}

impl GpuRenderer {
    /// Acquire an adapter and device. `Err` carries a human-readable reason (no adapter, init failure).
    pub fn new_headless() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&adapter_options())).map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let name = format!("{} ({:?})", info.name, info.backend);
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("GPU device ({name}): {e}"))?;
        let errors = Arc::new(Mutex::new(Errors::default()));
        let e = errors.clone();
        device.set_device_lost_callback(move |reason, msg| {
            if let Ok(mut g) = e.lock() {
                g.lost = Some(format!("{reason:?}: {msg}"));
            }
        });
        let e = errors.clone();
        device.on_uncaptured_error(Arc::new(move |err: wgpu::Error| {
            if let Ok(mut g) = e.lock() {
                match err {
                    wgpu::Error::OutOfMemory { .. } => g.lost = Some(err.to_string()),
                    other => g.other = Some(other.to_string()),
                }
            }
        }));
        let vello = vello::Renderer::new(
            &device,
            RendererOptions { use_cpu: false, antialiasing_support: AaSupport::area_only(), num_init_threads: None, pipeline_cache: None },
        )
        .map_err(|e| format!("Vello init ({name}): {e}"))?;
        let (glass, glass_note) = if std::env::var("SIMCORE_GPU_GLASS").is_ok_and(|v| v == "cpu") {
            (None, "disabled by SIMCORE_GPU_GLASS=cpu".to_string())
        } else {
            match GlassGpu::new(&device) {
                Ok(g) => (Some(g), String::new()),
                Err(e) => (None, e),
            }
        };
        Ok(Self {
            device,
            queue,
            vello,
            scene: vello::Scene::new(),
            anchor: ImageData {
                data: Blob::from(vec![0u8; 4]),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width: 1,
                height: 1,
            },
            adapter: name,
            errors,
            target: None,
            acc: None,
            gen: 0,
            glass,
            glass_note,
            backdrops: (0, 0),
            staging: None,
            sprites: HashMap::new(),
            bitmaps: HashMap::new(),
            frames: 0,
            last: FrameTimings::default(),
        })
    }

    /// Start a fresh Vello scene. Vello keeps drawn images resident in a persistent atlas, but a pass that
    /// draws no image at all resets that atlas texture to 1x1 while its cache still believes the images are
    /// resident (vello 0.12), so cached sprites would then draw as nothing. Every pass therefore draws one
    /// transparent pixel image, which keeps the atlas alive.
    fn begin_scene(&mut self) {
        self.scene.reset();
        self.scene.draw_image(&ImageBrush::new(self.anchor.clone()).with_quality(ImageQuality::Low), Affine::IDENTITY);
    }

    /// Route every backdrop node through the CPU path (read back, `apply_backdrop`, upload). Also selectable with
    /// `SIMCORE_GPU_GLASS=cpu` at creation.
    pub fn disable_gpu_glass(&mut self, reason: &str) {
        self.glass = None;
        self.glass_note = reason.to_string();
    }

    /// Backdrop nodes (glass, scroll edge) composited so far on the GPU and on the CPU fallback path.
    pub fn backdrop_counts(&self) -> (u64, u64) {
        self.backdrops
    }

    /// Cached sprites and bitmaps currently held (bounded by [`CACHE_MAX_ENTRIES`]).
    pub fn cache_len(&self) -> usize {
        self.sprites.len() + self.bitmaps.len()
    }

    fn check_errors(&self) -> Result<(), RenderError> {
        let mut g = self.errors.lock().map_err(|_| RenderError::Other("GPU error state poisoned".into()))?;
        if let Some(m) = &g.lost {
            return Err(RenderError::DeviceLost(m.clone()));
        }
        match g.other.take() {
            Some(m) => Err(RenderError::Other(m)),
            None => Ok(()),
        }
    }

    fn evict(&mut self) {
        let now = self.frames;
        self.sprites.retain(|_, c| now.saturating_sub(c.last_used) <= CACHE_TTL_FRAMES);
        self.bitmaps.retain(|_, c| now.saturating_sub(c.last_used) <= CACHE_TTL_FRAMES);
        let over = self.cache_len().saturating_sub(CACHE_MAX_ENTRIES);
        if over > 0 {
            let mut ages: Vec<u64> = self.sprites.values().map(|c| c.last_used).chain(self.bitmaps.values().map(|c| c.last_used)).collect();
            ages.sort_unstable();
            let cut = ages[over - 1];
            self.sprites.retain(|_, c| c.last_used > cut);
            self.bitmaps.retain(|_, c| c.last_used > cut);
        }
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if self.target.as_ref().is_some_and(|t| (t.w, t.h) == (w, h)) {
            return;
        }
        let desc = |label, usage| wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        };
        let tex = self.device.create_texture(&desc("simcore-target", wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST));
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.target = Some(Target { w, h, tex, view });
        if let Some(old) = self.acc.take() {
            self.vello.unregister_texture(old.img);
        }
        let tex = self.device.create_texture(&desc("simcore-accumulation", wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST));
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let img = self.vello.register_texture(tex.clone());
        self.acc = Some(Acc { tex, view, img });
        self.gen += 1;
    }

    /// Render the current Vello scene into the target. `acc_in_scene`: the scene draws the accumulation texture.
    fn render_vello(&mut self, w: u32, h: u32, acc_in_scene: bool) -> Result<(), RenderError> {
        self.ensure_target(w, h);
        if let (true, Some(acc)) = (acc_in_scene, self.acc.as_ref()) {
            self.vello.mark_override_image_dirty(&acc.img);
        }
        let target = self.target.as_ref().ok_or_else(|| RenderError::Other("no target".into()))?;
        self.vello
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &target.view,
                &RenderParams { base_color: Color::TRANSPARENT, width: w, height: h, antialiasing_method: AaConfig::Area },
            )
            .map_err(|e| self.map_error(format!("Vello render: {e}")))?;
        // The target now holds the whole frame: make it the accumulation.
        if let (Some(t), Some(a)) = (self.target.as_ref(), self.acc.as_ref()) {
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simcore-accumulate") });
            enc.copy_texture_to_texture(t.tex.as_image_copy(), a.tex.as_image_copy(), wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
            self.queue.submit([enc.finish()]);
        }
        Ok(())
    }

    /// Nothing painted yet: both textures start transparent.
    fn clear_frame(&mut self) {
        if let (Some(t), Some(a)) = (self.target.as_ref(), self.acc.as_ref()) {
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simcore-clear") });
            enc.clear_texture(&t.tex, &wgpu::ImageSubresourceRange::default());
            enc.clear_texture(&a.tex, &wgpu::ImageSubresourceRange::default());
            self.queue.submit([enc.finish()]);
        }
    }

    /// Read the target back as a premultiplied pixmap.
    fn readback(&mut self, w: u32, h: u32, t: &mut FrameTimings) -> Result<Pixmap, RenderError> {
        let t0 = Instant::now();
        let bpr = (w * 4).div_ceil(256) * 256;
        let size = bpr as u64 * h as u64;
        if self.staging.as_ref().is_none_or(|s| s.size != size) {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("simcore-readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.staging = Some(Staging { size, buf });
        }
        let (Some(staging), Some(target)) = (self.staging.as_ref(), self.target.as_ref()) else {
            return Err(RenderError::Other("no staging buffer".into()));
        };
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simcore-readback") });
        enc.copy_texture_to_buffer(
            target.tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &staging.buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let result = Arc::new(Mutex::new(None));
        let r = result.clone();
        staging.buf.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if let Ok(mut g) = r.lock() {
                *g = Some(res);
            }
        });
        t.encode += t0.elapsed();
        let t1 = Instant::now();
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| self.map_error(format!("GPU wait: {e:?}")))?;
        t.gpu += t1.elapsed();
        let t2 = Instant::now();
        let mapped = result.lock().ok().and_then(|mut g| g.take());
        match mapped {
            Some(Ok(())) => {}
            Some(Err(e)) => return Err(self.map_error(format!("readback map: {e}"))),
            None => return Err(self.map_error("readback never completed".into())),
        }
        let mut out = vec![0u8; (w * h * 4) as usize];
        {
            let view = staging.buf.slice(..).get_mapped_range().map_err(|e| RenderError::Other(format!("readback range: {e}")))?;
            for (dst, src) in out.chunks_exact_mut(w as usize * 4).zip(view.chunks_exact(bpr as usize)) {
                // Vello writes straight alpha; the capture convention is premultiplied.
                let (dst4, _) = dst.as_chunks_mut::<4>();
                let (src4, _) = src.as_chunks::<4>();
                for (d, s) in dst4.iter_mut().zip(src4) {
                    let a = s[3] as u32;
                    if a == 255 {
                        *d = *s;
                    } else {
                        let pm = |c: u8| {
                            let p = c as u32 * a + 128;
                            ((p + (p >> 8)) >> 8) as u8
                        };
                        *d = [pm(s[0]), pm(s[1]), pm(s[2]), s[3]];
                    }
                }
            }
        }
        staging.buf.unmap();
        t.readback += t2.elapsed();
        self.check_errors()?;
        Pixmap::from_vec(out, tiny_skia::IntSize::from_wh(w, h).ok_or_else(|| RenderError::Other("bad size".into()))?)
            .ok_or_else(|| RenderError::Other("readback size mismatch".into()))
    }

    /// An error from a GPU call: a lost device wins over the call's own message.
    fn map_error(&self, msg: String) -> RenderError {
        match self.check_errors() {
            Err(e) => e,
            Ok(()) => RenderError::Other(msg),
        }
    }

    /// Composite backdrop node `idx` on the CPU: read the frame back, `apply_backdrop`, upload to target and accumulation.
    fn backdrop_on_cpu(&mut self, scene: &Scene, idx: usize, scale: f64, w: u32, h: u32, t: &mut FrameTimings) -> Result<(), RenderError> {
        let mut pm = self.readback(w, h, t)?;
        apply_backdrop(&mut pm, scene, idx, scale);
        // The pixmap is premultiplied, the textures hold straight alpha.
        let mut data = pm.take();
        for px in data.as_chunks_mut::<4>().0 {
            let a = px[3] as u32;
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) };
        if let (Some(tg), Some(a)) = (self.target.as_ref(), self.acc.as_ref()) {
            self.queue.write_texture(tg.tex.as_image_copy(), &data, layout, size);
            self.queue.write_texture(a.tex.as_image_copy(), &data, layout, size);
        }
        Ok(())
    }

    fn bitmap(&mut self, px: &Arc<Pixmap>) -> ImageData {
        let key = Arc::as_ptr(px) as usize;
        let now = self.frames;
        let e = self.bitmaps.entry(key).or_insert_with(|| CachedBitmap {
            _src: px.clone(),
            image: ImageData {
                data: Blob::from(px.data().to_vec()),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width: px.width(),
                height: px.height(),
            },
            last_used: now,
        });
        e.last_used = now;
        e.image.clone()
    }

    /// The cached sprite for `layer` of `n`: its offset from the anchor and image.
    fn sprite(&mut self, n: &Node, fonts: &mut Fonts, scale: f64, layer: SpriteLayer) -> Option<(SpriteKey, i32, i32, ImageData)> {
        let key = sprite_key(n, fonts, scale, layer)?;
        let now = self.frames;
        let entry = self.sprites.entry(key.hash).or_insert_with(|| CachedSprite {
            image: node_sprite(n, fonts, scale, layer).map(|sp| {
                let (w, h) = (sp.pixmap.width(), sp.pixmap.height());
                let img = ImageData {
                    data: Blob::from(sp.pixmap.take()),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::AlphaPremultiplied,
                    width: w,
                    height: h,
                };
                (sp.x - key.anchor.0, sp.y - key.anchor.1, img)
            }),
            last_used: now,
        });
        entry.last_used = now;
        entry.image.clone().map(|(dx, dy, img)| (key, dx, dy, img))
    }
}

fn c8(c: simcore_render::Color, opacity: f32) -> Color {
    let q = |v: f32| (v * 255.0).round() as u8;
    Color::from_rgba8(q(c.r), q(c.g), q(c.b), q(c.a * opacity))
}

fn rrect(x: f64, y: f64, w: f64, h: f64, r: f64) -> RoundedRect {
    RoundedRect::new(x, y, x + w, y + h, r.min(w / 2.0).min(h / 2.0).max(0.0))
}

/// Open clip layers of the current Vello scene, by clipping node index.
#[derive(Default)]
struct Clips(Vec<usize>);

impl Clips {
    /// Make exactly `want` (outermost first) the open layers.
    fn sync(&mut self, scene: &mut vello::Scene, nodes: &[Node], want: &[usize], s: f64) {
        let common = self.0.iter().zip(want).take_while(|(a, b)| a == b).count();
        for _ in common..self.0.len() {
            scene.pop_layer();
        }
        self.0.truncate(common);
        for &i in &want[common..] {
            let (b, r) = (nodes[i].bounds, nodes[i].clip_radius.unwrap_or(0.0));
            scene.push_clip_layer(Fill::NonZero, Affine::scale(s), &rrect(b.x, b.y, b.w, b.h, r));
            self.0.push(i);
        }
    }

    fn close(&mut self, scene: &mut vello::Scene) {
        for _ in 0..self.0.len() {
            scene.pop_layer();
        }
        self.0.clear();
    }
}

impl Renderer for GpuRenderer {
    fn kind(&self) -> BackendKind {
        BackendKind::Gpu
    }

    fn capture(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Frame, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::Other(format!("bad scale {scale}")));
        }
        self.check_errors()?;
        let pw = (scene.size.0 * scale).round().max(1.0) as u32;
        let ph = (scene.size.1 * scale).round().max(1.0) as u32;
        let max = self.device.limits().max_texture_dimension_2d;
        if pw > max || ph > max {
            return Err(RenderError::Unsupported(format!("frame {pw}x{ph} exceeds the GPU texture limit {max}")));
        }
        self.frames += 1;
        let mut timings = FrameTimings::default();
        let t_all = Instant::now();
        let s = scale;
        let sf = scale as f32;
        let nodes = &scene.nodes;

        // `rendered`: the target holds the frame so far (and equals the accumulation texture, apart from `dirty`
        // drawing). `dirty`: the Vello scene holds drawing not yet rendered. `acc_in_scene`: the scene starts by
        // drawing the accumulation texture.
        let (mut rendered, mut dirty, mut acc_in_scene) = (false, false, false);
        self.ensure_target(pw, ph);
        self.begin_scene();
        let mut clips = Clips::default();
        // Clipping ancestors of each node (outermost first).
        let mut chains: Vec<Rc<Vec<usize>>> = Vec::with_capacity(nodes.len());

        for (i, n) in nodes.iter().enumerate() {
            let chain = match n.parent {
                None => Rc::new(vec![]),
                Some(p) => {
                    if clip_radius(&nodes[p], sf, pw, ph).is_some() {
                        let mut c = (*chains[p]).clone();
                        c.push(p);
                        Rc::new(c)
                    } else {
                        chains[p].clone()
                    }
                }
            };
            chains.push(chain.clone());

            let backdrop = needs_backdrop(n);
            if backdrop {
                clips.close(&mut self.scene);
                if dirty {
                    self.render_vello(pw, ph, acc_in_scene)?;
                    rendered = true;
                    dirty = false;
                } else if !rendered {
                    self.clear_frame();
                    rendered = true;
                }
                let on_gpu = match (self.glass.as_mut(), self.acc.as_ref(), self.target.as_ref()) {
                    (Some(glass), Some(acc), Some(target)) if GlassGpu::supports(scene, i, scale) => {
                        let fr = Surfaces {
                            device: &self.device,
                            queue: &self.queue,
                            acc: &acc.tex,
                            acc_view: &acc.view,
                            target: &target.tex,
                            target_view: &target.view,
                            gen: self.gen,
                            size: (pw, ph),
                        };
                        match glass.encode(&fr, scene, i, scale) {
                            Ok(()) => true,
                            Err(e) => {
                                log::warn!("GPU glass unavailable for node {}: {e}", n.id);
                                false
                            }
                        }
                    }
                    _ => false,
                };
                if on_gpu {
                    self.backdrops.0 += 1;
                } else {
                    self.backdrops.1 += 1;
                    self.backdrop_on_cpu(scene, i, scale, pw, ph, &mut timings)?;
                }
                self.begin_scene();
                if let Some(acc) = &self.acc {
                    self.scene.draw_image(&ImageBrush::new(acc.img.clone()).with_quality(ImageQuality::Low), Affine::IDENTITY);
                    acc_in_scene = true;
                }
            }
            clips.sync(&mut self.scene, nodes, &chain, s);

            let b = n.bounds;
            let xf = Affine::scale(s);
            // Own shadow (CPU-composited for backdrop nodes, in apply_backdrop).
            if let (Some(sd), false) = (&n.shadow, backdrop) {
                let radius = n.radius.max(n.clip_radius.unwrap_or(0.0));
                let sigma = (sd.radius * s).max(0.5);
                let (px, py) = (b.x * s + sd.dx * s, b.y * s + sd.dy * s);
                let (w, h) = (b.w * s, b.h * s);
                if w > 0.0 && h > 0.0 {
                    let r = (radius * s).min(w / 2.0).min(h / 2.0).max(0.0);
                    self.scene.draw_blurred_rounded_rect(Affine::IDENTITY, Rect::new(px, py, px + w, py + h), c8(sd.color, n.opacity), r, sigma);
                    dirty = true;
                }
            }
            if let Some(c) = n.fill {
                if n.radius > 0.0 {
                    if b.w > 0.0 && b.h > 0.0 {
                        self.scene.fill(Fill::NonZero, xf, c8(c, n.opacity), None, &rrect(b.x, b.y, b.w, b.h, n.radius));
                        dirty = true;
                    }
                } else {
                    // Snap to device pixels like UIKit (keeps hairlines crisp).
                    let (x0, y0) = ((b.x as f32 * sf).round() as f64, (b.y as f32 * sf).round() as f64);
                    let x1 = (((b.x + b.w) as f32 * sf).round() as f64).max(x0 + 1.0);
                    let y1 = (((b.y + b.h) as f32 * sf).round() as f64).max(y0 + 1.0);
                    self.scene.fill(Fill::NonZero, Affine::IDENTITY, c8(c, n.opacity), None, &Rect::new(x0, y0, x1, y1));
                    dirty = true;
                }
            }
            if let Some(g) = &n.gradient {
                dirty |= gradient(&mut self.scene, n, g, s);
            }
            if let Some((c, lw)) = n.stroke {
                if b.w >= 0.0 && b.h >= 0.0 {
                    let st = Stroke::new(lw).with_join(Join::Miter);
                    self.scene.stroke(&st, xf, c8(c, n.opacity), None, &rrect(b.x, b.y, b.w, b.h, n.radius));
                    dirty = true;
                }
            }
            if let Some((key, dx, dy, img)) = self.sprite(n, fonts, scale, SpriteLayer::Icons) {
                sprite_draw(&mut self.scene, &key, dx, dy, img);
                dirty = true;
            }
            if let Some(im) = &n.image {
                let data = self.bitmap(&im.pixmap);
                dirty |= image(&mut self.scene, n, im, data, s);
            }
            if let Some((key, dx, dy, img)) = self.sprite(n, fonts, scale, SpriteLayer::Text) {
                sprite_draw(&mut self.scene, &key, dx, dy, img);
                dirty = true;
            }
        }
        clips.close(&mut self.scene);

        if dirty {
            self.render_vello(pw, ph, acc_in_scene)?;
            rendered = true;
        }
        let pm = if rendered {
            self.readback(pw, ph, &mut timings)?
        } else {
            Pixmap::new(pw, ph).ok_or_else(|| RenderError::Other("pixmap size".into()))?
        };
        self.evict();
        self.check_errors()?;
        // Everything but the final wait and readback (scene encoding, Vello and glass submission) counts as encode.
        let wall = t_all.elapsed();
        timings.encode = wall.saturating_sub(timings.gpu + timings.readback).max(Duration::ZERO);
        self.last = timings;
        let (width, height) = (pm.width(), pm.height());
        Ok(Frame { width, height, rgba: pm.take() })
    }

    fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            backend: BackendKind::Gpu.to_string(),
            adapter: self.adapter.clone(),
            limitations: vec![
                if self.glass.is_some() {
                    "liquid glass and scroll-edge effects: GPU compute passes on an accumulation texture (single readback); nodes combining scroll edge with glass or a shadow, or with more than 4 ancestor clips, use the CPU path".to_string()
                } else {
                    format!("liquid glass and scroll-edge effects: CPU composite via readback ({})", self.glass_note)
                },
                "text, icon glyphs, SF Symbols, underline/strike: drawn by the CPU rasterizer into cached sprites".into(),
                "vector anti-aliasing, shadow blur and gradients differ from the CPU reference by small per-channel amounts (not byte-identical)".into(),
            ],
            fallback_reason: None,
            last_frame: self.last,
            frames: self.frames,
        }
    }
}

fn sprite_draw(scene: &mut vello::Scene, key: &SpriteKey, dx: i32, dy: i32, img: ImageData) {
    // Integer device-pixel translation, identity scale, nearest sampling: pixel-exact.
    let t = Affine::translate(((key.anchor.0 + dx) as f64, (key.anchor.1 + dy) as f64));
    scene.draw_image(&ImageBrush::new(img).with_quality(ImageQuality::Low), t);
}

/// Linear gradient over the node's rounded bounds (mirrors the CPU `draw_gradient`).
fn gradient(scene: &mut vello::Scene, n: &Node, g: &simcore_render::Gradient, s: f64) -> bool {
    let b = n.bounds;
    if g.stops.is_empty() || b.w <= 0.0 || b.h <= 0.0 {
        return false;
    }
    let shape = rrect(b.x, b.y, b.w, b.h, n.radius);
    let k = g.stops.len().saturating_sub(1).max(1) as f32;
    let stops: Vec<(f32, Color)> = g
        .stops
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let at = g.locations.as_ref().and_then(|l| l.get(i)).copied().unwrap_or(i as f32 / k);
            (at, AlphaColor::new([c.r, c.g, c.b, c.a * n.opacity]))
        })
        .collect();
    let (from, to) = if let Some([x0, y0, x1, y1]) = g.points {
        // Unit-box points placed on the bounds (see the CPU path).
        let at = |u: f32, v: f32| Point::new(b.x + u as f64 * b.w, b.y + v as f64 * b.h);
        (at(x0, y0), at(x1, y1))
    } else {
        let a = (g.angle as f64).to_radians();
        let (dx, dy) = (a.sin(), -a.cos());
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let half = (b.w * dx.abs() + b.h * dy.abs()) / 2.0;
        (Point::new(cx - dx * half, cy - dy * half), Point::new(cx + dx * half, cy + dy * half))
    };
    if from == to {
        return false;
    }
    let mut grad = Gradient::new_linear(from, to).with_stops(stops.as_slice());
    // tiny-skia interpolates straight (unpremultiplied) color.
    grad.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
    scene.fill(Fill::NonZero, Affine::scale(s), &grad, None, &shape);
    true
}

/// Draw `im.dest` of the bitmap, clipped to the node's rounded bounds (mirrors the CPU `draw_image`).
fn image(scene: &mut vello::Scene, n: &Node, im: &simcore_render::ImageRun, data: ImageData, s: f64) -> bool {
    let (b, d) = (n.bounds, im.dest);
    let (iw, ih) = (im.pixmap.width() as f64, im.pixmap.height() as f64);
    // A rotated bitmap may reach anywhere in the bounds.
    let d = if im.rotation != 0.0 { b } else { d };
    let (x0, y0) = (d.x.max(b.x), d.y.max(b.y));
    let (x1, y1) = ((d.x + d.w).min(b.x + b.w), (d.y + d.h).min(b.y + b.h));
    if x1 <= x0 || y1 <= y0 || b.w <= 0.0 || b.h <= 0.0 {
        return false;
    }
    let mut to_pt = Affine::scale_non_uniform(d.w / iw, d.h / ih).then_translate((d.x, d.y).into());
    if im.rotation != 0.0 {
        to_pt = to_pt.then_rotate_about((im.rotation as f64).to_radians(), Point::new(d.x + d.w / 2.0, d.y + d.h / 2.0));
    }
    let brush = ImageBrush::new(data).with_quality(ImageQuality::High).with_alpha(n.opacity).with_extend(Extend::Pad);
    let clipped = n.radius > 0.0;
    if clipped {
        scene.push_clip_layer(Fill::NonZero, Affine::scale(s), &rrect(b.x, b.y, b.w, b.h, n.radius));
    }
    scene.fill(Fill::NonZero, Affine::scale(s), &brush, Some(to_pt), &Rect::new(x0, y0, x1, y1));
    if clipped {
        scene.pop_layer();
    }
    true
}
