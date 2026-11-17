//! iOS platform chrome drawn by the host: the software keyboard and the Dynamic Island.
//!
//! Geometry and colors are measured from iPhone 17 Pro / iOS 26 simulator screenshots
//! (`tests/fixtures/showcase-ios`). The keyboard is built as ordinary scene nodes appended after
//! the app content, so hit testing, inspection and rasterization need no special cases.
//!
//! Known simplifications: the suggestion bar is static ("I", "The", "I'm", not tappable, no
//! prediction engine) and the 123 / emoji / dictation keys are inert (no alternate layers).

use crate::scene::{Color, Glyph, Node, Rect, Role, TextRun};
use crate::text::{FontSpec, TextAlign};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Keyboard height above the bottom safe inset (measured 335 total with a 34pt inset).
pub const KB_BASE_H: f64 = 301.0;

/// Chrome inputs for one layout pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChromeParams {
    /// Show the software keyboard (a text field is focused).
    pub keyboard: bool,
    /// Shift state (host-owned: auto-capitalization, shift key and typing update it).
    pub shift: bool,
    /// Dynamic Island `(x, y, w, h)` in points.
    pub island: Option<(f64, f64, f64, f64)>,
}

/// Auto-capitalization: start of text, or after sentence punctuation plus a space.
pub fn autocap(text: &str) -> bool {
    text.is_empty() || [". ", "! ", "? "].iter().any(|p| text.ends_with(p))
}

fn c(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

struct Theme {
    panel: Color,
    key: Color,
    label: Color,
    glyph: Color,
    suggest: Color,
    divider: Color,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            panel: c(23, 23, 23, 1.0),
            key: c(61, 61, 61, 1.0),
            label: c(255, 255, 255, 1.0),
            glyph: c(255, 255, 255, 1.0),
            suggest: c(172, 172, 172, 1.0),
            divider: c(50, 50, 52, 1.0),
        }
    } else {
        Theme {
            panel: c(223, 224, 230, 1.0),
            key: c(255, 255, 255, 1.0),
            label: c(0, 0, 0, 1.0),
            glyph: c(0, 0, 0, 1.0),
            suggest: c(51, 52, 58, 1.0),
            divider: c(205, 206, 213, 1.0),
        }
    }
}

fn text_run(text: &str, size: f32, weight: u16, color: Color) -> TextRun {
    let font = FontSpec::new(size, size * 1.1934, weight, TextAlign::Center);
    TextRun { text: text.to_string(), font, color, strike: false, offset_x: 0.0 }
}

struct Builder<'a> {
    out: &'a mut Vec<Node>,
    th: Theme,
}

impl Builder<'_> {
    fn push(&mut self, mut n: Node, parent: usize, r: Rect) -> usize {
        n.bounds = r;
        n.parent = Some(parent);
        self.out.push(n);
        self.out.len() - 1
    }

    fn key(&mut self, parent: usize, id: &str, label: &str, r: Rect, glyph: Option<Glyph>, text: Option<(&str, f32)>) {
        let mut n = Node::new(format!("keyboard#key-{id}"), "key");
        n.role = Role::Key;
        n.label = Some(label.to_string());
        n.fill = Some(self.th.key);
        n.radius = 7.0;
        n.glyph = glyph.map(|g| (g, self.th.label));
        let k = self.push(n, parent, r);
        if let Some((t, size)) = text {
            let lh = size as f64 * 1.1934;
            let mut tn = Node::new(format!("keyboard#key-{id}#text"), "text");
            tn.text = Some(text_run(t, size, 400, self.th.label));
            tn.inspect = false;
            // The visual center of caps sits a little above the line-box center.
            let y = r.y + (r.h - lh) / 2.0 - 1.8;
            self.push(tn, k, Rect { x: r.x, y, w: r.w, h: lh });
        }
    }
}

