//! Shared shaping geometry for measured, painted and selectable Compose text.
use crate::text::{line_indent, ComposeOverflow, FontSpec, Span};
use cosmic_text::{AttrsList, Buffer, BufferLine, Cursor, Ellipsize, EllipsizeHeightLimit, FontSystem, Hinting, LayoutLine, LayoutRun, ShapeBuffer, ShapeLine, Shaping, Wrap};
use std::ops::Deref;
use unicode_bidi::{bidi_class, BidiClass};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub(crate) struct TextBuffer {
    buffer: Buffer,
    adjusted: Option<Vec<(bool, Vec<LayoutLine>)>>,
}

impl Deref for TextBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Buffer { &self.buffer }
}

impl TextBuffer {
    pub fn new(buffer: Buffer, spec: &FontSpec, width: Option<f32>, spans: &[Span], fonts: &mut FontSystem, reflow: bool) -> Self {
        let scale = |span: &Span| span.compose.as_ref().and_then(|style| style.transform).map_or(1.0, |pair| pair[0]);
        let forced = |paragraph: &BufferLine| {
            let metadata = paragraph.attrs_list().get_span(0).metadata;
            let mode = spans.get(metadata).and_then(|span| span.compose.as_ref()?.paragraph_direction).unwrap_or(spec.compose_direction);
            let direction = match mode {
                1 => false,
                2 => true,
                5 if !has_strong_direction(paragraph.text()) => true,
                _ => return None,
            };
            let natural = paragraph.shape_opt().is_some_and(|shape| shape.rtl);
            (direction != natural).then_some(direction)
        };
        let adjusted = (reflow || spans.iter().any(|span| scale(span) != 1.0)
            || (spec.compose_direction != 0 || spans.iter().any(|span| span.compose.as_ref().is_some_and(|style| style.paragraph_direction.is_some())))
                && buffer.lines.iter().any(|paragraph| forced(paragraph).is_some())).then(|| {
            let mut scratch = ShapeBuffer::default();
            let wrap = if spec.no_wrap { Wrap::None } else { Wrap::WordOrGlyph };
            let limit = spec.lines.or(spec.no_wrap.then_some(1));
            let ellipse = match (spec.compose_overflow, limit) {
                (Some(ComposeOverflow::Ellipsis), Some(n)) => Ellipsize::End(EllipsizeHeightLimit::Lines(n)),
                (Some(ComposeOverflow::StartEllipsis), Some(1)) => Ellipsize::Start(EllipsizeHeightLimit::Lines(1)),
                (Some(ComposeOverflow::MiddleEllipsis), Some(1)) => Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)),
                _ => Ellipsize::None,
            };
            let mut paragraphs: Vec<(bool, Vec<LayoutLine>)> = buffer.lines.iter().map(|paragraph| {
                let mut shape = match forced(paragraph) {
                    Some(rtl) => shape_with_direction(paragraph, rtl, fonts),
                    None => paragraph.shape_opt().expect("laid-out paragraph").clone(),
                };
                for glyph in shape.spans.iter_mut().flat_map(|span| &mut span.words).flat_map(|word| &mut word.glyphs) {
                    let scale = spans.get(glyph.metadata).map_or(1.0, scale);
                    glyph.x_advance *= scale.abs().min(32.0);
                    glyph.x_offset *= scale.clamp(-32.0, 32.0);
                }
                let width = width.map(|width| (width - line_indent(paragraph)).max(0.0));
                if let Some(w) = width.filter(|_| reflow) {
                    if let Some(rows) = crate::line_break::reflow(paragraph, &shape, spec, w, spans, fonts) {
                        return (shape.rtl, rows);
                    }
                }
                let mut lines = Vec::new();
                shape.layout_to_buffer(&mut scratch, spec.size, width, wrap, if reflow { Ellipsize::None } else { ellipse }, paragraph.align(), &mut lines, None, Hinting::Disabled);
                (shape.rtl, lines)
            }).collect();
            let limit = spec.visible_lines();
            if reflow && spec.compose_overflow == Some(ComposeOverflow::Ellipsis)
                && limit < paragraphs.iter().map(|(_, lines)| lines.len()).sum()
            {
                if let Some((p, rtl, row)) = paragraphs.iter_mut().enumerate()
                    .flat_map(|(p, (rtl, lines))| lines.iter_mut().map(move |line| (p, *rtl, line))).nth(limit - 1)
                {
                    let paragraph = &buffer.lines[p];
                    let max = (width.unwrap_or(row.w) - line_indent(paragraph)).max(0.0);
                    crate::line_break::ellipsize(paragraph, row, spec, max, rtl, fonts);
                }
            }
            paragraphs
        });
        Self { buffer, adjusted }
    }

    pub fn layout_runs(&self) -> impl Iterator<Item = LayoutRun<'_>> {
        let ordinary = self.buffer.layout_runs().take(if self.adjusted.is_some() { 0 } else { usize::MAX });
        let mut y = 0.0;
        let transformed = self.adjusted.iter().flatten().enumerate().flat_map(|(i, (rtl, rows))| {
            let paragraph = &self.lines[i];
            rows.iter().map(move |row| (i, paragraph, *rtl, row))
        }).map(move |(i, paragraph, rtl, row)| {
            let line_height = row.line_height_opt.unwrap_or(self.metrics().line_height);
            let line_top = y;
            y += line_height;
            LayoutRun {
                line_i: i, text: paragraph.text(), rtl,
                glyphs: &row.glyphs, decorations: &row.decorations, line_top, line_height, line_w: row.w,
                line_y: line_top + (line_height + row.max_ascent - row.max_descent) / 2.0,
            }
        });
        ordinary.chain(transformed)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Cursor> {
        if self.adjusted.is_none() {
            return self.buffer.hit(x, y);
        }
        let run = self.layout_runs().find(|run| (run.line_top..run.line_top + run.line_height).contains(&y))?;
        let glyph = run.glyphs.iter().min_by(|a, b| {
            let distance = |g: &cosmic_text::LayoutGlyph| (x - x.clamp(g.x, g.x + g.w)).abs();
            distance(a).total_cmp(&distance(b))
        });
        let index = glyph.map_or(0, |g| {
            let cluster = run.text.get(g.start..g.end).unwrap_or("");
            let stops: Vec<_> = cluster.grapheme_indices(true).map(|(i, _)| i).chain([cluster.len()]).collect();
            let mut portion = if g.w > 0.0 { ((x - g.x) / g.w).clamp(0.0, 1.0) } else { 0.0 };
            if g.level.is_rtl() { portion = 1.0 - portion; }
            let i = (portion * (stops.len() - 1) as f32).round() as usize;
            g.start + stops[i]
        });
        Some(Cursor::new(run.line_i, index))
    }
}

