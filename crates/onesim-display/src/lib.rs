//! Framework-neutral display list, glass parameters, and renderer error/timing types.
//!
//! Kept free of rasterizer, font, layout and serde dependencies so GPU and WebGPU executors can
//! depend on it without pulling in `onesim-render`.

use std::fmt;
use std::time::Duration;

pub mod display;
pub mod glass;

pub use display::{DisplayList, Op, RRect, Resource, Sampling, VERSION};
pub use glass::{GlassSpec, GlassVariant, ScrollEdge};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// Why a frame failed. Callers recover differently per variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// Surface lost/outdated/resized underneath us: reconfigure and retry the same scene.
    SurfaceLost(String),
    /// Device lost or out of memory: drop GPU resources; reinitialize or fall back to CPU.
    DeviceLost(String),
    /// The scene needs something this backend cannot draw.
    Unsupported(String),
    /// Anything else (bad size, encoder failure, ...).
    Other(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::SurfaceLost(m) => write!(f, "surface lost: {m}"),
            RenderError::DeviceLost(m) => write!(f, "device lost: {m}"),
            RenderError::Unsupported(m) => write!(f, "unsupported: {m}"),
            RenderError::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for RenderError {}

/// Per-frame stage timings. A stage a backend does not have stays zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTimings {
    /// Scene to backend commands (CPU: the whole raster).
    pub encode: Duration,
    /// GPU submission until work is done (GPU only).
    pub gpu: Duration,
    /// Texture to CPU copy (GPU only).
    pub readback: Duration,
}
