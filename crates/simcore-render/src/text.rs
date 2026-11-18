use cosmic_text::{fontdb, Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight};
use std::collections::HashMap;

/// macOS system UI font (variable SF Pro: wght axis). Family name as registered in fontdb.
const SF_FILE: &str = "/System/Library/Fonts/SFNS.ttf";
const SF_FAMILY: &str = ".SF NS";
const MONO_FAMILY: &str = ".SF NS Mono";
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

/// A byte range of the text with its own color and/or weight (React Native nested `Text` spans).
#[derive(Debug, Clone, PartialEq)]
pub struct RichSpan {
    pub start: usize,
    pub end: usize,
    pub color: Option<[u8; 3]>,
    pub weight: Option<u16>,
    /// Own point size (nested `Text` with `fontSize`).
    pub size: Option<f32>,
    pub mono: bool,
}

/// Rich-text extras of a run: per-range styles and a line cap with ellipsis. Plain text has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rich {
    pub spans: Vec<RichSpan>,
    pub lines: Option<usize>,
}

/// One styled run of a concatenated `Text`: its own size (points), weight and color.
/// The `FontSpec` passed alongside is the largest run's (line metrics); every run scales from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub pt: f32,
    pub weight: u16,
    pub color: crate::scene::Color,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
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
}

impl FontSpec {
    pub fn new(size: f32, line_height: f32, weight: u16, align: TextAlign) -> Self {
        // Leading: 22/17 of the size up to large-title sizes, ~1.12 x size + 2.64 above (accessibility sizes, measured: 53pt -> 62pt).
        let pitch = if size > 34.0 { size * 1.12 + 2.64 } else { line_height.max(size * 22.0 / 17.0) };
        Self { size, pt: size, line_height, pitch, weight, align, tracking: tracking_em(size, weight), mono: false }
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

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    cache: SwashCache,
    sf: bool,
    inter: bool,
    has_mono: bool,
    measured: HashMap<(String, u32, u16, u32), (f64, f64)>,
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
        let sys = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let has_mono = sys.db().faces().any(|f| f.families.iter().any(|(n, _)| n == MONO_FAMILY));
        Self { sys, cache: SwashCache::new(), sf, inter, has_mono, measured: HashMap::new() }
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

    /// Shaped text; with a line cap (`rich.lines`) that the text exceeds, the last allowed line is cut
    /// to whole glyphs followed by `…` (tail truncation, like iOS).
    fn buffer(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span], rich: Option<&Rich>) -> Buffer {
        let b = self.build(text, f, width, spans, rich);
        let (Some(n), Some(_)) = (rich.and_then(|r| r.lines), width) else { return b };
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
            let probe = self.build(&format!("{}\u{2026}", rest[..bounds[mid]].trim_end()), &one, None, &[], None);
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
        let cut = Rich {
            spans: rich.map_or(vec![], |r| r.spans.iter().filter(|s| s.start < head.len()).map(|s| RichSpan { end: s.end.min(head.len()), ..s.clone() }).collect()),
            lines: None,
        };
        self.build(&format!("{head}\u{2026}"), f, width, &[], Some(&cut))
    }

