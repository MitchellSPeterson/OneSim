use cosmic_text::{fontdb, Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use std::collections::HashMap;

/// macOS system UI font (variable SF Pro: wght axis). Family name as registered in fontdb.
const SF_FILE: &str = "/System/Library/Fonts/SFNS.ttf";
const SF_FAMILY: &str = ".SF NS";
/// Open-licensed fallback (Inter, SIL OFL 1.1; variable `wght` axis), embedded for determinism.
const INTER: &[u8] = include_bytes!("../../../assets/fonts/InterVariable.ttf");
const INTER_FAMILY: &str = "Inter Variable";

/// Horizontal text alignment inside the text node's own bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Leading,
    Center,
    Trailing,
}

/// Font parameters shared by measuring and drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSpec {
    pub size: f32,
    /// Height of a single line.
    pub line_height: f32,
    /// Baseline-to-baseline distance for wrapped text (iOS: body 17pt -> 22pt).
    pub pitch: f32,
    pub weight: u16,
    pub align: TextAlign,
    /// Letter spacing in em (see [`tracking_em`]); independent of the raster scale.
    pub tracking: f32,
}

impl FontSpec {
    pub fn new(size: f32, line_height: f32, weight: u16, align: TextAlign) -> Self {
        Self { size, line_height, pitch: line_height.max(size * 22.0 / 17.0), weight, align, tracking: tracking_em(size, weight) }
    }
}

/// SF Pro ships optical sizes (`opsz`); cosmic-text only drives `wght`, so the variable font's
/// default (display) cut renders narrower than iOS's text cut at small sizes. Per-size tracking,
/// fitted to widths measured on an iOS 26 simulator, restores the iOS advance widths.
pub fn tracking_em(size: f32, weight: u16) -> f32 {
    const PTS: [(f32, f32); 8] =
        [(11.0, 0.062), (12.0, 0.058), (15.0, 0.043), (17.0, 0.035), (20.0, 0.02), (22.0, 0.0135), (28.0, 0.0145), (34.0, 0.012)];
    let t = match PTS.iter().position(|p| p.0 >= size) {
        Some(0) => PTS[0].1,
        Some(i) => {
            let (a, b) = (PTS[i - 1], PTS[i]);
            a.1 + (b.1 - a.1) * (size - a.0) / (b.0 - a.0)
        }
        None => 0.011,
    };
    // Heavier weights are measured slightly tighter at body size.
    if size < 20.0 && weight >= 600 {
        t - 0.007
    } else {
        t
    }
}

/// Inter has no SF-style optical sizes and is wider than SF Pro's display cut at small sizes, so
/// its per-size tracking (em) is fitted separately to the same iOS widths (`scripts/compare-ios.sh`).
/// Used instead of [`tracking_em`] when the bundled font is active.
pub fn inter_tracking_em(size: f32, weight: u16) -> f32 {
    fn lerp(pts: &[(f32, f32)], x: f32) -> f32 {
        let i = pts.iter().position(|p| p.0 >= x).unwrap_or(pts.len() - 1).max(1);
        let (a, b) = (pts[i - 1], pts[i]);
        a.1 + (b.1 - a.1) * ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
    }
    // Regular-weight tracking by size, then the extra needed per weight (SF gets heavier faster than Inter).
    const BY_SIZE: [(f32, f32); 9] = [
        (11.0, 0.0008), (12.0, -0.0032), (13.0, -0.0082), (15.0, -0.0182), (17.0, -0.0262),
        (20.0, -0.0412), (22.0, -0.0477), (28.0, -0.0467), (34.0, -0.0492),
    ];
    const BY_WEIGHT: [(f32, f32); 3] = [(400.0, 0.0), (600.0, 0.0085), (700.0, 0.0163)];
    // SF stops tightening heavy text at 20pt+ (see `tracking_em`), so the gap widens there.
    let big = if size >= 20.0 { 0.007 * ((weight as f32 - 400.0) / 200.0).clamp(0.0, 1.0) } else { 0.0 };
    lerp(&BY_SIZE, size) + lerp(&BY_WEIGHT, weight as f32) + big
}

