//! Resolve Compose text spans and shape pointer geometry with the same parameters as painting.

use crate::text::{line_indent, line_x, TextBullet};
use crate::{Color, Fonts, Rect, Span, TextRun};
use cosmic_text::Cursor;
use crate::text_layout::TextBuffer;
use onesim_protocol::ComposeTextSpan;
use serde_json::Value;
use std::{ops::Range, sync::Arc};

/// Resolve UTF-16 ranges in source order, with later specified properties winning.
pub(crate) fn spans(
    text: &str,
    props: Option<&Value>,
    base: Span,
    size: impl Fn(f32) -> f32,
    font_size: f32,
) -> Vec<Span> {
    let Some(props) = props.and_then(Value::as_array) else {
        return vec![];
    };
    let mut utf16 = 0;
    let mut boundaries: Vec<_> = text
        .char_indices()
        .map(|(byte, c)| {
            let pair = (utf16, byte);
            utf16 += c.len_utf16();
            pair
        })
        .collect();
    boundaries.push((utf16, text.len()));
    let offset = |index| {
        boundaries
            .binary_search_by_key(&index, |&(i, _)| i)
            .ok()
            .map(|i| boundaries[i].1)
    };
    let styles: Vec<_> = props
        .iter()
        .filter_map(|value| {
            let style: ComposeTextSpan = serde_json::from_value(value.clone()).ok()?;
            let (start, end) = (offset(style.start)?, offset(style.end)?);
            (start < end).then_some((start, end, style))
        })
        .collect();
    if styles.is_empty() {
        return vec![];
    }
    let mut edges: Vec<_> = styles
        .iter()
        .flat_map(|(start, end, _)| [*start, *end])
        .chain([0, text.len()])
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut runs = Vec::with_capacity(edges.len() - 1);
    for edge in edges.windows(2) {
        let [start, end] = [edge[0], edge[1]];
        let mut run = base.clone();
        for (style_start, _, style) in styles.iter().filter(|(a, b, _)| *a <= start && end <= *b) {
            if let Some([r, g, b, a]) = style.color {
                run.color = Color::rgba(r, g, b, a);
            }
            if let Some(pt) = style.font_size_em.map(|em| em * font_size).or(style.font_size).filter(|n| *n > 0.0) {
                run.pt = size(pt);
            }
            if let Some(weight) = style.font_weight {
                run.weight = weight;
            }
            if let Some(italic) = style.italic {
                run.italic = italic;
            }
            if let Some(decoration) = style.decoration {
                run.underline = decoration & 1 != 0;
                run.strike = decoration & 2 != 0;
            }
            if style.background.is_some()
                || style.letter_spacing.is_some()
                || style.letter_spacing_em.is_some()
                || style.font_family.is_some()
                || style.baseline_shift.is_some()
                || style.font_feature_settings.is_some()
                || style.brush.is_some()
                || style.paint_alpha.is_some()
                || style.text_shadow.is_some()
                || style.text_stroke.is_some()
                || style.text_fill == Some(true)
                || style.text_transform.is_some()
                || style.line_break.is_some()
                || style.auto_hyphens.is_some()
                || style.text_locale.is_some()
                || (style.color.is_some() && run.compose.is_some())
                || style.paragraph.is_some()
                || style.paragraph_height.is_some()
                || style.paragraph_height_em.is_some()
                || style.paragraph_indent.is_some()
                || style.bullet.is_some()
            {
                let extra = run.compose.get_or_insert_with(Default::default);
                if style.brush.is_some() || style.color.is_some() {
                    extra.brush.clone_from(&style.brush);
                }
                extra.paint_alpha = style.paint_alpha.or(extra.paint_alpha);
                extra.shadow = style.text_shadow.or(extra.shadow);
                if style.text_fill == Some(true) || style.text_stroke.is_some() {
                    extra.stroke.clone_from(&style.text_stroke);
                }
                extra.transform = style.text_transform.or(extra.transform);
                extra.line_break = style.line_break.or(extra.line_break);
                extra.auto_hyphens = style.auto_hyphens.or(extra.auto_hyphens);
                if let Some(locale) = &style.text_locale { extra.locale = Some(locale.clone()); }
                if let Some([r, g, b, a]) = style.background {
                    extra.background = Some(Color::rgba(r, g, b, a));
                }
                if let Some(spacing) = style
                    .letter_spacing_em
                    .map(|v| (v, true))
                    .or(style.letter_spacing.map(|v| (v, false)))
                {
                    extra.spacing = Some(spacing);
                }
                if let Some(family) = &style.font_family {
                    extra.family = Some(family.clone());
                }
                if let Some(features) = &style.font_feature_settings {
                    extra.feature_settings = Some(features.clone());
                }
                if let Some(shift) = style.baseline_shift {
                    extra.baseline_shift = Some(shift);
                }
                if let Some(paragraph) = style.paragraph {
                    extra.paragraph = Some(paragraph);
                    extra.paragraph_align = style
                        .paragraph_align
                        .as_deref()
                        .map(|align| match align {
                            "center" => crate::TextAlign::Center,
                            "end" => crate::TextAlign::Trailing,
                            "justify" => crate::TextAlign::Justify,
                            _ => crate::TextAlign::Leading,
                        })
                        .or(extra.paragraph_align);
                }
                extra.paragraph_height = style.paragraph_height_em.map(|em| em * font_size)
                    .or(style.paragraph_height).or(extra.paragraph_height);
                if let Some(indent) = style.paragraph_indent {
                    extra.paragraph_indent = Some(indent.map(|unit| unit.at(font_size)));
                }
                if start == *style_start {
                    if let Some(bullet) = &style.bullet {
                        extra.bullet = Some(crate::text::TextBullet {
                            width: bullet.width.at(font_size).max(0.0),
                            height: bullet.height.at(font_size).max(0.0),
                            padding: bullet.padding.at(font_size).max(0.0),
                            wire: bullet.clone(),
                        });
                    }
                }
            }
        }
        run.text = text[start..end].to_owned();
        runs.push(run);
    }
    runs
}

