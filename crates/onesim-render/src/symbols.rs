//! SF Symbols as a host capability. On macOS the system's own symbols are rasterized through
//! AppKit (`NSImage(systemSymbolName:)` + `NSImage.SymbolConfiguration`) into an alpha mask that the
//! rasterizer tints with the foreground color. Nothing Apple-licensed is stored in the repo.
//! Other platforms, or `SIMCORE_FALLBACK=1`, use bundled Phosphor Icons (MIT) via resvg instead
//! (see `bundled`). Unmapped names get `None`; callers draw a placeholder and report it.
//!
//! Monochrome is the default. Hierarchical, palette, multicolor and gradient draw RGBA on macOS;
//! the Phosphor fallback stays a tinted mask (those modes are partial there).

use onesim_protocol::ImageScale;
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
    /// 0 monochrome, 1 hierarchical, 2 palette, 3 multicolor.
    pub mode: u8,
    /// 0 automatic, 1 color, 2 draw. No variable value is applied.
    pub variable: u8,
    pub gradient: bool,
}

impl Default for SymbolSpec {
    fn default() -> Self {
        Self { name: String::new(), point_size: 0.0, weight: 400, scale: ImageScale::Medium, mode: 0, variable: 0, gradient: false }
    }
}

impl SymbolSpec {
    /// A mode that needs the symbol's own colors rather than a tinted mask.
    pub fn colored(&self) -> bool {
        self.mode != 0 || self.variable != 0 || self.gradient
    }
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
    /// Premultiplied RGBA when the symbol was drawn in color. Empty for a template mask.
    pub rgba: Vec<u8>,
}

/// Bundled Phosphor fallback instead of AppKit: no AppKit, or forced with `SIMCORE_FALLBACK=1`.
fn use_bundled() -> bool {
    !cfg!(target_os = "macos") || crate::fallback_forced()
}

/// `"system"` when symbols are rasterized from the OS, else `"bundled"` (Phosphor Icons).
/// (`"placeholder"` is reserved for builds without the bundled set.)
pub fn capability() -> &'static str {
    if use_bundled() {
        "bundled"
    } else {
        "system"
    }
}

type MetricsKey = (String, u32, u16, u8, bool, u8, u8, bool);
type MaskKey = (MetricsKey, u32, u32, u32, u32);

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
    (s.name.clone(), (s.point_size * 100.0).round() as u32, s.weight, s.scale as u8, use_bundled(), s.mode, s.variable, s.gradient)
}

fn color_key(spec: &SymbolSpec, color: [u8; 4]) -> u32 {
    if spec.colored() { u32::from_ne_bytes(color) } else { 0 }
}

/// Size of the symbol in points, or `None` when the name is unknown / unsupported on this host.
pub fn metrics(spec: &SymbolSpec) -> Option<SymbolMetrics> {
    let k = key(spec);
    if let Some(m) = with_cache(|c| c.metrics.get(&k).copied()) {
        return m;
    }
    let m = if use_bundled() { bundled::metrics(spec) } else { sys::metrics(spec) };
    with_cache(|c| c.metrics.insert(k, m));
    m
}

/// Rasterize `spec` into a `w_px` x `h_px` mask (the symbol is stretched to that box; callers
/// pass the aspect-correct size).
pub fn mask(spec: &SymbolSpec, w_px: u32, h_px: u32, device_scale: f64, color: [u8; 4]) -> Option<Arc<SymbolMask>> {
    let k = (key(spec), w_px, h_px, (device_scale * 100.0).round() as u32, color_key(spec, color));
    if let Some(m) = with_cache(|c| c.masks.get(&k).cloned()) {
        return m;
    }
    let m = if use_bundled() {
        bundled::mask(spec, w_px, h_px)
    } else {
        sys::mask(spec, w_px, h_px, device_scale, color)
    }
    .map(Arc::new);
    with_cache(|c| c.masks.insert(k, m.clone()));
    m
}

