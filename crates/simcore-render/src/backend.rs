//! Renderer backends: the seam between a laid-out [`Scene`] and pixels.
//!
//! The CPU rasterizer ([`CpuRenderer`], tiny-skia + cosmic-text) is the deterministic reference and is
//! always available. GPU backends (wgpu + Vello, crate `simcore-render-gpu`) implement the same
//! [`Renderer`] trait; layout, hit testing and input never depend on which one is active.
//!
//! **Capture convention** (every backend): [`Frame::rgba`] is 8-bit sRGB RGBA, *premultiplied* alpha,
//! rows top to bottom with no padding, `width = round(scene.w * scale)`, `height = round(scene.h * scale)`
//! (each at least 1). Full-screen scenes are opaque, so premultiplied equals straight there.

use std::fmt;
use std::time::Duration;

use crate::{Fonts, Scene};

/// Which implementation paints a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// tiny-skia + cosmic-text (reference).
    Cpu,
    /// wgpu + Vello.
    Gpu,
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BackendKind::Cpu => "cpu",
            BackendKind::Gpu => "gpu",
        })
    }
}

/// Requested backend (`--renderer`, `SIMCORE_RENDERER`). Apps never choose this.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackendPref {
    /// GPU when available, CPU otherwise. Headless capture resolves this to CPU (see [`BackendPref::resolve_headless`]).
    #[default]
    Auto,
    /// GPU or a clear error.
    Gpu,
    /// Always the reference rasterizer.
    Cpu,
}

impl std::str::FromStr for BackendPref {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Self::Auto),
            "gpu" => Ok(Self::Gpu),
            "cpu" => Ok(Self::Cpu),
            other => Err(format!("unknown renderer {other:?} (expected auto, gpu or cpu)")),
        }
    }
}

impl BackendPref {
    /// `SIMCORE_RENDERER`, or `Auto` when unset. An invalid value is an error, not a silent default.
    pub fn from_env() -> Result<Self, String> {
        std::env::var("SIMCORE_RENDERER").map_or(Ok(Self::Auto), |v| v.parse())
    }

    /// Headless screenshots must stay deterministic without a GPU: `Auto` means CPU there; only an
    /// explicit `Gpu` opts in.
    pub fn resolve_headless(self) -> Self {
        match self {
            Self::Auto => Self::Cpu,
            p => p,
        }
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

/// One captured frame (see the module docs for the pixel convention).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// Encode as PNG (demultiplies alpha, like the CPU path always has).
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let size = tiny_skia::IntSize::from_wh(self.width, self.height).ok_or("empty frame")?;
        let pm = tiny_skia::Pixmap::from_vec(self.rgba.clone(), size).ok_or("frame size does not match its pixels")?;
        pm.encode_png().map_err(|e| e.to_string())
    }
}

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

/// What a backend reports about itself, for logs, `--renderer` diagnostics and the RPC.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diagnostics {
    /// `"cpu"` or `"gpu"`.
    pub backend: String,
    /// Adapter / implementation name, e.g. `"tiny-skia"` or `"Apple M3 (Metal)"`.
    pub adapter: String,
    /// Features this backend draws differently from the CPU reference, or not at all.
    pub limitations: Vec<String>,
    /// Set when the active backend is not the one requested, with the reason.
    pub fallback_reason: Option<String>,
    /// Timings of the last frame.
    pub last_frame: FrameTimings,
    /// Frames rendered since creation.
    pub frames: u64,
}

/// A backend that paints scenes. Implementations own their caches; `fonts` is shared because
/// layout and painting must agree on the same font database.
pub trait Renderer {
    fn kind(&self) -> BackendKind;
    /// Render `scene` at `scale` device pixels per point and read it back (see the capture convention).
    fn capture(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Frame, RenderError>;
    fn diagnostics(&self) -> Diagnostics;
}

/// The reference rasterizer behind the [`Renderer`] seam. Output is byte-identical to [`crate::render`].
#[derive(Debug, Default)]
pub struct CpuRenderer {
    frames: u64,
    last: FrameTimings,
    fallback_reason: Option<String>,
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// A CPU renderer standing in for a GPU one that could not be used.
    pub fn fallback(reason: impl Into<String>) -> Self {
        Self { fallback_reason: Some(reason.into()), ..Self::default() }
    }
}

impl Renderer for CpuRenderer {
    fn kind(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn capture(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Frame, RenderError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::Other(format!("bad scale {scale}")));
        }
        let t = std::time::Instant::now();
        let pm = crate::render(scene, fonts, scale);
        self.last = FrameTimings { encode: t.elapsed(), ..FrameTimings::default() };
        self.frames += 1;
        let (width, height) = (pm.width(), pm.height());
        Ok(Frame { width, height, rgba: pm.take() })
    }

    fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            backend: BackendKind::Cpu.to_string(),
            adapter: "tiny-skia".into(),
            limitations: vec![],
            fallback_reason: self.fallback_reason.clone(),
            last_frame: self.last,
            frames: self.frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Node, Rect};
    use simcore_protocol::Appearance;

    fn scene() -> Scene {
        let mut n = Node::new("card", "rect");
        n.bounds = Rect { x: 10.0, y: 10.0, w: 50.0, h: 30.0 };
        n.fill = Some(Color::rgba(0.2, 0.4, 0.9, 1.0));
        n.radius = 8.0;
        Scene { size: (80.0, 60.0), appearance: Appearance::Light, ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] }
    }

    #[test]
    fn pref_parses_and_rejects() {
        assert_eq!("GPU".parse::<BackendPref>(), Ok(BackendPref::Gpu));
        assert_eq!("".parse::<BackendPref>(), Ok(BackendPref::Auto));
        assert!("metal".parse::<BackendPref>().is_err());
        assert_eq!(BackendPref::Auto.resolve_headless(), BackendPref::Cpu);
        assert_eq!(BackendPref::Gpu.resolve_headless(), BackendPref::Gpu);
    }

    #[test]
    fn cpu_renderer_matches_reference_raster() {
        let mut fonts = Fonts::new();
        let want = crate::render(&scene(), &mut fonts, 3.0);
        let mut r = CpuRenderer::new();
        let f = r.capture(&scene(), &mut fonts, 3.0).unwrap();
        assert_eq!((f.width, f.height), (240, 180));
        assert_eq!(f.rgba, want.data());
        assert_eq!(f.encode_png().unwrap(), want.encode_png().unwrap());
        let d = r.diagnostics();
        assert_eq!((d.backend.as_str(), d.frames, d.fallback_reason), ("cpu", 1, None));
    }

    #[test]
    fn cpu_renderer_rejects_bad_scale() {
        assert!(CpuRenderer::new().capture(&scene(), &mut Fonts::new(), 0.0).is_err());
    }
}
