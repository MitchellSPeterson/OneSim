//! Unicode and discretionary breaks over the already-shaped glyphs of a Compose paragraph.
use crate::text::{FontSpec, Span};
use cosmic_text::{Align, AttrsList, BufferLine, Ellipsize, FontSystem, Hinting, LayoutLine, LineEnding, ShapeBuffer, ShapeGlyph, ShapeLine, Shaping, Wrap};
use icu_segmenter::{LineSegmenter, WordSegmenter, options::{LineBreakOptions, LineBreakStrictness, WordBreakInvariantOptions}};
use std::collections::{BTreeMap, HashMap};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Break {
    Ordinary,
    Hyphen,
    Emergency,
}

pub(crate) fn requested(style: Option<&crate::text::ComposeSpanStyle>, text: &str) -> bool {
    text.contains('\u{ad}') || text.contains(|c: char| matches!(c, '\u{0e00}'..='\u{0eff}' | '\u{1000}'..='\u{109f}' | '\u{1780}'..='\u{17ff}'))
        || style.is_some_and(|style| style.auto_hyphens == Some(true)
        || style.line_break.is_some_and(|[strategy, strictness, word]| strategy > 1 || strictness > 0 || word > 1))
}

pub(crate) fn opportunities(text: &str, strictness: u8, phrase: bool, locale: Option<&str>) -> Vec<usize> {
    let locale = locale.and_then(|tag| tag.split("-u-").next()?.parse().ok());
    let mut options = LineBreakOptions::default();
    options.content_locale = locale.as_ref();
    options.strictness = Some(match strictness {
        2 => LineBreakStrictness::Loose,
        0 | 1 | 3 => LineBreakStrictness::Normal,
        _ => LineBreakStrictness::Strict,
    });
    let words = phrase.then(|| word_boundaries(text).collect::<std::collections::BTreeSet<_>>());
    LineSegmenter::new_dictionary(options).segment_str(text).filter(|&at| {
        at > 0 && words.as_ref().is_none_or(|words| words.contains(&at)
            || text[..at].ends_with(|c: char| c.is_whitespace() || c.is_ascii_punctuation()))
    }).collect()
}

pub(crate) fn word_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    WordSegmenter::new_dictionary(WordBreakInvariantOptions::default()).segment_str(text)
}