#[cfg(target_os = "macos")]
mod sys {
    use super::*;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSBitmapImageRep, NSColor, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage, NSImageSymbolColorRenderingMode,
        NSImageSymbolConfiguration, NSImageSymbolScale, NSImageSymbolVariableValueMode,
    };
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

    fn image(spec: &SymbolSpec, pt: f64, color: [u8; 4]) -> Option<Retained<NSImage>> {
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
        let mut cfg = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(pt, w, scale);
        if spec.colored() {
            let ns = NSColor::colorWithSRGBRed_green_blue_alpha(color[0] as f64 / 255.0, color[1] as f64 / 255.0, color[2] as f64 / 255.0, color[3] as f64 / 255.0);
            if spec.mode != 0 {
                let mode = match spec.mode {
                    1 => NSImageSymbolConfiguration::configurationWithHierarchicalColor(&ns),
                    2 => {
                        let fade = |k: f64| {
                            NSColor::colorWithSRGBRed_green_blue_alpha(color[0] as f64 / 255.0, color[1] as f64 / 255.0, color[2] as f64 / 255.0, color[3] as f64 / 255.0 * k)
                        };
                        let colors = [ns, fade(0.6), fade(0.3)];
                        NSImageSymbolConfiguration::configurationWithPaletteColors(&NSArray::from_retained_slice(&colors))
                    }
                    _ => NSImageSymbolConfiguration::configurationPreferringMulticolor(),
                };
                cfg = cfg.configurationByApplyingConfiguration(&mode);
            }
            if spec.variable != 0 {
                let mode = if spec.variable == 2 { NSImageSymbolVariableValueMode::Draw } else { NSImageSymbolVariableValueMode::Color };
                cfg = cfg.configurationByApplyingConfiguration(&NSImageSymbolConfiguration::configurationWithVariableValueMode(mode));
            }
            if spec.gradient {
                cfg = cfg.configurationByApplyingConfiguration(&NSImageSymbolConfiguration::configurationWithColorRenderingMode(NSImageSymbolColorRenderingMode::Gradient));
            }
        }
        base.imageWithSymbolConfiguration(&cfg)
    }

    pub fn metrics(spec: &SymbolSpec) -> Option<SymbolMetrics> {
        let s = image(spec, spec.point_size, [0, 0, 0, 255])?.size();
        (s.width > 0.0 && s.height > 0.0).then_some(SymbolMetrics { w: s.width, h: s.height })
    }

    pub fn mask(spec: &SymbolSpec, w_px: u32, h_px: u32, device_scale: f64, color: [u8; 4]) -> Option<SymbolMask> {
        if w_px == 0 || h_px == 0 {
            return None;
        }
        // Render the glyph at device resolution so weight/curvature match a native @Nx draw.
        let img = image(spec, spec.point_size * device_scale, color)?;
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
        let chunks = px.as_chunks::<4>().0;
        let alpha: Vec<u8> = chunks.iter().map(|p| p[3]).collect();
        // Unknown names never get here; a fully transparent result means AppKit drew nothing.
        if !alpha.iter().any(|&a| a > 0) {
            return None;
        }
        // A template draw is grayscale. Real hierarchical / palette / multicolor pixels have chroma.
        let colorful = chunks.iter().any(|p| p[3] > 40 && p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) > 18);
        let rgba = if spec.colored() && colorful { chunks.iter().flat_map(|p| premul(*p)).collect() } else { Vec::new() };
        Some(SymbolMask { w: w_px, h: h_px, alpha, rgba })
    }

    /// Store premultiplied RGBA. AppKit's bitmap is premultiplied; a straight-alpha pixel is converted.
    fn premul(p: [u8; 4]) -> [u8; 4] {
        let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
        if r <= a && g <= a && b <= a {
            [r, g, b, a]
        } else {
            let pm = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
            [pm(r), pm(g), pm(b), a]
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    use super::*;
    pub fn metrics(_: &SymbolSpec) -> Option<SymbolMetrics> {
        None
    }
    pub fn mask(_: &SymbolSpec, _: u32, _: u32, _: f64, _: [u8; 4]) -> Option<SymbolMask> {
        None
    }
}

/// Phosphor Icons fallback: SVG -> alpha mask via resvg, sized to approximate AppKit's layout box.
mod bundled {
    use super::*;
    use crate::symbols_map::MAP;
    use resvg::{tiny_skia, usvg};

    /// Phosphor icons are 256-unit squares. AppKit boxes are the glyph's ink plus a margin, and ink
    /// scales with the point size: ink = svg_units * K * pt / 256 * (imageScale factor); box = ink + PAD_* * pt.
    /// K, PAD_W and PAD_H are fitted to AppKit's `NSImage.size` and ink for common symbols.
    const K: f64 = 1.25;
    const PAD_W: f64 = 0.2;
    const PAD_H: f64 = 0.15;
    const SCALE: [f64; 3] = [0.78, 1.0, 1.29];

    struct Icon {
        tree: usvg::Tree,
        /// Tight ink bounds in SVG units.
        ink: (f32, f32, f32, f32),
    }

    type IconCache = HashMap<(usize, usize), Option<Arc<Icon>>>;
    static ICONS: Mutex<Option<IconCache>> = Mutex::new(None);

    /// Table row and style (0 regular, 1 bold, 2 fill) for `spec`.
    fn lookup(spec: &SymbolSpec) -> Option<(usize, usize)> {
        let (base, fill) = spec.name.strip_suffix(".fill").map_or((spec.name.as_str(), false), |b| (b, true));
        let row = MAP.iter().position(|r| r.0 == base)?;
        Some((row, if fill { 2 } else if spec.weight >= 600 { 1 } else { 0 }))
    }

    fn icon(spec: &SymbolSpec) -> Option<Arc<Icon>> {
        let k = lookup(spec)?;
        let mut g = ICONS.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(HashMap::new)
            .entry(k)
            .or_insert_with(|| {
                let svg = MAP[k.0].1[k.1].replace("currentColor", "#000");
                let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
                let b = tree.root().abs_bounding_box();
                (b.width() > 0.0 && b.height() > 0.0).then(|| Arc::new(Icon { ink: (b.x(), b.y(), b.width(), b.height()), tree }))
            })
            .clone()
    }

    /// Ink size in points.
    fn ink_pt(i: &Icon, spec: &SymbolSpec) -> (f64, f64) {
        let k = K * spec.point_size * SCALE[spec.scale as usize] / 256.0;
        (i.ink.2 as f64 * k, i.ink.3 as f64 * k)
    }

    pub fn metrics(spec: &SymbolSpec) -> Option<SymbolMetrics> {
        let i = icon(spec)?;
        let (w, h) = ink_pt(&i, spec);
        let pad = spec.point_size * SCALE[spec.scale as usize];
        Some(SymbolMetrics { w: w + PAD_W * pad, h: h + PAD_H * pad })
    }

    pub fn mask(spec: &SymbolSpec, w_px: u32, h_px: u32) -> Option<SymbolMask> {
        let i = icon(spec)?;
        let (w, h) = (w_px as f32, h_px as f32);
        // Ink occupies the same fraction of the (possibly stretched) box as in the natural box.
        let m = metrics(spec)?;
        let (iw, ih) = ink_pt(&i, spec);
        let (fw, fh) = ((iw / m.w) as f32, (ih / m.h) as f32);
        let (sx, sy) = (w * fw / i.ink.2, h * fh / i.ink.3);
        let tf = tiny_skia::Transform::from_translate((w - w * fw) / 2.0, (h - h * fh) / 2.0).pre_scale(sx, sy).pre_translate(-i.ink.0, -i.ink.1);
        let mut pm = tiny_skia::Pixmap::new(w_px, h_px)?;
        resvg::render(&i.tree, tf, &mut pm.as_mut());
        let alpha: Vec<u8> = pm.pixels().iter().map(|p| p.alpha()).collect();
        alpha.iter().any(|&a| a > 0).then_some(SymbolMask { w: w_px, h: h_px, alpha, rgba: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, pt: f64, scale: ImageScale) -> SymbolSpec {
        SymbolSpec { name: name.into(), point_size: pt, weight: 400, scale, ..SymbolSpec::default() }
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
        let a = mask(&spec("star.fill", 17.0, ImageScale::Medium), 60, 57, 3.0, [0, 0, 0, 255]).expect("mask");
        assert!(a.alpha.iter().filter(|&&p| p > 128).count() > 200);
        let b = mask(&spec("star.fill", 17.0, ImageScale::Medium), 60, 57, 3.0, [0, 0, 0, 255]).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(a.rgba.is_empty(), "the default symbol stays a tinted mask");
        assert!(mask(&spec("definitely.not.a.symbol", 17.0, ImageScale::Medium), 60, 57, 3.0, [0, 0, 0, 255]).is_none());
        let mut hier = spec("person.crop.circle.fill", 17.0, ImageScale::Medium);
        hier.mode = 1;
        let colored = mask(&hier, 60, 57, 3.0, [220, 30, 30, 255]).expect("hierarchical");
        assert!(!colored.rgba.is_empty(), "hierarchical red should keep its color");
        let red = colored.rgba.chunks(4).filter(|p| p[3] > 80).any(|p| p[0] > p[2] + 20);
        assert!(red, "hierarchical red is red in the bitmap");
    }
}