/// Append the keyboard (panel, suggestion bar, keys) for a `w` x `h` screen. Returns the top y.
/// `shift` is the effective shift state; `kb_h` the total keyboard height.
pub fn keyboard(out: &mut Vec<Node>, w: f64, h: f64, kb_h: f64, dark: bool, shift: bool) {
    let th = theme(dark);
    let top = h - kb_h;
    let mut kb = Node::new("keyboard", "keyboard");
    kb.value = Some(if shift { "shift" } else { "lower" }.to_string());
    kb.label = Some("keyboard".to_string());
    kb.bounds = Rect { x: 0.0, y: top, w, h: kb_h };
    kb.parent = Some(0);
    out.push(kb);
    let root = out.len() - 1;
    let mut b = Builder { out, th };

    // Panel: 30pt top corners, ~55pt (device) bottom corners, from two overlapping rounded rects.
    for (id, y, hh, r) in [("top", top, kb_h - 35.0, 28.0), ("bottom", h - 120.0, 120.0, 55.0)] {
        let mut n = Node::new(format!("keyboard#panel-{id}"), "panel");
        n.fill = Some(b.th.panel);
        n.radius = r;
        n.inspect = false;
        b.push(n, root, Rect { x: 0.0, y, w, h: hh });
    }

    // Static suggestions (no prediction engine), with hairline dividers.
    let third = w / 3.0;
    for (i, s) in ["I", "The", "I\u{2019}m"].iter().enumerate() {
        let mut n = Node::new(format!("keyboard#suggestion-{i}"), "suggestion");
        n.role = Role::Text;
        n.label = Some(s.to_string());
        let lh = 17.0 * 1.1934;
        n.text = Some(text_run(s, 17.0, 400, b.th.suggest));
        b.push(n, root, Rect { x: third * i as f64, y: top + 24.8 - lh / 2.0, w: third, h: lh });
        if i > 0 {
            let mut d = Node::new(format!("keyboard#divider-{i}"), "divider");
            d.fill = Some(b.th.divider);
            d.inspect = false;
            b.push(d, root, Rect { x: third * i as f64 - 0.5, y: top + 13.0, w: 1.0, h: 23.7 });
        }
    }

    // Keys. Letters are shown upper-case like iOS 26; the typed case follows shift.
    let (gap, margin, kh) = (6.0, 6.67, 42.8);
    let kw = (w - 2.0 * margin - 9.0 * gap) / 10.0;
    let pitch = kw + gap;
    let ys = [top + 52.5, top + 106.4, top + 160.3, top + 214.2];
    let row = |chars: &str, y: f64, x0: f64, b: &mut Builder| {
        for (i, ch) in chars.chars().enumerate() {
            let l = ch.to_string();
            let shown = if shift { l.to_uppercase() } else { l.clone() };
            b.key(root, &l, &l, Rect { x: x0 + pitch * i as f64, y, w: kw, h: kh }, None, Some((&shown, 22.0)));
        }
    };
    row("qwertyuiop", ys[0], margin, &mut b);
    row("asdfghjkl", ys[1], (w - (8.0 * pitch + kw)) / 2.0, &mut b);
    row("zxcvbnm", ys[2], margin + 44.7 + 14.4, &mut b);
    let sg = if shift { Glyph::ShiftFilled } else { Glyph::Shift };
    b.key(root, "shift", "shift", Rect { x: margin, y: ys[2], w: 44.7, h: kh }, Some(sg), None);
    b.key(root, "delete", "delete", Rect { x: w - margin - 44.7, y: ys[2], w: 44.7, h: kh }, Some(Glyph::Backspace), None);
    let wide = 92.7;
    b.key(root, "123", "123", Rect { x: margin, y: ys[3], w: wide, h: kh }, None, Some(("123", 17.0)));
    b.key(root, "space", "space", Rect { x: margin + wide + gap, y: ys[3], w: w - 2.0 * (margin + wide + gap), h: kh }, None, None);
    b.key(root, "return", "return", Rect { x: w - margin - wide, y: ys[3], w: wide, h: kh }, Some(Glyph::Return), None);

    // Emoji and dictation sit in the panel below the keys, without key caps.
    for (id, g, cx) in [("emoji", Glyph::Emoji, 42.4), ("dictation", Glyph::Mic, w - 42.4)] {
        let mut n = Node::new(format!("keyboard#key-{id}"), "key");
        n.role = Role::Key;
        n.label = Some(id.to_string());
        n.glyph = Some((g, b.th.glyph));
        b.push(n, root, Rect { x: cx - 22.0, y: h - 62.0, w: 44.0, h: 44.0 });
    }
}

/// The Dynamic Island: a black capsule drawn above everything.
pub fn island(out: &mut Vec<Node>, (x, y, w, h): (f64, f64, f64, f64)) {
    let mut n = Node::new("island", "island");
    n.fill = Some(c(0, 0, 0, 1.0));
    n.radius = h / 2.0;
    n.bounds = Rect { x, y, w, h };
    n.parent = Some(0);
    n.inspect = false;
    out.push(n);
}

fn paint(col: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8((col.r * 255.0).round() as u8, (col.g * 255.0).round() as u8, (col.b * 255.0).round() as u8, (col.a * 255.0).round() as u8);
    p
}

