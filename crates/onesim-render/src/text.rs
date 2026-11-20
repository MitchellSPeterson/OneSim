use cosmic_text::{fontdb, Align, Attrs, AttrsList, Buffer, BufferLine, Color, Ellipsize, EllipsizeHeightLimit, Family, FeatureTag, FontFeatures, FontSystem, Hinting, LineEnding, LineIter, Metrics, Shaping, Style, SwashCache, Weight, Wrap};
use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// macOS system UI font (variable SF Pro: wght axis). Family name as registered in fontdb.
const SF_FILE: &str = "/System/Library/Fonts/SFNS.ttf";
const SF_FAMILY: &str = ".SF NS";
const MONO_FAMILY: &str = ".SF NS Mono";
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
    /// Additional Compose span and paragraph properties.
    pub compose: Option<Box<ComposeSpanStyle>>,
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
    /// Compose's line limit applies across paragraphs. Other clients use their existing line-cap rules.
    pub compose_overflow: Option<ComposeOverflow>,
    pub no_wrap: bool,
}

impl FontSpec {
    pub fn new(size: f32, line_height: f32, weight: u16, align: TextAlign) -> Self {
        // Leading: 22/17 of the size up to large-title sizes, ~1.12 x size + 2.64 above (accessibility sizes, measured: 53pt -> 62pt).
        let pitch = if size > 34.0 { size * 1.12 + 2.64 } else { line_height.max(size * 22.0 / 17.0) };
        Self { size, pt: size, line_height, pitch, weight, align, tracking: tracking_em(size, weight), mono: false, lines: None, fixed_pitch: false, inter_point_tracking: false, android: false, compose_overflow: None, no_wrap: false }
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

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    compose_fonts: HashSet<String>,
    cache: SwashCache,
    sf: bool,
    inter: bool,
    has_mono: bool,
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
        Self { sys, compose_fonts: HashSet::new(), cache: SwashCache::new(), sf, inter, has_mono, measured: HashMap::new() }
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

    /// Family, tracking (em) and weight of one face. `sf_tracking` is SF's tracking for this face;
    /// `inter_size` selects Inter's tracking fit; `big` selects the accessibility-size fit.
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

    #[allow(clippy::too_many_arguments)]
    fn face(&self, mono: bool, android: bool, pt: f32, inter_size: f32, weight: u16, sf_tracking: f32, big: bool) -> (Family<'static>, f32, u16) {
        let wght = if big { (weight as i32 + INTER_BIG_WEIGHT).clamp(100, 900) as u16 } else { weight };
        if mono {
            (self.mono_family(), 0.0, wght)
        } else if android {
            (Family::Name(ROBOTO_FAMILY), sf_tracking, weight)
        } else if self.sf {
            (Family::Name(SF_FAMILY), sf_tracking, wght)
        } else {
            (Family::Name(INTER_FAMILY), if big { inter_ax_tracking(pt, weight) } else { inter_tracking_em(inter_size, weight) }, wght)
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
        let (Some(n @ 1..), Some(_), None) = (f.lines, width, f.compose_overflow) else { return b };
        if b.layout_runs().count() <= n {
            return b;
        }
        let Some(run) = b.layout_runs().nth(n - 1) else { return b };
        let Some(first) = run.glyphs.first() else { return b };
        // The last allowed line starts here.
        let start: usize = text.split('\n').take(run.line_i).map(|p| p.len() + 1).sum::<usize>() + first.start;
        let rest = text[start..].split('\n').next().unwrap_or("");
        // Like TextKit: a single line fills to the last character that fits, but with several lines the last
        // one keeps its word-wrapped fragment (`is cut…`), cut only further when the ellipsis does not fit.
        let frag_end = run.glyphs.last().map_or(0, |g| g.end).saturating_sub(first.start);
        let rest = if n > 1 && rest.is_char_boundary(frag_end) { &rest[..frag_end] } else { rest };
        let limit = width.unwrap_or(f32::MAX);
        let one = FontSpec { align: TextAlign::Leading, ..*f };
        let bounds: Vec<usize> = rest.char_indices().map(|(i, _)| i).chain([rest.len()]).collect();
        let (mut lo, mut hi) = (0, bounds.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let probe = self.build(&format!("{}\u{2026}", rest[..bounds[mid]].trim_end()), &one, None, &[]);
            if probe.layout_runs().map(|r| r.line_w).fold(0.0, f32::max) <= limit {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let mut head = text[..start + bounds[lo]].trim_end();
        if n > 1 && bounds[lo] < rest.len() {
            // Multi-line: cut back to a word boundary.
            head = head.rsplit_once(' ').map_or(head, |(h, _)| h.trim_end());
        }
        // The ellipsis takes the style of the last kept run (like iOS).
        let (mut at, mut cut) = (0, Vec::<Span>::new());
        for sp in spans {
            if at >= head.len() {
                break;
            }
            let take = (head.len() - at).min(sp.text.len());
            cut.push(Span { text: sp.text[..take].to_string(), ..sp.clone() });
            at += sp.text.len();
        }
        if let Some(last) = cut.last_mut() {
            last.text.push('\u{2026}');
        }
        self.build(&format!("{head}\u{2026}"), f, width, &cut)
    }

    fn build(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span]) -> Buffer {
        // Inter's glyphs are ~8% larger than SF's at the same size; at accessibility sizes that is a lot
        // of ink, so the fallback draws them slightly smaller (tracking is fitted for it).
        let big = !self.sf && !f.android && f.pt > 40.0;
        let size = if big { f.size * INTER_BIG_SCALE } else { f.size };
        let mut b = Buffer::new(&mut self.sys, Metrics::new(size, f.pitch));
        b.set_size(width, None);
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
        let (family, tracking, wght) = self.face(f.mono, f.android, f.pt, inter_size, f.weight, f.tracking, big);
        let attrs = Attrs::new().family(family).weight(Weight(wght)).letter_spacing(tracking);
        if !spans.is_empty() {
            let sf = self.sf;
            let runs = spans.iter().enumerate().map(|(i, s)| {
                let k = s.pt / f.pt;
                let size_px = f.size * k;
                let big = !sf && !f.android && s.pt > 40.0;
                let sf_tracking = if f.android { f.tracking } else { tracking_em(s.pt, s.weight) };
                let (mut fam, mut tracking, wght) = self.face(f.mono || s.mono, f.android, s.pt, inter_size * k, s.weight, sf_tracking, big);
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
                let a = Attrs::new().family(fam).weight(Weight(wght)).letter_spacing(tracking).metrics(m).color(c).metadata(i);
                let a = if let Some(settings) = s.compose.as_ref().and_then(|style| style.feature_settings.as_deref()) {
                    a.font_features(font_features(settings))
                } else { a };
                (s.text.as_str(), if s.italic { a.style(Style::Italic) } else { a })
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
        // Android text carries its own tracking (Compose letterSpacing) and face; iOS tracking follows size and weight.
        let tracking = if f.android { f.tracking.to_bits() ^ 0x8000_0000 } else { 0 };
        let line_metrics = f.compose_overflow.map_or([0; 2], |_| [f.line_height.to_bits(), f.pitch.to_bits()]);
        let key = (text.to_string(), f.size.to_bits(), f.weight, width.map_or(u32::MAX, f32::to_bits), tracking, line_metrics);
        let plain = spans.is_empty() && f.lines.is_none() && !f.no_wrap;
        if plain {
            if let Some(r) = self.measured.get(&key) {
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
        let r = (w as f64, height as f64);
        if plain {
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
            text: text.into(), pt: 18.0, weight: 400, color: crate::Color::rgba(0.0, 0.0, 0.0, 1.0),
            italic: false, underline: false, strike: false, mono: false, compose: Some(Box::new(ComposeSpanStyle {
                line_break: Some([2, 4, 1]), auto_hyphens: Some(true), locale: Some("en-US".into()), ..Default::default()
            })),
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
            text: text.into(), pt: 18.0, weight: 400, color: crate::Color::rgba(0.0, 0.0, 0.0, 1.0),
            italic: false, underline: false, strike: false, mono: false, compose: Some(Box::new(ComposeSpanStyle {
                transform: Some([1.6, -0.1]), ..Default::default()
            })),
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
            text: "sample".into(), pt: 14.0, weight: 400, color: crate::Color::rgba(0.0, 0.0, 0.0, 1.0),
            italic: false, underline: false, strike: false, mono: false, compose: Some(Box::new(ComposeSpanStyle {
                family: Some(family.into()), ..Default::default()
            })),
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
        let run = |pt| Span { text: "Wrapped text of two sizes that must wrap onto several lines".into(), pt, weight: 400, color: red, italic: false, underline: false, strike: false, mono: false, compose: None };
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
            let run = Span { text: t.into(), pt: size, weight: 400, color: crate::scene::Color::rgba(1.0, 0.0, 0.0, 1.0), italic: false, underline: false, strike: false, mono: false, compose: None };
            let capped = FontSpec { lines: Some(3), ..spec };
            let spanned = f.measure(t, &capped, Some(10000.0), &[run]).0;
            assert!((spanned - plain).abs() < 0.01, "{size}: spans {spanned} vs plain {plain}");
        }
    }
}
