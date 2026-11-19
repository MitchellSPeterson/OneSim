//! Presenting display lists to a window surface: [`WindowRenderer`].
//!
//! The executor draws into an offscreen `Rgba8Unorm` storage texture (surface formats have no `STORAGE`
//! usage); a fullscreen-triangle pass then copies it to the swap-chain texture. The 8-bit values are already
//! sRGB encoded (the capture convention), so the surface is configured with a non-sRGB format, or an sRGB one
//! viewed through its non-sRGB twin: no second gamma, output matches the CPU rasterizer.

use std::time::{Duration, Instant};

use onesim_render::{Builder, Fonts, RenderError, Scene};
use vello::wgpu;

use crate::executor::{target_usage, Executor};

const BLIT: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}
@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let d = vec2<i32>(textureDimensions(src));
    let c = clamp(vec2<i32>(pos.xy), vec2<i32>(0, 0), d - vec2<i32>(1, 1));
    return vec4<f32>(textureLoad(src, c, 0).rgb, 1.0);
}
"#;

/// What one [`WindowRenderer::render`] did.
#[derive(Clone, Copy, Debug, Default)]
pub struct Presented {
    /// A frame reached the swap chain (`false`: skipped because the window is occluded or acquiring timed out).
    pub shown: bool,
    /// With `shown == false`: the OS reports the window as not visible (minimized, covered, screen locked), as
    /// opposed to a transient acquire timeout. Retry when it becomes visible again, not every frame.
    pub occluded: bool,
    /// Scene to display list.
    pub build: Duration,
    /// Display list to submitted GPU work (Vello, glass passes, blit encode).
    pub execute: Duration,
    /// Acquiring the swap-chain texture (blocks on vsync under `Fifo`) and presenting.
    pub present: Duration,
}

struct Offscreen {
    w: u32,
    h: u32,
    tex: wgpu::Texture,
    bind: wgpu::BindGroup,
}

/// GPU renderer that presents to a window surface (vsync, bounded frame latency).
pub struct WindowRenderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
    exec: Executor,
    builder: Builder,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    target: Option<Offscreen>,
    adapter: String,
    frames: u64,
}

impl WindowRenderer {
    /// Create the surface for `window` (any `Arc<impl HasWindowHandle + HasDisplayHandle>`), pick a compatible
    /// adapter, and build an [`Executor`] on its device. `size` is the window's physical size in pixels.
    pub fn new(window: impl Into<wgpu::SurfaceTarget<'static>>, size: (u32, u32)) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window).map_err(|e| format!("create surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter for the window: {e}"))?;
        let info = adapter.get_info();
        let name = format!("{} ({:?})", info.name, info.backend);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("GPU device ({name}): {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| format!("surface has no formats ({name})"))?;
        let view_format = format.remove_srgb_suffix();
        let mut config = surface.get_default_config(&adapter, size.0.max(1), size.1.max(1)).ok_or_else(|| format!("surface unsupported by {name}"))?;
        config.format = format;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        config.view_formats = if view_format == format { vec![] } else { vec![view_format] };
        if let Some(a) = caps.alpha_modes.iter().find(|m| **m == wgpu::CompositeAlphaMode::Opaque) {
            config.alpha_mode = *a;
        }
        surface.configure(&device, &config);
        let mut exec = Executor::new(device, queue).map_err(|e| format!("{e} ({name})"))?;
        if std::env::var("SIMCORE_GPU_GLASS").is_ok_and(|v| v == "cpu") {
            exec.disable_gpu_glass("disabled by SIMCORE_GPU_GLASS=cpu");
        }
        let (pipeline, layout) = blit_pipeline(exec.device(), view_format);
        Ok(Self { surface, config, view_format, exec, builder: Builder::new(), pipeline, layout, target: None, adapter: name, frames: 0 })
    }

    /// Adapter name, e.g. `Apple M1 Pro (Metal)`.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// Swap-chain format and present mode, for diagnostics.
    pub fn surface_info(&self) -> String {
        format!("{:?} as {:?}, {:?}, latency {}", self.config.format, self.view_format, self.config.present_mode, self.config.desired_maximum_frame_latency)
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Resize the swap chain to the window's physical size.
    pub fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if (self.config.width, self.config.height) != (w, h) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(self.exec.device(), &self.config);
        }
    }