fn has_strong_direction(text: &str) -> bool {
    // UAX #9 P2 ignores the content of directional isolates.
    let mut depth = 0usize;
    for ch in text.chars() {
        match bidi_class(ch) {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => depth += 1,
            BidiClass::PDI => depth = depth.saturating_sub(1),
            BidiClass::L | BidiClass::R | BidiClass::AL if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

fn shape_with_direction(paragraph: &BufferLine, rtl: bool, fonts: &mut FontSystem) -> ShapeLine {
    // cosmic-text derives its base level from the first strong character. Add a
    // zero-width mark to the shaping input only, then restore the source offsets.
    let mark = if rtl { '\u{200f}' } else { '\u{200e}' };
    let bytes = mark.len_utf8();
    let text = format!("{mark}{}", paragraph.text());
    let attrs = paragraph.attrs_list();
    let mut prefixed = AttrsList::new(&attrs.defaults());
    for (range, style) in attrs.spans_iter() {
        prefixed.add_span(range.start + bytes..range.end + bytes, &style.as_attrs());
    }
    let mut shape = ShapeLine::new(fonts, &text, &prefixed, Shaping::Advanced, 8);
    for span in &mut shape.spans {
        for word in &mut span.words {
            word.glyphs.retain_mut(|glyph| {
                if glyph.end <= bytes { return false; }
                glyph.start = glyph.start.saturating_sub(bytes);
                glyph.end -= bytes;
                true
            });
        }
        span.words.retain(|word| !word.glyphs.is_empty());
    }
    shape.spans.retain(|span| !span.words.is_empty());
    shape
}

#[cfg(test)]
mod tests {
    use crate::text::{ComposeOverflow, FontSpec, Fonts, TextAlign};

    #[test]
    fn paragraph_direction_preserves_source_clusters_and_resolves_neutral_text() {
        let mut fonts = Fonts::with_fallback(true);
        let base = FontSpec { android: true, compose_overflow: Some(ComposeOverflow::Clip),
            ..FontSpec::new(18.0, 22.0, 400, TextAlign::Leading) };
        for (text, mode, rtl) in [
            ("ABC (123) 😀", 2, true),
            ("אבג (123)", 1, false),
            ("ABC", 5, false),
            ("123 - 456", 5, true),
            ("123 - 456", 4, false),
            ("\u{2066}ABC\u{2069} 12", 5, true),
            ("", 2, true),
        ] {
            let spec = FontSpec { compose_direction: mode, ..base };
            let buffer = fonts.buffer(text, &spec, Some(240.0), &[]);
            let line = buffer.layout_runs().next().expect("one paragraph");
            assert_eq!(line.rtl, rtl, "{text:?} in direction mode {mode}");
            assert_eq!(line.text, text);
            assert!(line.glyphs.iter().all(|glyph| text.get(glyph.start..glyph.end).is_some()));
        }
    }

    #[test]
    fn rtl_end_and_physical_right_are_opposite_edges() {
        let mut fonts = Fonts::with_fallback(true);
        let base = FontSpec { android: true, compose_overflow: Some(ComposeOverflow::Clip),
            compose_direction: 2, ..FontSpec::new(18.0, 22.0, 400, TextAlign::Leading) };
        let mut ink = |align| {
            let buffer = fonts.buffer("ABC (123)", &FontSpec { align, ..base }, Some(240.0), &[]);
            let line = buffer.layout_runs().next().unwrap();
            line.glyphs.iter().map(|glyph| glyph.x).fold(f32::INFINITY, f32::min)
        };
        assert!(ink(TextAlign::End) < 1.0);
        assert!(ink(TextAlign::Trailing) > 100.0);
    }
}