#[derive(Clone, Debug)]
pub(crate) struct ComposeText {
    buffer: Arc<TextBuffer>,
    line_offsets: Arc<[usize]>,
    scale: f32,
    visible_lines: usize,
    pub action: Option<u64>,
    pub selection_color: Option<Color>,
    pub handle_color: Option<Color>,
    pub layout_callback: Option<u64>,
    pub constraints: [f64; 4],
    pub auto_font_size: Option<f32>,
    pub pending_auto_size: bool,
    pub probe_layouts: Vec<onesim_protocol::ComposeTextLayout>,
    pub untruncated_extent: Option<(f32, f32, usize)>,
    pub link_ranges: Vec<[usize; 2]>,
    pub speech_ranges: Vec<(usize, usize, String)>,
    pub inline: Vec<onesim_protocol::ComposeInlineObject>,
    pub original_text: Option<String>,
    /// Local highlight rectangles in dp. The interaction state supplies these after layout.
    pub highlight: Vec<Rect>,
}

impl ComposeText {
    pub fn new(
        fonts: &mut Fonts,
        run: &TextRun,
        width: f64,
        scale: f32,
        action: Option<u64>,
        selection_color: Option<Color>,
    ) -> Self {
        let spec = run.font;
        let scaled = spec.scaled(scale);
        let buffer = fonts.buffer(
            &run.text,
            &scaled,
            Some(crate::raster::text_width(width as f32, scale)),
            &run.spans,
        );
        let mut at = 0;
        let line_offsets = buffer
            .lines
            .iter()
            .map(|line| {
                let start = at;
                at += line.text().len() + line.ending().as_str().len();
                start
            })
            .collect();
        Self {
            buffer: Arc::new(buffer),
            line_offsets,
            scale,
            visible_lines: spec.visible_lines(),
            action,
            selection_color,
            handle_color: None,
            layout_callback: None,
            constraints: [0.0, f64::INFINITY, 0.0, f64::INFINITY],
            auto_font_size: None,
            pending_auto_size: false,
            probe_layouts: vec![],
            untruncated_extent: None,
            link_ranges: vec![],
            speech_ranges: vec![],
            inline: vec![],
            original_text: None,
            highlight: vec![],
        }
    }