/// `"system"` when Apple's SF Pro is used, `"bundled"` for the Inter fallback.
pub fn fonts_capability() -> &'static str {
    if !crate::fallback_forced() && std::path::Path::new(SF_FILE).exists() {
        "system"
    } else {
        "bundled"
    }
}

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    cache: SwashCache,
    sf: bool,
    inter: bool,
    measured: HashMap<(String, u32, u16, u32), (f64, f64)>,
}

/// One drawn text line: x range and baseline, relative to the text origin (device px).
pub struct LineSpan {
    pub x0: f32,
    pub x1: f32,
    pub baseline: f32,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    /// SF Pro when present, else bundled Inter. `SIMCORE_FALLBACK=1` forces Inter.
    pub fn new() -> Self {
        Self::with_fallback(crate::fallback_forced())
    }

    pub fn with_fallback(force_inter: bool) -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let has = |db: &fontdb::Database| db.faces().any(|f| f.families.iter().any(|(n, _)| n == SF_FAMILY));
        if !force_inter && !has(&db) {
            let _ = db.load_font_file(SF_FILE);
        }
        let sf = !force_inter && has(&db);
        let inter = !sf;
        if inter {
            db.load_font_data(INTER.to_vec());
        }
        let sys = FontSystem::new_with_locale_and_db("en-US".into(), db);
        Self { sys, cache: SwashCache::new(), sf, inter, measured: HashMap::new() }
    }

    /// True when Apple's SF Pro is in use.
    pub fn has_sf(&self) -> bool {
        self.sf
    }

    /// True when the bundled Inter fallback is in use.
    pub fn has_inter(&self) -> bool {
        self.inter
    }

    fn buffer(&mut self, text: &str, f: &FontSpec, width: Option<f32>) -> Buffer {
        let mut b = Buffer::new(&mut self.sys, Metrics::new(f.size, f.pitch));
        b.set_size(width, None);
        let (family, tracking) = if self.sf {
            (Family::Name(SF_FAMILY), f.tracking)
        } else {
            (Family::Name(INTER_FAMILY), inter_tracking_em(f.size, f.weight))
        };
        let attrs = Attrs::new().family(family).weight(Weight(f.weight)).letter_spacing(tracking);
        b.set_text(text, &attrs, Shaping::Advanced, None);
        let align = match f.align {
            TextAlign::Leading => None,
            TextAlign::Center => Some(Align::Center),
            TextAlign::Trailing => Some(Align::Right),
        };
        for l in b.lines.iter_mut() {
            l.set_align(align);
        }
        b.shape_until_scroll(&mut self.sys, false);
        b
    }

    /// Size of `text`, wrapping at `width` when given.
    pub fn measure(&mut self, text: &str, f: &FontSpec, width: Option<f32>) -> (f64, f64) {
        let key = (text.to_string(), f.size.to_bits(), f.weight, width.map_or(u32::MAX, f32::to_bits));
        if let Some(r) = self.measured.get(&key) {
            return *r;
        }
        let b = self.buffer(text, &FontSpec { align: TextAlign::Leading, ..*f }, width);
        let (mut w, mut lines) = (0f32, 0usize);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            lines += 1;
        }
        let r = (w as f64, (f.line_height + (lines.max(1) - 1) as f32 * f.pitch) as f64);
        self.measured.insert(key, r);
        r
    }

    /// Calls `f(x, y, w, h, rgba)` for every glyph pixel run, relative to the text origin.
    /// Returns each laid-out line's extent (for strikethrough).
    pub fn draw(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        mut f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        let mut b = self.buffer(text, spec, Some(width));
        let spans = b
            .layout_runs()
            .filter_map(|r| {
                let first = r.glyphs.first()?;
                let last = r.glyphs.last()?;
                Some(LineSpan { x0: first.x, x1: last.x + last.w, baseline: r.line_y })
            })
            .collect();
        let c = Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
        b.draw(&mut self.sys, &mut self.cache, c, |x, y, w, h, c| f(x, y, w, h, [c.r(), c.g(), c.b(), c.a()]));
        spans
    }
}
