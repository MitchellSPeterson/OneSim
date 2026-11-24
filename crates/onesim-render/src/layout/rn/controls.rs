use super::{Cx, Item, Leaf};
use crate::layout::common::SceneParams;
use crate::scene::{Color, Node, Rect, Role, TextRun};
use crate::text::TextAlign;
use serde_json::Value;

/// Height of a refreshing `RefreshControl`'s band above the content, its spinner box and default tint (iOS 26.5).
pub(super) const REFRESH_H: f64 = 181.0 / 3.0;
pub(super) const ANDROID_ACCENT: Color = Color::rgba(0.0, 0.59, 0.53, 1.0);
pub(super) const REFRESH_SPINNER: f64 = 28.0;
/// (light, dark): the leading spoke's colour.
pub(super) const REFRESH_TINT: (Color, Color) = (Color { r: 174.0 / 255.0, g: 174.0 / 255.0, b: 175.0 / 255.0, a: 1.0 }, Color { r: 73.0 / 255.0, g: 73.0 / 255.0, b: 79.0 / 255.0, a: 1.0 });

/// expo-blur `BlurView`: a `UIBlurEffect` style (by `tint`) faded in by `intensity` (0...100, the effect animator's
/// fraction, taken as linear), drawn as a flat glass: backdrop blur sigma (pt), saturation, then `c * mul + add`.
/// Fitted on iOS 26.5 per style at full intensity (`default` from its 50% capture).
// ponytail: all `system*Material` thicknesses share systemMaterial's fit, `extraLight` is `light`, `prominent` is `regular`.
/// Expo Blur's Android overlay for the default blur method or a missing blur target.
pub(super) fn android_blur_tint(tint: &str, intensity: f64) -> Color {
    let (gray, opacity) = match tint {
        "dark" | "systemMaterialDark" => (25, 0.69),
        "light" | "extraLight" | "systemMaterialLight" | "systemUltraThinMaterialLight" | "systemThickMaterialLight" => (249, 0.78),
        "regular" => (179, 0.82),
        "systemUltraThinMaterial" => (191, 0.44),
        "systemThinMaterial" => (199, 0.97),
        "systemThinMaterialLight" => (199, 0.78),
        "systemThickMaterial" => (153, 0.97),
        "systemChromeMaterial" => (255, 0.75),
        "systemChromeMaterialLight" => (255, 0.97),
        "systemUltraThinMaterialDark" => (37, 0.55),
        "systemThinMaterialDark" => (37, 0.70),
        "systemThickMaterialDark" => (37, 0.90),
        "systemChromeMaterialDark" => (0, 0.75),
        _ => (255, 0.44),
    };
    let g = gray as f32 / 255.0;
    let a = (255.0 * intensity.clamp(0.0, 100.0) / 100.0 * opacity).floor() as f32 / 255.0;
    Color::rgba(g, g, g, a)
}

pub(super) fn blur_spec(tint: &str, intensity: f64, dark: bool) -> crate::glass::GlassSpec {
    // (sigma, mul, sat, add rgb 0...255)
    const LIGHT: (f32, f32, f32, [f32; 3]) = (28.0, 0.72, 1.67, [76.0, 78.0, 64.0]);
    const DARK: (f32, f32, f32, [f32; 3]) = (16.0, 0.328, 0.88, [13.0, 11.0, 1.0]);
    const REGULAR: [(f32, f32, f32, [f32; 3]); 2] = [(26.0, 0.632, 1.36, [74.0, 98.0, 62.0]), (18.0, 0.308, 1.24, [2.0, 20.0, 6.0])];
    const MATERIAL: [(f32, f32, f32, [f32; 3]); 2] = [(26.0, 0.143, 1.58, [208.0, 205.0, 202.0]), (26.0, 0.083, 3.32, [58.0, 52.0, 54.0])];
    let dark = if tint.ends_with("Dark") { true } else if tint.ends_with("Light") { false } else { dark };
    let (sigma, mul, sat, add) = match tint {
        "light" | "extraLight" => LIGHT,
        "dark" => DARK,
        t if t.starts_with("system") => MATERIAL[dark as usize],
        _ => REGULAR[dark as usize],
    };
    let k = (intensity / 100.0).clamp(0.01, 1.0) as f32;
    let (mul, sat, add) = (1.0 + (mul - 1.0) * k, 1.0 + (sat - 1.0) * k, add.map(|a| a * k));
    // `add` becomes a tint mixed in at `ta` (glass tints mix after the transform): c' = c (1 - ta) + t ta.
    let ta = add.iter().fold(0.0f32, |m, a| m.max(*a)) / 255.0;
    let tint = (ta > 0.0).then(|| Color { r: add[0] / 255.0 / ta, g: add[1] / 255.0 / ta, b: add[2] / 255.0 / ta, a: ta });
    crate::glass::GlassSpec {
        variant: crate::glass::GlassVariant::Clear,
        tint,
        interactive: false,
        blur: Some(sigma * k),
        mix: Some((mul / (1.0 - ta).max(0.01), 0.0)),
        shadow: None,
        flat: Some(sat),
    }
}