    pub fn original_offset(&self, display_offset: usize) -> usize {
        self.inline
            .iter()
            .take_while(|object| object.0 < display_offset)
            .fold(display_offset, |offset, object| {
                offset.saturating_add(object.2.saturating_sub(object.1).saturating_sub(1))
            })
    }

    pub fn selected_string(&self, text: &str, range: Range<usize>) -> Option<String> {
        if let Some(original) = &self.original_text {
            let offset = |byte| {
                self.original_offset(
                    text.get(..byte)
                        .map_or(0, |prefix| prefix.encode_utf16().count()),
                )
            };
            let (lo, hi) = (offset(range.start), offset(range.end));
            return String::from_utf16(
                &original
                    .encode_utf16()
                    .skip(lo)
                    .take(hi.saturating_sub(lo))
                    .collect::<Vec<_>>(),
            )
            .ok();
        }
        Some(text.get(range)?.to_owned())
    }

    pub fn inline_rects(&self, text: &str, base_size: f32) -> Vec<Option<Rect>> {
        if self.inline.is_empty() {
            return vec![];
        }
        let mut offsets = self
            .inline
            .iter()
            .map(|object| object.0)
            .enumerate()
            .peekable();
        let mut result = vec![None; self.inline.len()];
        let mut targets = std::collections::BTreeMap::new();
        let mut utf16 = 0;
        for (byte, c) in text.char_indices() {
            while offsets.peek().is_some_and(|(_, offset)| *offset <= utf16) {
                let (index, offset) = offsets.next().unwrap();
                if c == '\u{fffc}' && offset == utf16 {
                    targets.insert(byte, index);
                }
            }
            utf16 += c.len_utf16();
        }
        let scale = f64::from(self.scale);
        for line in self.buffer.layout_runs().take(self.visible_lines) {
            for glyph in line.glyphs {
                let byte = self.line_offsets[line.line_i] + glyph.start;
                let Some(&index) = targets.get(&byte) else {
                    continue;
                };
                let object = &self.inline[index];
                let (w, h) = (object.3, object.4);
                let (baseline, top, height) = (
                    f64::from(line.line_y) / scale,
                    f64::from(line.line_top) / scale,
                    f64::from(line.line_height) / scale,
                );
                let em = f64::from(base_size);
                let y = match object.5.as_str() {
                    "top" => top,
                    "bottom" => top + height - h,
                    "center" => top + (height - h) / 2.0,
                    "textTop" => baseline - em * 0.927,
                    "textBottom" => baseline + em * 0.244 - h,
                    "textCenter" => baseline + (em * (0.244 - 0.927) - h) / 2.0,
                    _ => baseline - h,
                };
                result[index] = Some(Rect {
                    x: f64::from(glyph.x + line_x(&self.buffer, &line)) / scale,
                    y,
                    w,
                    h,
                });
            }
        }
        result
    }

