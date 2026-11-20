use cosmic_text::{fontdb, Align, Attrs, Buffer, Color, Family, FeatureTag, FontFeatures, FontSystem, Metrics, Shaping, Stretch, Style, SwashCache, Weight};
use std::collections::HashMap;

/// macOS system UI font (variable SF Pro: wght axis). Family name as registered in fontdb.
const SF_FILE: &str = "/System/Library/Fonts/SFNS.ttf";
const SF_FAMILY: &str = ".SF NS";
const MONO_FAMILY: &str = ".SF NS Mono";
const ROUNDED_NAMES: &[&str] = &[".SF NS Rounded", ".SFNSRounded", "SF NS Rounded"];
const SERIF_NAMES: &[&str] = &[".New York", "New York", ".NewYork", "NewYork"];
/// `Text.Scale.secondary` relative to the current size. Fitted to the iOS 26.5 ink width of "Secondary" at 17pt.
pub const SECONDARY_TEXT_SCALE: f32 = 0.86;
/// Horizontal scale. cosmic-text varies `wght` only, so SF Pro's `wdth` axis never moves.
/// Condensed is 0.80: at 17pt the iOS ink of "Condensed" is 206px and the standard face is 257px
/// (iOS 26.5 and 27.2). Compressed is 0.70. There is no reference shot for it. Expanded stays 1.
fn width_scale(width: i8) -> f32 {
    match width {
        -2 => 0.70,
        -1 => 0.80,
        _ => 1.0,
    }
}
/// Open-licensed fallback (Inter, SIL OFL 1.1; variable `wght` axis), embedded for determinism.
const INTER: &[u8] = include_bytes!("../../../assets/fonts/InterVariable.ttf");
const INTER_FAMILY: &str = "Inter Variable";
/// Android's default sans-serif (Roboto, SIL OFL 1.1; variable `wdth`/`wght`), embedded: used for `FontSpec::android` text.
const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/RobotoVariable.ttf");
const ROBOTO_FAMILY: &str = "Roboto";

/// Android draws text at a whole-pixel size (measured on the emulator: 22 sp at 2.625x renders and measures
/// as 57 px, not 57.75), so glyphs are a little smaller than `sp * density`. Returns the size in dp.
pub(crate) fn android_glyph_size(sp: f32, density: f32) -> f32 {
    if density > 0.0 { (sp * density + 1e-3).floor() / density } else { sp }
}


/// Horizontal text alignment inside the text node's own bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Leading,
    Center,
    Trailing,
}

/// One styled run of a concatenated `Text` (SwiftUI) or nested React Native `Text`: its own size
/// (points), weight and color. The `FontSpec` passed alongside gives the line metrics (SwiftUI: the
/// largest run's); every run scales from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub pt: f32,
    pub weight: u16,
    pub color: crate::scene::Color,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// System monospaced face.
    pub mono: bool,
    /// Extra letter-spacing in points (kerning + tracking), added to the optical fit.
    pub extra: f32,
    /// 0 default, 1 serif, 2 rounded, 3 monospaced.
    pub design: u8,
    /// -2 compressed, -1 condensed, 0 standard, 1 expanded.
    pub width: i8,
    /// OpenType `tnum`.
    pub digits: bool,
    /// Baseline offset in points. Positive moves the glyphs up.
    pub baseline: f32,
    /// Underline pattern: 0 solid, 1 dot, 2 dash, 3 dashDot, 4 dashDotDot.
    pub line_pat: u8,
    /// Underline color when it differs from the run color.
    pub under_color: Option<crate::scene::Color>,
    /// Strike pattern, same codes as `line_pat`.
    pub strike_pat: u8,
    /// Strike color when it differs from the run color.
    pub strike_color: Option<crate::scene::Color>,
}

impl Span {
    pub fn basic(text: impl Into<String>, pt: f32, weight: u16, color: crate::scene::Color) -> Self {
        Self {
            text: text.into(),
            pt,
            weight,
            color,
            italic: false,
            underline: false,
            strike: false,
            mono: false,
            extra: 0.0,
            design: 0,
            width: 0,
            digits: false,
            baseline: 0.0,
            line_pat: 0,
            under_color: None,
            strike_pat: 0,
            strike_color: None,
        }
    }
}

