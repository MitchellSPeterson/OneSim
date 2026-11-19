//! Runtime renderer selection and recovery for screenshots (docs/rendering.md).

use simcore_render::{BackendKind, BackendPref, CpuRenderer, Diagnostics, Fonts, Frame, RenderError, Renderer, Scene};

type Factory = fn() -> Result<Box<dyn Renderer + Send>, String>;

#[cfg(feature = "gpu")]
fn gpu_factory() -> Result<Box<dyn Renderer + Send>, String> {
    simcore_render_gpu::GpuRenderer::new_headless().map(|r| Box::new(r) as Box<dyn Renderer + Send>)
}

#[cfg(not(feature = "gpu"))]
fn gpu_factory() -> Result<Box<dyn Renderer + Send>, String> {
    Err("built without the `gpu` feature".into())
}

/// The host's lazily created renderer plus the policy that picks and repairs it.
pub(crate) struct RendererSlot {
    /// `None`: read `SIMCORE_RENDERER` at first use (unset means CPU). `Some(Err)`: invalid value.
    pref: Option<Result<BackendPref, String>>,
    current: Option<Box<dyn Renderer + Send>>,
    gpu: Factory,
}

impl RendererSlot {
    pub(crate) fn new() -> Self {
        Self { pref: None, current: None, gpu: gpu_factory }
    }

    /// Explicit choice; drops the current renderer. Here `Auto` means "try GPU, fall back to CPU".
    pub(crate) fn set(&mut self, pref: BackendPref) {
        self.pref = Some(Ok(pref));
        self.current = None;
    }

    #[cfg(test)]
    pub(crate) fn with(pref: Result<BackendPref, String>, gpu: Factory) -> Self {
        Self { pref: Some(pref), current: None, gpu }
    }

    fn pref(&mut self) -> Result<BackendPref, String> {
        // Unset: CPU (headless captures stay deterministic). An explicit `auto` tries the GPU first.
        self.pref
            .get_or_insert_with(|| match std::env::var_os("SIMCORE_RENDERER") {
                None => Ok(BackendPref::Auto.resolve_headless()),
                Some(_) => BackendPref::from_env(),
            })
            .clone()
    }

    fn make_gpu(&self) -> Result<Box<dyn Renderer + Send>, String> {
        (self.gpu)().map_err(|e| format!("GPU renderer unavailable: {e}"))
    }

    fn ensure(&mut self) -> Result<&mut Box<dyn Renderer + Send>, String> {
        if self.current.is_none() {
            let r: Box<dyn Renderer + Send> = match self.pref()? {
                BackendPref::Cpu => Box::new(CpuRenderer::new()),
                BackendPref::Gpu => self.make_gpu()?,
                BackendPref::Auto => match self.make_gpu() {
                    Ok(r) => r,
                    Err(reason) => {
                        log::warn!("{reason}; using the CPU renderer");
                        Box::new(CpuRenderer::fallback(reason))
                    }
                },
            };
            self.current = Some(r);
        }
        Ok(self.current.as_mut().expect("just set"))
    }

    fn fall_back(&mut self, reason: String) {
        log::warn!("{reason}; using the CPU renderer");
        self.current = Some(Box::new(CpuRenderer::fallback(reason)));
    }

    /// Render and read back one frame, recovering per [`RenderError`] variant without touching the scene.
    pub(crate) fn capture(&mut self, scene: &Scene, fonts: &mut Fonts, scale: f64) -> Result<Frame, String> {
        let auto = matches!(self.pref(), Ok(BackendPref::Auto));
        let r = self.ensure()?;
        let mut err = match r.capture(scene, fonts, scale) {
            Ok(f) => return Ok(f),
            Err(e) => e,
        };
        if matches!(err, RenderError::SurfaceLost(_)) {
            err = match self.ensure()?.capture(scene, fonts, scale) {
                Ok(f) => return Ok(f),
                Err(e) => e,
            };
        }
        match err {
            RenderError::DeviceLost(m) if self.ensure()?.kind() == BackendKind::Gpu => {
                match self.make_gpu() {
                    Ok(fresh) => {
                        self.current = Some(fresh);
                        match self.ensure()?.capture(scene, fonts, scale) {
                            Ok(f) => return Ok(f),
                            Err(e) => self.fall_back(format!("GPU device lost ({m}); re-init failed: {e}")),
                        }
                    }
                    Err(e) => self.fall_back(format!("GPU device lost ({m}); {e}")),
                }
                self.ensure()?.capture(scene, fonts, scale).map_err(|e| e.to_string())
            }
            RenderError::Unsupported(m) if auto && self.ensure()?.kind() == BackendKind::Gpu => {
                log::warn!("GPU renderer cannot draw this frame ({m}); using the CPU renderer for it");
                CpuRenderer::fallback(m).capture(scene, fonts, scale).map_err(|e| e.to_string())
            }
            e => Err(e.to_string()),
        }
    }