/// Draw a keyboard icon inside `(x, y, w, h)`. Returns false for non-keyboard glyphs.
pub fn draw_glyph(pm: &mut Pixmap, g: Glyph, col: Color, b: (f32, f32, f32, f32), xf: Transform, mask: Option<&Mask>) -> bool {
    let (cx, cy) = (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0);
    let p = paint(col);
    let st = |w: f32| Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let poly = |pts: &[(f32, f32)], close: bool| {
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in pts.iter().enumerate() {
            if i == 0 {
                pb.move_to(*x, *y);
            } else {
                pb.line_to(*x, *y);
            }
        }
        if close {
            pb.close();
        }
        pb.finish()
    };
    match g {
        Glyph::Shift | Glyph::ShiftFilled => {
            let pts = [
                (cx, cy - 8.0),
                (cx + 9.3, cy + 0.8),
                (cx + 4.6, cy + 0.8),
                (cx + 4.6, cy + 8.0),
                (cx - 4.6, cy + 8.0),
                (cx - 4.6, cy + 0.8),
                (cx - 9.3, cy + 0.8),
            ];
            if let Some(path) = poly(&pts, true) {
                if g == Glyph::ShiftFilled {
                    pm.fill_path(&path, &p, FillRule::Winding, xf, mask);
                }
                pm.stroke_path(&path, &p, &st(1.9), xf, mask);
            }
        }
        Glyph::Backspace => {
            let pts = [(cx - 9.8, cy), (cx - 4.6, cy - 7.4), (cx + 9.6, cy - 7.4), (cx + 9.6, cy + 7.4), (cx - 4.6, cy + 7.4)];
            if let Some(path) = poly(&pts, true) {
                pm.stroke_path(&path, &p, &st(1.6), xf, mask);
            }
            let (ox, d) = (cx + 2.6, 3.2);
            for pts in [[(ox - d, cy - d), (ox + d, cy + d)], [(ox - d, cy + d), (ox + d, cy - d)]] {
                if let Some(path) = poly(&pts, false) {
                    pm.stroke_path(&path, &p, &st(1.6), xf, mask);
                }
            }
        }
        Glyph::Return => {
            let cy = cy - 1.0;
            let mut pb = PathBuilder::new();
            pb.move_to(cx + 9.0, cy - 7.0);
            pb.line_to(cx + 9.0, cy + 0.5);
            pb.quad_to(cx + 9.0, cy + 4.0, cx + 5.5, cy + 4.0);
            pb.line_to(cx - 8.5, cy + 4.0);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &p, &st(1.9), xf, mask);
            }
            if let Some(path) = poly(&[(cx - 4.2, cy - 0.3), (cx - 8.6, cy + 4.0), (cx - 4.2, cy + 8.3)], false) {
                pm.stroke_path(&path, &p, &st(1.9), xf, mask);
            }
        }
        Glyph::Emoji => {
            if let Some(path) = PathBuilder::from_circle(cx, cy, 12.2) {
                pm.stroke_path(&path, &p, &st(2.0), xf, mask);
            }
            for dx in [-4.6, 4.6] {
                if let Some(path) = PathBuilder::from_oval(tiny_skia::Rect::from_xywh(cx + dx - 1.2, cy - 5.6, 2.4, 3.8).unwrap()) {
                    pm.fill_path(&path, &p, FillRule::Winding, xf, mask);
                }
            }
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 7.4, cy + 1.2);
            pb.line_to(cx + 7.4, cy + 1.2);
            pb.cubic_to(cx + 7.4, cy + 11.0, cx - 7.4, cy + 11.0, cx - 7.4, cy + 1.2);
            pb.close();
            if let Some(path) = pb.finish() {
                pm.fill_path(&path, &p, FillRule::Winding, xf, mask);
            }
        }
        Glyph::Mic => {
            if let Some(path) = tiny_skia::Rect::from_xywh(cx - 4.8, cy - 14.5, 9.6, 17.5) {
                let mut pb = PathBuilder::new();
                let r = 4.8;
                pb.move_to(path.left(), path.top() + r);
                pb.cubic_to(path.left(), path.top() - 1.6, path.right(), path.top() - 1.6, path.right(), path.top() + r);
                pb.line_to(path.right(), path.bottom() - r);
                pb.cubic_to(path.right(), path.bottom() + 1.6, path.left(), path.bottom() + 1.6, path.left(), path.bottom() - r);
                pb.close();
                if let Some(pp) = pb.finish() {
                    pm.stroke_path(&pp, &p, &st(1.8), xf, mask);
                }
            }
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 8.6, cy - 1.0);
            pb.cubic_to(cx - 8.6, cy + 12.0, cx + 8.6, cy + 12.0, cx + 8.6, cy - 1.0);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &p, &st(1.8), xf, mask);
            }
            for pts in [[(cx, cy + 8.0), (cx, cy + 13.5)], [(cx - 4.6, cy + 13.5), (cx + 4.6, cy + 13.5)]] {
                if let Some(path) = poly(&pts, false) {
                    pm.stroke_path(&path, &p, &st(1.8), xf, mask);
                }
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocap_rules() {
        assert!(autocap("") && autocap("Hi. ") && autocap("What? "));
        assert!(!autocap("Hi") && !autocap("Hi.") && !autocap("a b "));
    }
}