    // ponytail: two styled-text models (SwiftUI `spans` runs, React Native `Rich` ranges); unify once both suites stay green.
    fn build(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span], rich: Option<&Rich>) -> Buffer {
        // Inter's glyphs are ~8% larger than SF's at the same size; at accessibility sizes that is a lot
        // of ink, so the fallback draws them slightly smaller (tracking is fitted for it).
        let big = !self.sf && f.pt > 40.0;
        let size = if big { f.size * INTER_BIG_SCALE } else { f.size };
        let mut b = Buffer::new(&mut self.sys, Metrics::new(size, f.pitch));
        b.set_size(width, None);
        let mono_family = if self.has_mono { Family::Name(MONO_FAMILY) } else { Family::Monospace };
        let (family, tracking) = if f.mono {
            (mono_family, 0.0)
        } else if self.sf {
            (Family::Name(SF_FAMILY), f.tracking)
        } else {
            (Family::Name(INTER_FAMILY), if big { inter_ax_tracking(f.pt, f.weight) } else { inter_tracking_em(f.size, f.weight) })
        };
        let wght = if big { (f.weight as i32 + INTER_BIG_WEIGHT).clamp(100, 900) as u16 } else { f.weight };
        let attrs = Attrs::new().family(family).weight(Weight(wght)).letter_spacing(tracking);
        if !spans.is_empty() {
            let (sf, fam) = (self.sf, family);
            let runs = spans.iter().enumerate().map(|(i, s)| {
                let k = s.pt / f.pt;
                let size_px = f.size * k;
                let big = !sf && s.pt > 40.0;
                let tracking = if sf {
                    tracking_em(s.pt, s.weight)
                } else if big {
                    inter_ax_tracking(s.pt, s.weight)
                } else {
                    inter_tracking_em(size_px, s.weight)
                };
                let wght = if big { (s.weight as i32 + INTER_BIG_WEIGHT).clamp(100, 900) as u16 } else { s.weight };
                let m = Metrics::new(if big { size_px * INTER_BIG_SCALE } else { size_px }, f.pitch * k);
                let c = Color::rgba((s.color.r * 255.0).round() as u8, (s.color.g * 255.0).round() as u8, (s.color.b * 255.0).round() as u8, (s.color.a * 255.0).round() as u8);
                let a = Attrs::new().family(fam).weight(Weight(wght)).letter_spacing(tracking).metrics(m).color(c).metadata(i);
                (s.text.as_str(), if s.italic { a.style(Style::Italic) } else { a })
            });
            b.set_rich_text(runs, &attrs, Shaping::Advanced, None);
        } else if let Some(r) = rich.filter(|r| !r.spans.is_empty()) {
                // Cut the text at every span edge; each piece takes the style of the span covering it.
                let mut cuts: Vec<usize> = vec![0, text.len()];
                cuts.extend(r.spans.iter().flat_map(|sp| [sp.start, sp.end]).filter(|c| *c <= text.len() && text.is_char_boundary(*c)));
                cuts.sort_unstable();
                cuts.dedup();
                let pieces: Vec<(&str, Attrs)> = cuts
                    .windows(2)
                    .map(|w| {
                        let sp = r.spans.iter().rfind(|sp| sp.start <= w[0] && w[1] <= sp.end);
                        let mut a = attrs.clone();
                        if let Some(c) = sp.and_then(|sp| sp.color) {
                            a = a.color(Color::rgb(c[0], c[1], c[2]));
                        }
                        if let Some(wt) = sp.and_then(|sp| sp.weight) {
                            let t = if self.sf { tracking_em(f.pt, wt) } else { inter_tracking_em(f.size, wt) };
                            a = a.weight(Weight(wt)).letter_spacing(t);
                        }
                        if sp.is_some_and(|sp| sp.mono) {
                            a = a.family(mono_family).letter_spacing(0.0);
                        }
                        if let Some(pt) = sp.and_then(|sp| sp.size) {
                            let k = f.size / f.pt;
                            a = a.metrics(Metrics::new(pt * k, f.pitch));
                        }
                        (&text[w[0]..w[1]], a)
                    })
                    .collect();
                b.set_rich_text(pieces, &attrs, Shaping::Advanced, None);
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
        self.measure_impl(text, f, width, spans, None)
    }

    /// React Native rich text: per-range styles and a line cap.
    pub fn measure_rich(&mut self, text: &str, f: &FontSpec, width: Option<f32>, rich: Option<&Rich>) -> (f64, f64) {
        self.measure_impl(text, f, width, &[], rich)
    }

    fn measure_impl(&mut self, text: &str, f: &FontSpec, width: Option<f32>, spans: &[Span], rich: Option<&Rich>) -> (f64, f64) {
        let key = (text.to_string(), f.size.to_bits(), f.weight, width.map_or(u32::MAX, f32::to_bits));
        let plain = spans.is_empty() && rich.is_none();
        if plain {
            if let Some(r) = self.measured.get(&key) {
                return *r;
            }
        }
        let b = self.buffer(text, &FontSpec { align: TextAlign::Leading, ..*f }, width, spans, rich);
        let (mut w, mut lines) = (0f32, 0usize);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            lines += 1;
        }
        let r = (w as f64, (f.line_height + (lines.max(1) - 1) as f32 * f.pitch) as f64);
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
        f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        self.draw_impl(text, spec, width, rgba, origin, spans, None, f)
    }

    /// React Native rich text: per-range styles and a line cap.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rich(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        origin: (f32, f32),
        rich: Option<&Rich>,
        f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        self.draw_impl(text, spec, width, rgba, origin, &[], rich, f)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_impl(
        &mut self,
        text: &str,
        spec: &FontSpec,
        width: f32,
        rgba: [u8; 4],
        origin: (f32, f32),
        spans: &[Span],
        rich: Option<&Rich>,
        mut f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) -> Vec<LineSpan> {
        let mut b = self.buffer(text, spec, Some(width), spans, rich);
        b.shape_until_scroll(&mut self.sys, false);
        let lines = b
            .layout_runs()
            .flat_map(|r| {
                let mut out: Vec<LineSpan> = vec![];
                for g in r.glyphs {
                    let run = (!spans.is_empty()).then_some(g.metadata);
                    match out.last_mut() {
                        Some(l) if l.run == run => l.x1 = g.x + g.w,
                        _ => out.push(LineSpan { x0: g.x, x1: g.x + g.w, baseline: r.line_y, run }),
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
                let gy = g.y - g.font_size * g.y_offset + run.line_y;
                let (key, gx, gy) = cosmic_text::CacheKey::new(
                    g.font_id,
                    g.glyph_id,
                    g.font_size,
                    (g.x + g.font_size * g.x_offset + fx, if frac { gy + fy } else { gy.trunc() }),
                    g.font_weight,
                    g.cache_key_flags,
                );
                let (bx, by) = (gx + ix as i32, gy + iy as i32);
                // cosmic-text replaces the color alpha with glyph coverage: reapply the run's own alpha.
                let sa = spans.get(g.metadata).map_or(1.0, |s| s.color.a);
                self.cache.with_pixels(&mut self.sys, key, g.color_opt.unwrap_or(c), |px, py, col| {
                    f(bx + px, by + py, 1, 1, [col.r(), col.g(), col.b(), (col.a() as f32 * sa).round() as u8])
                });
            }
        }
        lines
    }
}

#[cfg(test)]
mod rich_tests {
    use super::*;

    /// The rich path shapes with the same per-size tracking as the plain one.
    #[test]
    fn rich_path_matches_plain_widths() {
        let mut f = Fonts::new();
        let t = "Plain red blue bold green text.";
        for size in [15.0f32, 17.0] {
            let spec = FontSpec::new(size, size * 1.2, 400, TextAlign::Leading);
            let plain = f.measure(t, &spec, None, &[]).0;
            let empty = f.measure_rich(t, &spec, None, Some(&Rich::default())).0;
            let full = Rich { spans: vec![RichSpan { start: 0, end: t.len(), color: Some([255, 0, 0]), weight: Some(400), size: None, mono: false }], lines: Some(3) };
            let spanned = f.measure_rich(t, &spec, Some(10000.0), Some(&full)).0;
            assert_eq!((plain, empty), (plain, plain), "{size}");
            assert!((spanned - plain).abs() < 0.01, "{size}: spans {spanned} vs plain {plain}");
        }
    }
}