/// Font parameters shared by measuring and drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSpec {
    pub size: f32,
    /// Size in points (`size` is device pixels once rasterizing); selects per-size fits.
    pub pt: f32,
    /// Height of a single line.
    pub line_height: f32,
    /// Baseline-to-baseline distance for wrapped text (iOS: body 17pt -> 22pt).
    pub pitch: f32,
    pub weight: u16,
    pub align: TextAlign,
    /// Letter spacing in em (see [`tracking_em`]); independent of the raster scale.
    pub tracking: f32,
    /// System monospaced face (`ui-monospace`).
    pub mono: bool,
    /// Line cap: the last allowed line is cut to whole glyphs followed by `…` (React Native `numberOfLines`).
    pub lines: Option<usize>,
    /// Explicit paragraph line height (RN `lineHeight`): every run uses `pitch`; otherwise a run's pitch scales with its size.
    pub fixed_pitch: bool,
    /// Use the same Inter tracking for logical layout and device-pixel shaping.
    pub inter_point_tracking: bool,
    /// Android text: Roboto, with `tracking` taken as given (Compose `letterSpacing`).
    pub android: bool,
    /// Extra letter-spacing in points (kerning + tracking).
    pub extra: f32,
    /// 0 default, 1 serif, 2 rounded, 3 monospaced.
    pub design: u8,
    /// -2 compressed, -1 condensed, 0 standard, 1 expanded.
    pub width: i8,
    /// OpenType `tnum`.
    pub digits: bool,
    /// 0 tail, 1 head, 2 middle.
    pub trunc: u8,
    /// Baseline offset in points. Positive lengthens the line downward. Glyphs stay at the top.
    pub baseline: f32,
    /// 0 none, 1 uppercase, 2 lowercase.
    pub case_fold: u8,
    /// Tighten tracking before scaling and truncating.
    pub tighten: bool,
    /// Smallest scale `.minimumScaleFactor` may apply. `1` disables it.
    pub min_scale: f32,
    /// Underline the whole paragraph (view modifier). Runs carry their own flag.
    pub underline: bool,
    /// Paragraph italic (view modifier).
    pub italic: bool,
    /// Underline pattern for a paragraph that has no runs.
    pub line_pat: u8,
    /// Underline color when it differs from the text color.
    pub under_color: Option<crate::scene::Color>,
    /// Strike pattern for a paragraph that has no runs.
    pub strike_pat: u8,
    /// Strike color when it differs from the text color.
    pub strike_color: Option<crate::scene::Color>,
    /// Reserve at least this many lines of height.
    pub reserve: Option<usize>,
}

impl FontSpec {
    pub fn new(size: f32, line_height: f32, weight: u16, align: TextAlign) -> Self {
        // Leading: 22/17 of the size up to large-title sizes, ~1.12 x size + 2.64 above (accessibility sizes, measured: 53pt -> 62pt).
        let pitch = if size > 34.0 { size * 1.12 + 2.64 } else { line_height.max(size * 22.0 / 17.0) };
        Self {
            size,
            pt: size,
            line_height,
            pitch,
            weight,
            align,
            tracking: tracking_em(size, weight),
            mono: false,
            lines: None,
            fixed_pitch: false,
            inter_point_tracking: false,
            android: false,
            extra: 0.0,
            design: 0,
            width: 0,
            digits: false,
            trunc: 0,
            baseline: 0.0,
            case_fold: 0,
            tighten: false,
            min_scale: 1.0,
            underline: false,
            italic: false,
            line_pat: 0,
            under_color: None,
            strike_pat: 0,
            strike_color: None,
            reserve: None,
        }
    }
}