/// Where the bitmap lands inside a `size` box (box-relative points) for the `resizeMode` prop.
pub(super) fn image_dest(p: &Value, (bw, bh): (f64, f64), (iw, ih): (f64, f64), scale: f64) -> Rect {
    let fit = |s: f64| Rect { x: (bw - iw * s) / 2.0, y: (bh - ih * s) / 2.0, w: iw * s, h: ih * s };
    match p.get("resizeMode").or(p.get("contentFit")).and_then(Value::as_str) {
        Some("stretch" | "fill") => Rect { x: 0.0, y: 0.0, w: bw, h: bh },
        Some("contain") => fit((bw / iw).min(bh / ih)),
        // Natural size (pixels / device scale), scaled down to fit.
        Some("center") => fit((1.0 / scale).min(bw / iw).min(bh / ih)),
        Some("none") => fit(1.0 / scale),
        Some("scale-down") => fit((1.0 / scale).min((bw / iw).min(bh / ih))),
        // `repeat` fills the box with a bitmap tiled to it ([`crate::layout::common::tiled`]).
        Some("repeat") => Rect { x: 0.0, y: 0.0, w: bw, h: bh },
        _ => fit((bw / iw).max(bh / ih)),
    }
}

/// A text input: node fields, the text run (placeholder when empty) and the caret. Returns the child nodes.
pub(super) fn android_switch(cx: &mut Cx<'_, '_>, n: &mut Node, p: &Value, label: Option<String>, parent: usize) -> Vec<Node> {
    let on = p.get("on").and_then(Value::as_bool) == Some(true);
    let cy = n.bounds.y + n.bounds.h / 2.0;
    let x = n.bounds.x + (n.bounds.w - 36.0) / 2.0;
    n.kind = "switch";
    n.role = Role::Toggle;
    n.value = Some(if on { "on" } else { "off" }.into());
    n.label = label;
    n.disabled = p.get("enabled").and_then(Value::as_bool) == Some(false);

    let mut track = Node::new(format!("{}#track", n.id), "switch_track");
    track.bounds = Rect { x, y: cy - 7.0, w: 36.0, h: 14.0 };
    track.radius = 7.0;
    track.fill = cx.color(p.get("trackTintColor")).or(Some(if on {
        Color { a: 0.5, ..ANDROID_ACCENT }
    } else {
        Color::rgba(0.5, 0.5, 0.5, 0.55)
    }));
    let opacity = n.opacity * if n.disabled { 0.5 } else { 1.0 };
    track.parent = Some(parent);
    track.inspect = false;
    track.opacity = opacity;

    let mut thumb = Node::new(format!("{}#knob", n.id), "switch_knob");
    thumb.bounds = Rect { x: x + if on { 17.0 } else { -1.0 }, y: cy - 10.0, w: 20.0, h: 20.0 };
    thumb.radius = 10.0;
    thumb.fill = cx.color(p.get("thumbTintColor")).or(Some(if on { ANDROID_ACCENT } else { Color::rgba(0.95, 0.95, 0.95, 1.0) }));
    thumb.shadow = Some(crate::Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.22), radius: 1.0, dx: 0.0, dy: 1.0 });
    thumb.parent = Some(parent);
    thumb.inspect = false;
    thumb.opacity = opacity;
    vec![track, thumb]
}

