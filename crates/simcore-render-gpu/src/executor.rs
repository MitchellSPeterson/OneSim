//! [`Executor`]: runs a [`DisplayList`] on a wgpu device, with no fonts, scene or layout.
//!
//! Vector ops (fills, strokes, gradients, shadows, bitmaps, sprites, clips) go to Vello, which writes an
//! `Rgba8Unorm` storage texture (straight alpha). Backdrop ops (`Glass`, `BackdropShadow`, `Edge`) read what is
//! painted so far, which Vello cannot: the frame is split there. The segment so far is rendered into the target
//! and copied into an accumulation texture, compute passes (see [`crate::glass`]) composite the op, and the next
//! segment starts by drawing the accumulation texture (registered with Vello) as a full-frame image. A backdrop
//! op the device cannot run takes the CPU path on native builds: read the
//! frame back, `simcore_render::apply_backdrop_op`, upload.
//!
//! Nothing here blocks on the GPU except that fallback, so with the `native` feature off the executor builds
//! for `wasm32-unknown-unknown` (create it with [`Executor::new_async`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use simcore_render::display::{DisplayList, Op, RRect, Sampling};
use simcore_render::{FrameTimings, RenderError};
use vello::kurbo::{Affine, Join, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{color::AlphaColor, Blob, Color, Extend, Fill, Gradient, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality, InterpolationAlphaSpace};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

use crate::glass::{Backdrop, GlassGpu, OwnShadow, Surfaces};

/// Texture usages the caller's target must have: Vello and the glass passes write it as a storage texture and the
/// executor copies from it.
pub fn target_usage() -> wgpu::TextureUsages {
    wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC
}

/// What one [`Executor::execute`] did.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExecStats {
    /// Backdrop ops composited by GPU compute passes / by the CPU fallback this frame.
    pub backdrops: (u64, u64),
    /// Time the CPU fallback spent waiting for the GPU and copying the frame out (zero otherwise).
    pub fallback: FrameTimings,
    /// CPU time spent encoding and submitting Vello passes. Measured on native only.
    pub vello: std::time::Duration,
}

#[derive(Default)]
struct Errors {
    lost: Option<String>,
    other: Option<String>,
}

/// Everything painted so far in the frame (straight alpha), registered with Vello to start the next segment.
struct Acc {
    w: u32,
    h: u32,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    img: ImageData,
}

/// The caller's current target and a view of it.
struct Tgt {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Display-list executor on a wgpu device.
pub struct Executor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    vello: vello::Renderer,
    scene: vello::Scene,
    /// 1x1 transparent image drawn first in every pass (see [`Executor::begin_scene`]).
    anchor: ImageData,
    errors: Arc<Mutex<Errors>>,
    resources: HashMap<u64, ImageData>,
    acc: Option<Acc>,
    tgt: Option<Tgt>,
    /// Bumped whenever `tgt` / `acc` change (invalidates the glass bind groups).
    gen: u64,
    glass: Option<GlassGpu>,
    glass_note: String,
    backdrops: (u64, u64),
    #[cfg(feature = "native")]
    readback: crate::readback::Readback,
}

fn rrect(r: &RRect) -> RoundedRect {
    RoundedRect::new(r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64, r.r.min(r.w / 2.0).min(r.h / 2.0).max(0.0) as f64)
}

fn c8(c: simcore_render::Color) -> Color {
    let q = |v: f32| (v * 255.0).round() as u8;
    Color::from_rgba8(q(c.r), q(c.g), q(c.b), q(c.a))
}

