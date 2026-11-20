//! WebGPU presenter for OneSim's browser preview (docs/web.md).
//!
//! The runtime module (`app.wasm`, wasm32-wasip1) lowers each frame to an encoded
//! [`onesim_render::display::DisplayList`]; this module (wasm32-unknown-unknown) decodes it, runs
//! [`onesim_render_gpu::Executor`] into an offscreen `Rgba8Unorm` storage texture and blits that to the canvas
//! with a fullscreen triangle (canvas textures cannot be storage textures). Pixel values are already
//! sRGB-encoded 8-bit, so the blit is a plain copy and colors match the CPU rasterizer.
//!
//! Only compiled for wasm32; on native targets the crate is empty so the workspace builds unchanged.
#![cfg(target_arch = "wasm32")]

use std::fmt::Display;

use onesim_render::display::DisplayList;
use onesim_render::RenderError;
use onesim_render_gpu::{target_usage, wgpu, Executor};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

const BLIT: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(src, vec2<i32>(pos.xy), 0).rgb, 1.0);
}
";

fn err(e: impl Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

struct Target {
    w: u32,
    h: u32,
    tex: wgpu::Texture,
    bind: wgpu::BindGroup,
}

/// A GPU-backed presenter bound to one canvas. Create with [`init`].
#[wasm_bindgen]
pub struct Gpu {
    exec: Executor,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    target: Option<Target>,
    info: String,
    lost: Option<String>,
    last_error: Option<String>,
    frames: u64,
}

/// Acquire a WebGPU adapter and device, configure `canvas` and compile the pipelines.
/// Rejects with a message when WebGPU is missing, no adapter is offered or setup fails; the canvas then
/// holds a `webgpu` context and cannot give out `2d` (the caller must replace it before falling back).
#[wasm_bindgen]
pub async fn init(canvas: HtmlCanvasElement) -> Result<Gpu, JsValue> {
    console_error_panic_hook::set_once();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::BROWSER_WEBGPU, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let (w, h) = (canvas.width().max(1), canvas.height().max(1));
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas)).map_err(|e| err(format!("canvas WebGPU context: {e}")))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false, ..Default::default() })
        .await
        .map_err(|e| err(format!("no WebGPU adapter: {e}")))?;
    let ai = adapter.get_info();
    let limits = adapter.limits();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits { max_texture_dimension_2d: limits.max_texture_dimension_2d, ..wgpu::Limits::default() },
            ..Default::default()
        })
        .await
        .map_err(|e| err(format!("WebGPU device: {e}")))?;
    let caps = surface.get_capabilities(&adapter);
    let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
        .into_iter()
        .find(|f| caps.formats.contains(f))
        .ok_or_else(|| err(format!("canvas offers no bgra8unorm/rgba8unorm format ({:?})", caps.formats)))?;
    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) { wgpu::CompositeAlphaMode::Opaque } else { caps.alpha_modes.first().copied().unwrap_or_default() };
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: w,
        height: h,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
        color_space: wgpu::SurfaceColorSpace::Auto,
    };
    surface.configure(&device, &config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("onesim-blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("onesim-blit"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("onesim-blit"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("onesim-blit"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let exec = Executor::new_async(device, queue).await.map_err(|e| err(format!("GPU renderer init: {e}")))?;
    let info = serde_json::json!({
        "adapter": ai.name, "backend": format!("{:?}", ai.backend), "vendor": ai.vendor, "deviceType": format!("{:?}", ai.device_type),
        "driver": ai.driver, "format": format!("{format:?}"), "maxTextureDimension2d": limits.max_texture_dimension_2d,
        "glassOnGpu": exec.glass_on_gpu(), "limitations": exec.glass_note(),
    })
    .to_string();
    Ok(Gpu { exec, surface, config, pipeline, layout, target: None, info, lost: None, last_error: None, frames: 0 })
}

#[wasm_bindgen]
impl Gpu {
    /// Decode `bytes` (a `DisplayList::encode` frame) and present it. Throws on decode failure, device loss or
    /// any GPU error; after a throw the caller should treat this presenter as unusable (see `lost`).
    pub fn render(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        if let Some(m) = &self.lost {
            return Err(err(format!("device lost: {m}")));
        }
        let dl = DisplayList::decode(bytes).map_err(|e| err(format!("display list: {e}")))?;
        let r = self.present(dl);
        match r {
            Ok(()) => {
                self.frames += 1;
                Ok(())
            }
            Err(e) => {
                if let RenderError::DeviceLost(m) = &e {
                    self.lost = Some(m.clone());
                }
                // A failed frame may have applied only part of its resource changes: restart the table.
                self.exec.clear_resources();
                self.last_error = Some(e.to_string());
                Err(err(e))
            }
        }
    }

    /// Match the canvas backing store to `w` x `h` device pixels (frames of another size also do this on their own).
    pub fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if (self.config.width, self.config.height) != (w, h) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(self.exec.device(), &self.config);
        }
    }

    /// JSON: adapter, backend, vendor, deviceType, driver, canvas format, texture limit, `glassOnGpu` and `limitations`.
    pub fn info(&self) -> String {
        self.info.clone()
    }

    /// True once the device was lost (or ran out of memory); the presenter must be replaced or abandoned.
    pub fn lost(&mut self) -> bool {
        if self.lost.is_none() {
            match self.exec.check_errors() {
                Err(RenderError::DeviceLost(m)) => self.lost = Some(m),
                Err(e) => self.last_error = Some(e.to_string()),
                Ok(()) => {}
            }
        }
        self.lost.is_some()
    }

    /// The loss reason or the latest GPU error text, if any.
    pub fn error(&self) -> Option<String> {
        self.lost.clone().or_else(|| self.last_error.clone())
    }

    /// Frames presented so far.
    pub fn frames(&self) -> f64 {
        self.frames as f64
    }
}

impl Gpu {
    fn present(&mut self, dl: DisplayList) -> Result<(), RenderError> {
        let (w, h) = (dl.width, dl.height);
        let max = self.exec.device().limits().max_texture_dimension_2d;
        if w == 0 || h == 0 || w > max || h > max {
            return Err(RenderError::Unsupported(format!("frame {w}x{h} is outside the WebGPU texture range 1..={max}")));
        }
        if (self.config.width, self.config.height) != (w, h) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(self.exec.device(), &self.config);
        }
        if self.target.as_ref().is_none_or(|t| (t.w, t.h) != (w, h)) {
            let tex = self.exec.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("onesim-offscreen"),
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
                label: Some("onesim-blit"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
            });
            self.target = Some(Target { w, h, tex, bind });
        }
        let t = self.target.as_ref().ok_or_else(|| RenderError::Other("no target".into()))?;
        self.exec.execute_owned(dl, &t.tex)?;
        let frame = self.acquire()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.exec.device().create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("onesim-blit") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("onesim-blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &t.bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.exec.queue().submit([enc.finish()]);
        self.exec.queue().present(frame);
        self.exec.check_errors()
    }

    /// The canvas texture, reconfiguring once if the surface went stale.
    fn acquire(&self) -> Result<wgpu::SurfaceTexture, RenderError> {
        use wgpu::CurrentSurfaceTexture as C;
        for attempt in 0..2 {
            match self.surface.get_current_texture() {
                C::Success(t) | C::Suboptimal(t) => return Ok(t),
                C::Outdated | C::Lost | C::Timeout | C::Occluded if attempt == 0 => self.surface.configure(self.exec.device(), &self.config),
                other => return Err(RenderError::SurfaceLost(format!("{other:?}"))),
            }
        }
        Err(RenderError::SurfaceLost("surface unavailable".into()))
    }
}
