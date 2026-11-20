use cosmic_text::{fontdb, Align, Attrs, CacheKey, LayoutGlyph, Stretch, SwashContent, AttrsList, Buffer, BufferLine, Color, Ellipsize, EllipsizeHeightLimit, Family, FeatureTag, FontFeatures, FontSystem, Hinting, LineEnding, LineIter, Metrics, Shaping, Style, SwashCache, Weight, Wrap};
use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::raster::PaintClip;

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

/// `width` is the visual wrap. Shaping happens before the horizontal scale, so the shaper's limit is `width / k`.
fn shape_limit(width: Option<f32>, width_code: i8) -> Option<f32> {
    let k = width_scale(width_code);
    width.map(|w| if (k - 1.0).abs() >= 0.001 && k > 0.0 { w / k } else { w })
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
    Justify,
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
    /// Index into the spans the caller passed. Truncation drops runs, and glyph metadata still has to find the original one.
    pub(crate) origin: Option<usize>,
    /// Additional Compose span and paragraph properties.
    pub compose: Option<Box<ComposeSpanStyle>>,
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
            origin: None,
            compose: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComposeSpanStyle {
    pub background: Option<crate::scene::Color>,
    pub spacing: Option<(f32, bool)>,
    pub family: Option<String>,
    pub baseline_shift: Option<f32>,
    pub feature_settings: Option<String>,
    pub brush: Option<onesim_protocol::ComposeBrush>,
    pub paint_alpha: Option<f32>,
    pub shadow: Option<onesim_protocol::ComposeTextShadow>,
    pub stroke: Option<onesim_protocol::ComposeStroke>,
    pub transform: Option<[f32; 2]>,
    pub line_break: Option<[u8; 3]>,
    pub auto_hyphens: Option<bool>,
    pub locale: Option<String>,
    pub paragraph: Option<usize>,
    pub paragraph_align: Option<TextAlign>,
    pub paragraph_height: Option<f32>,
    pub paragraph_indent: Option<[f32; 2]>,
    pub bullet: Option<TextBullet>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBullet {
    pub width: f32,
    pub height: f32,
    pub padding: f32,
    pub wire: onesim_protocol::ComposeBullet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeOverflow {
    Clip,
    Ellipsis,
    StartEllipsis,
    MiddleEllipsis,
    Visible,
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
    /// Compose's line limit applies across paragraphs. Other clients use their existing line-cap rules.
    pub compose_overflow: Option<ComposeOverflow>,
    pub no_wrap: bool,
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
            compose_overflow: None,
            no_wrap: false,
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

    pub(crate) fn visible_lines(&self) -> usize {
        if self.compose_overflow.is_some() { self.lines.unwrap_or(usize::MAX) } else { usize::MAX }
    }

    pub(crate) fn drawn_lines(&self) -> usize {
        if self.compose_overflow == Some(ComposeOverflow::Visible) { usize::MAX } else { self.visible_lines() }
    }

    pub(crate) fn scaled(self, scale: f32) -> Self {
        Self { size: self.size * scale, line_height: self.line_height * scale, pitch: self.pitch * scale, ..self }
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
type MeasureKey = (String, u32, u16, u32, u32, [u32; 2]);
const MEASURE_MAX_ENTRIES: usize = 4096;
const MEASURE_MAX_TEXT_BYTES: usize = 1024 * 1024;

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    compose_fonts: HashSet<String>,
    cache: SwashCache,
    sf: bool,
    inter: bool,
    has_mono: bool,
    rounded: Option<&'static str>,
    serif: Option<&'static str>,
    measured: HashMap<MeasureKey, (f64, f64)>,
    measured_text_bytes: usize,
}

/// One drawn text line: x range and baseline, relative to the text origin (device px).
pub struct LineSpan {
    pub x0: f32,
    pub x1: f32,
    pub baseline: f32,
    /// Index of the styled run this extent belongs to (`None` for plain text).
    pub run: Option<usize>,
}

pub(crate) struct PositionedGlyph {
    pub key: CacheKey,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PositionedGlyph {
    fn bounds(&self) -> PaintClip {
        PaintClip::pixels(self.x, self.y, self.width, self.height)
    }
}

fn glyph_positioning(origin: (f32, f32), fractional: bool) -> impl Fn(&LayoutGlyph, f32, f32, f32) -> (CacheKey, i32, i32) {
    let (ix, iy) = if fractional { (origin.0.floor(), origin.1.floor()) } else { (origin.0.round(), origin.1.round()) };
    let (fx, fy) = if fractional { (origin.0 - ix, origin.1 - iy) } else { (0.0, 0.0) };
    move |g, baseline, rise, width_scale| {
        let gy = g.y - g.font_size * g.y_offset + baseline - rise;
        let pen = g.x + g.font_size * g.x_offset + fx;
        let (key, gx, gy) = CacheKey::new(
            g.font_id,
            g.glyph_id,
            g.font_size,
            (pen, if fractional { gy + fy } else { gy.trunc() }),
            g.font_weight,
            g.cache_key_flags,
        );
        let x = if width_scale == 1.0 { gx + ix as i32 } else { (ix + pen * width_scale).round() as i32 };
        (key, x, gy + iy as i32)
    }
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
        Self { sys, compose_fonts: HashSet::new(), cache: SwashCache::new(), sf, inter, has_mono, rounded, serif, measured: HashMap::new(), measured_text_bytes: 0 }
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

    pub(crate) fn register_compose_fonts(&mut self, resources: &serde_json::Value) {
        let Some(families) = resources.as_object() else { return };
        for (family, list) in families {
            if !family.starts_with("onesim:") || self.compose_fonts.contains(family) {
                continue;
            }
            let Some(faces) = list.as_array() else { continue };
            let mut registered = false;
            for face in faces {
                let Ok(face) = serde_json::from_value::<onesim_protocol::ComposeFontResource>(face.clone()) else { continue };
                let Ok(data) = base64::engine::general_purpose::STANDARD.decode(face.data) else { continue };
                let db = self.sys.db_mut();
                let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(data)));
                for id in ids {
                    let Some(mut info) = db.face(id).cloned() else { continue };
                    db.remove_face(id);
                    if info.index != face.index { continue; }
                    info.families = vec![(family.clone(), fontdb::Language::English_UnitedStates)];
                    info.weight = Weight(face.weight);
                    info.style = if face.italic { Style::Italic } else { Style::Normal };
                    db.push_face_info(info);
                    registered = true;
                }
            }
            if registered {
                self.compose_fonts.insert(family.clone());
            }
        }
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

    /// Shaped text; with a line cap (`f.lines`) that the text exceeds, the last allowed line is cut
    /// to whole glyphs followed by `…` (tail truncation, like iOS).
    pub(crate) fn buffer(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> crate::text_layout::TextBuffer {
        let reflow = width.is_some() && !f.no_wrap && matches!(
            f.compose_overflow, Some(ComposeOverflow::Clip | ComposeOverflow::Visible | ComposeOverflow::Ellipsis)
        ) && (text.contains('\u{ad}') || spans.iter().any(|span| {
            crate::line_break::requested(span.compose.as_deref(), &span.text)
        }));
        let raw_spec = if reflow {
            FontSpec { lines: None, compose_overflow: Some(ComposeOverflow::Clip), ..*f }
        } else { *f };
        let raw = self.buffer_raw(text, &raw_spec, width, spans);
        crate::text_layout::TextBuffer::new(raw, f, width, spans, &mut self.sys, reflow)
    }

    fn buffer_raw(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> Buffer {
        if f.compose_overflow.is_none() {
            return self.buffer_platform(text, f, width, spans);
        }
        let mut b = self.build(text, f, width, spans);
        if let (Some(ComposeOverflow::Ellipsis), Some(limit @ 1..)) = (f.compose_overflow, f.lines) {
            if b.layout_runs().count() > limit {
                let last = b.layout_runs().nth(limit - 1).expect("visible line");
                let paragraph = last.line_i;
                let count = b.layout_runs().take(limit).filter(|line| line.line_i == paragraph).count();
                let line = &mut b.lines[paragraph];
                let text = format!("{}\u{2026}", line.text());
                let mut attrs = line.attrs_list().clone();
                let start = line.text().len();
                if start > 0 {
                    let last_style = line.attrs_list().get_span(start - 1);
                    attrs.add_span(start..text.len(), &last_style);
                }
                let metadata = line.metadata();
                line.set_text(text, line.ending(), attrs);
                if let Some(metadata) = metadata { line.set_metadata(metadata); }
                line.layout(&mut self.sys, f.size, indented_width(width, line), if f.no_wrap { Wrap::None } else { Wrap::WordOrGlyph },
                    Ellipsize::End(EllipsizeHeightLimit::Lines(count)), None, 8, Hinting::Disabled);
            }
            return b;
        }
        b
    }

    /// Shaped text. A line cap cuts the last allowed line with `…`: tail (the default), head, or middle.
    fn buffer_platform(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> Buffer {
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
        b.set_size(shape_limit(width, f.width), None);
        if f.no_wrap {
            b.set_wrap(Wrap::None);
        }
        if let Some(lines) = f.lines.or(f.no_wrap.then_some(1)) {
            let limit = EllipsizeHeightLimit::Lines(lines);
            let ellipsize = match f.compose_overflow {
                Some(ComposeOverflow::Ellipsis) => Ellipsize::End(limit),
                Some(ComposeOverflow::StartEllipsis) if lines == 1 => Ellipsize::Start(limit),
                Some(ComposeOverflow::MiddleEllipsis) if lines == 1 => Ellipsize::Middle(limit),
                _ => Ellipsize::None,
            };
            b.set_ellipsize(ellipsize);
        }
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
                let (mut fam, tracking, wght, stretch) = self.face(f.mono || s.mono || s.design == 3, f.android, s.pt, inter_size * k, s.weight, sf_tracking, big, s.design, s.width);
                let mut tracking = tracking + extra_em(s.extra, s.pt);
                let mut pitch = if f.fixed_pitch { f.pitch } else { f.pitch * k };
                if let Some(extra) = &s.compose {
                    if let Some((spacing, em)) = extra.spacing {
                        tracking = if em { spacing } else { spacing / s.pt };
                    }
                    fam = match extra.family.as_deref() {
                        Some("serif") => Family::Serif,
                        Some("monospace") => Family::Monospace,
                        Some("cursive") => Family::Cursive,
                        Some("sans-serif" | "default") => Family::Name(ROBOTO_FAMILY),
                        Some(family) if family.starts_with("onesim:") => Family::Name(family),
                        Some("system:serif") => Family::Serif,
                        Some("system:monospace") => Family::Monospace,
                        Some("system:cursive") => Family::Cursive,
                        Some(family) if family.starts_with("system:") => Family::Name(ROBOTO_FAMILY),
                        _ => fam,
                    };
                    if let Some(height) = extra.paragraph_height.filter(|h| *h > 0.0) {
                        pitch = height.max(s.pt * 1.1719) * (f.size / f.pt);
                    }
                }
                let m = Metrics::new(if big { size_px * INTER_BIG_SCALE } else { size_px }, pitch);
                let c = Color::rgba((s.color.r * 255.0).round() as u8, (s.color.g * 255.0).round() as u8, (s.color.b * 255.0).round() as u8, (s.color.a * 255.0).round() as u8);
                let meta = s.origin.unwrap_or(i);
                let mut a = Attrs::new().family(fam).weight(Weight(wght)).stretch(stretch).letter_spacing(tracking).metrics(m).color(c).metadata(meta);
                if s.digits || f.digits {
                    a = a.font_features(tabular());
                }
                if let Some(settings) = s.compose.as_ref().and_then(|style| style.feature_settings.as_deref()) {
                    a = a.font_features(font_features(settings));
                }
                (s.text.as_str(), if s.italic || f.italic { a.style(Style::Italic) } else { a })
            });
            if f.compose_overflow.is_some() {
                // Compose caret offsets need the original separators. The generic rich-text
                // setter normalizes line endings and can split CRLF into separate paragraphs.
                let styles: Vec<_> = runs.scan(0, |at, (text, attrs)| {
                    let start = *at;
                    *at += text.len();
                    Some((start..*at, attrs))
                }).collect();
                let paragraph_edges = styles.windows(2).zip(spans.windows(2)).filter_map(|(wire, spans)| {
                    let paragraph = |span: &Span| span.compose.as_ref().and_then(|style| style.paragraph);
                    (paragraph(&spans[0]) != paragraph(&spans[1])).then_some(wire[1].0.start)
                }).collect::<Vec<_>>();
                b.lines = LineIter::new(text).flat_map(|(range, ending)| {
                    let internal = paragraph_edges.iter().copied().filter(move |offset| range.start < *offset && *offset < range.end);
                    let mut start = range.start;
                    internal.chain([range.end]).map(move |end| {
                        let line = (start..end, if end == range.end { ending } else { LineEnding::None });
                        start = end;
                        line
                    })
                }).map(|(range, ending)| {
                    let mut line_attrs = AttrsList::new(&attrs);
                    let first = styles.partition_point(|(span, _)| span.end <= range.start);
                    for (span, style) in styles[first..].iter().take_while(|(span, _)| span.start < range.end) {
                        let start = span.start.max(range.start) - range.start;
                        let end = span.end.min(range.end) - range.start;
                        line_attrs.add_span(start..end, style);
                    }
                    let align = spans.get(first).and_then(|span| span.compose.as_ref()?.paragraph_align).unwrap_or(f.align);
                    let mut line = BufferLine::new(text[range.clone()].to_owned(), ending, line_attrs, Shaping::Advanced);
                    line.set_align(buffer_align(align));
                    let extra = spans.get(first).and_then(|span| span.compose.as_ref());
                    let mut indent = extra.and_then(|style| style.paragraph_indent).unwrap_or([0.0; 2]);
                    if let Some(bullet) = extra.and_then(|style| style.bullet.as_ref())
                        .filter(|_| styles.get(first).is_some_and(|(span, _)| span.start == range.start))
                    {
                        // Android reserves enough leading margin even without an explicit TextIndent.
                        let additional = (bullet.width + bullet.padding - indent[0].max(0.0)).max(0.0);
                        indent[0] += additional;
                        indent[1] += additional;
                    }
                    (line, indent)
                }).flat_map(|(line, indent)| self.indent_lines(line, f, width, indent).into_iter().flatten()).collect();
                if b.lines.last().is_none_or(|line| line.ending() != LineEnding::None) {
                    b.lines.push(BufferLine::new(String::new(), LineEnding::None, AttrsList::new(&attrs), Shaping::Advanced));
                }
                // These paragraphs were assigned directly. Lay out unshaped lines without
                // invalidating the cached per-paragraph widths used by indented lines.
                for i in 0..b.lines.len() {
                    b.line_layout(&mut self.sys, i);
                }
                return b;
            } else {
                b.set_rich_text(runs, &attrs, Shaping::Advanced, None);
            }
        } else {
            b.set_text(text, &attrs, Shaping::Advanced, None);
        }
        for line in &mut b.lines {
            line.set_align(buffer_align(f.align));
        }
        b.shape_until_scroll(&mut self.sys, false);
        b
    }

    fn indent_lines(
        &mut self,
        mut line: BufferLine,
        spec: &FontSpec,
        width: Option<f32>,
        indents: [f32; 2],
    ) -> [Option<BufferLine>; 2] {
        let scale = spec.size / spec.pt;
        let [first, rest] = indents.map(|indent| (indent * scale - 1e-4).ceil());
        if first == 0.0 && rest == 0.0 {
            return [Some(line), None];
        }
        let wrap = if spec.no_wrap { Wrap::None } else { Wrap::WordOrGlyph };
        let ellipsis = match (spec.compose_overflow, spec.lines.or(spec.no_wrap.then_some(1))) {
            (Some(ComposeOverflow::Ellipsis), Some(lines)) => Ellipsize::End(EllipsizeHeightLimit::Lines(lines)),
            (Some(ComposeOverflow::StartEllipsis), Some(1)) => Ellipsize::Start(EllipsizeHeightLimit::Lines(1)),
            (Some(ComposeOverflow::MiddleEllipsis), Some(1)) => Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)),
            _ => Ellipsize::None,
        };
        let mut layout = |line: &mut BufferLine, inset: f32, wrap, ellipsis| {
            line.set_metadata(inset.to_bits() as usize);
            let width = indented_width(width, line);
            line.layout(&mut self.sys, spec.size, width, wrap, ellipsis, None, 8, Hinting::Disabled)
                .get(1).and_then(|next| next.glyphs.iter().map(|glyph| glyph.start).min())
        };
        let split = layout(&mut line, first, wrap, if first == rest { ellipsis } else { Ellipsize::None });
        let tail = split.filter(|split| first != rest && *split > 0).map(|split| {
            let mut tail = line.split_off(split);
            layout(&mut line, first, Wrap::None, ellipsis);
            layout(&mut tail, rest, wrap, ellipsis);
            tail
        });
        if first != rest && tail.is_none() && ellipsis != Ellipsize::None {
            line.reset_layout();
            layout(&mut line, first, wrap, ellipsis);
        }
        [Some(line), tail]
    }

    /// Inspect the full, untruncated layout, including paragraphs and placeholders.
    pub(crate) fn compose_extent(&mut self, run: &crate::TextRun, width: Option<f32>) -> (f32, f32, usize) {
        let spec = FontSpec { lines: None, compose_overflow: Some(ComposeOverflow::Clip), ..run.font };
        let buffer = self.buffer(&run.text, &spec, width, &run.spans);
        buffer.layout_runs().fold((0.0f32, 0.0, 0), |(width, _, count), line| (
            width.max(line.line_w + line_indent(&buffer.lines[line.line_i])),
            line.line_top + line.line_height,
            count + 1,
        ))
    }

    pub(crate) fn compose_fits(&mut self, run: &crate::TextRun, width: Option<f32>, height: f64) -> bool {
        let (text_width, text_height, lines) = self.compose_extent(run, width);
        lines <= run.font.visible_lines()
            && width.is_none_or(|width| text_width <= width + 1e-4)
            && f64::from(text_height) <= height + 1e-4
    }

    /// Size of `text`, wrapping at `width` when given.
    pub fn measure(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> (f64, f64) {
        // Styled advances vary independently of the default cache key.
        let styled = f.extra != 0.0 || f.design != 0 || f.width != 0 || f.digits || f.trunc != 0 || f.case_fold != 0 || f.italic || f.mono;
        let key = (spans.is_empty() && f.lines.is_none() && !f.no_wrap && !styled && text.len() <= MEASURE_MAX_TEXT_BYTES).then(|| {
            // Android text carries its own tracking (Compose letterSpacing) and face; iOS tracking follows size and weight.
            let tracking = if f.android { f.tracking.to_bits() ^ 0x8000_0000 } else { 0 };
            (text.to_string(), f.size.to_bits(), f.weight, width.map_or(u32::MAX, f32::to_bits), tracking, f.compose_overflow.map_or([0; 2], |_| [f.line_height.to_bits(), f.pitch.to_bits()]))
        });
        if let Some(key) = &key {
            if let Some(r) = self.measured.get(key) {
                return *r;
            }
        }
        let b = self.buffer(text, &FontSpec { align: TextAlign::Leading, ..*f }, width, spans);
        let (mut w, mut lines, mut height) = (0f32, 0usize, 0f32);
        for run in b.layout_runs().take(f.visible_lines()) {
            w = w.max(run.line_w + line_indent(&b.lines[run.line_i]));
            lines += 1;
            height = run.line_top + run.line_height;
        }
        let height = if f.compose_overflow.is_some() && lines > 0 { height } else {
            f.line_height + (lines.max(1) - 1) as f32 * f.pitch
        };
        let r = (w as f64 * f64::from(width_scale(f.width)), f64::from(height));
        if let Some(key) = key {
            if self.measured.len() >= MEASURE_MAX_ENTRIES || self.measured_text_bytes + text.len() > MEASURE_MAX_TEXT_BYTES {
                self.measured.clear();
                self.measured_text_bytes = 0;
            }
            self.measured_text_bytes += text.len();
            self.measured.insert(key, r);
        }
        r
    }

    pub(crate) fn compose_intrinsic_width(&mut self, text: &crate::TextRun, minimum: bool) -> f32 {
        let spec = FontSpec { lines: None, no_wrap: true, ..text.font };
        let buffer = self.buffer(&text.text, &spec, None, &text.spans);
        buffer.layout_runs().map(|line| {
            let width = if minimum {
                let mut start = 0;
                crate::line_break::opportunities(line.text, 3, false, None).into_iter().map(|end| {
                    let visible = start + line.text[start..end].trim_end_matches(char::is_whitespace).len();
                    let width = line.glyphs.iter().filter(|g| g.start >= start && g.start < visible).map(|g| g.w).sum::<f32>();
                    start = end;
                    width
                }).fold(0.0, f32::max)
            } else {
                line.line_w
            };
            width + line_indent(&buffer.lines[line.line_i])
        }).fold(0.0, f32::max)
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
        f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        self.draw_if_visible(text, spec, width, rgba, origin, spans, None, f).unwrap_or_default()
    }

    /// Visible mask glyphs that can be composited individually. Overlap needs a run sprite so the
    /// masks combine before the executor applies the ancestor clip.
    pub(crate) fn mask_glyphs(&mut self, text: &str, spec: &FontSpec, width: f32, origin: (f32, f32), clip: PaintClip) -> Option<Vec<PositionedGlyph>> {
        if spec.underline || width_scale(spec.width) != 1.0 {
            return None;
        }
        let b = self.buffer(text, spec, Some(width), &[]);
        let positioned = glyph_positioning(origin, spec.pt > 40.0);
        let rise = placed_rise(spec, &[], 0);
        let mut glyphs: Vec<PositionedGlyph> = Vec::new();
        for run in b.layout_runs() {
            for g in run.glyphs {
                let (key, x, y) = positioned(g, run.line_y, rise, 1.0);
                let Some(image) = self.cache.get_image(&mut self.sys, key) else { continue };
                let p = image.placement;
                let glyph = PositionedGlyph { key, x: x + p.left, y: y - p.top, width: p.width, height: p.height };
                let bounds = glyph.bounds();
                if p.width == 0 || p.height == 0 || !clip.overlaps(bounds) {
                    continue;
                }
                if image.content != SwashContent::Mask || glyphs.iter().any(|prior| bounds.overlaps(prior.bounds())) {
                    return None;
                }
                glyphs.push(glyph);
            }
        }
        Some(glyphs)
    }

    /// Called with a key validated by `mask_glyphs`, against the same Swash cache.
    pub(crate) fn glyph_mask(&mut self, key: CacheKey) -> &[u8] {
        &self.cache.get_image(&mut self.sys, key).as_ref().expect("known glyph mask").data
    }

    /// For undecorated text, test the glyph images before emitting pixels. Sprite callers
    /// need the full run whenever any glyph can paint inside the clip.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_if_visible(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        origin: (f32, f32),
        spans: &[Span],
        clip: Option<crate::raster::PaintClip>,
        mut f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Result<Vec<LineSpan>, crate::raster::PaintClip> {
        let mut b = self.buffer_platform(text, spec, Some(width), spans);
        b.shape_until_scroll(&mut self.sys, false);
        // Body-size text snaps like UIKit (whole-pixel baseline and origin); accessibility sizes keep
        // the sub-pixel phase, which iOS does at large sizes (measured on the scrolled fixtures).
        let positioned = glyph_positioning(origin, spec.pt > 40.0);
        if let Some(clip) = clip {
            let mut ink: Option<crate::raster::PaintClip> = None;
            let visible = b.layout_runs().any(|run| {
                run.glyphs.iter().any(|g| {
                    let k = glyph_width_scale(spec, spans, g.metadata);
                    let (key, x, y) = positioned(g, run.line_y, placed_rise(spec, spans, g.metadata), k);
                    self.cache.get_image(&mut self.sys, key).as_ref().is_some_and(|image| {
                        let p = image.placement;
                        if p.width == 0 || p.height == 0 {
                            return false;
                        }
                        let left = scale_pixel(p.left, k);
                        let right = scale_pixel(p.left + p.width as i32 - 1, k);
                        let glyph = PaintClip::pixels(x + left, y - p.top, (right - left + 1) as u32, p.height);
                        ink = Some(ink.map_or(glyph, |bounds| bounds.union(glyph)));
                        clip.overlaps(glyph)
                    })
                })
            });
            if !visible {
                return ink.map_or_else(|| Ok(Vec::new()), Err);
            }
        }
        let lines = b
            .layout_runs()
            .take(spec.drawn_lines())
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
        for run in b.layout_runs() {
            for g in run.glyphs {
                let k = glyph_width_scale(spec, spans, g.metadata);
                let (key, bx, by) = positioned(g, run.line_y, placed_rise(spec, spans, g.metadata), k);
                // cosmic-text replaces the color alpha with glyph coverage: reapply the run's own alpha.
                let sa = spans.get(g.metadata).map_or(1.0, |s| s.color.a);
                let paint = g.color_opt.unwrap_or(c);
                if k == 1.0 {
                    self.cache.with_pixels(&mut self.sys, key, paint, |px, py, col| {
                        f(bx + px, by + py, 1, 1, [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8])
                    });
                } else {
                    let mut src = Vec::new();
                    self.cache.with_pixels(&mut self.sys, key, paint, |px, py, col| {
                        src.push((px, py, [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8]));
                    });
                    let mut acc: HashMap<(i32, i32), [u8; 4]> = HashMap::new();
                    for (px, py, rgba) in src {
                        let dx = scale_pixel(px, k);
                        acc.entry((dx, py)).and_modify(|e| { if rgba[3] > e[3] { *e = rgba; } }).or_insert(rgba);
                    }
                    for ((dx, dy), rgba) in acc {
                        f(bx + dx, by + dy, 1, 1, rgba);
                    }
                }
            }
        }
        Ok(lines)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_compose(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        origin: (f32, f32),
        spans: &[Span],
        mut f: impl FnMut(i32, i32, usize, [u8; 4]),
    ) -> Vec<LineSpan> {
        let b = self.buffer(text, spec, Some(width), spans);
        let lines = b
            .layout_runs()
            .take(spec.drawn_lines())
            .flat_map(|r| {
                let mut out: Vec<LineSpan> = vec![];
                let inset = line_x(&b, &r);
                for g in r.glyphs {
                    let run = (!spans.is_empty()).then_some(g.metadata);
                    match out.last_mut() {
                        Some(l) if l.run == run => l.x1 = inset + g.x + g.w,
                        _ => out.push(LineSpan { x0: inset + g.x, x1: inset + g.x + g.w, baseline: r.line_y, run }),
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
        let (mut paragraph, mut marker_effect) = (None, None);
        for run in b.layout_runs().take(spec.drawn_lines()) {
            if paragraph != Some(run.line_i) {
                paragraph = Some(run.line_i);
                // Android's leading-margin span restores the Paint style but retains its path
                // effect for the rest of the paragraph. A dash alone ignores filled glyphs.
                marker_effect = run.glyphs.iter().find_map(|g| {
                    let bullet = spans.get(g.metadata)?.compose.as_ref()?.bullet.as_ref()?;
                    bullet.wire.stroke_style.as_ref()?.path_effect.as_ref()
                }).filter(|effect| crate::compose_path_effect::affects_fill(effect));
            }
            let inset = line_x(&b, &run);
            for g in run.glyphs {
                let shift = spans.get(g.metadata).and_then(|s| s.compose.as_ref()?.baseline_shift).unwrap_or(0.0);
                let gy = g.y - g.font_size * (g.y_offset + shift) + run.line_y;
                let (key, gx, gy) = cosmic_text::CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size,
                    (inset + g.x + g.font_size * g.x_offset + fx, if frac { gy + fy } else { gy.trunc() }),
                    g.font_weight,
                    g.cache_key_flags,
                );
                let (bx, by) = (gx + ix as i32, gy + iy as i32);
                // cosmic-text replaces the color alpha with glyph coverage: reapply the run's own alpha.
                let sa = spans.get(g.metadata).map_or(1.0, |span| {
                    let brush = span.compose.as_ref().and_then(|style| style.brush.as_ref());
                    if brush.is_some() { 1.0 } else { span.color.a }
                });
                let style = spans.get(g.metadata).and_then(|span| span.compose.as_ref());
                let transform = style.and_then(|style| style.transform).filter(|transform| *transform != [1.0, 0.0]);
                let stroke = style.and_then(|style| style.stroke.as_ref());
                let mut emit = |x, y, color| f(bx + x, by + y, g.metadata, color);
                let effect = stroke.map_or(marker_effect, |stroke| stroke.path_effect.as_ref());
                let commands = if transform.is_some() || stroke.is_some() || effect.is_some() {
                    self.cache.get_outline_commands(&mut self.sys, key)
                } else {
                    None
                };
                if let Some(commands) = commands {
                    let ink = g.color_opt.unwrap_or(c);
                    let col = crate::Color::rgba(f32::from(ink.r()) / 255.0, f32::from(ink.g()) / 255.0, f32::from(ink.b()) / 255.0, sa);
                    crate::compose_paint::raster_outline(commands, transform.unwrap_or([1.0, 0.0]), stroke, effect, col, &mut emit);
                    continue;
                }
                let mut ink = Vec::new();
                self.cache.with_pixels(&mut self.sys, key, g.color_opt.unwrap_or(c), |px, py, col| {
                    let color = [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8];
                    if transform.is_some() { ink.push((px, py, color)); } else { emit(px, py, color); }
                });
                if let Some(transform) = transform {
                    crate::compose_paint::transform_glyph(&ink, transform, emit);
                }
            }
        }
        lines
    }
}

fn font_features(settings: &str) -> FontFeatures {
    let mut features = FontFeatures::new();
    for setting in settings.split(',').map(str::trim) {
        let Some(quote @ ('\'' | '"')) = setting.chars().next() else { continue };
        let Some((tag, value)) = setting[1..].split_once(quote) else { continue };
        let Ok(tag) = <&[u8; 4]>::try_from(tag.as_bytes()) else { continue };
        let value = match value.trim() {
            "" | "on" => 1,
            "off" => 0,
            value => match value.parse() { Ok(value) => value, Err(_) => continue },
        };
        features.set(FeatureTag::new(tag), value);
    }
    features
}

pub(crate) fn line_indent(line: &BufferLine) -> f32 {
    line.metadata().map_or(0.0, |bits| f32::from_bits(bits as u32))
}

pub(crate) fn line_x(buffer: &Buffer, run: &cosmic_text::LayoutRun<'_>) -> f32 {
    if run.rtl { 0.0 } else { line_indent(&buffer.lines[run.line_i]) }
}

fn indented_width(width: Option<f32>, line: &BufferLine) -> Option<f32> {
    width.map(|width| (width - line_indent(line)).max(0.0))
}

fn buffer_align(align: TextAlign) -> Option<Align> {
    match align {
        TextAlign::Leading => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::Trailing => Some(Align::Right),
        TextAlign::Justify => Some(Align::Justified),
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

fn scale_pixel(x: i32, k: f32) -> i32 {
    if k == 1.0 { x } else { (x as f32 * k).round() as i32 }
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
    for (i, sp) in spans.iter().enumerate() {
        let a = at;
        let b = at + sp.text.len();
        at = b;
        if b <= start || a >= end {
            continue;
        }
        let lo = start.max(a) - a;
        let hi = end.min(b) - a;
        if sp.text.is_char_boundary(lo) && sp.text.is_char_boundary(hi) {
            out.push(Span { text: sp.text[lo..hi].to_string(), origin: Some(sp.origin.unwrap_or(i)), ..sp.clone() });
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
    fn automatic_hyphenation_keeps_discretionary_ink_out_of_source_offsets() {
        let mut fonts = Fonts::with_fallback(true);
        let text = "Extraordinary typography";
        let mut spec = FontSpec::new(18.0, 22.0, 400, TextAlign::Leading);
        spec.android = true;
        spec.compose_overflow = Some(ComposeOverflow::Clip);
        let span = Span {
            compose: Some(Box::new(ComposeSpanStyle {
                line_break: Some([2, 4, 1]), auto_hyphens: Some(true), locale: Some("en-US".into()), ..Default::default()
            })),
            ..Span::basic(text, 18.0, 400, crate::Color::rgba(0.0, 0.0, 0.0, 1.0))
        };
        let layout = fonts.buffer(text, &spec, Some(112.0), std::slice::from_ref(&span));
        let runs: Vec<_> = layout.layout_runs().collect();
        assert!(runs.len() >= 2);
        assert_eq!(runs[0].text, text);
        let discretionary = runs[0].glyphs.iter().find(|g| g.start == g.end).expect("hyphenated first line");
        assert!(discretionary.start > 0 && discretionary.start < "Extraordinary".len());
        assert_eq!(runs[1].glyphs.iter().map(|g| g.start).min(), Some(discretionary.start));
    }

    #[test]
    fn horizontal_scale_updates_measurement_and_character_hit_testing() {
        let mut fonts = Fonts::with_fallback(true);
        let mut spec = FontSpec::new(18.0, 22.0, 400, TextAlign::Leading);
        spec.android = true;
        spec.compose_overflow = Some(ComposeOverflow::Clip);
        let text = "WW";
        let span = Span {
            compose: Some(Box::new(ComposeSpanStyle {
                transform: Some([1.6, -0.1]), ..Default::default()
            })),
            ..Span::basic(text, 18.0, 400, crate::Color::rgba(0.0, 0.0, 0.0, 1.0))
        };
        let original = fonts.measure(text, &spec, None, &[]).0;
        let layout = fonts.buffer(text, &spec, None, std::slice::from_ref(&span));
        let first = layout.layout_runs().next().unwrap();
        assert!((f64::from(first.line_w) - original * 1.6).abs() < 0.25);
        assert_eq!(layout.hit(first.glyphs[1].x + 0.5, first.line_y).unwrap().index, 1);
    }

    #[test]
    fn compose_uses_the_registered_face_for_a_bundled_font() {
        let mut fonts = Fonts::with_fallback(true);
        let family = "onesim:sample";
        let data = base64::engine::general_purpose::STANDARD.encode(INTER);
        fonts.register_compose_fonts(&serde_json::json!({family: [{"data":data,"weight":400,"italic":false}]}));
        let mut spec = FontSpec::new(14.0, 17.0, 400, TextAlign::Leading);
        spec.compose_overflow = Some(ComposeOverflow::Clip);
        spec.android = true;
        let span = Span {
            compose: Some(Box::new(ComposeSpanStyle {
                family: Some(family.into()), ..Default::default()
            })),
            ..Span::basic("sample", 14.0, 400, crate::Color::rgba(0.0, 0.0, 0.0, 1.0))
        };
        let buffer = fonts.buffer(&span.text, &spec, None, std::slice::from_ref(&span));
        let id = buffer.layout_runs().next().unwrap().glyphs[0].font_id;
        assert_eq!(fonts.sys.db().face(id).unwrap().families[0].0, family);
    }

    #[test]
    fn compose_end_ellipsis_marks_a_truncated_paragraph() {
        let mut fonts = Fonts::with_fallback(true);
        let mut spec = FontSpec::new(14.0, 17.0, 400, TextAlign::Leading);
        spec.android = true;
        spec.lines = Some(1);
        spec.compose_overflow = Some(ComposeOverflow::Ellipsis);
        let buffer = fonts.buffer("first\nsecond", &spec, Some(150.0), &[]);
        let first = buffer.layout_runs().next().unwrap();
        assert_eq!(first.text, "first…");
    }

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

    /// A condensed phrase measured with no wrap must stay one line when drawn at that measured width.
    #[test]
    fn condensed_wrap_uses_the_visual_width() {
        let mut fonts = Fonts::new();
        let text = "hello world again";
        let spec = FontSpec { width: -1, ..FontSpec::new(17.0, 22.0, 400, TextAlign::Leading) };
        let (w, h) = fonts.measure(text, &spec, None, &[]);
        let (_, h2) = fonts.measure(text, &spec, Some(w as f32), &[]);
        assert!((h2 - h).abs() < 0.6, "measured width {w} rewrapped: {h} vs {h2}");
    }

    #[test]
    fn ink_culling_keeps_transformed_glyphs() {
        let mut fonts = Fonts::with_fallback(true);
        let plain = FontSpec::new(17.0, 22.0, 400, TextAlign::Leading);
        for (text, spec) in [
            ("                    A", FontSpec { width: -2, ..plain }),
            ("A", FontSpec { baseline: -32.0, ..plain }),
        ] {
            let rgba = [0, 0, 0, 255];
            let mut pixel = None;
            fonts.draw(text, &spec, 300.0, rgba, (0.0, 0.0), &[], |x, y, _, _, c| {
                if c[3] > 0 {
                    pixel = Some((x, y));
                }
            });
            let (x, y) = pixel.expect("ink");
            let clip = PaintClip::pixels(x, y, 1, 1);
            assert!(fonts.draw_if_visible(text, &spec, 300.0, rgba, (0.0, 0.0), &[], Some(clip), |_, _, _, _, _| {}).is_ok());
        }
    }

    /// Head truncation that drops a whole run must keep the surviving run's index.
    #[test]
    fn truncation_keeps_the_original_run_index() {
        let mut fonts = Fonts::new();
        let spec = FontSpec { lines: Some(1), trunc: 1, ..FontSpec::new(17.0, 22.0, 400, TextAlign::Leading) };
        let red = crate::scene::Color::rgba(1.0, 0.0, 0.0, 1.0);
        let blue = crate::scene::Color::rgba(0.0, 0.0, 1.0, 1.0);
        let prefix = "drop this whole prefix ";
        let suffix = "kept tail of the line";
        let spans = [
            Span { underline: false, ..Span::basic(prefix, 17.0, 400, red) },
            Span { underline: true, ..Span::basic(suffix, 17.0, 400, blue) },
        ];
        let text = format!("{prefix}{suffix}");
        let lines = fonts.draw(&text, &spec, 80.0, [0, 0, 0, 255], (0.0, 0.0), &spans, |_, _, _, _, _| {});
        let runs: Vec<_> = lines.iter().map(|l| l.run).collect();
        assert_eq!(runs, vec![Some(1)], "{runs:?}");
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