impl Executor {
    /// Blocking constructor for native hosts. Use [`Executor::new_async`] where blocking is not allowed (the browser).
    #[cfg(feature = "native")]
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        pollster::block_on(Self::new_async(device, queue))
    }

    /// Take over `device` and `queue`: compile the Vello and glass pipelines and install error callbacks.
    /// `Err` carries the Vello failure; a glass shader failure only disables GPU glass (see [`Executor::glass_note`]).
    pub async fn new_async(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
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
        .map_err(|e| format!("Vello init: {e}"))?;
        let (glass, glass_note) = match GlassGpu::new(&device).await {
            Ok(g) => (Some(g), String::new()),
            Err(e) => (None, e),
        };
        Ok(Self {
            #[cfg(feature = "native")]
            readback: crate::readback::Readback::default(),
            device,
            queue,
            vello,
            scene: vello::Scene::new(),
            anchor: ImageData { data: Blob::from(vec![0u8; 4]), format: ImageFormat::Rgba8, alpha_type: ImageAlphaType::AlphaPremultiplied, width: 1, height: 1 },
            errors,
            resources: HashMap::new(),
            acc: None,
            tgt: None,
            gen: 0,
            glass,
            glass_note,
            backdrops: (0, 0),
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Whether backdrop ops run as GPU compute passes (otherwise the CPU fallback, or an error without `native`).
    pub fn glass_on_gpu(&self) -> bool {
        self.glass.is_some()
    }

    /// Why GPU glass is off (empty while it is on).
    pub fn glass_note(&self) -> &str {
        &self.glass_note
    }

    /// Route every backdrop op through the CPU fallback.
    pub fn disable_gpu_glass(&mut self, reason: &str) {
        self.glass = None;
        self.glass_note = reason.to_string();
    }

    /// Backdrop ops composited so far on the GPU and on the CPU fallback.
    pub fn backdrop_counts(&self) -> (u64, u64) {
        self.backdrops
    }

    /// Resources currently held.
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Forget every resource (after a failed frame, or when the sender restarts its resource table).
    pub fn clear_resources(&mut self) {
        self.resources.clear();
    }

    /// A device-lost or other error the device reported since the last check.
    pub fn check_errors(&self) -> Result<(), RenderError> {
        let mut g = self.errors.lock().map_err(|_| RenderError::Other("GPU error state poisoned".into()))?;
        if let Some(m) = &g.lost {
            return Err(RenderError::DeviceLost(m.clone()));
        }
        match g.other.take() {
            Some(m) => Err(RenderError::Other(m)),
            None => Ok(()),
        }
    }

    /// An error from a GPU call: a lost device wins over the call's own message.
    pub(crate) fn map_error(&self, msg: String) -> RenderError {
        match self.check_errors() {
            Err(e) => e,
            Ok(()) => RenderError::Other(msg),
        }
    }

    /// Start a fresh Vello scene. Vello keeps drawn images resident in a persistent atlas, but a pass that
    /// draws no image at all resets that atlas texture to 1x1 while its cache still believes the images are
    /// resident (vello 0.12), so cached sprites would then draw as nothing. Every pass therefore draws one
    /// transparent pixel image, which keeps the atlas alive.
    fn begin_scene(&mut self) {
        self.scene.reset();
        self.scene.draw_image(&ImageBrush::new(self.anchor.clone()).with_quality(ImageQuality::Low), Affine::IDENTITY);
    }

    fn set_target(&mut self, target: &wgpu::Texture) {
        if self.tgt.as_ref().is_some_and(|t| t.tex == *target) {
            return;
        }
        self.tgt = Some(Tgt { tex: target.clone(), view: target.create_view(&wgpu::TextureViewDescriptor::default()) });
        self.gen += 1;
    }

    /// The accumulation texture, created (or resized) on first use by a frame with backdrop ops.
    fn ensure_acc(&mut self, w: u32, h: u32) {
        if self.acc.as_ref().is_some_and(|a| (a.w, a.h) == (w, h)) {
            return;
        }
        if let Some(old) = self.acc.take() {
            self.vello.unregister_texture(old.img);
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("simcore-accumulation"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let img = self.vello.register_texture(tex.clone());
        self.acc = Some(Acc { w, h, tex, view, img });
        self.gen += 1;
    }

    /// Render the current Vello scene into the target; with `to_acc`, then copy the whole frame into the
    /// accumulation texture. `acc_in_scene`: the scene draws the accumulation texture.
    fn render_vello(&mut self, (w, h): (u32, u32), acc_in_scene: bool, to_acc: bool, _stats: &mut ExecStats) -> Result<(), RenderError> {
        if let (true, Some(acc)) = (acc_in_scene, self.acc.as_ref()) {
            self.vello.mark_override_image_dirty(&acc.img);
        }
        let tgt = self.tgt.as_ref().ok_or_else(|| RenderError::Other("no target".into()))?;
        #[cfg(feature = "native")]
        let start = std::time::Instant::now();
        self.vello
            .render_to_texture(&self.device, &self.queue, &self.scene, &tgt.view, &RenderParams { base_color: Color::TRANSPARENT, width: w, height: h, antialiasing_method: AaConfig::Area })
            .map_err(|e| self.map_error(format!("Vello render: {e}")))?;
        #[cfg(feature = "native")]
        {
            _stats.vello += start.elapsed();
        }
        if let (true, Some(t), Some(a)) = (to_acc, self.tgt.as_ref(), self.acc.as_ref()) {
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simcore-accumulate") });
            enc.copy_texture_to_texture(t.tex.as_image_copy(), a.tex.as_image_copy(), wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
            self.queue.submit([enc.finish()]);
        }
        Ok(())
    }

    /// Run `dl` into `target`, a `Rgba8Unorm` texture of the list's size with [`target_usage`]. Vello writes
    /// straight alpha; every pixel of `target` is written. Work is submitted, not waited for.
    pub fn execute(&mut self, dl: &DisplayList, target: &wgpu::Texture) -> Result<ExecStats, RenderError> {
        self.check_errors()?;
        let (w, h) = (dl.width, dl.height);
        if target.format() != wgpu::TextureFormat::Rgba8Unorm || !target.usage().contains(target_usage()) || (target.width(), target.height()) != (w, h) {
            return Err(RenderError::Other(format!(
                "target must be {w}x{h} Rgba8Unorm with {:?}, got {}x{} {:?} {:?}",
                target_usage(),
                target.width(),
                target.height(),
                target.format(),
                target.usage()
            )));
        }
        let max = self.device.limits().max_texture_dimension_2d;
        if w > max || h > max {
            return Err(RenderError::Unsupported(format!("frame {w}x{h} exceeds the GPU texture limit {max}")));
        }
        for id in &dl.dropped {
            self.resources.remove(id);
        }
        for r in &dl.added {
            if r.rgba.len() as u64 != r.width as u64 * r.height as u64 * 4 || r.width == 0 || r.height == 0 {
                return Err(RenderError::Other(format!("resource {}: {} bytes for {}x{}", r.id, r.rgba.len(), r.width, r.height)));
            }
            let image = ImageData { data: Blob::from(r.rgba.clone()), format: ImageFormat::Rgba8, alpha_type: ImageAlphaType::AlphaPremultiplied, width: r.width, height: r.height };
            self.resources.insert(r.id, image);
        }
        self.set_target(target);
        // The last op that paints (trailing pops do not), and whether the frame has backdrop ops at all (only then is the accumulation texture needed).
        let tail = dl.ops.iter().rposition(|o| !matches!(o, Op::PopClip));
        if dl.ops.iter().any(|o| o.is_backdrop()) {
            self.ensure_acc(w, h);
        }
        let before = self.backdrops;
        let mut stats = ExecStats::default();

        // `rendered`: the target holds the frame so far. `dirty`: the Vello scene holds drawing not yet rendered.
        // `acc_in_scene`: the scene starts by drawing the accumulation texture. `clips`: logical clip stack, all
        // of it open as layers in the scene.
        let (mut rendered, mut dirty, mut acc_in_scene) = (false, false, false);
        let mut clips: Vec<RRect> = vec![];
        self.begin_scene();
        let missing = |id: u64| RenderError::Other(format!("display list uses unknown resource {id}"));

        for (i, op) in dl.ops.iter().enumerate() {
            match op {
                Op::PushClip(r) => {
                    self.scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rrect(r));
                    clips.push(*r);
                }
                Op::PopClip => {
                    if clips.pop().is_some() {
                        self.scene.pop_layer();
                    }
                }
                Op::Fill { shape, color } => {
                    if shape.w > 0.0 && shape.h > 0.0 {
                        if shape.r > 0.0 {
                            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c8(*color), None, &rrect(shape));
                        } else {
                            let (x, y) = (shape.x as f64, shape.y as f64);
                            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c8(*color), None, &Rect::new(x, y, x + shape.w as f64, y + shape.h as f64));
                        }
                        dirty = true;
                    }
                }
                Op::Stroke { shape, width, color } => {
                    let st = Stroke::new(*width as f64).with_join(Join::Miter);
                    self.scene.stroke(&st, Affine::IDENTITY, c8(*color), None, &rrect(shape));
                    dirty = true;
                }
                Op::Gradient { shape, p0, p1, stops } => {
                    let (from, to) = (Point::new(p0.0 as f64, p0.1 as f64), Point::new(p1.0 as f64, p1.1 as f64));
                    if stops.is_empty() || from == to {
                        continue;
                    }
                    let stops: Vec<(f32, Color)> = stops.iter().map(|(at, c)| (*at, AlphaColor::new([c.r, c.g, c.b, c.a]))).collect();
                    let mut grad = Gradient::new_linear(from, to).with_stops(stops.as_slice());
                    // tiny-skia interpolates straight (unpremultiplied) color.
                    grad.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
                    self.scene.fill(Fill::NonZero, Affine::IDENTITY, &grad, None, &rrect(shape));
                    dirty = true;
                }
                Op::Shadow { shape, color, sigma, dx, dy } => {
                    let (x, y) = ((shape.x + dx) as f64, (shape.y + dy) as f64);
                    let (sw, sh) = (shape.w as f64, shape.h as f64);
                    if sw > 0.0 && sh > 0.0 {
                        let r = (shape.r as f64).min(sw / 2.0).min(sh / 2.0).max(0.0);
                        self.scene.draw_blurred_rounded_rect(Affine::IDENTITY, Rect::new(x, y, x + sw, y + sh), c8(*color), r, sigma.max(0.5) as f64);
                        dirty = true;
                    }
                }
                Op::Image { id, transform, sampling, opacity, shape } => {
                    let data = self.resources.get(id).ok_or_else(|| missing(*id))?;
                    let quality = match sampling {
                        Sampling::Nearest => ImageQuality::Low,
                        Sampling::Bilinear => ImageQuality::Medium,
                        Sampling::Bicubic => ImageQuality::High,
                    };
                    let (iw, ih) = (data.width as f64, data.height as f64);
                    let brush = ImageBrush::new(data.clone()).with_quality(quality).with_alpha(*opacity).with_extend(Extend::Pad);
                    let xf = Affine::new(transform.map(|v| v as f64));
                    if let Some(shape) = shape {
                        self.scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, Some(xf), &rrect(shape));
                    } else {
                        self.scene.fill(Fill::NonZero, xf, &brush, None, &Rect::new(0.0, 0.0, iw, ih));
                    }
                    dirty = true;
                }
                Op::Sprite { id, x, y } => {
                    let data = self.resources.get(id).ok_or_else(|| missing(*id))?;
                    // Integer device-pixel translation, identity scale, nearest sampling: pixel-exact.
                    self.scene.draw_image(&ImageBrush::new(data.clone()).with_quality(ImageQuality::Low), Affine::translate((*x as f64, *y as f64)));
                    dirty = true;
                }
                Op::Glass { spec, .. } if spec.variant == simcore_render::glass::GlassVariant::Identity => {}
                Op::Glass { .. } | Op::BackdropShadow { .. } | Op::Edge(_) => {
                    // Close the layers, render what is pending and make it the accumulation.
                    for _ in 0..clips.len() {
                        self.scene.pop_layer();
                    }
                    if dirty || !rendered {
                        self.render_vello((w, h), acc_in_scene, true, &mut stats)?;
                        rendered = true;
                    }
                    // Whatever follows reads the accumulation: this pass's result must reach it.
                    let copy_back = tail.is_some_and(|t| i < t);
                    let on_cpu = self.backdrop(op, &clips, dl, copy_back, &mut stats)?;
                    self.begin_scene();
                    if let Some(acc) = &self.acc {
                        self.scene.draw_image(&ImageBrush::new(acc.img.clone()).with_quality(ImageQuality::Low), Affine::IDENTITY);
                        acc_in_scene = true;
                    }
                    // The CPU path wrote the accumulation texture only (the target need not be copyable to): the
                    // scene, which starts by drawing it, must be rendered even if nothing else follows.
                    dirty = on_cpu;
                    for c in &clips {
                        self.scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rrect(c));
                    }
                }
            }
        }
        for _ in 0..clips.len() {
            self.scene.pop_layer();
        }
        if dirty || !rendered {
            self.render_vello((w, h), acc_in_scene, false, &mut stats)?;
        }
        self.check_errors()?;
        stats.backdrops = (self.backdrops.0 - before.0, self.backdrops.1 - before.1);
        Ok(stats)
    }

    /// Composite one backdrop op on the GPU, or on the CPU when it cannot.
    fn backdrop(&mut self, op: &Op, clips: &[RRect], dl: &DisplayList, copy_back: bool, stats: &mut ExecStats) -> Result<bool, RenderError> {
        let bd = match op {
            Op::Glass { shape, spec, opacity, halo_radius } => {
                Backdrop { glass: Some((spec, *shape, *halo_radius)), opacity: *opacity, shadow: None, edge: None }
            }
            Op::BackdropShadow { shape, color, sigma, dx, dy } => {
                Backdrop { glass: None, opacity: 1.0, shadow: Some(OwnShadow { shape: *shape, color: *color, sigma: *sigma, dx: *dx, dy: *dy }), edge: None }
            }
            Op::Edge(e) => Backdrop { glass: None, opacity: 1.0, shadow: None, edge: Some(*e) },
            _ => return Ok(false),
        };
        let mut why = if self.glass.is_none() { self.glass_note.clone() } else { String::new() };
        if let (Some(glass), Some(acc), Some(tgt)) = (self.glass.as_mut(), self.acc.as_ref(), self.tgt.as_ref()) {
            let fr = Surfaces {
                device: &self.device,
                queue: &self.queue,
                acc: &acc.tex,
                acc_view: &acc.view,
                target: &tgt.tex,
                target_view: &tgt.view,
                gen: self.gen,
                size: (dl.width, dl.height),
            };
            match glass.encode(&fr, &bd, clips, dl, copy_back) {
                Ok(()) => {
                    self.backdrops.0 += 1;
                    return Ok(false);
                }
                Err(e) => {
                    log::warn!("GPU backdrop pass unavailable: {e}");
                    why = e;
                }
            }
        }
        self.backdrops.1 += 1;
        self.backdrop_on_cpu(op, clips, dl, &why, stats)?;
        Ok(true)
    }

    /// Composite a backdrop op on the CPU: read the frame back, `apply_backdrop_op`, upload to the accumulation texture (the caller then re-renders the frame from it).
    #[cfg(feature = "native")]
    fn backdrop_on_cpu(&mut self, op: &Op, clips: &[RRect], dl: &DisplayList, _why: &str, stats: &mut ExecStats) -> Result<(), RenderError> {
        let (w, h) = (dl.width, dl.height);
        let Some(tgt) = self.tgt.as_ref() else { return Err(RenderError::Other("no target".into())) };
        let data = self.readback.read(&self.device, &self.queue, &tgt.tex, (w, h), true, &mut stats.fallback).map_err(|m| self.map_error(m))?;
        let size = tiny_skia::IntSize::from_wh(w, h).ok_or_else(|| RenderError::Other("bad size".into()))?;
        let mut pm = tiny_skia::Pixmap::from_vec(data, size).ok_or_else(|| RenderError::Other("readback size mismatch".into()))?;
        simcore_render::apply_backdrop_op(&mut pm, op, clips, dl.scale, dl.dark, dl.ios);
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
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) };
        if let Some(a) = self.acc.as_ref() {
            self.queue.write_texture(a.tex.as_image_copy(), &data, layout, extent);
        }
        Ok(())
    }

    #[cfg(not(feature = "native"))]
    fn backdrop_on_cpu(&mut self, _op: &Op, _clips: &[RRect], _dl: &DisplayList, why: &str, _stats: &mut ExecStats) -> Result<(), RenderError> {
        Err(RenderError::Unsupported(format!("backdrop op needs the CPU fallback, which this build does not have ({why})")))
    }
}