    pub fn bullet_rects<'a>(&self, spans: &'a [Span], width: f64) -> Vec<(Rect, &'a TextBullet)> {
        let bullets: std::collections::BTreeMap<_, _> = spans.iter().scan(0, |offset, span| {
            let byte = *offset;
            *offset += span.text.len();
            Some((byte, span.compose.as_ref().and_then(|style| style.bullet.as_ref())))
        }).filter_map(|(byte, bullet)| Some((byte, bullet?))).collect();
        let mut previous = None;
        let scale = f64::from(self.scale);
        self.buffer.layout_runs().take(self.visible_lines).filter_map(|line| {
            if previous.replace(line.line_i) == Some(line.line_i) {
                return None;
            }
            let bullet = *bullets.get(&self.line_offsets[line.line_i])?;
            let indent = f64::from(line_indent(&self.buffer.lines[line.line_i])) / scale;
            let (w, h, padding) = (f64::from(bullet.width), f64::from(bullet.height), f64::from(bullet.padding));
            let offset = (indent - w - padding).max(0.0);
            let x = if line.rtl { width - offset - w } else { offset };
            let y = f64::from(line.line_top + line.line_height / 2.0) / scale - h / 2.0;
            Some((Rect { x, y, w, h }, bullet))
        }).collect()
    }

    pub fn offset_at(&self, text: &str, x: f64, y: f64) -> usize {
        let y = y as f32 * self.scale;
        let runs = || self.buffer.layout_runs().take(self.visible_lines);
        let Some(line) = runs()
            .find(|line| y < line.line_top + line.line_height)
            .or_else(|| runs().last())
        else {
            return 0;
        };
        let y = y.clamp(
            line.line_top,
            line.line_top + (line.line_height - 0.01).max(0.0),
        );
        let (left, right) = line
            .glyphs
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), g| {
                (lo.min(g.x), hi.max(g.x + g.w))
            });
        let x_px = x as f32 * self.scale - line_x(&self.buffer, &line);
        let x = if left <= right {
            x_px.clamp(left, right)
        } else {
            x_px
        };
        self.buffer
            .hit(x, y)
            .and_then(|cursor| Some(self.line_offsets.get(cursor.line)? + cursor.index))
            .map_or(0, |byte| text.floor_char_boundary(byte.min(text.len())))
    }

    pub fn link_points<'a>(&'a self, text: &str, range: [usize; 2]) -> impl Iterator<Item = (f64, f64)> + 'a {
        let mut utf16 = 0;
        let mut bytes = [text.len(); 2];
        for (byte, character) in text.char_indices() {
            for index in 0..2 {
                if utf16 == range[index] { bytes[index] = byte; }
            }
            utf16 += character.len_utf16();
            if utf16 >= range[1] && bytes[1] == text.len() {
                bytes[1] = byte + character.len_utf8();
                break;
            }
        }
        let scale = f64::from(self.scale);
        self.buffer.layout_runs().take(self.visible_lines).flat_map(move |line| {
            let line_offset = self.line_offsets[line.line_i];
            let inset = line_x(&self.buffer, &line);
            line.glyphs.iter().filter_map(move |glyph| {
                let start = line_offset + glyph.start;
                let end = line_offset + glyph.end;
                (start < bytes[1] && bytes[0] < end && glyph.w > 0.0).then(|| (
                    f64::from(inset + glyph.x + glyph.w / 2.0) / scale,
                    f64::from(line.line_top + line.line_height / 2.0) / scale,
                ))
            })
        })
    }

    pub fn click_offset(&self, text: &str, x: f64, y: f64) -> Option<usize> {
        self.action?;
        if self.link_ranges.is_empty() {
            return Some(text[..self.offset_at(text, x, y)].encode_utf16().count());
        }
        let (x, y) = (x as f32 * self.scale, y as f32 * self.scale);
        let line = self
            .buffer
            .layout_runs()
            .take(self.visible_lines)
            .find(|line| (line.line_top..line.line_top + line.line_height).contains(&y))?;
        let x = x - line_x(&self.buffer, &line);
        let glyph = line.glyphs.iter().find(|g| (g.x..g.x + g.w).contains(&x))?;
        let byte = *self.line_offsets.get(line.line_i)? + glyph.start;
        let offset = text.get(..byte)?.encode_utf16().count();
        self.link_ranges
            .iter()
            .any(|[start, end]| (*start..*end).contains(&offset))
            .then_some(offset)
    }

    pub fn selection_caret(&self, byte: usize, end: bool) -> Option<(Rect, bool)> {
        let target = if end { byte.saturating_sub(1) } else { byte };
        for line in self.buffer.layout_runs().take(self.visible_lines) {
            let offset = self.line_offsets[line.line_i];
            let Some(local) = target.checked_sub(offset) else {
                continue;
            };
            let Some(glyph) = line
                .glyphs
                .iter()
                .find(|glyph| (glyph.start..glyph.end).contains(&local))
            else {
                continue;
            };
            let rtl = glyph.level.is_rtl();
            let local = byte.saturating_sub(offset);
            let x = if local == glyph.end {
                if rtl {
                    glyph.x
                } else {
                    glyph.x + glyph.w
                }
            } else {
                line.cursor_position(&Cursor::new(line.line_i, local))?
            };
            return Some((
                Rect {
                    x: f64::from((x + line_x(&self.buffer, &line)) / self.scale),
                    y: f64::from(line.line_top / self.scale),
                    w: 0.0,
                    h: f64::from(line.line_height / self.scale),
                },
                rtl,
            ));
        }
        None
    }

    pub fn speak(&self, text: &str, range: Range<usize>) -> String {
        let mut at = range.start;
        let mut out = String::new();
        let boundaries = text
            .char_indices()
            .scan(0, |index, (byte, c)| {
                let result = (*index, byte);
                *index += c.len_utf16();
                Some(result)
            })
            .chain([(text.encode_utf16().count(), text.len())])
            .collect::<Vec<_>>();
        let byte = |i| {
            boundaries
                .binary_search_by_key(&i, |(offset, _)| *offset)
                .ok()
                .map(|i| boundaries[i].1)
        };
        for (start, end, spoken) in &self.speech_ranges {
            let (Some(start), Some(end)) = (byte(*start), byte(*end)) else {
                continue;
            };
            if at <= start && end <= range.end && start < end {
                out.push_str(&self.selected_string(text, at..start).unwrap_or_default());
                out.push_str(spoken);
                at = end;
            }
        }
        out.push_str(
            &self
                .selected_string(text, at..range.end)
                .unwrap_or_default(),
        );
        out
    }

    pub fn rects(&self, range: Range<usize>, bounds: Rect) -> Vec<Rect> {
        if range.is_empty() {
            return vec![];
        }
        let cursor = |byte| {
            let line = self
                .line_offsets
                .partition_point(|i| *i <= byte)
                .saturating_sub(1);
            Cursor::new(
                line,
                byte.saturating_sub(self.line_offsets.get(line).copied().unwrap_or(0)),
            )
        };
        let (start, end) = (cursor(range.start), cursor(range.end));
        let scale = f64::from(self.scale);
        self.buffer
            .layout_runs()
            .take(self.visible_lines)
            .filter(|line| (start.line..=end.line).contains(&line.line_i))
            .flat_map(|line| {
                let y = (f64::from(line.line_top) / scale).clamp(0.0, bounds.h);
                let bottom = (y + f64::from(line.line_height) / scale).min(bounds.h);
                let inset = line_x(&self.buffer, &line);
                line.highlight(start, end).filter_map(move |(x, width)| {
                    let x = x + inset;
                    let right = (f64::from(x + width) / scale).clamp(0.0, bounds.w);
                    let x = (f64::from(x) / scale).clamp(0.0, bounds.w);
                    (right > x && bottom > y).then_some(Rect {
                        x,
                        y,
                        w: right - x,
                        h: bottom - y,
                    })
                })
            })
            .collect()
    }

    pub fn layout_data(
        &self,
        run: &TextRun,
        bounds: Rect,
    ) -> Option<onesim_protocol::ComposeTextLayout> {
        use std::fmt::Write;
        use std::hash::{Hash, Hasher};
        let id = self.layout_callback?;
        let mut hash = std::hash::DefaultHasher::new();
        format!("{run:?}").hash(&mut hash);
        self.original_text.hash(&mut hash);
        self.pending_auto_size.hash(&mut hash);
        let scale = f64::from(self.scale);
        let (width, height) = ((bounds.w * scale).round(), (bounds.h * scale).round());
        let pixels = self
            .constraints
            .map(|value| (value * scale).round().clamp(0.0, f64::from(i32::MAX)) as i32);
        let mut geometry = format!(
            "{width},{height},{},{},{},{},{},{};{}",
            self.scale,
            pixels[0],
            pixels[1],
            pixels[2],
            pixels[3],
            self.auto_font_size.unwrap_or(-1.0),
            hash.finish()
        );
        for rect in self.inline_rects(&run.text, run.font.pt) {
            if let Some(rect) = rect {
                let _ = write!(
                    geometry,
                    ",{},{},{},{}",
                    rect.x * scale,
                    rect.y * scale,
                    (rect.x + rect.w) * scale,
                    (rect.y + rect.h) * scale
                );
            } else {
                geometry.push_str(",0,0,-1,-1");
            }
        }
        geometry.push(';');
        let mut utf16 = vec![0; run.text.len() + 1];
        let mut index = 0;
        for (i, c) in run.text.char_indices() {
            utf16[i..i + c.len_utf8()].fill(index);
            index += c.len_utf16();
        }
        utf16[run.text.len()] = index;
        let mut lines = 0;
        let mut rows = self.buffer.layout_runs().peekable();
        while lines < self.visible_lines {
            let Some(line) = rows.next() else { break };
            let offset = self.line_offsets[line.line_i];
            let to_utf16 = |i: usize| self.original_offset(utf16[(offset + i).min(run.text.len())]);
            let start = line.glyphs.iter().map(|g| g.start).min().unwrap_or(0);
            let end = line.glyphs.iter().map(|g| g.end).max().unwrap_or(start);
            let visible = line.text.get(start..end).map_or(end, |s| {
                start + s.trim_end_matches(char::is_whitespace).len()
            });
            let logical_end = rows.peek().filter(|next| next.line_i == line.line_i).map_or_else(
                || line.text.len() + self.buffer.lines[line.line_i].ending().as_str().len(),
                |next| next.glyphs.iter().map(|glyph| glyph.start).min().unwrap_or(end).max(end),
            );
            let left = line
                .glyphs
                .iter()
                .map(|g| g.x)
                .reduce(f32::min)
                .unwrap_or(0.0);
            let right = line
                .glyphs
                .iter()
                .map(|g| g.x + g.w)
                .reduce(f32::max)
                .unwrap_or(left);
            let inset = line_x(&self.buffer, &line);
            let bottom = line.line_top + line.line_height;
            let _ = write!(
                geometry,
                "{},{},{},{},{},{},{},{},{}",
                to_utf16(start),
                to_utf16(logical_end),
                to_utf16(visible),
                left + inset,
                right + inset,
                line.line_top,
                bottom,
                line.line_y,
                usize::from(line.rtl)
            );
            for g in line.glyphs {
                let _ = write!(
                    geometry,
                    ",{},{},{},{},{}",
                    to_utf16(g.start),
                    to_utf16(g.end),
                    inset + g.x,
                    inset + g.x + g.w,
                    usize::from(g.level.is_rtl())
                );
            }
            geometry.push(';');
            lines += 1;
        }
        geometry.push('w');
        let source = self.original_text.as_deref().unwrap_or(&run.text);
        let (mut byte, mut offset) = (0, 0);
        for boundary in crate::line_break::word_boundaries(source) {
            offset += source[byte..boundary].encode_utf16().count();
            byte = boundary;
            let _ = write!(geometry, ",{offset}");
        }
        geometry.push(';');
        let exceeded_max_lines = self.untruncated_extent.is_some_and(|(_, _, lines)| lines > self.visible_lines)
            || lines < self.buffer.layout_runs().count();
        let overflow_height = exceeded_max_lines
            || self
                .buffer
                .layout_runs()
                .any(|r| f64::from(r.line_top + r.line_height) > height + 0.5);
        let overflow_width = self
            .buffer
            .layout_runs()
            .any(|r| f64::from(r.line_w + line_indent(&self.buffer.lines[r.line_i])) > width + 0.5);
        let ellipsized = self.untruncated_extent.is_some_and(|(full_width, _, _)| {
            exceeded_max_lines || (f64::from(full_width) * scale) > width + 0.5
        });
        let _ = write!(
            geometry,
            "{}",
            usize::from(overflow_height) | (usize::from(overflow_width) << 1) | (usize::from(exceeded_max_lines) << 2) | (usize::from(ellipsized) << 3)
        );
        Some(onesim_protocol::ComposeTextLayout { id, geometry })
    }
}
