//! Shared layout types and helpers used across `swiftui`, `rn`, and `compose`
//! to break circular dependencies between the three framework layout modules.

use crate::device::Device;
use crate::scene::{Color, Node, Rect, Role, Shadow};
use onesim_protocol::{Appearance, DynamicTypeSize, IosVersion};
use std::collections::HashMap;

pub(crate) const PILL: f64 = 1e6;
pub(crate) const SWITCH: (f64, f64) = (63.0, 28.0);
pub(crate) const KNOB: (f64, f64) = (37.0, 24.0);
/// Bottom safe-area inset under an iOS 26 tab bar (the 61 pt capsule 20 pt above the bottom, plus its margin).
pub(crate) const TAB_BAR_INSET: f64 = 83.0;

pub(crate) fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, a)
}

pub(crate) fn control_fill_for(dark: bool, light_a: f32) -> Color {
    Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, if dark { light_a * 2.0 } else { light_a })
}

/// Fill `sw` as an iOS switch track (role, value, fill, disabled dimming; `fill` overrides the state's default color) and return its knob node
/// (no bounds yet). Shared by SwiftUI `Toggle` and React Native `Switch`.
pub(crate) fn switch_style(sw: &mut Node, id: &str, dark: bool, on: bool, disabled: bool, fill: Option<Color>, thumb: Option<Color>) -> Node {
    sw.role = Role::Toggle;
    sw.value = Some(if on { "on" } else { "off" }.into());
    sw.disabled = disabled;
    sw.radius = PILL;
    sw.fill = fill.or(Some(if on { rgba(52, 199, 89, 1.0) } else { control_fill_for(dark, 0.16) }));
    if disabled {
        sw.opacity = 0.5;
    }
    let mut knob = Node::new(format!("{id}#knob"), "knob");
    knob.fill = Some(thumb.unwrap_or(rgba(255, 255, 255, 1.0)));
    knob.radius = PILL;
    knob.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.18), radius: 1.0, dx: 0.0, dy: 1.0 });
    knob.inspect = false;
    knob
}

/// Inputs to one layout pass.
pub struct SceneParams<'a> {
    pub viewport: (f64, f64),
    /// Chrome source (safe areas, island, status bar, keyboard); `viewport`/`scale` may override its geometry.
    pub device: &'a Device,
    pub scale: f64,
    pub appearance: Appearance,
    pub dynamic_type: DynamicTypeSize,
    /// Simulated iOS release: version-specific look (see `crate::look`).
    pub ios: IosVersion,
    /// Scroll offsets keyed by node id.
    pub scroll: &'a HashMap<String, (f64, f64)>,
    /// Detent / drag position of sheets by layer id.
    pub sheets: &'a HashMap<String, crate::SheetPos>,
    /// Node id of the focused text field.
    pub focus: Option<&'a str>,
    /// Let offsets past the edges show (rubber band) instead of clamping them: only while
    /// `Interaction::live`, so offsets left over from other content never show as overscroll.
    pub overscroll: bool,
    pub status_bar: bool,
    pub chrome: crate::chrome::ChromeParams,
}

impl<'a> SceneParams<'a> {
    /// The same parameters (struct update syntax needs a value, and `SceneParams` holds borrows).
    pub(crate) fn reborrow(&self) -> SceneParams<'a> {
        SceneParams { focus: self.focus, chrome: self.chrome, ..*self }
    }
}

pub(crate) struct NavBack {
    pub action: Option<u64>,
    pub label: String,
    pub pill: bool,
}

/// One tab bar item: SwiftUI `TabView` or Expo Router native tabs. Node ids are `<host>#tab-<key>`.
pub(crate) struct TabBarItem {
    pub key: String,
    pub title: String,
    /// Icon bitmaps (guest-downloaded) or an SF symbol name, normal and selected.
    pub icon: Option<String>,
    pub selected_icon: Option<String>,
    pub symbol: Option<String>,
    pub badge: Option<String>,
    pub action: Option<u64>,
}

pub(crate) struct TabBar {
    pub items: Vec<TabBarItem>,
    pub selected: usize,
    pub tint: Option<Color>,
}

/// Template rendering: `pm`'s alpha mask filled with `c`.
pub(crate) fn tinted(pm: &tiny_skia::Pixmap, c: Color) -> Option<std::sync::Arc<tiny_skia::Pixmap>> {
    let mut out = tiny_skia::Pixmap::new(pm.width(), pm.height())?;
    for (o, i) in out.pixels_mut().iter_mut().zip(pm.pixels()) {
        let a = i.alpha() as f32 / 255.0 * c.a;
        let ch = |v: f32| ((v * a * 255.0).round() as u8).min((a * 255.0).round() as u8);
        *o = tiny_skia::PremultipliedColorU8::from_rgba(ch(c.r), ch(c.g), ch(c.b), (a * 255.0).round() as u8)?;
    }
    Some(std::sync::Arc::new(out))
}

/// `resizeMode="repeat"`: `pm` tiled from the top-left corner at one bitmap pixel per device pixel (its natural size)
/// over a `w` x `h` device-pixel canvas.
pub(crate) fn tiled(pm: &tiny_skia::Pixmap, w: u32, h: u32) -> Option<std::sync::Arc<tiny_skia::Pixmap>> {
    let mut out = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let pattern = tiny_skia::Pattern::new(pm.as_ref(), tiny_skia::SpreadMode::Repeat, tiny_skia::FilterQuality::Nearest, 1.0, tiny_skia::Transform::identity());
    let paint = tiny_skia::Paint { shader: pattern, ..tiny_skia::Paint::default() };
    out.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32)?, &paint, tiny_skia::Transform::identity(), None);
    Some(std::sync::Arc::new(out))
}

/// Modal layers use viewport coordinates and a virtual screen parent at index 0.
#[derive(Default)]
pub(super) struct ModalLayers(pub(super) Vec<Vec<Node>>);

impl ModalLayers {
    pub fn paint(&mut self, out: &mut Vec<Node>) {
        for layer in self.0.drain(..) {
            place_nodes(layer, (0.0, 0.0), Some(0), 1.0, out);
        }
    }
}

pub(super) fn place_nodes(nodes: impl IntoIterator<Item = Node>, origin: (f64, f64), parent: Option<usize>, opacity: f32, out: &mut Vec<Node>) {
    let base = out.len();
    out.extend(nodes.into_iter().map(|mut node| {
        node.parent = node.parent.and_then(|p| p.checked_sub(1)).map(|p| p + base).or(parent);
        node.bounds.x += origin.0;
        node.bounds.y += origin.1;
        node.opacity *= opacity;
        node
    }));
}

#[derive(Debug, Clone)]
pub(super) struct Embedded {
    pub size: (f64, f64),
    pub(super) nodes: Vec<Node>,
    pub(super) modals: Vec<Node>,
}

impl Embedded {
    pub fn place(&self, origin: (f64, f64), parent: Option<usize>, opacity: f32, out: &mut Vec<Node>, modals: &mut ModalLayers) {
        place_nodes(self.nodes.iter().cloned(), origin, parent, opacity, out);
        if !self.modals.is_empty() {
            modals.0.push(self.modals.clone());
        }
    }
}

pub(super) struct Placement<'a> {
    pub bounds: Rect,
    pub parent: usize,
    pub opacity: f32,
    pub nodes: &'a mut Vec<Node>,
    pub modals: &'a mut ModalLayers,
}
