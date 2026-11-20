//! Platform chrome drawn by the host, including the iOS keyboard and Android system bars.
//!
//! Geometry and colors are measured from iPhone 17 Pro / iOS 26 simulator screenshots
//! (`tests/fixtures/showcase-ios`). The keyboard is built as ordinary scene nodes appended after
//! the app content, so hit testing, inspection and rasterization need no special cases.
//!
//! Known simplifications: the suggestion bar is static ("I", "The", "I'm", not tappable, no
//! prediction engine) and the 123 / emoji / dictation keys are inert (no alternate layers).

use crate::device::Device;
use crate::look::Look;
use crate::SceneParams;
use crate::scene::{Color, Glyph, Node, Rect, Role, TextRun};
use crate::text::{android_glyph_size, FontSpec, Fonts, TextAlign};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Android status bar over the top inset: clock at the start, wifi and battery at the end (black icons on a
/// light app, white in dark theme: `enableEdgeToEdge`'s default auto contrast). Geometry measured on the
/// Android 17 Pixel 10a emulator in demo mode (tests/fixtures/compose-android/capture.sh).
// ponytail: no cellular signal glyph (the emulator reference hides it).
pub(crate) fn android_status_bar(nodes: &mut Vec<Node>, params: &SceneParams, fonts: &mut Fonts, dark: bool) {
    let dev = params.device;
    let top = dev.safe_top;
    let fg = if dark { Color::rgba(1.0, 1.0, 1.0, 1.0) } else { Color::rgba(0.0, 0.0, 0.0, 1.0) };
    let mut bar = Node::new("statusbar", "statusbar");
    bar.bounds = Rect { x: 0.0, y: 0.0, w: params.viewport.0, h: top };
    bar.parent = Some(0);
    bar.inspect = false;
    let p = nodes.len();
    nodes.push(bar);
    let mut add = |id: &str, kind: &'static str, bounds: Rect, f: &dyn Fn(&mut Node)| {
        let mut n = Node::new(id, kind);
        n.bounds = bounds;
        n.parent = Some(p);
        n.inspect = false;
        f(&mut n);
        nodes.push(n);
    };

    let mut spec = FontSpec::new(android_glyph_size(dev.status_bar.font as f32, dev.scale as f32), 20.0, 400, TextAlign::Leading);
    spec.pitch = 20.0;
    spec.tracking = 0.0;
    spec.android = true;
    let (tw, th) = fonts.measure("9:41", &spec, None, &[]);
    let (cx0, cy0) = dev.status_bar.time;
    let run = TextRun { text: "9:41".into(), font: spec, color: fg, strike: false, offset_x: 0.0, spans: vec![] };
    add("statusbar#time", "text", Rect { x: cx0 - tw / 2.0, y: cy0 - th / 2.0, w: tw.ceil(), h: th }, &|n| n.text = Some(run.clone()));

    // Battery: a horizontal 23.8x12.9 dp pill with a 1.6x6.5 dp terminal nub, right edge at `icons.0`.
    let (right, _) = dev.status_bar.icons;
    let cy = cy0 + 0.6;
    add("statusbar#battery-tip", "battery", Rect { x: right - 1.6, y: cy - 3.25, w: 1.6, h: 6.5 }, &|n| {
        n.fill = Some(fg);
        n.radius = 0.6;
    });
    let body = Rect { x: right - 1.6 - 1.1 - 23.8, y: cy - 6.45, w: 23.8, h: 12.9 };
    add("statusbar#battery", "battery", body, &|n| {
        n.fill = Some(fg);
        n.radius = 3.2;
    });
    add("statusbar#wifi", "wifi", Rect { x: body.x - 6.2 - 16.0, y: cy0 - 5.6, w: 16.0, h: 11.8 }, &|n| n.glyph = Some((Glyph::AndroidWifi, fg)));
}

