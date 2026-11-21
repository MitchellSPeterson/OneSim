//! Headless renderer: [`Builder`] + [`Executor`] + one readback per frame.

use std::time::{Duration, Instant};

use onesim_render::{BackendKind, Builder, Diagnostics, Fonts, Frame, FrameTimings, RenderError, Renderer, Scene};
use vello::wgpu;

use crate::executor::{target_usage, Executor};
use crate::readback::Readback;

/// Whether a GPU adapter can be acquired at all (cheap probe; creates no device).
pub fn adapter_available() -> bool {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(instance.request_adapter(&adapter_options())).is_ok()
}

fn adapter_options() -> wgpu::RequestAdapterOptions<'static, 'static> {
    wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false, ..Default::default() }
}

struct Target {
    w: u32,
    h: u32,
    tex: wgpu::Texture,
}

/// Headless (render-to-texture + readback) GPU renderer.
pub struct GpuRenderer {
    exec: Executor,
    builder: Builder,
    readback: Readback,
    adapter: String,
    target: Option<Target>,
    frames: u64,
    last: FrameTimings,
    build_time: Duration,
    vello_time: Duration,
}

impl GpuRenderer {
    /// Acquire an adapter and device. `Err` carries a human-readable reason (no adapter, init failure).
    pub fn new_headless() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&adapter_options())).map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let name = format!("{} ({:?})", info.name, info.backend);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("GPU device ({name}): {e}"))?;
        let mut exec = Executor::new(device, queue).map_err(|e| format!("{e} ({name})"))?;
        if std::env::var("SIMCORE_GPU_GLASS").is_ok_and(|v| v == "cpu") {
            exec.disable_gpu_glass("disabled by SIMCORE_GPU_GLASS=cpu");
        }
        Ok(Self { exec, builder: Builder::new(), readback: Readback::default(), adapter: name, target: None, frames: 0, last: FrameTimings::default(), build_time: Duration::ZERO, vello_time: Duration::ZERO })
    }

    /// Route every backdrop op through the CPU path (read back, `apply_backdrop_op`, upload). Also selectable with
    /// `SIMCORE_GPU_GLASS=cpu` at creation.
    pub fn disable_gpu_glass(&mut self, reason: &str) {
        self.exec.disable_gpu_glass(reason);
    }

    /// Backdrop ops (glass, shadow, scroll edge) composited so far on the GPU and on the CPU fallback path.
    pub fn backdrop_counts(&self) -> (u64, u64) {
        self.exec.backdrop_counts()
    }

    /// Cached sprites and bitmaps currently held (bounded by [`onesim_render::CACHE_MAX_ENTRIES`]).
    pub fn cache_len(&self) -> usize {
        self.exec.resource_count()
    }

    /// Display-list build time, Vello submission time, and sprite cache hits/misses for the last capture.
    pub fn encode_profile(&self) -> (Duration, Duration, (usize, usize)) {
        (self.build_time, self.vello_time, self.builder.sprite_cache_counts())
    }

    fn ensure_target(&mut self, w: u32, h: u32) -> &wgpu::Texture {
        if self.target.as_ref().is_none_or(|t| (t.w, t.h) != (w, h)) {
            let tex = self.exec.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("onesim-target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: target_usage(),
                view_formats: &[],
            });
            self.target = Some(Target { w, h, tex });
        }
        &self.target.as_ref().expect("target just created").tex
    }

    fn frame(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64, pw: u32, ph: u32, timings: &mut FrameTimings) -> Result<Frame, RenderError> {
        let start = Instant::now();
        let dl = self.builder.build(scene, fonts, scale);
        self.build_time = start.elapsed();
        let target = self.ensure_target(pw, ph).clone();
        let stats = self.exec.execute_owned(dl, &target)?;
        self.vello_time = stats.vello;
        timings.gpu += stats.fallback.gpu;
        timings.readback += stats.fallback.readback;
        let data = self
            .readback
            .read(self.exec.device(), self.exec.queue(), &target, (pw, ph), true, timings)
            .map_err(|m| self.exec.map_error(m))?;
        self.exec.check_errors()?;
        Ok(Frame { width: pw, height: ph, rgba: data })
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
        self.exec.check_errors()?;
        let pw = (scene.size.0 * scale).round().max(1.0) as u32;
        let ph = (scene.size.1 * scale).round().max(1.0) as u32;
        let max = self.exec.device().limits().max_texture_dimension_2d;
        if pw > max || ph > max {
            return Err(RenderError::Unsupported(format!("frame {pw}x{ph} exceeds the GPU texture limit {max}")));
        }
        self.frames += 1;
        let mut timings = FrameTimings::default();
        let t_all = Instant::now();
        let out = self.frame(scene, fonts, scale, pw, ph, &mut timings);
        if out.is_err() {
            // The executor may not have applied this frame's resources: start both tables over.
            self.builder.reset();
            self.exec.clear_resources();
        }
        let frame = out?;
        // Everything but the final wait and readback (display list, Vello and glass submission) counts as encode.
        let wall = t_all.elapsed();
        timings.encode = wall.saturating_sub(timings.gpu + timings.readback).max(Duration::ZERO);
        self.last = timings;
        Ok(frame)
    }

    fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            backend: BackendKind::Gpu.to_string(),
            adapter: self.adapter.clone(),
            limitations: vec![
                if self.exec.glass_on_gpu() {
                    "liquid glass and scroll-edge effects: GPU compute passes on an accumulation texture (single readback)".to_string()
                } else {
                    format!("liquid glass and scroll-edge effects: CPU composite via readback ({})", self.exec.glass_note())
                },
                "text, icon glyphs, SF Symbols, underline/strike: drawn by the CPU rasterizer into cached sprites (display list resources)".into(),
                "vector anti-aliasing, shadow blur and gradients differ from the CPU reference by small per-channel amounts (not byte-identical)".into(),
            ],
            fallback_reason: None,
            last_frame: self.last,
            frames: self.frames,
        }
    }
}
