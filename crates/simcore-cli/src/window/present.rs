//! Presenters: where a laid-out scene becomes pixels in the window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use simcore_render::{CpuRenderer, Fonts, RenderError, Renderer, Scene};
use simcore_render_gpu::WindowRenderer;
use winit::window::Window;

/// Stage timings and outcome of one present.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drawn {
    /// Reached the screen (false: occluded or the swap chain timed out).
    pub shown: bool,
    /// Not shown because the window is hidden (not a transient timeout).
    pub occluded: bool,
    pub list: Duration,
    pub exec: Duration,
    pub present: Duration,
}

pub enum Presenter {
    Gpu(Box<WindowRenderer>),
    Cpu(Cpu),
}

pub struct Cpu {
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    renderer: CpuRenderer,
}

impl Presenter {
    /// GPU presenter on `window`, or why not.
    pub fn gpu(window: &Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        WindowRenderer::new(window.clone(), (size.width, size.height)).map(|g| Presenter::Gpu(Box::new(g)))
    }

    /// CPU (tiny-skia + softbuffer) presenter on `window`. Any GPU presenter on it must be dropped first.
    pub fn cpu(window: &Arc<Window>) -> Result<Self, String> {
        let ctx = softbuffer::Context::new(window.clone()).map_err(|e| format!("softbuffer context: {e}"))?;
        let surface = softbuffer::Surface::new(&ctx, window.clone()).map_err(|e| format!("softbuffer surface: {e}"))?;
        Ok(Presenter::Cpu(Cpu { surface, renderer: CpuRenderer::new() }))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Presenter::Gpu(_) => "gpu",
            Presenter::Cpu(_) => "cpu",
        }
    }

    /// Adapter and surface description for the diagnostics line.
    pub fn describe(&self) -> String {
        match self {
            Presenter::Gpu(g) => format!("{}; {}", g.adapter(), g.surface_info()),
            Presenter::Cpu(_) => "tiny-skia + softbuffer".into(),
        }
    }

    pub fn adapter(&self) -> String {
        match self {
            Presenter::Gpu(g) => g.adapter().to_string(),
            Presenter::Cpu(_) => "tiny-skia".into(),
        }
    }

    /// `scene` as this presenter would show it, read back (GPU: through the blit pipeline and surface format).
    pub fn snapshot(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<simcore_render::Frame, String> {
        match self {
            Presenter::Gpu(g) => g.snapshot(scene, fonts, scale),
            Presenter::Cpu(c) => c.renderer.capture(scene, fonts, scale).map_err(|e| e.to_string()),
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if let Presenter::Gpu(g) = self {
            g.resize(w, h);
        }
    }

    pub fn draw(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64, (w, h): (u32, u32)) -> Result<Drawn, RenderError> {
        match self {
            Presenter::Gpu(g) => {
                g.resize(w, h);
                let p = g.render(scene, fonts, scale)?;
                Ok(Drawn { shown: p.shown, occluded: p.occluded, list: p.build, exec: p.execute, present: p.present })
            }
            Presenter::Cpu(c) => c.draw(scene, fonts, scale, (w, h)),
        }
    }
}

impl Cpu {
    fn draw(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64, (w, h): (u32, u32)) -> Result<Drawn, RenderError> {
        let t = Instant::now();
        let frame = self.renderer.capture(scene, fonts, scale)?;
        let exec = t.elapsed();
        let t = Instant::now();
        let (Some(nw), Some(nh)) = (std::num::NonZeroU32::new(w), std::num::NonZeroU32::new(h)) else {
            return Ok(Drawn::default());
        };
        self.surface.resize(nw, nh).map_err(|e| RenderError::SurfaceLost(e.to_string()))?;
        let mut buf = self.surface.buffer_mut().map_err(|e| RenderError::SurfaceLost(e.to_string()))?;
        buf.fill(0);
        let (fw, fh) = (frame.width.min(w) as usize, frame.height.min(h) as usize);
        for y in 0..fh {
            let src = &frame.rgba[y * frame.width as usize * 4..][..fw * 4];
            let dst = &mut buf[y * w as usize..][..fw];
            for (d, px) in dst.iter_mut().zip(src.as_chunks::<4>().0) {
                *d = u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
            }
        }
        buf.present().map_err(|e| RenderError::SurfaceLost(e.to_string()))?;
        Ok(Drawn { shown: true, occluded: false, list: Duration::ZERO, exec, present: t.elapsed() })
    }
}