pub(crate) fn reflow(
    paragraph: &BufferLine,
    shape: &ShapeLine,
    spec: &FontSpec,
    width: f32,
    spans: &[Span],
    fonts: &mut FontSystem,
) -> Option<Vec<LayoutLine>> {
    let text = paragraph.text();
    if text.is_empty() || shape.spans.is_empty() || width <= 0.0 {
        return None;
    }
    let glyphs: Vec<_> = shape.spans.iter().flat_map(|span| &span.words).flat_map(|word| &word.glyphs).collect();
    let first = glyphs.iter().min_by_key(|glyph| glyph.start)?;
    let style = |metadata| spans.get(metadata).and_then(|span: &Span| span.compose.as_deref());
    let style_at = |byte| {
        let glyph = glyphs.iter().find(|g| (g.start..g.end).contains(&byte)).unwrap_or(first);
        style(glyph.metadata)
    };
    let options = style(first.metadata);
    if !requested(options, text) { return None; }
    let [strategy, strictness, word_mode] = options.and_then(|style| style.line_break).unwrap_or([1, 3, 1]);
    let automatic = options.is_some_and(|style| style.auto_hyphens == Some(true));
    let mut advances: BTreeMap<usize, f32> = BTreeMap::new();
    for glyph in &glyphs {
        let font_size = glyph.metrics_opt.map_or(spec.size, |m| m.font_size);
        *advances.entry(glyph.end).or_default() += glyph.x_advance * font_size;
    }
    let mut pen = 0.0;
    for x in advances.values_mut() {
        pen += *x;
        *x = pen;
    }
    if pen <= width { return None; }
    let prefix = |offset| advances.range(..=offset).next_back().map_or(0.0, |(_, width)| *width);
    let locale = options.and_then(|style| style.locale.as_deref());
    let mut breaks = BTreeMap::from([(0, Break::Ordinary)]);
    for at in opportunities(text, strictness, word_mode == 2, locale) {
        if advances.contains_key(&at) {
            let kind = if text[..at].ends_with('\u{ad}') && at < text.len() { Break::Hyphen } else { Break::Ordinary };
            breaks.insert(at, kind);
        }
    }
    if automatic {
        for (start, word) in text.unicode_word_indices() {
            if !word.chars().all(char::is_alphabetic) { continue; }
            let locale = style_at(start).and_then(|style| style.locale.as_deref()).or(locale).unwrap_or("en");
            let Some(language) = locale.split('-').next().and_then(|lang| lang.as_bytes().try_into().ok())
                .and_then(hypher::Lang::from_iso) else { continue };
            let mut offset = start;
            for syllable in hypher::hyphenate(word, language) {
                offset += syllable.len();
                if offset < start + word.len() && advances.contains_key(&offset) {
                    breaks.entry(offset).or_insert(Break::Hyphen);
                }
            }
        }
    }
    for &end in advances.keys() {
        breaks.entry(end).or_insert(Break::Emergency);
    }
    breaks.insert(text.len(), Break::Ordinary);
    let breaks: Vec<_> = breaks.into_iter().collect();
    let ends: Vec<_> = breaks.iter().map(|&(end, _)| text[..end].trim_end().len()).collect();
    let fallback_hyphen = spec.size * 0.33;
    let cost_width = |a: usize, b: usize| {
        let fragment = prefix(ends[b]) - prefix(breaks[a].0);
        fragment + if breaks[b].1 == Break::Hyphen { fallback_hyphen } else { 0.0 }
    };
    let count = breaks.len();
    let target = if strategy == 3 { (pen / (pen / width).ceil()).max(width * 0.65) } else { width };
    let mut next = vec![0; count - 1];
    let mut cost = vec![0.0f32; count];
    for a in (0..count - 1).rev() {
        let mut best = (f32::INFINITY, a + 1);
        for b in a + 1..count {
            let used = cost_width(a, b);
            if used > width && b > a + 1 { break; }
            let unused = target - used;
            let spacing = if strategy == 1 {
                (width - used).max(0.0) * width * 0.2
            } else if strategy == 3 || b + 1 != count { unused * unused }
            else { 0.0 };
            let penalty = match breaks[b].1 {
                Break::Ordinary => 0.0,
                Break::Hyphen => width * width * 0.025,
                Break::Emergency => width * width * 2.0,
            };
            let future = if strategy == 1 { 0.0 } else { cost[b] };
            let score = spacing + penalty + future + (used - width).max(0.0) * width * 4.0;
            if score <= best.0 { best = (score, b); }
        }
        (cost[a], next[a]) = best;
    }

    let mut rows = Vec::new();
    let mut hyphens = HashMap::<usize, Option<ShapeGlyph>>::new();
    let mut scratch = ShapeBuffer::default();
    let mut a = 0;
    while a + 1 < count {
        let b = next[a];
        let start = breaks[a].0;
        let end = ends[b].max(start);
        let mut fragment = shape.clone();
        for word in fragment.spans.iter_mut().flat_map(|span| &mut span.words) {
            word.glyphs.retain(|g| start <= g.start && g.end <= end);
        }
        for span in &mut fragment.spans {
            span.words.retain(|word| !word.glyphs.is_empty());
        }
        fragment.spans.retain(|span| !span.words.is_empty());
        if breaks[b].1 == Break::Hyphen {
            if let Some(word) = fragment.spans.iter_mut().flat_map(|span| &mut span.words)
                .max_by_key(|word| word.glyphs.iter().map(|glyph| glyph.end).max())
            {
                if let Some(last) = word.glyphs.iter().max_by_key(|glyph| glyph.end) {
                    let sample = hyphens.entry(last.metadata).or_insert_with(|| {
                        let attrs = paragraph.attrs_list().get_span(last.start);
                        let mut hyphen = BufferLine::new("‐".to_owned(), LineEnding::None, AttrsList::new(&attrs), Shaping::Advanced);
                        hyphen.shape(fonts, 8).spans.first()?.words.first()?.glyphs.first().cloned()
                    });
                    if let Some(glyph) = sample {
                        let mut glyph = glyph.clone();
                        glyph.start = end;
                        glyph.end = end;
                        word.glyphs.push(glyph);
                    }
                }
            }
        }
        let mut row = Vec::new();
        fragment.layout_to_buffer(&mut scratch, spec.size, Some(width), Wrap::None, Ellipsize::None, paragraph.align(), &mut row, None, Hinting::Disabled);
        if paragraph.align() == Some(Align::Justified) && b + 1 != count {
            for line in &mut row {
                justify(line, text, width);
            }
        }
        rows.append(&mut row);
        a = b;
    }
    Some(rows)
}

