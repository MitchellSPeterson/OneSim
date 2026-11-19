//! GPU backend (wgpu + Vello) behind [`simcore_render::Renderer`].

use simcore_render::{BackendKind, Diagnostics, Fonts, Frame, RenderError, Renderer, Scene};

/// Headless (render-to-texture + readback) GPU renderer.
pub struct GpuRenderer {
    _private: (),
}

impl GpuRenderer {
    /// Acquire an adapter and device. `Err` carries a human-readable reason (no adapter, init failure).
    pub fn new_headless() -> Result<Self, String> {
        Err("GPU backend not implemented yet".into())
    }
}

impl Renderer for GpuRenderer {
    fn kind(&self) -> BackendKind {
        BackendKind::Gpu
    }
    fn capture(&mut self, _scene: &Scene, _fonts: &mut Fonts, _scale: f64) -> Result<Frame, RenderError> {
        Err(RenderError::Unsupported("not implemented".into()))
    }
    fn diagnostics(&self) -> Diagnostics {
        Diagnostics { backend: "gpu".into(), ..Diagnostics::default() }
    }
}