/// SF Pro ships optical sizes (`opsz`); cosmic-text only drives `wght`, so the variable font's
/// default (display) cut renders narrower than iOS's text cut at small sizes. Per-size tracking,
/// fitted to widths measured on an iOS 26 simulator, restores the iOS advance widths.
pub fn tracking_em(size: f32, weight: u16) -> f32 {
    const PTS: [(f32, f32); 11] = [
        (11.0, 0.062), (12.0, 0.058), (15.0, 0.043), (17.0, 0.035), (20.0, 0.02), (22.0, 0.0135), (28.0, 0.0145), (34.0, 0.012),
        (43.0, 0.0048), (53.0, 0.0055), (60.0, 0.0040),
    ];
    let t = match PTS.iter().position(|p| p.0 >= size) {
        Some(0) => PTS[0].1,
        Some(i) => {
            let (a, b) = (PTS[i - 1], PTS[i]);
            a.1 + (b.1 - a.1) * (size - a.0) / (b.0 - a.0)
        }
        None => 0.0039,
    };
    // Heavier weights are measured slightly tighter at body size.
    if size < 20.0 && weight >= 600 {
        t - 0.007
    } else if size >= 40.0 && weight >= 700 {
        t - 0.0146 // bold at accessibility sizes (measured)
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
    lerp(&BY_SIZE, size) + inter_weight_tracking(size, weight)
}

/// Extra Inter tracking per weight (SF gets heavier faster than Inter).
fn inter_weight_tracking(size: f32, weight: u16) -> f32 {
    const BY_WEIGHT: [(f32, f32); 3] = [(400.0, 0.0), (600.0, 0.0085), (700.0, 0.0163)];
    let i = if weight >= 600 { 2 } else { 1 };
    let (a, b) = (BY_WEIGHT[i - 1], BY_WEIGHT[i]);
    let w = a.1 + (b.1 - a.1) * ((weight as f32 - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    // SF stops tightening heavy text at 20pt+ (see `tracking_em`), so the gap widens there.
    let big = if size >= 20.0 { 0.007 * ((weight as f32 - 400.0) / 200.0).clamp(0.0, 1.0) } else { 0.0 };
    // Medium (500) is heavier in SF than the 400..600 interpolation says: measured on the Expo template's 14pt text.
    let medium = if (450..=550).contains(&weight) { 0.02 } else { 0.0 };
    w + big + medium
}

/// Inter tracking at accessibility sizes (> 40pt), where it is drawn at `INTER_BIG_SCALE`.
fn inter_ax_tracking(pt: f32, weight: u16) -> f32 {
    const PTS: [(f32, f32); 3] = [(43.0, -0.021), (53.0, -0.023), (60.0, -0.025)];
    let t = if pt <= PTS[0].0 {
        PTS[0].1
    } else if pt >= PTS[2].0 {
        PTS[2].1
    } else {
        let i = if pt <= PTS[1].0 { 1 } else { 2 };
        let (a, b) = (PTS[i - 1], PTS[i]);
        a.1 + (b.1 - a.1) * (pt - a.0) / (b.0 - a.0)
    };
    t + inter_weight_tracking(pt, weight)
}

const INTER_BIG_SCALE: f32 = 0.93;
/// Inter weight offset at accessibility sizes.
const INTER_BIG_WEIGHT: i32 = 0;

/// `"system"` when Apple's SF Pro is used, `"bundled"` for the Inter fallback.
pub fn fonts_capability() -> &'static str {
    if !crate::fallback_forced() && std::path::Path::new(SF_FILE).exists() {
        "system"
    } else {
        "bundled"
    }
}

/// Text, size bits, weight, wrap width bits, Android tracking bits.
type MeasureKey = (String, u32, u16, u32, u32);

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    cache: SwashCache,
    sf: bool,
    inter: bool,
    has_mono: bool,
    rounded: Option<&'static str>,
    serif: Option<&'static str>,
    measured: HashMap<MeasureKey, (f64, f64)>,
}

/// One drawn text line: x range and baseline, relative to the text origin (device px).
pub struct LineSpan {
    pub x0: f32,
    pub x1: f32,
    pub baseline: f32,
    /// Index of the styled run this extent belongs to (`None` for plain text).
    pub run: Option<usize>,
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
        db.load_font_data(ROBOTO.to_vec());
        let sys = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let has_mono = sys.db().faces().any(|f| f.families.iter().any(|(n, _)| n == MONO_FAMILY));
        let named = |names: &'static [&'static str]| {
            names.iter().copied().find(|n| sys.db().faces().any(|f| f.families.iter().any(|(name, _)| name == n)))
        };
        let rounded = named(ROUNDED_NAMES);
        let serif = named(SERIF_NAMES);
        Self { sys, cache: SwashCache::new(), sf, inter, has_mono, rounded, serif, measured: HashMap::new() }
    }

    /// True when Apple's SF Pro is in use.
    pub fn has_sf(&self) -> bool {
        self.sf
    }

    /// True when the bundled Inter fallback is in use.
    /// True when drawing with Apple's SF Pro (false: the bundled Inter fallback).
    pub fn is_sf(&self) -> bool {
        self.sf
    }

    pub fn has_inter(&self) -> bool {
        self.inter
    }

    fn mono_family(&self) -> Family<'static> {
        if self.has_mono { Family::Name(MONO_FAMILY) } else { Family::Monospace }
    }

    /// Family, tracking (em), weight and stretch. `sf_tracking` is SF's tracking for the default face.
    /// A design that switches family drops that optical fit (it was measured for SF Pro).
    /// `width` is a horizontal scale in measure and draw, not a face: cosmic-text never sets `wdth`.
    #[allow(clippy::too_many_arguments)]
    fn face(&self, mono: bool, android: bool, pt: f32, inter_size: f32, weight: u16, sf_tracking: f32, big: bool, design: u8, _width: i8) -> (Family<'static>, f32, u16, Stretch) {
        let wght = if big { (weight as i32 + INTER_BIG_WEIGHT).clamp(100, 900) as u16 } else { weight };
        if mono || design == 3 {
            return (self.mono_family(), 0.0, wght, Stretch::Normal);
        }
        if android {
            return (Family::Name(ROBOTO_FAMILY), sf_tracking, weight, Stretch::Normal);
        }
        if design == 1 {
            let fam = self.serif.map(Family::Name).unwrap_or(Family::Serif);
            return (fam, 0.0, wght, Stretch::Normal);
        }
        if design == 2 {
            if let Some(name) = self.rounded {
                return (Family::Name(name), 0.0, wght, Stretch::Normal);
            }
        }
        if self.sf {
            (Family::Name(SF_FAMILY), sf_tracking, wght, Stretch::Normal)
        } else {
            (Family::Name(INTER_FAMILY), if big { inter_ax_tracking(pt, weight) } else { inter_tracking_em(inter_size, weight) }, wght, Stretch::Normal)
        }
    }

    /// Shaped text. A line cap cuts the last allowed line with `…`: tail (the default), head, or middle.
    fn buffer(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> Buffer {
        let folded = fold_case(text, f.case_fold);
        let folded_spans: Vec<Span> = spans.iter().map(|s| Span { text: fold_case(&s.text, f.case_fold).into_owned(), ..s.clone() }).collect();
        let text = folded.as_ref();
        let spans = if spans.is_empty() { spans } else { folded_spans.as_slice() };
        let b = self.build(text, f, width, spans);
        let (Some(n), Some(limit)) = (f.lines, width) else { return b };
        if b.layout_runs().count() <= n {
            return b;
        }
        let shown = self.clip_text(text, f, &b, limit, n);
        let cut = slice_spans(spans, text, &shown);
        self.build(&shown, f, width, &cut)
    }

    fn clip_text(&mut self, text: &str, f: &FontSpec, b: &Buffer, limit: f32, n: usize) -> String {
        match f.trunc {
            1 => self.trunc_end(text, f, limit, n, true),
            2 => self.trunc_middle(text, f, limit, n),
            _ => self.trunc_tail(text, f, b, limit, n),
        }
    }

    /// The string `buffer` shapes when a line cap cuts `text` (tests).
    #[cfg(test)]
    fn truncated_string(&mut self, text: &str, f: &FontSpec, width: f32) -> String {
        let folded = fold_case(text, f.case_fold);
        let text = folded.as_ref();
        let b = self.build(text, f, Some(width), &[]);
        let Some(n) = f.lines else { return text.to_string() };
        if b.layout_runs().count() <= n {
            return text.to_string();
        }
        self.clip_text(text, f, &b, width, n)
    }

    /// Tail truncation: the existing TextKit-like cut of the last allowed line.
    fn trunc_tail(&mut self, text: &str, f: &FontSpec, b: &Buffer, limit: f32, n: usize) -> String {
        let Some(run) = b.layout_runs().nth(n - 1) else { return text.to_string() };
        let Some(first) = run.glyphs.first() else { return text.to_string() };
        let start: usize = text.split('\n').take(run.line_i).map(|p| p.len() + 1).sum::<usize>() + first.start;
        let rest = text[start..].split('\n').next().unwrap_or("");
        let frag_end = run.glyphs.last().map_or(0, |g| g.end).saturating_sub(first.start);
        let rest = if n > 1 && rest.is_char_boundary(frag_end) { &rest[..frag_end] } else { rest };
        let one = FontSpec { align: TextAlign::Leading, lines: None, trunc: 0, ..*f };
        let bounds: Vec<usize> = rest.char_indices().map(|(i, _)| i).chain([rest.len()]).collect();
        let (mut lo, mut hi) = (0, bounds.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.fits(&format!("{}\u{2026}", rest[..bounds[mid]].trim_end()), &one, limit, 1) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let mut head = text[..start + bounds[lo]].trim_end().to_string();
        if n > 1 && bounds[lo] < rest.len() {
            if let Some((h, _)) = head.rsplit_once(' ') {
                head = h.trim_end().to_string();
            }
        }
        format!("{head}\u{2026}")
    }

    /// Head (`leading` ellipsis) or the shared suffix search. `head` true keeps the end of `text`.
    fn trunc_end(&mut self, text: &str, f: &FontSpec, limit: f32, n: usize, _head: bool) -> String {
        let one = FontSpec { align: TextAlign::Leading, lines: None, trunc: 0, ..*f };
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let (mut lo, mut hi) = (0, bounds.len() - 1);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let probe = format!("\u{2026}{}", text[bounds[mid]..].trim_start());
            if self.fits(&probe, &one, limit, n) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        format!("\u{2026}{}", text[bounds[lo]..].trim_start())
    }

    /// Middle truncation keeps the longest head and tail that fit, then the split whose two sides
    /// are closest in width. Equal character counts miss iOS, which centers the ellipsis
    /// ("The quic…e lazy dog" in 160pt, not "The quick…lazy dog").
    fn trunc_middle(&mut self, text: &str, f: &FontSpec, limit: f32, n: usize) -> String {
        let one = FontSpec { align: TextAlign::Leading, lines: None, trunc: 0, ..*f };
        if self.fits(text, &one, limit, n) {
            return text.to_string();
        }
        let idx: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let nchars = idx.len() - 1;
        if nchars == 0 {
            return format!("{text}\u{2026}");
        }
        let mut cands: Vec<(f64, f64, usize, String)> = Vec::new();
        for head in 0..=nchars {
            let pre_end = idx[head];
            let mut lo = 0;
            let mut hi = nchars - head;
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                let suf_at = idx[nchars - mid];
                if pre_end > suf_at {
                    hi = mid - 1;
                    continue;
                }
                let pre = text[..pre_end].trim_end();
                let suf = text[suf_at..].trim_start();
                if self.fits(&format!("{pre}\u{2026}{suf}"), &one, limit, n) {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let suf_at = idx[nchars - lo];
            if pre_end > suf_at {
                continue;
            }
            let pre = text[..pre_end].trim_end();
            let suf = text[suf_at..].trim_start();
            let shown = format!("{pre}\u{2026}{suf}");
            if !self.fits(&shown, &one, limit, n) {
                continue;
            }
            let kept = pre.chars().count() + suf.chars().count();
            let hw = self.measure(pre, &one, None, &[]).0;
            let tw = self.measure(suf, &one, None, &[]).0;
            let width = self.measure(&shown, &one, None, &[]).0;
            cands.push((width, (hw - tw).abs(), kept, shown));
        }
        // Among cuts that fill the line, put the ellipsis at the center.
        let max_w = cands.iter().map(|c| c.0).fold(0.0, f64::max);
        let mut best: Option<(f64, usize, String)> = None;
        for (w, imb, kept, shown) in cands {
            if w < max_w - 14.0 {
                continue;
            }
            let replace = match &best {
                None => true,
                Some((i, k, _)) => imb < *i - 0.4 || ((imb - *i).abs() <= 0.4 && kept > *k),
            };
            if replace {
                best = Some((imb, kept, shown));
            }
        }
        best.map(|(_, _, s)| s).unwrap_or_else(|| self.trunc_tail_simple(text, &one, limit))
    }

    fn trunc_tail_simple(&mut self, text: &str, f: &FontSpec, limit: f32) -> String {
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let (mut lo, mut hi) = (0, bounds.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.fits(&format!("{}\u{2026}", text[..bounds[mid]].trim_end()), f, limit, 1) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        format!("{}\u{2026}", text[..bounds[lo]].trim_end())
    }

    fn fits(&mut self, text: &str, f: &FontSpec, limit: f32, lines: usize) -> bool {
        let b = self.build(text, f, Some(limit), &[]);
        let n = b.layout_runs().count();
        let w = b.layout_runs().map(|r| r.line_w).fold(0.0_f32, f32::max);
        n <= lines && w * width_scale(f.width) <= limit + 0.5
    }

    fn build(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> Buffer {
        // Inter's glyphs are ~8% larger than SF's at the same size; at accessibility sizes that is a lot
        // of ink, so the fallback draws them slightly smaller (tracking is fitted for it).
        let big = !self.sf && !f.android && f.pt > 40.0;
        let size = if big { f.size * INTER_BIG_SCALE } else { f.size };
        let mut b = Buffer::new(&mut self.sys, Metrics::new(size, f.pitch));
        b.set_size(width, None);
        let inter_size = if f.inter_point_tracking { f.pt } else { f.size };
        let (family, tracking, wght, stretch) = self.face(f.mono || f.design == 3, f.android, f.pt, inter_size, f.weight, f.tracking, big, f.design, f.width);
        let tracking = tracking + extra_em(f.extra, f.pt);
        let mut attrs = Attrs::new().family(family).weight(Weight(wght)).stretch(stretch).letter_spacing(tracking);
        if f.italic {
            attrs = attrs.style(Style::Italic);
        }
        if f.digits {
            attrs = attrs.font_features(tabular());
        }
        if !spans.is_empty() {
            let sf = self.sf;
            let runs = spans.iter().enumerate().map(|(i, s)| {
                let k = s.pt / f.pt.max(0.01);
                let size_px = f.size * k;
                let big = !sf && !f.android && s.pt > 40.0;
                let sf_tracking = if f.android { f.tracking } else { tracking_em(s.pt, s.weight) };
                let (fam, tracking, wght, stretch) = self.face(f.mono || s.mono || s.design == 3, f.android, s.pt, inter_size * k, s.weight, sf_tracking, big, s.design, s.width);
                let tracking = tracking + extra_em(s.extra, s.pt);
                let m = Metrics::new(if big { size_px * INTER_BIG_SCALE } else { size_px }, if f.fixed_pitch { f.pitch } else { f.pitch * k });
                let c = Color::rgba((s.color.r * 255.0).round() as u8, (s.color.g * 255.0).round() as u8, (s.color.b * 255.0).round() as u8, (s.color.a * 255.0).round() as u8);
                let mut a = Attrs::new().family(fam).weight(Weight(wght)).stretch(stretch).letter_spacing(tracking).metrics(m).color(c).metadata(i);
                if s.digits || f.digits {
                    a = a.font_features(tabular());
                }
                (s.text.as_str(), if s.italic || f.italic { a.style(Style::Italic) } else { a })
            });
            b.set_rich_text(runs, &attrs, Shaping::Advanced, None);
        } else {
            b.set_text(text, &attrs, Shaping::Advanced, None);
        }
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
    pub fn measure(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> (f64, f64) {
        // Android text carries its own tracking (Compose letterSpacing) and face; iOS tracking follows size and weight.
        let tracking = if f.android { f.tracking.to_bits() ^ 0x8000_0000 } else { 0 };
        let key = (text.to_string(), f.size.to_bits(), f.weight, width.map_or(u32::MAX, f32::to_bits), tracking);
        // New styling changes the advance. Leave the default-key cache for the fitted optical size only.
        let styled = f.extra != 0.0 || f.design != 0 || f.width != 0 || f.digits || f.trunc != 0 || f.case_fold != 0 || f.italic || f.mono;
        let plain = spans.is_empty() && f.lines.is_none() && !styled;
        if plain {
            if let Some(r) = self.measured.get(&key) {
                return *r;
            }
        }
        let b = self.buffer(text, &FontSpec { align: TextAlign::Leading, ..*f }, width, spans);
        let (mut w, mut lines) = (0f32, 0usize);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            lines += 1;
        }
        let r = (w as f64 * f64::from(width_scale(f.width)), (f.line_height + (lines.max(1) - 1) as f32 * f.pitch) as f64);
        if plain {
            self.measured.insert(key, r);
        }
        r
    }

    /// Calls `f(x, y, w, h, rgba)` for every glyph pixel run, relative to the text origin.
    /// Returns each laid-out line's extent (for strikethrough).
    /// Rasterize `text` with its origin at `origin` (device px, fractional): `f(x, y, w, h, rgba)` gets
    /// absolute pixels. Glyphs keep their sub-pixel phase in both axes (cosmic-text's own `draw`
    /// truncates the baseline to whole pixels, which CoreText does not).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        origin: (f32, f32),
        spans: &[Span],
        mut f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        let mut b = self.buffer(text, spec, Some(width), spans);
        b.shape_until_scroll(&mut self.sys, false);
        let lines = b
            .layout_runs()
            .flat_map(|r| {
                let mut out: Vec<LineSpan> = vec![];
                for g in r.glyphs {
                    let run = (!spans.is_empty()).then_some(g.metadata);
                    let k = glyph_width_scale(spec, spans, g.metadata);
                    let rise = placed_rise(spec, spans, g.metadata);
                    let x1 = (g.x + g.w) * k;
                    match out.last_mut() {
                        Some(l) if l.run == run => l.x1 = x1,
                        _ => out.push(LineSpan { x0: g.x * k, x1, baseline: r.line_y - rise, run }),
                    }
                }
                out
            })
            .collect();
        let c = Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
        // Body-size text snaps like UIKit (whole-pixel baseline and origin); accessibility sizes keep
        // the sub-pixel phase, which iOS does at large sizes (measured on the scrolled fixtures).
        let frac = spec.pt > 40.0;
        let (ix, iy) = if frac { (origin.0.floor(), origin.1.floor()) } else { (origin.0.round(), origin.1.round()) };
        let (fx, fy) = if frac { (origin.0 - ix, origin.1 - iy) } else { (0.0, 0.0) };
        for run in b.layout_runs() {
            for g in run.glyphs {
                let k = glyph_width_scale(spec, spans, g.metadata);
                // Positive baselineOffset grows the line box (layout) and leaves the glyphs at its top.
                let gy = g.y - g.font_size * g.y_offset + run.line_y - placed_rise(spec, spans, g.metadata);
                let pen = g.x + g.font_size * g.x_offset + fx;
                let (key, gx, gy) = cosmic_text::CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size,
                    (pen, if frac { gy + fy } else { gy.trunc() }),
                    g.font_weight,
                    g.cache_key_flags,
                );
                let by = gy + iy as i32;
                // cosmic-text replaces the color alpha with glyph coverage: reapply the run's own alpha.
                let sa = spans.get(g.metadata).map_or(1.0, |s| s.color.a);
                let paint = g.color_opt.unwrap_or(c);
                if (k - 1.0).abs() < 0.001 {
                    let bx = gx + ix as i32;
                    self.cache.with_pixels(&mut self.sys, key, paint, |px, py, col| {
                        f(bx + px, by + py, 1, 1, [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8])
                    });
                } else {
                    let bx = (ix + pen * k).round() as i32;
                    let mut src = Vec::new();
                    self.cache.with_pixels(&mut self.sys, key, paint, |px, py, col| {
                        src.push((px, py, [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8]));
                    });
                    let mut acc: HashMap<(i32, i32), [u8; 4]> = HashMap::new();
                    for (px, py, rgba) in src {
                        let dx = (px as f32 * k).round() as i32;
                        acc.entry((dx, py)).and_modify(|e| { if rgba[3] > e[3] { *e = rgba; } }).or_insert(rgba);
                    }
                    for ((dx, dy), rgba) in acc {
                        f(bx + dx, by + dy, 1, 1, rgba);
                    }
                }
            }
        }
        lines
    }
}

/// Positive part of the offsets (extra height below the glyphs) and negative part (glyphs move down).
pub(crate) fn baseline_extent(spec: &FontSpec, spans: &[Span]) -> (f32, f32) {
    let mut hi = spec.baseline;
    let mut lo = spec.baseline;
    for s in spans {
        hi = hi.max(s.baseline);
        lo = lo.min(s.baseline);
    }
    (hi.max(0.0), lo.min(0.0))
}

fn glyph_width_scale(spec: &FontSpec, spans: &[Span], meta: usize) -> f32 {
    let w = if spans.is_empty() { spec.width } else { spans.get(meta).map(|s| s.width).unwrap_or(spec.width) };
    width_scale(w)
}

/// Pixels to move this glyph up. A positive offset is consumed by the taller line box, so the
/// glyphs stay at the top and a center-aligned sibling sits lower.
fn placed_rise(spec: &FontSpec, spans: &[Span], meta: usize) -> f32 {
    let here = if spans.is_empty() { spec.baseline } else { spans.get(meta).map(|s| s.baseline).unwrap_or(spec.baseline) };
    let (rise, _) = baseline_extent(spec, spans);
    (here - rise) * (spec.size / spec.pt.max(1.0))
}

fn extra_em(points: f32, pt: f32) -> f32 {
    if pt > 0.0 { points / pt } else { 0.0 }
}

fn tabular() -> FontFeatures {
    let mut feats = FontFeatures::new();
    feats.enable(FeatureTag::new(b"tnum"));
    feats
}

fn fold_case(text: &str, case_fold: u8) -> std::borrow::Cow<'_, str> {
    match case_fold {
        1 => std::borrow::Cow::Owned(text.to_uppercase()),
        2 => std::borrow::Cow::Owned(text.to_lowercase()),
        _ => std::borrow::Cow::Borrowed(text),
    }
}

fn take_range(spans: &[Span], start: usize, end: usize) -> Vec<Span> {
    let mut at = 0;
    let mut out = Vec::new();
    for sp in spans {
        let a = at;
        let b = at + sp.text.len();
        at = b;
        if b <= start || a >= end {
            continue;
        }
        let lo = start.max(a) - a;
        let hi = end.min(b) - a;
        if sp.text.is_char_boundary(lo) && sp.text.is_char_boundary(hi) {
            out.push(Span { text: sp.text[lo..hi].to_string(), ..sp.clone() });
        }
    }
    out
}

/// Map a truncated string (a slice of `text` plus one ellipsis) back onto `spans`.
fn slice_spans(spans: &[Span], text: &str, shown: &str) -> Vec<Span> {
    if spans.is_empty() {
        return vec![];
    }
    let ell = '\u{2026}';
    if let Some(pre) = shown.strip_suffix(ell) {
        if text.starts_with(pre) {
            let mut cut = take_range(spans, 0, pre.len());
            if let Some(last) = cut.last_mut() {
                last.text.push(ell);
            }
            return cut;
        }
    }
    if let Some(suf) = shown.strip_prefix(ell) {
        if text.ends_with(suf) {
            let start = text.len() - suf.len();
            let mut cut = take_range(spans, start, text.len());
            if let Some(first) = cut.first_mut() {
                first.text.insert(0, ell);
            }
            return cut;
        }
    }
    if let Some((pre, suf)) = shown.split_once(ell) {
        if text.starts_with(pre) && text.ends_with(suf) {
            let mut left = take_range(spans, 0, pre.len());
            let right = take_range(spans, text.len() - suf.len(), text.len());
            if let Some(last) = left.last_mut() {
                last.text.push(ell);
            }
            left.extend(right);
            return left;
        }
    }
    spans.to_vec()
}

#[cfg(test)]
mod span_tests {
    use super::*;

    #[test]
    fn inter_paragraph_wraps_at_the_same_word_at_both_scales() {
        let mut fonts = Fonts::with_fallback(true);
        let text = "Line height 26: the quick brown fox jumps over the lazy dog and keeps running far away.";
        let logical = FontSpec { inter_point_tracking: true, ..FontSpec::new(15.0, 26.0, 400, TextAlign::Leading) };
        let layout = fonts.build(text, &logical, Some(370.0), &[]);
        let first_line = |b: &Buffer| {
            let run = b.layout_runs().next().unwrap();
            text[..run.glyphs.last().unwrap().end].trim_end()
        };
        assert_eq!(first_line(&layout), "Line height 26: the quick brown fox jumps over the");
        for scale in [2.0, 3.0] {
            let physical = FontSpec { size: logical.size * scale, pitch: logical.pitch * scale, ..logical };
            let draw = fonts.build(text, &physical, Some(370.0 * scale), &[]);
            assert_eq!(first_line(&draw), first_line(&layout));
        }
    }

    /// Runs scale their line pitch with their size unless the paragraph fixes it.
    #[test]
    fn run_pitch_rule() {
        let mut f = Fonts::new();
        let red = crate::scene::Color::rgba(1.0, 0.0, 0.0, 1.0);
        let run = |pt| Span { pt, ..Span::basic("Wrapped text of two sizes that must wrap onto several lines", pt, 400, red) };
        let spec = FontSpec::new(20.0, 24.0, 400, TextAlign::Leading);
        let h = |f: &mut Fonts, fixed| {
            let s = FontSpec { fixed_pitch: fixed, ..spec };
            let b = f.buffer("Wrapped text of two sizes that must wrap onto several lines", &s, Some(60.0), &[run(10.0)]);
            b.layout_runs().map(|r| r.line_y).fold(0.0f32, f32::max)
        };
        assert!(h(&mut f, false) < h(&mut f, true), "scaled runs pack tighter than a fixed pitch");
    }

    /// A single full-text run (with a line cap) shapes with the same per-size tracking as plain text.
    #[test]
    fn span_path_matches_plain_widths() {
        let mut f = Fonts::new();
        let t = "Plain red blue bold green text.";
        for size in [15.0f32, 17.0] {
            let spec = FontSpec::new(size, size * 1.2, 400, TextAlign::Leading);
            let plain = f.measure(t, &spec, None, &[]).0;
            let run = Span::basic(t, size, 400, crate::scene::Color::rgba(1.0, 0.0, 0.0, 1.0));
            let capped = FontSpec { lines: Some(3), ..spec };
            let spanned = f.measure(t, &capped, Some(10000.0), &[run]).0;
            assert!((spanned - plain).abs() < 0.01, "{size}: spans {spanned} vs plain {plain}");
        }
    }

    #[test]
    fn extra_tracking_widens_a_word() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let plain = fonts.measure("Tracking", &spec, None, &[]).0;
        let wide = fonts.measure("Tracking", &FontSpec { extra: 4.0, ..spec }, None, &[]).0;
        assert!(wide > plain + 3.0, "tracked {wide} vs plain {plain}");
    }

    #[test]
    fn head_and_middle_truncation_keep_their_ends() {
        let mut fonts = Fonts::new();
        let text = "The quick brown fox jumps over the lazy dog";
        let head = FontSpec { lines: Some(1), trunc: 1, ..FontSpec::new(17.0, 22.0, 400, TextAlign::Leading) };
        let shown = fonts.buffer(text, &head, Some(70.0), &[]);
        let glyphs = shown.layout_runs().flat_map(|r| r.glyphs.iter().map(|g| g.end)).max().unwrap_or(0);
        assert!(glyphs > 0);
        let h = fonts.truncated_string(text, &head, 70.0);
        assert!(h.starts_with('\u{2026}'), "{h}");
        assert!(text.ends_with(h.trim_start_matches('\u{2026}')), "{h}");
        let mid = FontSpec { trunc: 2, ..head };
        let m = fonts.truncated_string(text, &mid, 90.0);
        assert!(m.contains('\u{2026}'), "{m}");
        assert!(text.starts_with(m.split('\u{2026}').next().unwrap()), "{m}");
        assert!(text.ends_with(m.split('\u{2026}').next_back().unwrap()), "{m}");
    }

    #[test]
    fn uppercase_case_fold_changes_the_shaped_text() {
        let mut fonts = Fonts::new();
        let spec = FontSpec { case_fold: 1, ..FontSpec::new(17.0, 22.0, 400, TextAlign::Leading) };
        let lower = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let (up, _) = fonts.measure("width", &spec, None, &[]);
        let (lo, _) = fonts.measure("width", &lower, None, &[]);
        assert!((up - lo).abs() > 0.2, "uppercase {up} vs lower {lo}");
    }

    #[test]
    fn baseline_offset_does_not_change_the_measure() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let plain = fonts.measure("Offset", &spec, None, &[]);
        let raised = fonts.measure("Offset", &FontSpec { baseline: 8.0, ..spec }, None, &[]);
        assert_eq!(plain, raised);
    }

    #[test]
    fn monospaced_equalizes_wide_and_narrow_letters() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let width = |fonts: &mut Fonts, text, mono| fonts.measure(text, &FontSpec { mono, ..spec }, None, &[]).0;
        let prop = (width(&mut fonts, "WWWWWWWW", false) - width(&mut fonts, "iiiiiiii", false)).abs();
        let mono = (width(&mut fonts, "WWWWWWWW", true) - width(&mut fonts, "iiiiiiii", true)).abs();
        assert!(prop > 8.0, "proportional gap {prop}");
        assert!(mono < prop * 0.25, "mono gap {mono} vs proportional {prop}");
    }

    /// Tabular digits shrink the gap between 0 and 1 when the proportional face has one.
    #[test]
    fn tabular_digits_narrow_the_digit_gap() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let width = |fonts: &mut Fonts, text, digits| fonts.measure(text, &FontSpec { digits, ..spec }, None, &[]).0;
        let prop = (width(&mut fonts, "00000000", false) - width(&mut fonts, "11111111", false)).abs();
        let tab = (width(&mut fonts, "00000000", true) - width(&mut fonts, "11111111", true)).abs();
        if prop > 0.4 {
            assert!(tab < prop * 0.5, "tabular gap {tab} vs proportional {prop}");
        }
    }

    #[test]
    fn design_and_width_select_a_different_measure() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let w = |fonts: &mut Fonts, design: u8, width: i8| fonts.measure("Typography", &FontSpec { design, width, ..spec }, None, &[]).0;
        let plain = w(&mut fonts, 0, 0);
        let serif = w(&mut fonts, 1, 0);
        let rounded = w(&mut fonts, 2, 0);
        let mono = w(&mut fonts, 3, 0);
        let condensed = w(&mut fonts, 0, -1);
        let compressed = w(&mut fonts, 0, -2);
        assert!((mono - plain).abs() > 0.5, "monospaced design {mono} vs {plain}");
        // Condensed and compressed are horizontal scales of the standard face.
        assert!((condensed / plain - 0.80).abs() < 0.02, "condensed {condensed} vs {plain}");
        assert!((compressed - plain).abs() > 0.5, "compressed {compressed} vs {plain}");
        if fonts.is_sf() {
            assert!((serif - plain).abs() > 0.5, "serif {serif} vs {plain}");
            assert!((rounded - plain).abs() > 0.5, "rounded {rounded} vs {plain}");
        }
    }

    /// iOS 26.5, 17pt, 160pt wide: middle keeps "The quic…e lazy dog".
    #[test]
    fn middle_truncation_matches_the_ios_split() {
        let mut fonts = Fonts::new();
        let text = "The quick brown fox jumps over the lazy dog";
        let mid = FontSpec { lines: Some(1), trunc: 2, ..FontSpec::new(17.0, 22.0, 400, TextAlign::Leading) };
        let shown = fonts.truncated_string(text, &mid, 160.0);
        if fonts.is_sf() {
            assert_eq!(shown, "The quic\u{2026}e lazy dog");
            let head = FontSpec { trunc: 1, ..mid };
            assert_eq!(fonts.truncated_string(text, &head, 160.0), "\u{2026}s over the lazy dog");
            let tail = FontSpec { trunc: 0, ..mid };
            assert_eq!(fonts.truncated_string(text, &tail, 160.0), "The quick brown fo\u{2026}");
        } else {
            assert!(shown.contains('\u{2026}'), "{shown}");
        }
    }

    #[test]
    fn condensed_width_scales_the_advance() {
        let mut fonts = Fonts::new();
        let spec = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        let plain = fonts.measure("Condensed", &spec, None, &[]).0;
        let cond = fonts.measure("Condensed", &FontSpec { width: -1, ..spec }, None, &[]).0;
        let ratio = cond / plain;
        assert!((ratio - 0.80).abs() < 0.001, "condensed ratio {ratio} ({cond} vs {plain})");
        let compressed = fonts.measure("Condensed", &FontSpec { width: -2, ..spec }, None, &[]).0;
        assert!((compressed / plain - 0.70).abs() < 0.02, "compressed ratio {}", compressed / plain);
    }
}