pub(super) fn text_field(cx: &mut Cx, n: &mut Node, it: &Item, me: usize, l: &taffy::Layout, params: &SceneParams) -> Vec<Node> {
    let p = it.props;
    let Some(Leaf::Text(t)) = cx.tree.get_node_context(it.id) else { return vec![] };
    let t = t.clone();
    let value = p.get("__simcoreText").and_then(Value::as_str).unwrap_or("");
    let placeholder = p.get("placeholder").and_then(Value::as_str).unwrap_or("");
    let (x, y, w, h) = (n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h);
    n.kind = "text_field";
    n.role = Role::TextField;
    n.field = it.node.id.parse().ok();
    n.value = Some(value.to_string());
    n.label = Some(if placeholder.is_empty() { p.get("accessibilityLabel").and_then(Value::as_str).unwrap_or("") } else { placeholder }.to_string());
    n.focused = params.focus == Some(it.node.id.as_str());
    n.disabled = p.get("editable").and_then(Value::as_bool) == Some(false);
    n.clip_radius = Some(n.radius);
    let empty = value.is_empty();
    let color = if empty {
        cx.color(p.get("placeholderTextColor")).unwrap_or(if cx.dark { Color::rgba(235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.3) } else { Color::rgba(60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.3) })
    } else {
        t.color
    };
    let (b, pd) = (l.border, l.padding);
    let (ix, iy) = (x + (b.left + pd.left) as f64, y + (b.top + pd.top) as f64);
    let (cw, ch) = (w - (b.left + pd.left + b.right + pd.right) as f64, h - (b.top + pd.top + b.bottom + pd.bottom) as f64);
    let lh = t.spec.line_height as f64;
    let single = t.single;
    let shown = t.text.clone();
    let (tw, th) = cx.fonts.measure(&shown, &t.spec, if single { None } else { Some(cw as f32) }, &[]);
    let tw = if empty { 0.0 } else { tw };
    let overflow = if single { (tw - cw).max(0.0) } else { 0.0 };
    let ty = if single { iy + (ch - lh) / 2.0 } else { iy };
    let tbox = Rect { x: ix, y: ty, w: if single { tw.max(cw) } else { cw }, h: if single { lh } else { th.max(lh) } };
    let mut text = Node::new(format!("{}#text", it.node.id), "text");
    text.bounds = tbox;
    text.parent = Some(me);
    text.opacity = n.opacity;
    text.inspect = false;
    text.text = Some(TextRun { text: shown, font: t.spec, color, strike: false, offset_x: -overflow as f32, spans: vec![] });
    let mut kids = vec![text];
    if n.focused && !n.disabled {
        let (cx0, cy0) = if single {
            let start = match t.spec.align {
                TextAlign::Center => (cw - tw).max(0.0) / 2.0,
                TextAlign::Trailing | TextAlign::End => (cw - tw).max(0.0),
                TextAlign::Leading | TextAlign::Justify | TextAlign::Left => 0.0,
            };
            ((start + tw - overflow).min(cw - 2.0) + 1.0, ty)
        } else {
            let last = value.rsplit('\n').next().unwrap_or("");
            let lw = if last.is_empty() { 0.0 } else { cx.fonts.measure(last, &t.spec, None, &[]).0 };
            (lw.min(cw - 2.0) + 1.0, ty + if empty { 0.0 } else { (th - lh).max(0.0) })
        };
        let mut c = Node::new(format!("{}#caret", it.node.id), "caret");
        c.bounds = Rect { x: ix + cx0, y: cy0, w: 2.0, h: lh };
        c.fill = Some(if cx.dark { Color::rgba(10.0 / 255.0, 145.0 / 255.0, 1.0, 1.0) } else { Color::rgba(0.0, 136.0 / 255.0, 1.0, 1.0) });
        c.radius = 1.0;
        c.parent = Some(me);
        c.opacity = n.opacity;
        c.inspect = false;
        kids.push(c);
    }
    kids
}