    fn reconfigure(&self) {
        self.surface.configure(self.exec.device(), &self.config);
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if self.target.as_ref().is_some_and(|t| (t.w, t.h) == (w, h)) {
            return;
        }
        let tex = self.exec.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("onesim-window-target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: target_usage() | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.exec.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onesim-window-blit"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        self.target = Some(Offscreen { w, h, tex, bind });
    }

    /// Acquire the next swap-chain texture, repairing an outdated or lost swap chain once.
    /// `Ok(None)`: nothing to draw into (see [`Presented::occluded`] for why).
    fn acquire(&mut self) -> Result<Result<wgpu::SurfaceTexture, bool>, RenderError> {
        for attempt in 0..2 {
            match self.surface.get_current_texture() {
                // Suboptimal is usable now; the swap chain is fixed on the next resize or reconfigure.
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => return Ok(Ok(f)),
                wgpu::CurrentSurfaceTexture::Timeout => return Ok(Err(false)),
                wgpu::CurrentSurfaceTexture::Occluded => return Ok(Err(true)),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.reconfigure();
                    if attempt == 1 {
                        return Err(RenderError::SurfaceLost("swap chain outdated or lost after reconfigure".into()));
                    }
                }
                wgpu::CurrentSurfaceTexture::Validation => return Err(self.exec.check_errors().err().unwrap_or_else(|| RenderError::Other("surface validation error".into()))),
            }
        }
        Err(RenderError::SurfaceLost("no swap-chain texture".into()))
    }

    fn blit(&self, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, (w, h): (u32, u32)) {
        let Some(off) = self.target.as_ref() else { return };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("onesim-window-blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, off.w.min(w) as f32, off.h.min(h) as f32, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &off.bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Scene to display list to the offscreen target. Returns the two stage times.
    fn draw_offscreen(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<(Duration, Duration), RenderError> {
        let t = Instant::now();
        let dl = self.builder.build(scene, fonts, scale);
        let build = t.elapsed();
        let t = Instant::now();
        self.ensure_target(dl.width, dl.height);
        let Some(off) = self.target.as_ref() else { return Err(RenderError::Other("no target".into())) };
        if let Err(e) = self.exec.execute(&dl, &off.tex) {
            self.builder.reset();
            self.exec.clear_resources();
            return Err(e);
        }
        Ok((build, t.elapsed()))
    }

    /// Render `scene` at `scale` device pixels per point and present it. The frame is drawn at the top-left of the
    /// swap chain (it should be the window's size). Surface loss or an outdated swap chain is repaired once and
    /// the frame retried; a second failure is [`RenderError::SurfaceLost`]. An occluded window or an acquire
    /// timeout skips the frame before any drawing work (`shown == false`).
    pub fn render(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Presented, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::Other(format!("bad scale {scale}")));
        }
        self.exec.check_errors()?;
        let mut out = Presented::default();
        // Acquire first: a hidden window costs nothing, and the vsync wait is not part of building the frame.
        let t = Instant::now();
        let frame = match self.acquire()? {
            Ok(f) => f,
            Err(occluded) => {
                out.occluded = occluded;
                out.present = t.elapsed();
                return Ok(out);
            }
        };
        out.present = t.elapsed();

        (out.build, out.execute) = self.draw_offscreen(scene, fonts, scale)?;

        let t = Instant::now();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.view_format), ..Default::default() });
        let mut enc = self.exec.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("onesim-window-blit") });
        self.blit(&mut enc, &view, (self.config.width, self.config.height));
        self.exec.queue().submit([enc.finish()]);
        self.exec.queue().present(frame);
        out.present += t.elapsed();
        out.shown = true;
        self.frames += 1;
        self.exec.check_errors()?;
        Ok(out)
    }

    /// `scene` as the swap chain would receive it, without presenting: the same display list, executor, blit pipeline
    /// and surface format, read back as straight RGBA. For tests and screenshots of a window the OS will not capture
    /// (covered, or the screen is locked).
    pub fn snapshot(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<onesim_render::Frame, String> {
        self.draw_offscreen(scene, fonts, scale).map_err(|e| e.to_string())?;
        let (w, h) = self.target.as_ref().map(|t| (t.w, t.h)).ok_or("nothing rendered yet")?;
        let tex = self.exec.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("onesim-window-snapshot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.view_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.exec.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("onesim-window-snapshot") });
        self.blit(&mut enc, &view, (w, h));
        self.exec.queue().submit([enc.finish()]);
        let mut rgba = crate::read_texture(self.exec.device(), self.exec.queue(), &tex)?;
        if matches!(self.view_format, wgpu::TextureFormat::Bgra8Unorm) {
            rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p.swap(0, 2));
        }
        Ok(onesim_render::Frame { width: w, height: h, rgba })
    }
}

fn blit_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("onesim-window-blit"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("onesim-window-blit"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("onesim-window-blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("onesim-window-blit"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}
