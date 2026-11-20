//! Shared shaping geometry for measured, painted and selectable Compose text.
use crate::text::{line_indent, ComposeOverflow, FontSpec, Span};
use cosmic_text::{Buffer, Cursor, Ellipsize, EllipsizeHeightLimit, FontSystem, Hinting, LayoutLine, LayoutRun, ShapeBuffer, Wrap};
use std::ops::Deref;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub(crate) struct TextBuffer {
    buffer: Buffer,
    adjusted: Option<Vec<Vec<LayoutLine>>>,
}

impl Deref for TextBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Buffer { &self.buffer }
}

impl TextBuffer {
    pub fn new(buffer: Buffer, spec: &FontSpec, width: Option<f32>, spans: &[Span], fonts: &mut FontSystem, reflow: bool) -> Self {
        let scale = |span: &Span| span.compose.as_ref().and_then(|style| style.transform).map_or(1.0, |pair| pair[0]);
        let adjusted = (reflow || spans.iter().any(|span| scale(span) != 1.0)).then(|| {
            let mut scratch = ShapeBuffer::default();
            let wrap = if spec.no_wrap { Wrap::None } else { Wrap::WordOrGlyph };
            let limit = spec.lines.or(spec.no_wrap.then_some(1));
            let ellipse = match (spec.compose_overflow, limit) {
                (Some(ComposeOverflow::Ellipsis), Some(n)) => Ellipsize::End(EllipsizeHeightLimit::Lines(n)),
                (Some(ComposeOverflow::StartEllipsis), Some(1)) => Ellipsize::Start(EllipsizeHeightLimit::Lines(1)),
                (Some(ComposeOverflow::MiddleEllipsis), Some(1)) => Ellipsize::Middle(EllipsizeHeightLimit::Lines(1)),
                _ => Ellipsize::None,
            };
            let mut paragraphs: Vec<Vec<LayoutLine>> = buffer.lines.iter().map(|paragraph| {
                let mut shape = paragraph.shape_opt().expect("laid-out paragraph").clone();
                for glyph in shape.spans.iter_mut().flat_map(|span| &mut span.words).flat_map(|word| &mut word.glyphs) {
                    let scale = spans.get(glyph.metadata).map_or(1.0, scale);
                    glyph.x_advance *= scale.abs().min(32.0);
                    glyph.x_offset *= scale.clamp(-32.0, 32.0);
                }
                let width = width.map(|width| (width - line_indent(paragraph)).max(0.0));
                if let Some(w) = width.filter(|_| reflow) {
                    if let Some(rows) = crate::line_break::reflow(paragraph, &shape, spec, w, spans, fonts) {
                        return rows;
                    }
                }
                let mut lines = Vec::new();
                shape.layout_to_buffer(&mut scratch, spec.size, width, wrap, if reflow { Ellipsize::None } else { ellipse }, paragraph.align(), &mut lines, None, Hinting::Disabled);
                lines
            }).collect();
            let limit = spec.visible_lines();
            if reflow && spec.compose_overflow == Some(ComposeOverflow::Ellipsis)
                && limit < paragraphs.iter().map(Vec::len).sum()
            {
                if let Some((p, row)) = paragraphs.iter_mut().enumerate()
                    .flat_map(|(p, lines)| lines.iter_mut().map(move |line| (p, line))).nth(limit - 1)
                {
                    let paragraph = &buffer.lines[p];
                    let max = (width.unwrap_or(row.w) - line_indent(paragraph)).max(0.0);
                    crate::line_break::ellipsize(paragraph, row, spec, max, fonts);
                }
            }
            paragraphs
        });
        Self { buffer, adjusted }
    }

    pub fn layout_runs(&self) -> impl Iterator<Item = LayoutRun<'_>> {
        let ordinary = self.buffer.layout_runs().take(if self.adjusted.is_some() { 0 } else { usize::MAX });
        let mut y = 0.0;
        let transformed = self.adjusted.iter().flatten().enumerate().flat_map(|(i, rows)| {
            let paragraph = &self.lines[i];
            rows.iter().map(move |row| (i, paragraph, row))
        }).map(move |(i, paragraph, row)| {
            let line_height = row.line_height_opt.unwrap_or(self.metrics().line_height);
            let line_top = y;
            y += line_height;
            LayoutRun {
                line_i: i, text: paragraph.text(), rtl: paragraph.shape_opt().is_some_and(|shape| shape.rtl),
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
