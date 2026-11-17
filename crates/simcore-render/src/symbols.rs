//! SF Symbols as a host capability. On macOS the system's own symbols are rasterized through
//! AppKit (`NSImage(systemSymbolName:)` + `NSImage.SymbolConfiguration`) into an alpha mask that the
//! rasterizer tints with the foreground color. Nothing Apple-licensed is stored in the repo.
//! Other platforms (and unknown names) get `None`; callers draw a placeholder and report it.
//!
//! ponytail: monochrome only; multicolor / hierarchical / palette rendering modes are out of scope.

use simcore_protocol::ImageScale;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Which symbol to render at what size, in points.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSpec {
    pub name: String,
    pub point_size: f64,
    /// OpenType wght (100..=900).
    pub weight: u16,
    pub scale: ImageScale,
}

/// Natural size of a symbol in points (AppKit image size at the configuration).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SymbolMetrics {
    pub w: f64,
    pub h: f64,
}

/// Alpha coverage, `w * h` device pixels, row 0 on top.
#[derive(Debug)]
pub struct SymbolMask {
    pub w: u32,
    pub h: u32,
    pub alpha: Vec<u8>,
}

/// `"system"` when symbols are rasterized from the OS, else `"placeholder"`.
pub fn capability() -> &'static str {
    if cfg!(target_os = "macos") {
        "system"
    } else {
        "placeholder"
    }
}

type MetricsKey = (String, u32, u16, u8);
type MaskKey = (MetricsKey, u32, u32, u32);

#[derive(Default)]
struct Cache {
    metrics: HashMap<MetricsKey, Option<SymbolMetrics>>,
    masks: HashMap<MaskKey, Option<Arc<SymbolMask>>>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Cache::default))
}

fn key(s: &SymbolSpec) -> MetricsKey {
    (s.name.clone(), (s.point_size * 100.0).round() as u32, s.weight, s.scale as u8)
}

/// Size of the symbol in points, or `None` when the name is unknown / unsupported on this host.
pub fn metrics(spec: &SymbolSpec) -> Option<SymbolMetrics> {
    let k = key(spec);
    if let Some(m) = with_cache(|c| c.metrics.get(&k).copied()) {
        return m;
    }
    let m = sys::metrics(spec);
    with_cache(|c| c.metrics.insert(k, m));
    m
}

/// Rasterize `spec` into a `w_px` x `h_px` mask (the symbol is stretched to that box; callers
/// pass the aspect-correct size).
pub fn mask(spec: &SymbolSpec, w_px: u32, h_px: u32, device_scale: f64) -> Option<Arc<SymbolMask>> {
    let k = (key(spec), w_px, h_px, (device_scale * 100.0).round() as u32);
    if let Some(m) = with_cache(|c| c.masks.get(&k).cloned()) {
        return m;
    }
    let m = sys::mask(spec, w_px, h_px, device_scale).map(Arc::new);
    with_cache(|c| c.masks.insert(k, m.clone()));
    m
}

#[cfg(target_os = "macos")]
mod sys {
    use super::*;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage, NSImageSymbolConfiguration,
        NSImageSymbolScale,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    fn image(spec: &SymbolSpec, pt: f64) -> Option<Retained<NSImage>> {
        let name = NSString::from_str(&spec.name);
        let base = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None)?;
        let scale = match spec.scale {
            ImageScale::Small => NSImageSymbolScale::Small,
            ImageScale::Medium => NSImageSymbolScale::Medium,
            ImageScale::Large => NSImageSymbolScale::Large,
        };
        // NSFontWeight: ultraLight -0.8 ... regular 0 ... black 0.62.
        const WEIGHTS: [f64; 9] = [-0.8, -0.6, -0.4, 0.0, 0.23, 0.3, 0.4, 0.56, 0.62];
        let w = WEIGHTS[(spec.weight.clamp(100, 900) as usize / 100 - 1).min(8)];
        let cfg = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(pt, w, scale);
        base.imageWithSymbolConfiguration(&cfg)
    }

    pub fn metrics(spec: &SymbolSpec) -> Option<SymbolMetrics> {
        let s = image(spec, spec.point_size)?.size();
        (s.width > 0.0 && s.height > 0.0).then_some(SymbolMetrics { w: s.width, h: s.height })
    }

    pub fn mask(spec: &SymbolSpec, w_px: u32, h_px: u32, device_scale: f64) -> Option<SymbolMask> {
        if w_px == 0 || h_px == 0 {
            return None;
        }
        // Render the glyph at device resolution so weight/curvature match a native @Nx draw.
        let img = image(spec, spec.point_size * device_scale)?;
        // SAFETY: plain RGBA8 interleaved bitmap owned by the rep (planes = null).
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                objc2::AnyThread::alloc(),
                std::ptr::null_mut(),
                w_px as isize,
                h_px as isize,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                (w_px * 4) as isize,
                32,
            )
        }?;
        let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&ctx));
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w_px as f64, h_px as f64));
        img.drawInRect_fromRect_operation_fraction(rect, NSRect::ZERO, NSCompositingOperation::SourceOver, 1.0);
        ctx.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        // SAFETY: the rep owns w_px*h_px*4 bytes and stays alive through this copy.
        let px = unsafe { std::slice::from_raw_parts(data, (w_px * h_px * 4) as usize) };
        let alpha: Vec<u8> = px.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        // Unknown names never get here; a fully transparent result means AppKit drew nothing.
        alpha.iter().any(|&a| a > 0).then_some(SymbolMask { w: w_px, h: h_px, alpha })
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    use super::*;
    pub fn metrics(_: &SymbolSpec) -> Option<SymbolMetrics> {
        None
    }
    pub fn mask(_: &SymbolSpec, _: u32, _: u32, _: f64) -> Option<SymbolMask> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, pt: f64, scale: ImageScale) -> SymbolSpec {
        SymbolSpec { name: name.into(), point_size: pt, weight: 400, scale }
    }

    #[test]
    fn sizing_scales_and_caches() {
        if capability() != "system" {
            return; // placeholder hosts: nothing to measure
        }
        let m = metrics(&spec("star.fill", 17.0, ImageScale::Medium)).expect("star.fill exists on macOS");
        assert!(m.w > 10.0 && m.w < 30.0 && m.h > 10.0 && m.h < 30.0, "{m:?}");
        // Large symbols are bigger, small ones smaller, same point size.
        let s = metrics(&spec("star.fill", 17.0, ImageScale::Small)).unwrap();
        let l = metrics(&spec("star.fill", 17.0, ImageScale::Large)).unwrap();
        assert!(s.w < m.w && m.w < l.w, "{s:?} {m:?} {l:?}");
        // Point size scales linearly-ish.
        let big = metrics(&spec("star.fill", 34.0, ImageScale::Medium)).unwrap();
        assert!((big.w / m.w - 2.0).abs() < 0.15, "{big:?} vs {m:?}");
        assert!(metrics(&spec("definitely.not.a.symbol", 17.0, ImageScale::Medium)).is_none());
        // Masks: painted, cached (same Arc), distinct per device scale.
        let a = mask(&spec("star.fill", 17.0, ImageScale::Medium), 60, 57, 3.0).expect("mask");
        assert!(a.alpha.iter().filter(|&&p| p > 128).count() > 200);
        let b = mask(&spec("star.fill", 17.0, ImageScale::Medium), 60, 57, 3.0).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(mask(&spec("definitely.not.a.symbol", 17.0, ImageScale::Medium), 60, 57, 3.0).is_none());
    }
}