    /// What is (or would be) in use. Does not create a GPU device just to answer.
    pub(crate) fn diagnostics(&mut self) -> Result<Diagnostics, String> {
        self.ensure().map(|r| r.diagnostics())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simcore_protocol::Appearance;
    use simcore_render::{Color, Node, Rect};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scene() -> Scene {
        let mut n = Node::new("card", "rect");
        n.bounds = Rect { x: 4.0, y: 4.0, w: 30.0, h: 20.0 };
        n.fill = Some(Color::rgba(0.9, 0.2, 0.3, 1.0));
        n.radius = 5.0;
        Scene { size: (50.0, 40.0), appearance: Appearance::Light, nodes: vec![n], unsupported_symbols: vec![], notes: vec![] }
    }

    fn fail() -> Result<Box<dyn Renderer + Send>, String> {
        Err("no adapter".into())
    }

    #[test]
    fn cpu_pref_is_byte_identical_to_render_png() {
        let mut fonts = Fonts::new();
        let want = simcore_render::render_png(&scene(), &mut fonts, 2.0).unwrap();
        let mut slot = RendererSlot::with(Ok(BackendPref::Cpu), fail);
        let got = slot.capture(&scene(), &mut fonts, 2.0).unwrap().encode_png().unwrap();
        assert_eq!(got, want);
        let d = slot.diagnostics().unwrap();
        assert_eq!((d.backend.as_str(), d.frames, d.fallback_reason), ("cpu", 1, None));
    }

    #[test]
    fn gpu_pref_without_gpu_is_a_clear_error() {
        let mut slot = RendererSlot::with(Ok(BackendPref::Gpu), fail);
        let e = slot.capture(&scene(), &mut Fonts::new(), 1.0).unwrap_err();
        assert_eq!(e, "GPU renderer unavailable: no adapter");
    }

    #[cfg(not(feature = "gpu"))]
    #[test]
    fn real_factory_names_the_missing_feature() {
        let mut slot = RendererSlot::new();
        slot.set(BackendPref::Gpu);
        assert!(slot.diagnostics().unwrap_err().contains("built without the `gpu` feature"));
    }

    #[test]
    fn auto_falls_back_to_cpu_with_reason() {
        let mut slot = RendererSlot::with(Ok(BackendPref::Auto), fail);
        slot.capture(&scene(), &mut Fonts::new(), 1.0).unwrap();
        let d = slot.diagnostics().unwrap();
        assert_eq!(d.backend, "cpu");
        assert_eq!(d.fallback_reason.as_deref(), Some("GPU renderer unavailable: no adapter"));
    }

    #[test]
    fn invalid_env_value_is_named_in_the_error() {
        let mut slot = RendererSlot::with("metal".parse::<BackendPref>(), fail);
        let e = slot.capture(&scene(), &mut Fonts::new(), 1.0).unwrap_err();
        assert!(e.contains("metal"), "{e}");
    }

    static CALLS: AtomicU32 = AtomicU32::new(0);

    /// Reports a lost surface once, then a lost device forever.
    struct Flaky(bool);
    impl Renderer for Flaky {
        fn kind(&self) -> BackendKind {
            BackendKind::Gpu
        }
        fn capture(&mut self, _: &Scene, _: &mut Fonts, _: f64) -> Result<Frame, RenderError> {
            CALLS.fetch_add(1, Ordering::SeqCst);
            if !std::mem::replace(&mut self.0, true) {
                return Err(RenderError::SurfaceLost("resized".into()));
            }
            Err(RenderError::DeviceLost("gone".into()))
        }
        fn diagnostics(&self) -> Diagnostics {
            Diagnostics { backend: "gpu".into(), ..Diagnostics::default() }
        }
    }

    #[test]
    fn device_loss_reinitializes_once_then_falls_back_to_cpu() {
        CALLS.store(0, Ordering::SeqCst);
        let mut slot = RendererSlot::with(Ok(BackendPref::Gpu), || Ok(Box::new(Flaky(true))));
        let f = slot.capture(&scene(), &mut Fonts::new(), 1.0).unwrap();
        assert_eq!((f.width, f.height), (50, 40));
        let d = slot.diagnostics().unwrap();
        assert_eq!(d.backend, "cpu");
        assert!(d.fallback_reason.unwrap().contains("device lost (gone)"));
        // initial device-lost capture + one capture on the re-initialized device
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    }
}