/// Gesture navigation handle centred in the bottom inset (60% black on light apps, 92% white on dark).
pub(crate) fn android_nav_handle(nodes: &mut Vec<Node>, params: &SceneParams, dark: bool) {
    let (w, h) = params.viewport;
    let inset = params.device.safe_bottom;
    if inset <= 0.0 {
        return;
    }
    let mut bar = Node::new("navigation_handle", "navigation_handle");
    let (bw, bh) = (108.0, 4.0);
    bar.bounds = Rect { x: (w - bw) / 2.0, y: h - inset / 2.0 - bh / 2.0, w: bw, h: bh };
    bar.parent = Some(0);
    bar.radius = bh / 2.0;
    bar.fill = Some(if dark { Color::rgba(1.0, 1.0, 1.0, 0.92) } else { Color::rgba(0.0, 0.0, 0.0, 0.6) });
    bar.inspect = false;
    nodes.push(bar);
}

/// Chrome inputs for one layout pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChromeParams {
    /// Show the software keyboard (a text field is focused).
    pub keyboard: bool,
    /// Shift state (host-owned: auto-capitalization, shift key and typing update it).
    pub shift: bool,
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

fn theme(dark: bool, look: &Look) -> Theme {
    if dark && look.kb_dark_27 {
        // iOS 27 (keyboard-dark.png): lighter panel and keys.
        Theme {
            panel: c(32, 32, 32, 1.0),
            key: c(68, 68, 68, 1.0),
            label: c(255, 255, 255, 1.0),
            glyph: c(255, 255, 255, 1.0),
            suggest: c(172, 172, 172, 1.0),
            divider: c(56, 56, 58, 1.0),
        }
    } else if dark {
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
    TextRun { text: text.to_string(), font, color, strike: false, offset_x: 0.0, spans: vec![] }
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
#[allow(clippy::too_many_arguments)]
pub fn keyboard(out: &mut Vec<Node>, w: f64, h: f64, kb_h: f64, dark: bool, shift: bool, dev: &Device, look: &Look) {
    // Emoji / dictation stay where the device's iOS 26 keyboard puts them when the panel height changes (iOS 27).
    let kb_shrink = dev.kb_base_h + dev.safe_bottom - kb_h;
    let th = theme(dark, look);
    let focused = out.iter().find(|n| n.focused && n.field.is_some());
    let input = focused.and_then(|n| n.input.as_deref());
    let hide_suggestions = focused.is_some_and(|n| n.secure)
        || input.is_some_and(|i| i.autocorrection_disabled == Some(true));
    let submit = match input.and_then(|i| i.submit_label.as_deref()) {
        Some("done") => Some("Done"),
        Some("go") => Some("Go"),
        Some("send") => Some("Send"),
        Some("join") => Some("Join"),
        Some("route") => Some("Route"),
        Some("search") => Some("Search"),
        Some("next") => Some("Next"),
        Some("continue") => Some("Continue"),
        _ => None,
    };
    let key_type = input.and_then(|i| i.keyboard_type.as_deref());
    let phone = key_type == Some("phonePad");
    let keypad = match key_type {
        Some("numberPad" | "asciiCapableNumberPad") => Some(""),
        Some("decimalPad") => Some("."),
        Some("phonePad") => Some("*"),
        _ => None,
    };
    // Suggestions never use the contents of a secure field.
    let word = focused
        .filter(|_| !hide_suggestions)
        .and_then(|n| n.value.as_deref())
        .and_then(|v| v.rsplit(char::is_whitespace).next())
        .filter(|w| !w.is_empty() && look.typed_word_suggestion)
        .map(|w| format!("\u{201C}{w}\u{201D}"));
    let top = h - kb_h;
    let mut kb = Node::new("keyboard", "keyboard");
    kb.value = Some(if shift { "shift" } else { "lower" }.to_string());
    kb.label = Some("keyboard".to_string());
    kb.bounds = Rect { x: 0.0, y: top, w, h: kb_h };
    kb.parent = Some(0);
    out.push(kb);
    let root = out.len() - 1;
    let mut b = Builder { out, th };
    if dev.wide_keyboard && keypad.is_none() {
        return wide(&mut b, root, w, top, kb_h, &word, shift, look, submit, hide_suggestions);
    }

    // Panel: 30pt top corners, ~55pt (device) bottom corners, from two overlapping rounded rects.
    for (id, y, hh, r) in [("top", top, kb_h - 35.0, if dev.home_button { 22.0 } else { 28.0 }), ("bottom", h - 120.0, 120.0, if dev.home_button { 0.0 } else { 55.0 })] {
        let mut n = Node::new(format!("keyboard#panel-{id}"), "panel");
        n.fill = Some(b.th.panel);
        n.radius = r;
        n.inspect = false;
        b.push(n, root, Rect { x: 0.0, y, w, h: hh });
    }

    if let Some(extra) = keypad {
        let gap = 6.0;
        let width = (w - 4.0 * gap) / 3.0;
        let last_key = if phone { "#" } else { "delete" };
        let rows = if phone { 5.0 } else { 4.0 };
        let height = (kb_h - dev.safe_bottom - (rows + 1.0) * gap) / rows;
        for (i, key) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", extra, "0", last_key, "+", "", "delete"]
            .into_iter().take(if phone { 15 } else { 12 }).enumerate()
        {
            if key.is_empty() {
                continue;
            }
            let bounds = Rect { x: gap + (i % 3) as f64 * (width + gap), y: top + gap + (i / 3) as f64 * (height + gap), w: width, h: height };
            let glyph = (key == "delete").then_some(Glyph::Backspace);
            b.key(root, key, key, bounds, glyph, glyph.is_none().then_some((key, 24.0)));
        }
        return;
    }

    // Static suggestions (no prediction engine), with hairline dividers.
    let third = w / 3.0;
    let words: [&str; 3] = match &word {
        // ponytail: no prediction engine, so the two completions next to the typed word stay empty.
        Some(wd) => [wd.as_str(), "", ""],
        None => ["I", "The", "I\u{2019}m"],
    };
    for (i, s) in words.iter().enumerate() {
        // iOS 27 references were all captured with the suggestion words; iOS 26 only on the 17 Pro.
        if !hide_suggestions && (dev.suggestions || look.always_suggestions) && !s.is_empty() {
            let mut n = Node::new(format!("keyboard#suggestion-{i}"), "suggestion");
            n.role = Role::Text;
            n.label = Some(s.to_string());
            let lh = 17.0 * 1.1934;
            n.text = Some(text_run(s, 17.0, 400, b.th.suggest));
            b.push(n, root, Rect { x: third * i as f64, y: top + 24.8 - lh / 2.0, w: third, h: lh });
        }
        if !hide_suggestions && i > 0 {
            let mut d = Node::new(format!("keyboard#divider-{i}"), "divider");
            d.fill = Some(b.th.divider);
            d.inspect = false;
            b.push(d, root, Rect { x: third * i as f64 - 0.5, y: top + 13.0, w: 1.0, h: 23.7 });
        }
    }

    // Keys. iOS 26 shows letters upper-case (the typed case follows shift); iOS 27 shows the shift state.
    let (gap, margin) = (6.0, 6.67);
    let (pitch_y, kh) = (dev.key_pitch, if dev.home_button { 41.5 } else { dev.key_pitch - 11.1 });
    let kw = (w - 2.0 * margin - 9.0 * gap) / 10.0;
    let pitch = kw + gap;
    let ys = [0.0, 1.0, 2.0, 3.0].map(|i| top + 52.5 + i * pitch_y);
    let caps = shift || !look.key_case_follows_shift;
    let row = |chars: &str, y: f64, x0: f64, b: &mut Builder| {
        for (i, ch) in chars.chars().enumerate() {
            let l = ch.to_string();
            let shown = if caps { l.to_uppercase() } else { l.clone() };
            b.key(root, &l, &l, Rect { x: x0 + pitch * i as f64, y, w: kw, h: kh }, None, Some((&shown, 22.0)));
        }
    };
    // Function keys are a fixed multiple of the key pitch (measured across devices); 17 Pro: 44.7 / 92.7.
    let (side, wide) = (1.297 * pitch - 6.5, 2.5 * pitch - 6.0);
    row("qwertyuiop", ys[0], margin, &mut b);
    row("asdfghjkl", ys[1], (w - (8.0 * pitch + kw)) / 2.0, &mut b);
    row("zxcvbnm", ys[2], (w - (6.0 * pitch + kw)) / 2.0, &mut b);
    let sg = if shift { Glyph::ShiftFilled } else { Glyph::Shift };
    b.key(root, "shift", "shift", Rect { x: margin, y: ys[2], w: side, h: kh }, Some(sg), None);
    b.key(root, "delete", "delete", Rect { x: w - margin - side, y: ys[2], w: side, h: kh }, Some(Glyph::Backspace), None);
    if dev.home_button {
        // Bottom row (47pt touch rows): 123, emoji, dictation, space, return; touch frames inset 3pt.
        let (y, bh) = (ys[3], kh);
        let mut x = margin - 0.17;
        let mut put = |id: &str, tw: f64, g: Option<Glyph>, t: Option<(&str, f32)>, b: &mut Builder| {
            b.key(root, id, id, Rect { x, y, w: tw, h: bh }, g, t);
            x += tw + 6.0;
        };
        let (emoji, mic) = (Some(Glyph::Emoji), Some(Glyph::Mic));
        let space = w - 2.0 * (margin - 0.17) - 4.0 * gap - 40.0 - 40.0 - 31.0 - 86.0;
        put("123", 40.0, None, Some(("123", 17.0)), &mut b);
        put("emoji", 40.0, emoji, None, &mut b);
        put("dictation", 31.0, mic, None, &mut b);
        put("space", space, None, None, &mut b);
        put("return", 86.0, submit.is_none().then_some(Glyph::Return), submit.map(|label| (label, 16.0)), &mut b);
        return;
    }
    b.key(root, "123", "123", Rect { x: margin, y: ys[3], w: wide, h: kh }, None, Some(("123", 17.0)));
    b.key(root, "space", "space", Rect { x: margin + wide + gap, y: ys[3], w: w - 2.0 * (margin + wide + gap), h: kh }, None, None);
    b.key(root, "return", "return", Rect { x: w - margin - wide, y: ys[3], w: wide, h: kh }, submit.is_none().then_some(Glyph::Return), submit.map(|label| (label, 16.0)));

    // Emoji and dictation sit in the panel below the keys, without key caps.
    for (id, g, cx) in [("emoji", Glyph::Emoji, 42.4), ("dictation", Glyph::Mic, w - 42.4)] {
        let mut n = Node::new(format!("keyboard#key-{id}"), "key");
        n.role = Role::Key;
        n.label = Some(id.to_string());
        n.glyph = Some((g, b.th.glyph));
        b.push(n, root, Rect { x: cx - 22.0, y: ys[3] + kh + 16.0 - kb_shrink, w: 44.0, h: 44.0 });
    }
}

/// Wide (iPad-style) keyboard of the unfolded iPhone Duo (tests/fixtures/ios27/devices/iphone-duo):
/// a full-width panel, an 801pt key block centred, 79.2pt key pitch, caps inset 2pt / 8pt and 38.5pt tall.
#[allow(clippy::too_many_arguments)]
fn wide(
    b: &mut Builder,
    root: usize,
    w: f64,
    top: f64,
    kb_h: f64,
    word: &Option<String>,
    shift: bool,
    look: &Look,
    submit: Option<&str>,
    hide_suggestions: bool,
) {
    for (id, y, hh, r) in [
        ("top", top, kb_h, 26.0),
        ("bottom", top + 26.0, kb_h - 26.0, 0.0),
    ] {
        let mut n = Node::new(format!("keyboard#panel-{id}"), "panel");
        n.fill = Some(b.th.panel);
        n.radius = r;
        n.inspect = false;
        b.push(
            n,
            root,
            Rect {
                x: 0.0,
                y,
                w,
                h: hh,
            },
        );
    }
    let l = (w - 801.0) / 2.0;
    // "More" (three dots) left of the suggestions.
    for i in 0..3 {
        let mut d = Node::new(format!("keyboard#more-{i}"), "dot");
        d.fill = Some(c(145, 146, 150, 1.0));
        d.radius = 1.6;
        d.inspect = false;
        b.push(
            d,
            root,
            Rect {
                x: 32.9 + 8.0 * i as f64,
                y: top + 22.4,
                w: 3.2,
                h: 3.2,
            },
        );
    }
    let words: [&str; 3] = match word {
        Some(wd) => [wd.as_str(), "", ""],
        None => ["I", "The", "I\u{2019}m"],
    };
    for (i, s) in words.iter().enumerate() {
        let x = l + 267.0 * i as f64;
        if !hide_suggestions && !s.is_empty() {
            let mut n = Node::new(format!("keyboard#suggestion-{i}"), "suggestion");
            n.role = Role::Text;
            n.label = Some(s.to_string());
            let lh = 17.0 * 1.1934;
            n.text = Some(text_run(s, 17.0, 400, b.th.suggest));
            b.push(
                n,
                root,
                Rect {
                    x,
                    y: top + 24.8 - lh / 2.0,
                    w: 267.0,
                    h: lh,
                },
            );
        }
        if !hide_suggestions && i > 0 {
            let mut d = Node::new(format!("keyboard#divider-{i}"), "divider");
            d.fill = Some(b.th.divider);
            d.inspect = false;
            b.push(
                d,
                root,
                Rect {
                    x: x - 0.75,
                    y: top + 13.0,
                    w: 1.0,
                    h: 23.7,
                },
            );
        }
    }
    let caps = shift || !look.key_case_follows_shift;
    let rows = [49.0, 100.7, 152.7, 204.3].map(|d| top + d);
    let cap = |fx: f64, fy: f64, fw: f64| Rect {
        x: fx + 2.0,
        y: fy + 1.2,
        w: fw - 10.0,
        h: 38.5,
    };
    let letters = |chars: &str, y: f64, x0: f64, b: &mut Builder| {
        for (i, ch) in chars.chars().enumerate() {
            let k = ch.to_string();
            let shown = if caps { k.to_uppercase() } else { k.clone() };
            b.key(
                root,
                &k,
                &k,
                cap(x0 + 79.2 * i as f64, y, 79.0),
                None,
                Some((&shown, 25.0)),
            );
        }
    };
    letters("qwertyuiop", rows[0], l + 8.0, b);
    letters("asdfghjkl", rows[1], l + 47.7, b);
    letters("zxcvbnm", rows[2], l + 126.7, b);
    let sg = if shift {
        Glyph::ShiftFilled
    } else {
        Glyph::Shift
    };
    b.key(
        root,
        "shift",
        "shift",
        cap(l + 8.0, rows[2], 102.7),
        Some(sg),
        None,
    );
    b.key(
        root,
        "delete",
        "delete",
        cap(l + 696.3, rows[2], 102.7),
        Some(Glyph::Backspace),
        None,
    );
    b.key(
        root,
        "123",
        "123",
        cap(l + 8.0, rows[3], 111.0),
        None,
        Some(("123", 17.0)),
    );
    b.key(
        root,
        "emoji",
        "emoji",
        cap(l + 119.0, rows[3], 73.7),
        Some(Glyph::Emoji),
        None,
    );
    b.key(
        root,
        "space",
        "space",
        cap(l + 192.7, rows[3], 421.7),
        None,
        None,
    );
    b.key(
        root,
        "dictation",
        "dictation",
        cap(l + 614.3, rows[3], 73.7),
        Some(Glyph::Mic),
        None,
    );
    b.key(
        root,
        "return",
        "return",
        cap(l + 688.0, rows[3], 111.0),
        submit.is_none().then_some(Glyph::Return),
        submit.map(|label| (label, 16.0)),
    );
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