fn justify(line: &mut LayoutLine, text: &str, width: f32) {
    let is_gap = |glyph: &cosmic_text::LayoutGlyph| {
        text.get(glyph.start..glyph.end).is_some_and(|s| !s.is_empty() && s.chars().all(char::is_whitespace))
    };
    let gaps = line.glyphs.iter().filter(|glyph| is_gap(glyph)).count();
    if gaps == 0 { return; }
    let extra = (width - line.w).max(0.0) / gaps as f32;
    let mut offset = 0.0;
    for glyph in &mut line.glyphs {
        glyph.x += offset;
        if is_gap(glyph) {
            glyph.w += extra;
            offset += extra;
        }
    }
    line.w += offset;
}

pub(crate) fn ellipsize(paragraph: &BufferLine, line: &mut LayoutLine, spec: &FontSpec, width: f32, rtl: bool, fonts: &mut FontSystem) {
    let attrs = line.glyphs.iter().max_by_key(|glyph| glyph.end)
        .map(|last| paragraph.attrs_list().get_span(last.start))
        .unwrap_or_else(|| paragraph.attrs_list().defaults());
    let mut marker = BufferLine::new("…".to_owned(), LineEnding::None, AttrsList::new(&attrs), Shaping::Advanced);
    let Some(mut marker) = marker.layout(fonts, spec.size, None, Wrap::None, Ellipsize::None, None, 8, Hinting::Disabled)
        .first().and_then(|line| line.glyphs.first()).cloned() else { return };

    // Clear a discretionary line-end hyphen before adding the final ellipsis.
    line.glyphs.retain(|glyph| glyph.start < glyph.end);
    let mut width_of_glyphs = line.glyphs.iter().map(|g| g.w).sum::<f32>();
    while width_of_glyphs + marker.w > width {
        let Some((i, _)) = line.glyphs.iter().enumerate().max_by_key(|(_, glyph)| glyph.end) else { break; };
        width_of_glyphs -= line.glyphs.remove(i).w;
    }
    let used = width_of_glyphs + marker.w;
    let alignment = paragraph.align().unwrap_or(if rtl { Align::Right } else { Align::Left });
    let start = match alignment {
        Align::Right => (width - used).max(0.0),
        Align::End if !rtl => (width - used).max(0.0),
        Align::Center => ((width - used) / 2.0).max(0.0),
        _ => 0.0,
    };
    line.glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut x = start + if rtl { marker.w } else { 0.0 };
    for glyph in &mut line.glyphs {
        glyph.x = x;
        x += glyph.w;
    }
    marker.start = line.glyphs.iter().map(|g| g.end).max().unwrap_or(0);
    marker.end = marker.start;
    marker.x = if rtl { start } else { x };
    if rtl { line.glyphs.insert(0, marker); } else { line.glyphs.push(marker); }
    line.w = used;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_breaks_a_thai_run_without_spaces() {
        let text = "ภาษาไทยภาษาไทย";
        let line_breaks = opportunities(text, 3, false, Some("th"));
        let words: Vec<_> = word_boundaries(text).collect();
        assert!(line_breaks.contains(&"ภาษาไทย".len()));
        assert!(words.contains(&"ภาษาไทย".len()));
        assert_eq!(line_breaks.last(), Some(&text.len()));
    }

    #[test]
    fn japanese_strictness_changes_small_kana_breaks() {
        let text = "あぁい";
        assert!(opportunities(text, 2, false, Some("ja")).contains(&3));
        assert!(!opportunities(text, 4, false, Some("ja")).contains(&3));
    }
}
