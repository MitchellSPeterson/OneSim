//! SwiftUI-semantics layout: parent proposes, child chooses, parent places.
//!
//! Stacks probe each child at 0 and infinity before the final proposal; `Ctx::memo`
//! caches results per (node, proposal) so nesting stays polynomial instead of 3^depth.
//!
//! Visual constants (row height, card radius, nav bar heights, control sizes) are measured from
//! an iOS 26 / iPhone 17 Pro simulator, see `tests/fixtures/showcase-ios/README.md`.

use crate::device::Device;
use crate::scene::{Color, Glyph, Node, Rect, Role, Scene, ScrollInfo, Shadow, SymbolRun, TextRun};
use crate::symbols::{self, SymbolSpec};
use crate::text::{FontSpec, Fonts, Span, TextAlign};
use simcore_protocol::{
    Alignment, Appearance, ButtonStyle, Detent, Layer, PickerOption, PickerStyle, DynamicTypeSize, GlassShapeKind, GlassVariant, ContentMode, DisplayMode, Font, ImageScale, ListStyle, Modifier, Section, Semantic, ShapeKind, Stroke,
    ViewKind, ViewNode,
};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

const INF: f64 = f64::INFINITY;
const DEFAULT_SPACING: f64 = 8.0;
/// Radius that always clamps to a capsule / circle.
const PILL: f64 = 1.0e6;

// Presentation layer (alert / sheet). Estimated from iOS 26 behavior, to be tuned against the simulator
// references (`alert`, `alert-dark`, `sheet`, `sheet-dark` in tests/fixtures/showcase-ios).
/// Black dimming over the presenting screen (light, dark): measured on the simulator.
const ALERT_SCRIM: (f32, f32) = (0.20, 0.46);
const SHEET_SCRIM: (f32, f32) = (0.20, 0.46);
const ALERT_W: f64 = 320.0;
const ALERT_RADIUS: f64 = 34.0;
/// Card center sits this far below the screen center.
const ALERT_DY: f64 = 14.0;
/// Text side inset, title top, gaps, button metrics.
const ALERT_TEXT_PAD: f64 = 30.0;
const ALERT_TOP: f64 = 22.0;
const ALERT_TITLE_MESSAGE_GAP: f64 = 7.33;
const ALERT_TEXT_BUTTON_GAP: f64 = 20.33;
const ALERT_BTN_H: f64 = 48.0;
const ALERT_BTN_GAP: f64 = 8.0;
const ALERT_BTN_PAD: f64 = 16.0;
/// Alert message size (pt, scales with nothing: measured 18pt line).
const ALERT_MESSAGE_PT: f32 = 15.0;
/// Button fill (light, dark) and gray the presenter's prominent buttons turn while an alert is up.
// Alert message: iOS draws a vibrant gray, darker than .secondary over the card (sampled from alert(-dark).png).
const ALERT_MESSAGE_FG: ((u8, u8, u8), (u8, u8, u8)) = ((106, 107, 108), (162, 162, 162));
const ALERT_BTN_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((209, 209, 210), (47, 47, 48));
const ALERT_DESAT: (u8, u8) = (160, 115);
/// Confirmation popover (measured on the simulator).
/// Glass (mul, add) of the popover on a light backdrop, and its wide soft shadow (alpha, sigma, dy).
const POPOVER_GLASS: (f32, f32) = (0.196, 0.76);
const POPOVER_SHADOW: (f32, f32, f32) = (0.16, 40.0, 12.0);
/// Glass (mul, add) of a floating sheet (light, dark).
const SHEET_GLASS: ((f32, f32), (f32, f32)) = ((0.43, 0.585), (0.35, 0.118));
const POPOVER_W: f64 = 240.0;
const POPOVER_RADIUS: f64 = 40.0;
const POPOVER_ARROW_W: f64 = 30.0;
const POPOVER_ARROW_H: f64 = 12.5;
/// Arrow tip sits this far inside the anchor.
const POPOVER_TIP_INSET: f64 = 5.0;
const POPOVER_MARGIN: f64 = 8.0;
/// Length of the arrow when the popover sits beside its anchor.
const POPOVER_SIDE_ARROW: f64 = 12.5;
const POPOVER_BTN_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((214, 214, 215), (48, 49, 49));
const POPOVER_ARROW: ((u8, u8, u8), (u8, u8, u8)) = ((242, 242, 244), (17, 20, 18));
/// Medium detent height as a fraction of the screen (floating card top at y 415 on 874).
const SHEET_MEDIUM: f64 = 0.525;
/// A sheet below its large detent floats: inset from the sides and bottom, glass fill.
const SHEET_FLOAT_INSET: f64 = 8.0;
const SHEET_FLOAT_RADIUS: f64 = 36.0;
/// White alpha (light, dark) of the back button's disc while a sheet is up (measured: 195 on 194, 13 on 0).
const BACK_FLAT: (f32, f32) = (0.017, 0.05);
/// A floating sheet shows the large-sheet layout (402 wide) scaled by this, about its top center (the card is 386 wide).
const SHEET_FLOAT_SCALE: f64 = 0.96;
/// Inset-grouped card color inside a floating sheet (light, dark).
const FLOAT_CARD: ((u8, u8, u8), (u8, u8, u8)) = ((222, 222, 222), (46, 46, 47));
/// Row separator inside a floating sheet's card (light, dark).
const FLOAT_SEP: ((u8, u8, u8), (u8, u8, u8)) = ((189, 189, 189), (74, 74, 76));
/// A sheet presented from a sheet sits this much lower per level.
const SHEET_STACK_STEP: f64 = 10.0;
/// Grabber colors (light, dark).
const GRABBER_FG: ((u8, u8, u8), (u8, u8, u8)) = ((177, 177, 180), (103, 103, 108));
/// Drag indicator: size and distance from the sheet's top.
const GRABBER: (f64, f64) = (34.7, 5.0);
const GRABBER_Y: f64 = 4.67;
/// Gap between the safe-area top and the sheet's top edge (large detent).
const SHEET_TOP_GAP: f64 = 0.0;
const SHEET_RADIUS: f64 = 40.0;
/// Backdrop blur (sigma, pt) under floating sheets, alert and dialog glass.
const PRESENT_BLUR: f32 = 12.0;
/// Space above the sheet content (a list starts this far below the sheet's top edge).
const SHEET_CONTENT_TOP: f64 = 35.0;

// Form controls (iOS 26.5, measured on `form*.png` in tests/fixtures/showcase-ios).
/// Menu picker: chevron.up.chevron.down (pt, weight), gap after the value, trailing inset.
const PICKER_CHEVRON: (f64, u16) = (13.0, 600);
const PICKER_CHEVRON_GAP: f64 = 5.0;
const PICKER_TRAIL_INSET: f64 = 0.0;
/// Segmented control: height, thumb inset, track fill alpha, font (pt, weight, selected weight).
const SEGMENTED_H: f64 = 31.0;
const SEGMENTED_INSET: f64 = 2.0;
const SEGMENTED_TRACK_A: f32 = 0.12;
const SEGMENTED_FONT: (f32, u16, u16) = (13.0, 500, 600);
const SEGMENTED_THUMB_DARK: (u8, u8, u8) = (105, 105, 111);
/// SecureField bullet: diameter, pitch, first bullet's x in the field (pt).
const SECURE_BULLET: (f64, f64, f64) = (7.33, 10.22, 1.67);
/// Slider: thumb (w, h), track thickness and fill alpha, thumb shadow (alpha, radius, dy).
const SLIDER_THUMB: (f64, f64) = (37.0, 24.0);
const SLIDER_H: f64 = 31.0;
const SLIDER_TRACK: f64 = 6.0;
/// Unfilled slider track (light, dark).
const SLIDER_TRACK_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((230, 230, 230), (50, 50, 52));
const SLIDER_SHADOW: (f32, f64, f64) = (0.15, 3.0, 1.0);
/// Linear progress: bar thickness, track alpha, gap under a label; spinner box.
const PROGRESS_H: f64 = 4.0;
/// Linear progress track (light, dark).
const PROGRESS_TRACK_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((228, 228, 230), (61, 61, 65));
const PROGRESS_LABEL_GAP: f64 = 8.0;
const SPINNER: f64 = 20.0;
/// Picker menu: row height, vertical padding, text x, right padding, min width, radius,
/// checkmark (pt, weight) and centre x, offset from the anchor, glass mix (light, dark).
const MENU_ROW_H: f64 = 42.0;
const MENU_PAD_V: f64 = 10.0;
const MENU_TEXT_X: f64 = 43.7;
const MENU_PAD_R: f64 = 24.0;
const MENU_MIN_W: f64 = 250.0;
const MENU_RADIUS: f64 = 26.0;
const MENU_CHECK: (f64, u16) = (13.0, 600);
const MENU_CHECK_X: f64 = 27.67;
const MENU_DX: f64 = 0.0;
/// Half the height of the picker's hit area: the card's bottom lines up with it.
const MENU_DY: f64 = 17.17;
const MENU_GLASS: ((f32, f32), (f32, f32)) = ((0.196, 0.76), (0.35, 0.118));

// iOS 26 measurements (points).
const ROW_MIN_H: f64 = 52.0;
const ROW_VPAD: f64 = 10.0;
/// Rows holding bordered buttons get extra vertical padding (measured: 34.3pt button -> 64.3pt row).
const ROW_VPAD_BORDERED: f64 = 15.0;
const CARD_RADIUS: f64 = 26.0;
const SECTION_GAP: f64 = 17.7;
/// Extra gap above a header-less section that follows another (35pt in total).
const HEADERLESS_EXTRA_GAP: f64 = 17.3;
const HEADER_PAD: f64 = 20.0;
const NAV_INLINE_H: f64 = 54.0;
/// Scroll distance over which the inline title fades in as the large title finishes collapsing.
const INLINE_FADE: f64 = 12.0;
pub(crate) const SWITCH: (f64, f64) = (63.0, 28.0);
/// Switch knob size (inset 2pt from the track edge).
pub(crate) const KNOB: (f64, f64) = (37.0, 24.0);
const STEPPER: (f64, f64) = (94.0, 32.0);
/// `Label` in a list row: icon centred in a leading column of this width, title starts after it.
/// Measured: icon centred 12pt in, title at +40 (column 24 + gap 16), icons at imageScale large.
const LABEL_ICON_COL: f64 = 24.0;
const LABEL_ROW_GAP: f64 = 16.0;
/// Icon-to-title gap of a `Label` outside a list (not measured).
const LABEL_GAP: f64 = 8.0;

/// Screen geometry shared by the root and the presentation layers.
#[derive(Clone, Copy)]
struct Geo {
    w: f64,
    h: f64,
    safe_top: f64,
    /// Bottom inset including the keyboard (the root avoids it).
    safe_bottom: f64,
    /// Device bottom inset (presented content ignores the keyboard).
    dev_bottom: f64,
    body: f32,
}

#[derive(Clone, Copy)]
struct Prop {
    w: Option<f64>,
    h: Option<f64>,
}

#[derive(Clone, Copy, Default)]
struct Size {
    w: f64,
    h: f64,
}

/// Relative-positioned layout result. `node` (if any) becomes a scene node.
#[derive(Clone)]
struct Lay {
    size: Size,
    /// Boxed: layout recursion holds many `Lay`s per frame and `Node` is large.
    node: Option<Box<Node>>,
    kids: Vec<(f64, f64, Lay)>,
    /// Hit/paint bounds growth (left, top, right, bottom) beyond `size`.
    infl: (f64, f64, f64, f64),
}

impl Lay {
    fn new(size: Size, node: Option<Node>, kids: Vec<(f64, f64, Lay)>) -> Self {
        Self { size, node: node.map(Box::new), kids, infl: (0.0, 0.0, 0.0, 0.0) }
    }
}

#[derive(Clone, Copy)]
struct Env {
    fg: Option<Color>,
    /// Axis of the directly enclosing stack (true = vertical), for Spacer/Divider.
    axis: Option<bool>,
    size: f32,
    base_weight: u16,
    weight: Option<u16>,
    strike: bool,
    align: TextAlign,
    /// Accent tint in effect (automatic buttons recolour their label content).
    tint: Option<Color>,
    /// Top/bottom safe-area insets for full-bleed scrollers (consumed by list/scroll).
    safe_top: f64,
    bottom_inset: f64,
    /// `imageScale` in effect for symbols.
    img: ImageScale,
    /// Laid out as (part of) a list row: `Label` uses the fixed icon column and accent icon.
    in_row: bool,
}

impl Env {
    fn new() -> Self {
        Env {
            fg: None,
            axis: None,
            size: 17.0,
            base_weight: 400,
            weight: None,
            strike: false,
            align: TextAlign::Leading,
            tint: None,
            safe_top: 0.0,
            bottom_inset: 0.0,
            img: ImageScale::Medium,
            in_row: false,
        }
    }
}

/// A back button for [`Ctx::nav_bar`]: a 44pt circle (SwiftUI) or a pill carrying the previous title (React Native).
pub(crate) struct NavBack {
    pub action: Option<u64>,
    pub label: String,
    pub pill: bool,
}

struct NavBar<'a> {
    id: &'a str,
    w: f64,
    h: f64,
    top: f64,
    bar_h: f64,
    title: Option<&'a str>,
    large: bool,
    large_title: Option<Lay>,
    title_h: f64,
    band: f64,
    offset: f64,
    back: Option<NavBack>,
}

struct Ctx<'a> {
    fonts: &'a mut Fonts,
    dark: bool,
    dts: DynamicTypeSize,
    scale: f64,
    /// Side margin of lists, cards and titles: 20pt on Plus/Max-width screens (>= 414pt), else 16.
    margin: f64,
    scroll: &'a HashMap<String, (f64, f64)>,
    overscroll: bool,
    focus: Option<&'a str>,
    /// Keyed by (node address, proposal bits). A node's Env is fixed by its ancestors,
    /// so node + proposal fully determines its layout within one pass.
    memo: HashMap<(usize, u64, u64), Lay>,
    /// Symbols that fell back to a placeholder.
    unsupported: BTreeSet<String>,
    /// Elevated grouped colors (content of a sheet in dark mode).
    elevated: bool,
    /// Inside a floating (non-large) sheet: lists are transparent over its glass.
    floating: bool,
    /// An alert is up: prominent tints in the presenter turn gray.
    desat: bool,
    /// A sheet is up over this screen: its back button loses the glass circle (alerts keep it).
    sheet_up: bool,
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

fn control_fill_for(dark: bool, light_a: f32) -> Color {
    Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, if dark { light_a * 2.0 } else { light_a })
}

/// Inputs to one layout pass.
pub struct SceneParams<'a> {
    pub viewport: (f64, f64),
    /// Chrome source (safe areas, island, status bar, keyboard); `viewport`/`scale` may override its geometry.
    pub device: &'a Device,
    pub scale: f64,
    pub appearance: Appearance,
    pub dynamic_type: DynamicTypeSize,
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

fn ctx<'a>(params: &'a SceneParams, fonts: &'a mut Fonts) -> Ctx<'a> {
    Ctx {
        fonts,
        dark: params.appearance == Appearance::Dark,
        dts: params.dynamic_type,
        scale: params.scale.max(1.0),
        margin: if params.viewport.0 >= 414.0 { 20.0 } else { 16.0 },
        scroll: params.scroll,
        overscroll: params.overscroll,
        focus: params.focus,
        memo: HashMap::new(),
        unsupported: BTreeSet::new(),
        elevated: false,
        floating: false,
        desat: false,
        sheet_up: false,
    }
}

/// Status bar and Dynamic Island on top of `nodes`, for layouts that draw their own content (React Native).
pub(crate) fn add_chrome(nodes: &mut Vec<Node>, params: &SceneParams, fonts: &mut Fonts) {
    if params.status_bar {
        let lay = ctx(params, fonts).status_bar(params.viewport.0, params.device);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, nodes);
    }
    if let Some(isl) = params.device.island {
        crate::chrome::island(nodes, isl);
    }
}

/// Height of the navigation bar below the status bar for a React Native stack header.
pub(crate) fn rn_nav_height(params: &SceneParams, fonts: &mut Fonts, w: f64, title: Option<&str>, large: bool) -> f64 {
    let (_, _, band) = ctx(params, fonts).nav_large("nav", w, title, large);
    NAV_INLINE_H + band
}

/// Append a React Native native-stack header (`top` = safe-area top) to `out`: the same bar the SwiftUI
/// `NavigationStack` draws. `id` is the stack tag; the back pill's node is `{id}#back`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rn_nav_bar(params: &SceneParams, fonts: &mut Fonts, id: &str, top: f64, title: Option<&str>, large: bool, back: Option<NavBack>, parent: usize, out: &mut Vec<Node>) {
    let mut cx = ctx(params, fonts);
    let w = params.viewport.0;
    let (large_title, title_h, band) = cx.nav_large(id, w, title, large);
    let bar_h = NAV_INLINE_H + band;
    let bar = NavBar { id, w, h: params.viewport.1, top, bar_h, title, large: large && title.is_some(), large_title, title_h, band, offset: 0.0, back };
    flatten(cx.nav_bar(bar), 0.0, top, Some(parent), 1.0, out);
}

/// Bottom safe-area inset under an iOS 26 tab bar (the 61 pt capsule 20 pt above the bottom, plus its margin).
pub(crate) const TAB_BAR_INSET: f64 = 83.0;

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

/// Append the tab bar of the React Native tab host `host` to `out`.
pub(crate) fn rn_tab_bar(params: &SceneParams, fonts: &mut Fonts, t: &TabBar, host: &str, parent: usize, opacity: f32, out: &mut Vec<Node>) {
    let (w, h) = params.viewport;
    let start = out.len();
    flatten(ctx(params, fonts).tab_bar(t, host, w, h), 0.0, 0.0, Some(parent), opacity, out);
    // Bitmap icons fill their node; `dest` is absolute.
    for n in &mut out[start..] {
        let b = n.bounds;
        if let Some(im) = n.image.as_mut() {
            im.dest = b;
        }
    }
}

/// `alert` layers over a React Native screen (`Alert.alert`): the same scrim and card as `.alert`.
pub(crate) fn rn_alerts(params: &SceneParams, fonts: &mut Fonts, layers: &[Layer], out: &mut Vec<Node>) {
    let mut cx = ctx(params, fonts);
    let (w, h) = params.viewport;
    let geo = Geo { w, h, safe_top: params.device.safe_top, safe_bottom: params.device.safe_bottom, dev_bottom: params.device.safe_bottom, body: 17.0 };
    for layer in layers {
        if let Layer::Alert { title, message, actions } = layer {
            let lay = cx.alert(title, message.as_deref(), actions, &geo);
            flatten(lay, 0.0, 0.0, Some(0), 1.0, out);
        }
    }
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (w, h) = params.viewport;
    // Keyboard avoidance: the keyboard top replaces the bottom safe-area inset.
    let kb = params.chrome.keyboard && params.focus.is_some();
    let kb_h = params.device.kb_base_h + params.device.safe_bottom;
    let (safe_top, dev_bottom) = (params.device.safe_top, params.device.safe_bottom);
    let safe_bottom = if kb { kb_h } else { dev_bottom };
    let mut cx = ctx(params, fonts);
    // Unstyled text is `.body`, which scales with Dynamic Type.
    let body = simcore_protocol::TextStyle::Body.size_at(params.dynamic_type) as f32;
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(cx.semantic(Semantic::Background));
    let mut nodes = vec![screen];
    let (layers, root): (&[Layer], &ViewNode) = match &root.kind {
        ViewKind::Presentation { content, layers } => (layers, &**content),
        _ => (&[], root),
    };
    cx.desat = !layers.is_empty();
    cx.sheet_up = layers.iter().any(|l| matches!(l, Layer::Sheet { .. }));
    let geo = Geo { w, h, safe_top, safe_bottom, dev_bottom, body };
    let lay = cx.place_root(root, &geo);
    flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    for (i, layer) in layers.iter().enumerate() {
        let id = format!("layer{i}");
        // Whatever lies under a higher layer is desaturated; a layer's own content is not.
        cx.desat = i + 1 < layers.len();
        cx.sheet_up = layers[i + 1..].iter().any(|l| matches!(l, Layer::Sheet { .. }));
        let lay = match layer {
            Layer::Sheet { content, detents, drag_indicator, dismiss } => {
                cx.sheet(&id, content, detents, *drag_indicator, *dismiss, params.sheets.get(&id).copied().unwrap_or_default(), layers[..i].iter().filter(|l| matches!(l, Layer::Sheet { .. })).count(), &geo)
            }
            Layer::Cover { content } => cx.cover(content, &geo),
            Layer::Alert { title, message, actions } => cx.alert(title, message.as_deref(), actions, &geo),
            Layer::Menu { anchor, items, dismiss } => {
                let rect = anchor.as_ref().and_then(|a| nodes.iter().find(|n| &n.id == a)).map(|n| n.bounds);
                cx.menu(items, rect, *dismiss, &geo)
            }
            Layer::Dialog { title, title_visible, message, actions, anchor, dismiss } => {
                // The popover points at the anchor's label (its first text), not the whole row.
                let rect = anchor.as_ref().and_then(|a| nodes.iter().position(|n| &n.id == a)).map(|i| {
                    let under = |mut j: usize| loop {
                        if j == i {
                            return true;
                        }
                        match nodes[j].parent {
                            Some(p) => j = p,
                            None => return false,
                        }
                    };
                    nodes.iter().enumerate().skip(i).find(|(j, n)| n.text.is_some() && under(*j)).map_or(nodes[i].bounds, |(_, n)| n.bounds)
                });
                cx.popover(title_visible.then_some(title.as_str()), message.as_deref(), actions, rect, *dismiss, &geo)
            }
        };
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    }
    if params.status_bar {
        let lay = cx.status_bar(w, params.device);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    }
    let dark = params.appearance == Appearance::Dark;
    if kb {
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift, params.device);
    }
    if let Some(isl) = params.device.island {
        crate::chrome::island(&mut nodes, isl);
    }
    Scene { size: params.viewport, appearance: params.appearance, nodes, unsupported_symbols: cx.unsupported.into_iter().collect(), notes: vec![] }
}

fn flatten(lay: Lay, x: f64, y: f64, mut parent: Option<usize>, mut opacity: f32, out: &mut Vec<Node>) {
    if let Some(mut n) = lay.node {
        let (l, t, r, b) = lay.infl;
        n.bounds = Rect { x: x - l, y: y - t, w: lay.size.w + l + r, h: lay.size.h + t + b };
        n.parent = parent;
        opacity *= n.opacity;
        n.opacity = opacity;
        out.push(*n);
        parent = Some(out.len() - 1);
    }
    for (dx, dy, k) in lay.kids {
        flatten(k, x + dx, y + dy, parent, opacity, out);
    }
}

fn h_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Leading | TopLeading | BottomLeading => 0.0,
        Trailing | TopTrailing | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

fn v_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Top | TopLeading | TopTrailing => 0.0,
        Bottom | BottomLeading | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

fn first_text(v: &ViewNode) -> Option<String> {
    match &v.kind {
        ViewKind::Text { text, .. } => Some(text.clone()),
        ViewKind::VStack { children, .. } | ViewKind::HStack { children, .. } | ViewKind::ZStack { children, .. } => {
            children.iter().find_map(first_text)
        }
        ViewKind::Button { label, .. } | ViewKind::Toggle { label, .. } | ViewKind::Stepper { label, .. } | ViewKind::Picker { label, .. } => {
            first_text(label)
        }
        ViewKind::Label { title, .. } => first_text(title),
        _ => None,
    }
}

/// Corner radius of whatever a modifier chain painted last (for shadows).
fn lay_radius(l: &Lay) -> f64 {
    match &l.node {
        Some(n) if n.clip_radius.is_some() => n.clip_radius.unwrap_or(0.0),
        Some(n) if n.radius > 0.0 => n.radius,
        _ if l.kids.len() == 1 => lay_radius(&l.kids[0].2),
        _ => 0.0,
    }
}

/// x offset of the first text inside `l` (separator inset), relative to `l`'s origin.
fn first_text_x(l: &Lay, ox: f64) -> Option<f64> {
    if l.node.as_ref().is_some_and(|n| n.text.is_some()) {
        return Some(ox);
    }
    // A segmented control's labels don't set the separator inset.
    if l.node.as_ref().is_some_and(|n| n.kind == "segmented") {
        return None;
    }
    l.kids.iter().find_map(|(dx, _, k)| first_text_x(k, ox + dx))
}

/// Grow the row's button hit area to the whole row. `row` is (x0,y0,x1,y1) in `l`'s parent space.
fn expand_button(l: &mut Lay, ox: f64, oy: f64, row: (f64, f64, f64, f64)) -> bool {
    if let Some(n) = &l.node {
        if n.kind == "button" && n.action.is_some() {
            l.infl = (ox - row.0, oy - row.1, row.2 - (ox + l.size.w), row.3 - (oy + l.size.h));
            return true;
        }
        return false;
    }
    if l.kids.len() == 1 {
        let (dx, dy, k) = &mut l.kids[0];
        return expand_button(k, ox + *dx, oy + *dy, row);
    }
    false
}

/// Rows holding bordered buttons, segmented pickers or sliders get the taller row padding.
fn has_bordered(v: &ViewNode) -> bool {
    match &v.kind {
        ViewKind::Picker { style: PickerStyle::Segmented, .. } | ViewKind::Slider { .. } => true,
        ViewKind::Button { style, label, .. } => {
            matches!(style, ButtonStyle::Bordered | ButtonStyle::BorderedProminent) || has_bordered(label)
        }
        ViewKind::VStack { children, .. } | ViewKind::HStack { children, .. } | ViewKind::ZStack { children, .. } => {
            children.iter().any(has_bordered)
        }
        _ => false,
    }
}

fn is_list(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { style: ListStyle::InsetGrouped, .. })
}

fn is_scroller(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { .. } | ViewKind::Scroll { .. })
}

/// A corner radius scaled by `k` (pill/circle radii stay pills).
fn scaled_radius(x: f64, k: f64) -> f64 {
    if x >= PILL / 2.0 {
        x
    } else {
        x * k
    }
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

impl Ctx<'_> {
    /// Offset along a scroll axis: within `[0, max]`, or up to one viewport `len` past an edge
    /// while overscroll is live (the rubber band never goes further).
    fn clamp_offset(&self, o: f64, max: f64, len: f64) -> f64 {
        let slack = if self.overscroll { len } else { 0.0 };
        o.clamp(-slack, max + slack)
    }

    fn semantic(&self, s: Semantic) -> Color {
        let d = self.dark;
        match s {
            Semantic::Primary if d => rgba(255, 255, 255, 1.0),
            Semantic::Primary => rgba(0, 0, 0, 1.0),
            Semantic::Secondary if d => rgba(235, 235, 245, 0.6),
            Semantic::Secondary => rgba(60, 60, 67, 0.6),
            Semantic::Tertiary if d => rgba(235, 235, 245, 0.3),
            Semantic::Tertiary => rgba(60, 60, 67, 0.3),
            Semantic::Accent if d => rgba(10, 145, 255, 1.0),
            Semantic::Accent => rgba(0, 136, 255, 1.0),
            Semantic::Background if d => rgba(0, 0, 0, 1.0),
            Semantic::Background => rgba(255, 255, 255, 1.0),
            Semantic::Separator if d => rgba(56, 56, 59, 1.0),
            Semantic::Separator => rgba(232, 232, 232, 1.0),
            Semantic::GroupedBackground if d && self.elevated => rgba(28, 28, 30, 1.0),
            Semantic::GroupedBackground if d => rgba(0, 0, 0, 1.0),
            Semantic::GroupedBackground => rgba(242, 242, 247, 1.0),
            Semantic::SecondaryGroupedBackground if d && self.elevated => rgba(44, 44, 46, 1.0),
            Semantic::SecondaryGroupedBackground if d => rgba(28, 28, 30, 1.0),
            Semantic::SecondaryGroupedBackground => rgba(255, 255, 255, 1.0),
        }
    }

    fn glass_spec(&self, variant: GlassVariant, tint: Option<&simcore_protocol::Color>, interactive: bool) -> crate::glass::GlassSpec {
        crate::glass::GlassSpec { variant, tint: tint.map(|c| self.color(c)), interactive, blur: None, mix: None, shadow: None, flat: None }
    }

    /// Opaque (light, dark) color pair for the current appearance.
    fn pick(&self, (l, d): ((u8, u8, u8), (u8, u8, u8))) -> Color {
        let c = if self.dark { d } else { l };
        rgba(c.0, c.1, c.2, 1.0)
    }

    /// Neutral control fill (systemFill-like) of the given strength.
    fn control_fill(&self, light_a: f32) -> Color {
        control_fill_for(self.dark, light_a)
    }

    fn color(&self, c: &simcore_protocol::Color) -> Color {
        match c {
            simcore_protocol::Color::Semantic { semantic } => self.semantic(*semantic),
            simcore_protocol::Color::Rgba { r, g, b, a, dark } => {
                let [r, g, b] = if self.dark { dark.unwrap_or([*r, *g, *b]) } else { [*r, *g, *b] };
                Color::rgba(r as f32, g as f32, b as f32, *a as f32)
            }
        }
    }

    /// SF line height, rounded up to the device pixel like UIKit.
    fn line_h(&self, size: f32) -> f32 {
        let s = self.scale as f32;
        (size * 1.1934 * s).ceil() / s
    }

    fn spec(&self, env: &Env) -> FontSpec {
        FontSpec::new(env.size, self.line_h(env.size), env.weight.unwrap_or(env.base_weight), env.align)
    }

    fn hairline(&self) -> f64 {
        1.0 / self.scale
    }

    /// A text leaf with an explicit font.
    fn text_lay(&mut self, id: &str, text: &str, spec: FontSpec, color: Color, wrap: Option<f32>) -> Lay {
        self.text_lay_runs(id, text, spec, color, wrap, vec![])
    }

    #[allow(clippy::too_many_arguments)]
    fn text_lay_runs(&mut self, id: &str, text: &str, spec: FontSpec, color: Color, wrap: Option<f32>, spans: Vec<Span>) -> Lay {
        let (w, h) = self.fonts.measure(text, &spec, wrap, &spans);
        // SwiftUI rounds text sizes up to the device pixel.
        let w = (w * self.scale).ceil() / self.scale;
        let mut n = Node::new(id, "text");
        n.text = Some(TextRun { text: text.to_string(), font: spec, color, strike: false, offset_x: 0.0, spans });
        n.role = Role::Text;
        n.label = Some(text.to_string());
        Lay::new(Size { w, h }, Some(n), vec![])
    }

    fn view(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let bits = |x: Option<f64>| x.map_or(u64::MAX, f64::to_bits);
        let key = (v as *const ViewNode as usize, bits(p.w), bits(p.h));
        if let Some(l) = self.memo.get(&key) {
            return l.clone();
        }
        let l = self.mods(v, &v.modifiers, p, env);
        self.memo.insert(key, l.clone());
        l
    }

    /// Applies modifiers outermost-first by recursion (array order is innermost first).
    fn mods(&mut self, v: &ViewNode, mods: &[Modifier], p: Prop, env: Env) -> Lay {
        let Some((last, rest)) = mods.split_last() else {
            return self.base(v, p, env);
        };
        match last {
            Modifier::Padding(e) => {
                let ph = e.leading + e.trailing;
                let pv = e.top + e.bottom;
                let inner = self.mods(
                    v,
                    rest,
                    Prop { w: p.w.map(|w| (w - ph).max(0.0)), h: p.h.map(|h| (h - pv).max(0.0)) },
                    env,
                );
                let size = Size { w: inner.size.w + ph, h: inner.size.h + pv };
                Lay::new(size, None, vec![(e.leading, e.top, inner)])
            }
            Modifier::Frame(f) => {
                let cw = child_prop(p.w, f.width, f.min_width, f.max_width);
                let ch = child_prop(p.h, f.height, f.min_height, f.max_height);
                let inner = self.mods(v, rest, Prop { w: cw, h: ch }, env);
                let w = frame_len(p.w, inner.size.w, f.width, f.min_width, f.max_width);
                let h = frame_len(p.h, inner.size.h, f.height, f.min_height, f.max_height);
                let (dx, dy) = ((w - inner.size.w) * h_frac(f.alignment), (h - inner.size.h) * v_frac(f.alignment));
                Lay::new(Size { w, h }, None, vec![(dx, dy, inner)])
            }
            Modifier::ForegroundColor(c) => {
                let mut env = Env { fg: Some(self.color(c)), ..env };
                // Inside a tinted control the hierarchical styles stay accent-coloured, dimmed.
                if let (Some(t), simcore_protocol::Color::Semantic { semantic }) = (env.tint, c) {
                    let k = match semantic {
                        Semantic::Primary => Some(1.0),
                        Semantic::Secondary => Some(0.5),
                        Semantic::Tertiary => Some(0.25),
                        _ => None,
                    };
                    if let Some(k) = k {
                        env.fg = Some(Color { a: t.a * k, ..t });
                    }
                }
                self.mods(v, rest, p, env)
            }
            Modifier::Background(bg) => {
                let inner = self.mods(v, rest, p, env);
                let bgl = self.view(bg, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                Lay::new(inner.size, None, vec![(0.0, 0.0, bgl), (0.0, 0.0, inner)])
            }
            Modifier::Overlay(o) => {
                let inner = self.mods(v, rest, p, env);
                let ovl = self.view(&o.view, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let (dx, dy) = ((inner.size.w - ovl.size.w) * h_frac(o.alignment), (inner.size.h - ovl.size.h) * v_frac(o.alignment));
                Lay::new(inner.size, None, vec![(0.0, 0.0, inner), (dx, dy, ovl)])
            }
            Modifier::CornerRadius(r) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#clip{}", v.id, rest.len()), "clip");
                n.clip_radius = Some(*r);
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Opacity(o) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#opacity{}", v.id, rest.len()), "opacity");
                n.opacity = *o as f32;
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Shadow(s) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#shadow{}", v.id, rest.len()), "shadow");
                n.radius = lay_radius(&inner);
                n.shadow = Some(Shadow { color: self.color(&s.color), radius: s.radius, dx: s.x, dy: s.y });
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
            Modifier::Font(f) => {
                let (size, weight) = self.font_size_weight(f);
                self.mods(v, rest, p, Env { size: size as f32, base_weight: weight.value(), ..env })
            }
            Modifier::FontWeight(w) => {
                let weight = env.weight.or(Some(w.value()));
                self.mods(v, rest, p, Env { weight, ..env })
            }
            Modifier::Strikethrough(b) => self.mods(v, rest, p, Env { strike: *b, ..env }),
            Modifier::MultilineTextAlignment(a) => {
                let align = match a {
                    Alignment::Center | Alignment::Top | Alignment::Bottom => TextAlign::Center,
                    a if h_frac(*a) == 1.0 => TextAlign::Trailing,
                    _ => TextAlign::Leading,
                };
                self.mods(v, rest, p, Env { align, ..env })
            }
            Modifier::ImageScale(img) => self.mods(v, rest, p, Env { img: *img, ..env }),
            Modifier::NavigationTitle(_) => self.mods(v, rest, p, env),
            Modifier::Glass(g) => {
                let inner = self.mods(v, rest, p, env);
                let mut n = Node::new(format!("{}#glass{}", v.id, rest.len()), "glass");
                n.radius = match (g.shape.shape, g.shape.corner_radius) {
                    (GlassShapeKind::Rect, None) => 0.0,
                    (GlassShapeKind::Rect | GlassShapeKind::RoundedRectangle, r) => r.unwrap_or(0.0),
                    _ => PILL,
                };
                n.glass = Some(self.glass_spec(g.variant, g.tint.as_ref(), g.interactive));
                n.inspect = false;
                Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
            }
        }
    }

    /// Point size (at the current Dynamic Type size) and weight of a `Font`.
    fn font_size_weight(&self, f: &Font) -> (f64, simcore_protocol::Weight) {
        match f {
            Font::Style { style, weight } => (style.size_at(self.dts), weight.unwrap_or(style.spec().1)),
            Font::System { size, weight } => (*size, *weight),
        }
    }

    fn base(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let child_env = Env { axis: None, ..env };
        match &v.kind {
            ViewKind::Text { text, runs } => {
                let wrap = p.w.filter(|w| w.is_finite()).map(|w| w.max(1.0) as f32);
                let mut spec = self.spec(&env);
                let spans: Vec<Span> = runs
                    .iter()
                    .map(|r| {
                        let (pt, base) = r.font.as_ref().map_or((env.size, env.base_weight), |f| {
                            let (s, w) = self.font_size_weight(f);
                            (s as f32, w.value())
                        });
                        let weight = r.font_weight.map(|w| w.value()).or(if r.font.is_some() { None } else { env.weight }).unwrap_or(base);
                        Span { text: r.text.clone(), pt, weight, color: r.foreground_color.as_ref().map_or(fg, |c| self.color(c)), italic: r.italic, underline: r.underline, strike: r.strikethrough || env.strike, mono: false }
                    })
                    .collect();
                // Line metrics come from the largest run.
                if let Some(big) = spans.iter().max_by(|a, b| a.pt.total_cmp(&b.pt)) {
                    spec = FontSpec::new(big.pt, self.line_h(big.pt), big.weight, env.align);
                }
                let mut l = self.text_lay_runs(&v.id, text, spec, fg, wrap, spans);
                if let Some(t) = l.node.as_mut().and_then(|n| n.text.as_mut()) {
                    t.strike = env.strike;
                }
                l
            }
            ViewKind::VStack { spacing, alignment, children } => {
                self.stack(v, "vstack", children, *spacing, *alignment, true, p, child_env)
            }
            ViewKind::HStack { spacing, alignment, children } => {
                self.stack(v, "hstack", children, *spacing, *alignment, false, p, child_env)
            }
            ViewKind::ZStack { alignment, children } => {
                let lays: Vec<Lay> = children.iter().map(|c| self.view(c, p, child_env)).collect();
                let size = Size {
                    w: lays.iter().map(|l| l.size.w).fold(0.0, f64::max),
                    h: lays.iter().map(|l| l.size.h).fold(0.0, f64::max),
                };
                let kids = lays
                    .into_iter()
                    .map(|l| ((size.w - l.size.w) * h_frac(*alignment), (size.h - l.size.h) * v_frac(*alignment), l))
                    .collect();
                Lay::new(size, Some(Node::new(&v.id, "zstack")), kids)
            }
            ViewKind::Spacer { min_length } => {
                let min = min_length.unwrap_or(0.0);
                let main = |prop: Option<f64>| prop.unwrap_or(0.0).max(min);
                let size = match env.axis {
                    Some(true) => Size { w: 0.0, h: main(p.h) },
                    Some(false) => Size { w: main(p.w), h: 0.0 },
                    None => Size { w: min, h: min },
                };
                Lay::new(size, Some(Node::new(&v.id, "spacer")), vec![])
            }
            ViewKind::Button { action, label, style, disabled, .. } => self.button(v, *action, label, *style, *disabled, p, env),
            ViewKind::Color { color } => {
                let mut n = Node::new(&v.id, "color");
                n.fill = Some(self.color(color));
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Rectangle => {
                let mut n = Node::new(&v.id, "rectangle");
                n.fill = Some(fg);
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Shape { shape, corner_radius, fill, stroke } => {
                let size = if *shape == ShapeKind::Circle {
                    let side = match (p.w, p.h) {
                        (Some(w), Some(h)) => w.min(h),
                        (Some(w), None) => w,
                        (None, Some(h)) => h,
                        _ => 10.0,
                    };
                    Size { w: side, h: side }
                } else {
                    greedy(p)
                };
                let mut n = Node::new(&v.id, "shape");
                n.radius = match shape {
                    ShapeKind::Rectangle => 0.0,
                    ShapeKind::Circle | ShapeKind::Capsule => PILL,
                    ShapeKind::RoundedRectangle => corner_radius.unwrap_or(0.0),
                };
                n.fill = fill.as_ref().map(|c| self.color(c));
                n.stroke = stroke.as_ref().map(|Stroke { color, line_width }| (self.color(color), *line_width));
                if n.fill.is_none() && n.stroke.is_none() {
                    n.fill = Some(fg);
                }
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Divider => {
                let hl = self.hairline();
                let len = |x: Option<f64>| x.filter(|x| x.is_finite()).unwrap_or(0.0);
                let size = if env.axis == Some(false) { Size { w: hl, h: len(p.h) } } else { Size { w: len(p.w), h: hl } };
                let mut n = Node::new(&v.id, "divider");
                // UIKit's translucent separator (measured 198 on white).
                n.fill = Some(if self.dark { rgba(84, 84, 88, 0.6) } else { rgba(60, 60, 67, 0.29) });
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Empty | ViewKind::Rn { .. } => Lay::new(Size::default(), Some(Node::new(&v.id, "empty")), vec![]),
            // Only the root's presentation layer is drawn (see `layout`); nested, it is transparent.
            ViewKind::Presentation { content, .. } => self.view(content, p, env),
            ViewKind::Scroll { axis, child } => self.scroll(v, *axis == simcore_protocol::Axis::Vertical, child, p, env),
            ViewKind::List { style, sections } => self.list(v, *style, sections, p, env),
            ViewKind::TabView { selected, tabs, content } => self.tab_view(v, *selected, tabs, content, p, env),
            ViewKind::Navigation { title, display_mode, back, content } => {
                self.navigation(v, title.as_deref(), *display_mode, back.as_ref(), content, p, env)
            }
            ViewKind::Toggle { is_on, action, label, disabled } => self.toggle(v, *is_on, *action, label, *disabled, p, env),
            ViewKind::Stepper { label, decrement, increment } => self.stepper(v, label, *decrement, *increment, p, env),
            ViewKind::TextField { placeholder, text, field, secure } => {
                if !*secure || text.is_empty() {
                    return self.text_field(v, placeholder, text, *field, p, env);
                }
                // SecureField: one bullet (a dot) per character.
                let mut l = self.text_field(v, placeholder, "", *field, p, env);
                let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
                let (d, pitch, x0) = SECURE_BULLET;
                let y = (l.size.h - d) / 2.0;
                l.kids.retain(|(_, _, k)| k.node.as_ref().is_none_or(|n| n.kind != "text"));
                for i in 0..text.chars().count() {
                    let mut b = Node::new(format!("{}#bullet{i}", v.id), "bullet");
                    b.fill = Some(fg);
                    b.radius = PILL;
                    b.inspect = false;
                    l.kids.push((x0 + i as f64 * pitch, y, Lay::new(Size { w: d, h: d }, Some(b), vec![])));
                }
                if let Some(n) = l.node.as_mut() {
                    n.value = Some(text.clone());
                }
                l
            }
            ViewKind::Picker { label, style, options, selected, open, disabled } => {
                if *style == PickerStyle::Segmented {
                    self.segmented(v, options, *selected, *disabled, p)
                } else {
                    self.menu_picker(v, label, options, *selected, *open, *disabled, p, child_env)
                }
            }
            ViewKind::Slider { value, min, max, field, disabled, .. } => self.slider(v, *value, (*min, *max), *field, *disabled, p),
            ViewKind::Progress { value, label } => self.progress(v, *value, label.as_deref(), p, child_env),
            ViewKind::Image { system_name, resizable, content_mode, scale } => {
                self.image(v, system_name, *resizable, *content_mode, scale.unwrap_or(env.img), p, env, fg)
            }
            ViewKind::Label { title, icon } => self.label(v, title, icon, p, child_env),
        }
    }

    /// SF Symbol. Non-resizable images are glyph-sized from the font; resizable ones stretch to
    /// the proposal (or fit / fill it keeping the symbol's aspect ratio).
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, v: &ViewNode, name: &str, resizable: bool, mode: Option<ContentMode>, scale: ImageScale, p: Prop, env: Env, fg: Color) -> Lay {
        let spec = SymbolSpec { name: name.to_string(), point_size: env.size as f64, weight: env.weight.unwrap_or(env.base_weight), scale };
        let nat = symbols::metrics(&spec);
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (nw, nh) = nat.map_or((env.size as f64, env.size as f64), |m| (m.w, m.h));
        let (size, glyph) = if !resizable {
            (Size { w: nw, h: nh }, (nw, nh))
        } else {
            let (pw, ph) = (fin(p.w), fin(p.h));
            let ratio = nw / nh;
            match mode {
                None => {
                    let s = Size { w: pw.unwrap_or(nw), h: ph.unwrap_or(nh) };
                    (s, (s.w, s.h))
                }
                Some(m) => {
                    let (bw, bh) = match (pw, ph) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => (w, w / ratio),
                        (None, Some(h)) => (h * ratio, h),
                        (None, None) => (nw, nh),
                    };
                    let by_w = (bw, bw / ratio);
                    let by_h = (bh * ratio, bh);
                    let fit = if by_w.1 <= bh { by_w } else { by_h };
                    let fill = if by_w.1 >= bh { by_w } else { by_h };
                    match m {
                        // Fit shrink-wraps the symbol; fill occupies the whole proposal and crops.
                        ContentMode::Fit => (Size { w: fit.0, h: fit.1 }, fit),
                        ContentMode::Fill => (Size { w: bw, h: bh }, fill),
                    }
                }
            }
        };
        let mut n = Node::new(&v.id, "image");
        n.role = Role::Image;
        n.label = Some(name.to_string());
        if nat.is_some() {
            n.symbol = Some(SymbolRun { spec, color: fg, size: glyph });
        } else {
            // Placeholder: rounded-square outline, reported as `unsupported_symbol`.
            self.unsupported.insert(name.to_string());
            n.stroke = Some((fg, 1.0));
            n.radius = size.w.min(size.h) * 0.2;
        }
        Lay::new(size, Some(n), vec![])
    }

    /// iOS `Label`: icon then title. In a list row the icon is centred in a fixed leading column.
    fn label(&mut self, v: &ViewNode, title: &ViewNode, icon: &ViewNode, p: Prop, env: Env) -> Lay {
        let icon_fg = env.fg.or_else(|| env.in_row.then(|| self.semantic(Semantic::Accent)));
        // ponytail: list rows force imageScale large on the icon, an explicit imageScale on the Label is ignored there.
        let img = if env.in_row { ImageScale::Large } else { env.img };
        let il = self.view(icon, Prop { w: None, h: None }, Env { fg: icon_fg, img, ..env });
        let (col, gap) = if env.in_row { (LABEL_ICON_COL, LABEL_ROW_GAP) } else { (il.size.w, LABEL_GAP) };
        let tw = p.w.filter(|w| w.is_finite()).map(|w| (w - col - gap).max(0.0));
        let tl = self.view(title, Prop { w: tw, h: None }, env);
        let h = il.size.h.max(tl.size.h);
        let w = col + gap + tl.size.w;
        let ix = (col - il.size.w) / 2.0;
        let mut n = Node::new(&v.id, "label");
        n.label = first_text(title);
        Lay::new(
            Size { w, h },
            Some(n),
            vec![(ix, (h - il.size.h) / 2.0, il), (col + gap, (h - tl.size.h) / 2.0, tl)],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn button(&mut self, v: &ViewNode, action: u64, label: &ViewNode, style: ButtonStyle, disabled: bool, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let accent = self.semantic(Semantic::Accent);
        let dim = self.semantic(Semantic::Tertiary);
        let mut n = Node::new(&v.id, "button");
        n.role = Role::Button;
        n.action = (!disabled).then_some(action);
        n.disabled = disabled;
        n.label = first_text(label);
        match style {
            ButtonStyle::Automatic | ButtonStyle::Plain => {
                let (fg, tint) = match (style, disabled) {
                    (_, true) => (dim, None),
                    (ButtonStyle::Automatic, _) => (accent, Some(accent)),
                    _ => (env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)), None),
                };
                let l = self.view(label, p, Env { fg: Some(fg), tint, ..child_env });
                Lay::new(l.size, Some(n), vec![(0.0, 0.0, l)])
            }
            ButtonStyle::Bordered | ButtonStyle::BorderedProminent => {
                let prominent = style == ButtonStyle::BorderedProminent;
                let (fg, bg) = match (prominent, disabled) {
                    (_, true) => (dim, self.control_fill(0.16)),
                    (true, _) => (rgba(255, 255, 255, 1.0), if self.desat { let g = if self.dark { ALERT_DESAT.1 } else { ALERT_DESAT.0 }; rgba(g, g, g, 1.0) } else { accent }),
                    (false, _) => (accent, self.control_fill(0.16)),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.fill = Some(bg);
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::Glass | ButtonStyle::GlassProminent => {
                let prominent = style == ButtonStyle::GlassProminent;
                let fg = match (prominent, disabled) {
                    (_, true) => dim,
                    (true, _) => rgba(255, 255, 255, 1.0),
                    (false, _) => self.semantic(Semantic::Primary),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.glass = Some(self.glass_spec(GlassVariant::Regular, prominent.then_some(simcore_protocol::Color::Semantic { semantic: Semantic::Accent }).as_ref(), true));
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::NavigationLink => {
                let fg = if disabled { dim } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
                // 7x12 at body size, growing with Dynamic Type (21.7x37.3, 8.6pt inset at AX5; measured).
                let k = (simcore_protocol::TextStyle::Body.size_at(self.dts) / 17.0).max(1.0);
                let (chev_w, chev_h, inset) = (7.0 * k, 12.0 * k, 2.3 + 2.97 * (k - 1.0));
                let inner_p = Prop { w: p.w.filter(|w| w.is_finite()).map(|w| (w - chev_w - 12.0).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let w = p.w.filter(|w| w.is_finite()).unwrap_or(l.size.w + 12.0 + chev_w);
                let h = l.size.h.max(chev_h);
                let mut c = Node::new(format!("{}#chevron", v.id), "chevron");
                c.glyph = Some((Glyph::Chevron, self.semantic(Semantic::Tertiary)));
                c.inspect = false;
                let cl = Lay::new(Size { w: chev_w, h: chev_h }, Some(c), vec![]);
                let ly = (h - l.size.h) / 2.0;
                Lay::new(Size { w, h }, Some(n), vec![(0.0, ly, l), (w - chev_w - inset, (h - chev_h) / 2.0, cl)])
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn toggle(&mut self, v: &ViewNode, on: bool, action: u64, label: &ViewNode, disabled: bool, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - SWITCH.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + SWITCH.0);
        let h = l.size.h.max(SWITCH.1);
        let mut sw = Node::new(format!("{}#switch", v.id), "switch");
        let knob = switch_style(&mut sw, &v.id, self.dark, on, disabled, None, None);
        sw.label = first_text(label);
        sw.action = (!disabled).then_some(action);
        let (kw, kh) = KNOB;
        let kx = if on { SWITCH.0 - 2.0 - kw } else { 2.0 };
        let kl = Lay::new(Size { w: kw, h: kh }, Some(knob), vec![]);
        let sl = Lay::new(Size { w: SWITCH.0, h: SWITCH.1 }, Some(sw), vec![(kx, (SWITCH.1 - kh) / 2.0, kl)]);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(Node::new(&v.id, "toggle")), vec![(0.0, ly, l), (w - SWITCH.0 + 2.0, (h - SWITCH.1) / 2.0, sl)])
    }

    fn stepper(&mut self, v: &ViewNode, label: &ViewNode, dec: Option<u64>, inc: Option<u64>, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - STEPPER.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + STEPPER.0);
        let h = l.size.h.max(STEPPER.1);
        let mut root = Node::new(&v.id, "stepper");
        root.role = Role::Stepper;
        root.label = first_text(label);
        let mut ctl = Node::new(format!("{}#control", v.id), "stepper-control");
        ctl.fill = Some(self.control_fill(0.12));
        ctl.radius = PILL;
        ctl.inspect = false;
        let half = STEPPER.0 / 2.0;
        let mut kids = vec![];
        for (name, action, glyph, x) in [("decrement", dec, Glyph::Minus, 0.0), ("increment", inc, Glyph::Plus, half)] {
            let mut b = Node::new(format!("{}#{name}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(name.into());
            b.action = action;
            b.disabled = action.is_none();
            let c = if action.is_some() { self.semantic(Semantic::Primary) } else { self.semantic(Semantic::Tertiary) };
            b.glyph = Some((glyph, c));
            kids.push((x, 0.0, Lay::new(Size { w: half, h: STEPPER.1 }, Some(b), vec![])));
        }
        let mut div = Node::new(format!("{}#divider", v.id), "divider");
        div.fill = Some(self.semantic(Semantic::Tertiary));
        div.inspect = false;
        kids.push((half - self.hairline() / 2.0, 8.0, Lay::new(Size { w: self.hairline(), h: 16.0 }, Some(div), vec![])));
        let cl = Lay::new(Size { w: STEPPER.0, h: STEPPER.1 }, Some(ctl), kids);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(root), vec![(0.0, ly, l), (w - STEPPER.0, (h - STEPPER.1) / 2.0, cl)])
    }

    fn text_field(&mut self, v: &ViewNode, placeholder: &str, text: &str, field: u64, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(120.0);
        let h = 22.0;
        let spec = self.spec(&Env { align: TextAlign::Leading, ..env });
        let focused = self.focus == Some(v.id.as_str());
        let empty = text.is_empty();
        let shown = if empty { placeholder } else { text };
        let color = if empty { self.semantic(Semantic::Tertiary) } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
        let mut t = self.text_lay(&format!("{}#text", v.id), shown, spec, color, None);
        let tw = if empty { 0.0 } else { t.size.w };
        let overflow = (tw - (w - 2.0)).max(0.0);
        if let Some(tr) = t.node.as_mut().and_then(|n| n.text.as_mut()) {
            tr.offset_x = -overflow as f32;
        }
        if let Some(n) = t.node.as_mut() {
            n.inspect = false;
        }
        let mut n = Node::new(&v.id, "text_field");
        n.role = Role::TextField;
        n.field = Some(field);
        n.value = Some(text.to_string());
        n.label = Some(placeholder.to_string());
        n.focused = focused;
        n.clip_radius = Some(0.0);
        let mut kids = vec![(0.0, (h - t.size.h) / 2.0, t)];
        if focused {
            let mut c = Node::new(format!("{}#caret", v.id), "caret");
            c.fill = Some(self.semantic(Semantic::Accent));
            c.radius = 1.0;
            c.inspect = false;
            kids.push(((tw - overflow).min(w - 2.0) + 1.0, 1.0, Lay::new(Size { w: 2.0, h: h - 2.0 }, Some(c), vec![])));
        }
        Lay::new(Size { w, h }, Some(n), kids)
    }

    /// An SF Symbol leaf at `pt` points.
    fn symbol_lay(&mut self, id: String, name: &str, pt: f64, weight: u16, color: Color) -> Lay {
        let spec = SymbolSpec { name: name.into(), point_size: pt, weight, scale: ImageScale::Medium };
        let (w, h) = symbols::metrics(&spec).map_or((pt, pt), |m| (m.w, m.h));
        let mut n = Node::new(id, "image");
        n.symbol = Some(SymbolRun { spec, color, size: (w, h) });
        n.inspect = false;
        Lay::new(Size { w, h }, Some(n), vec![])
    }

    /// Menu `Picker` row: label leading; selected value and chevron.up.chevron.down trailing, secondary.
    /// The whole control is a button that opens the menu layer.
    #[allow(clippy::too_many_arguments)]
    fn menu_picker(&mut self, v: &ViewNode, label: &ViewNode, options: &[PickerOption], selected: Option<usize>, open: Option<u64>, disabled: bool, p: Prop, env: Env) -> Lay {
        let fin = p.w.filter(|w| w.is_finite());
        let tint = if disabled { self.semantic(Semantic::Tertiary) } else { self.semantic(Semantic::Secondary) };
        let text = selected.and_then(|i| options.get(i)).map_or("", |o| o.text.as_str());
        let spec = self.spec(&Env { align: TextAlign::Leading, ..env });
        let mut vl = self.text_lay(&format!("{}#value", v.id), text, spec, tint, None);
        if let Some(n) = vl.node.as_mut() {
            n.inspect = false;
        }
        let cl = self.symbol_lay(format!("{}#chevron", v.id), "chevron.up.chevron.down", PICKER_CHEVRON.0, PICKER_CHEVRON.1, tint);
        let trail = vl.size.w + PICKER_CHEVRON_GAP + cl.size.w;
        let l = self.view(label, Prop { w: fin.map(|w| (w - trail - 8.0).max(0.0)), h: None }, env);
        let w = fin.unwrap_or(l.size.w + 8.0 + trail);
        let h = l.size.h.max(vl.size.h);
        let mut n = Node::new(&v.id, "picker");
        n.role = Role::Button;
        n.action = open.filter(|_| !disabled);
        n.disabled = disabled;
        n.label = first_text(label);
        n.value = Some(text.to_string());
        let (lh, vh, ch) = (l.size.h, vl.size.h, cl.size.h);
        Lay::new(Size { w, h }, Some(n), vec![(0.0, (h - lh) / 2.0, l), (w - trail - PICKER_TRAIL_INSET, (h - vh) / 2.0, vl), (w - cl.size.w - PICKER_TRAIL_INSET, (h - ch) / 2.0, cl)])
    }

    /// Segmented `Picker`: a capsule track, the selected segment a raised white capsule.
    fn segmented(&mut self, v: &ViewNode, options: &[PickerOption], selected: Option<usize>, disabled: bool, p: Prop) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(options.len() as f64 * 80.0);
        let h = SEGMENTED_H;
        let mut root = Node::new(&v.id, "segmented");
        root.fill = Some(self.control_fill(SEGMENTED_TRACK_A));
        root.radius = PILL;
        root.disabled = disabled;
        root.value = selected.and_then(|i| options.get(i)).map(|o| o.text.clone());
        let n = options.len().max(1) as f64;
        let sw = w / n;
        let primary = self.semantic(Semantic::Primary);
        let mut kids = vec![];
        for (i, o) in options.iter().enumerate() {
            let on = selected == Some(i);
            if on {
                let mut thumb = Node::new(format!("{}#thumb", v.id), "segment-thumb");
                thumb.fill = Some(if self.dark { rgba(SEGMENTED_THUMB_DARK.0, SEGMENTED_THUMB_DARK.1, SEGMENTED_THUMB_DARK.2, 1.0) } else { rgba(255, 255, 255, 1.0) });
                thumb.radius = PILL;
                thumb.shadow = (!self.dark).then_some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.12), radius: 4.0, dx: 0.0, dy: 2.0 });
                thumb.inspect = false;
                kids.push((i as f64 * sw + SEGMENTED_INSET, SEGMENTED_INSET, Lay::new(Size { w: sw - 2.0 * SEGMENTED_INSET, h: h - 2.0 * SEGMENTED_INSET }, Some(thumb), vec![])));
            }
            let fs = SEGMENTED_FONT.0;
            let spec = FontSpec::new(fs, self.line_h(fs), if on { SEGMENTED_FONT.2 } else { SEGMENTED_FONT.1 }, TextAlign::Leading);
            let mut t = self.text_lay(&format!("{}#seg{i}#text", v.id), &o.text, spec, if disabled { self.semantic(Semantic::Tertiary) } else { primary }, None);
            if let Some(tn) = t.node.as_mut() {
                tn.inspect = false;
            }
            let mut b = Node::new(format!("{}#seg{i}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!disabled && !on).then_some(o.action);
            b.disabled = disabled;
            let (tw, th) = (t.size.w, t.size.h);
            kids.push((i as f64 * sw, 0.0, Lay::new(Size { w: sw, h }, Some(b), vec![((sw - tw) / 2.0, (h - th) / 2.0, t)])));
        }
        Lay::new(Size { w, h }, Some(root), kids)
    }

    /// `Slider`: capsule track, accent fill up to the value, a white capsule thumb. Tapping or
    /// dragging sets the value (see `Interaction`), through `field`.
    fn slider(&mut self, v: &ViewNode, value: f64, range: (f64, f64), field: Option<u64>, disabled: bool, p: Prop) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(200.0);
        let h = SLIDER_H;
        let span = range.1 - range.0;
        let frac = if span > 0.0 { ((value - range.0) / span).clamp(0.0, 1.0) } else { 0.0 };
        let (tw, th) = SLIDER_THUMB;
        let tx = frac * (w - tw);
        let mut n = Node::new(&v.id, "slider");
        n.role = Role::Slider;
        n.field = field.filter(|_| !disabled);
        n.range = Some((range.0, range.1, tw));
        n.value = Some(format!("{value}"));
        n.disabled = disabled;
        let ty = (h - SLIDER_TRACK) / 2.0;
        let mut track = Node::new(format!("{}#track", v.id), "slider-track");
        track.fill = Some(self.pick(SLIDER_TRACK_FILL));
        track.radius = PILL;
        track.inspect = false;
        let mut fill = Node::new(format!("{}#fill", v.id), "slider-fill");
        fill.fill = Some(self.semantic(Semantic::Accent));
        fill.radius = PILL;
        fill.inspect = false;
        let mut thumb = Node::new(format!("{}#thumb", v.id), "slider-thumb");
        thumb.fill = Some(rgba(255, 255, 255, 1.0));
        thumb.radius = PILL;
        thumb.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, SLIDER_SHADOW.0), radius: SLIDER_SHADOW.1, dx: 0.0, dy: SLIDER_SHADOW.2 });
        thumb.inspect = false;
        let kids = vec![
            (0.0, ty, Lay::new(Size { w, h: SLIDER_TRACK }, Some(track), vec![])),
            (0.0, ty, Lay::new(Size { w: tx + tw / 2.0, h: SLIDER_TRACK }, Some(fill), vec![])),
            (tx, (h - th) / 2.0, Lay::new(Size { w: tw, h: th }, Some(thumb), vec![])),
        ];
        let mut l = Lay::new(Size { w, h }, Some(n), kids);
        if disabled {
            if let Some(n) = l.node.as_mut() {
                n.opacity = 0.5;
            }
        }
        l
    }

    /// `ProgressView`: a linear capsule bar with a value, else the spinner; a label sits above the
    /// bar or below the spinner.
    fn progress(&mut self, v: &ViewNode, value: Option<f64>, label: Option<&ViewNode>, p: Prop, env: Env) -> Lay {
        let fin = p.w.filter(|w| w.is_finite());
        let ll = label.map(|l| self.view(l, Prop { w: fin, h: None }, env));
        let lh = ll.as_ref().map_or(0.0, |l| l.size.h + PROGRESS_LABEL_GAP);
        let mut n = Node::new(&v.id, "progress");
        n.value = value.map(|f| format!("{f}"));
        n.label = label.and_then(first_text);
        let mut kids = vec![];
        let size = if let Some(f) = value {
            let w = fin.unwrap_or(200.0);
            let mut track = Node::new(format!("{}#track", v.id), "progress-track");
            track.fill = Some(self.pick(PROGRESS_TRACK_FILL));
            track.radius = PILL;
            track.inspect = false;
            let mut bar = Node::new(format!("{}#bar", v.id), "progress-bar");
            bar.fill = Some(self.semantic(Semantic::Accent));
            bar.radius = PILL;
            bar.inspect = false;
            let fw = (w * f.clamp(0.0, 1.0)).max(if f > 0.0 { PROGRESS_H } else { 0.0 });
            let bl = Lay::new(Size { w: fw, h: PROGRESS_H }, Some(bar), vec![]);
            kids.push((0.0, lh, Lay::new(Size { w, h: PROGRESS_H }, Some(track), vec![(0.0, 0.0, bl)])));
            if let Some(l) = ll {
                kids.push((0.0, 0.0, l));
            }
            Size { w, h: lh + PROGRESS_H }
        } else {
            let lw = ll.as_ref().map_or(0.0, |l| l.size.w);
            let w = SPINNER.max(lw);
            let mut sp = Node::new(format!("{}#spinner", v.id), "spinner");
            sp.glyph = Some((Glyph::Spinner, self.semantic(Semantic::Secondary)));
            sp.inspect = false;
            kids.push(((w - SPINNER) / 2.0, 0.0, Lay::new(Size { w: SPINNER, h: SPINNER }, Some(sp), vec![])));
            if let Some(l) = ll {
                kids.push(((w - lw) / 2.0, SPINNER + PROGRESS_LABEL_GAP, l));
            }
            Size { w, h: SPINNER + lh }
        };
        Lay::new(size, Some(n), kids)
    }

    /// Menu of a menu `Picker` (iOS 26): a glass card under (or over) the anchor row's trailing
    /// edge, one row per option, a checkmark before the selected one. A tap outside closes it.
    fn menu(&mut self, items: &[PickerOption], anchor: Option<Rect>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let primary = self.semantic(Semantic::Primary);
        let spec = FontSpec::new(17.0, self.line_h(17.0), 400, TextAlign::Leading);
        let texts: Vec<Lay> = items.iter().enumerate().map(|(i, o)| {
            let mut t = self.text_lay(&format!("menu#{i}#text"), &o.text, spec, primary, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            t
        }).collect();
        let cw = (texts.iter().map(|t| t.size.w).fold(0.0, f64::max) + MENU_TEXT_X + MENU_PAD_R).clamp(MENU_MIN_W, w - 2.0 * POPOVER_MARGIN);
        let ch = items.len() as f64 * MENU_ROW_H + 2.0 * MENU_PAD_V;
        let mut kids = vec![];
        for (i, (o, t)) in items.iter().zip(texts).enumerate() {
            let mut b = Node::new(format!("menu#{i}"), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = Some(o.action);
            b.value = o.checked.then(|| "checked".into());
            let th = t.size.h;
            let mut bk = vec![(MENU_TEXT_X, (MENU_ROW_H - th) / 2.0, t)];
            if o.checked {
                let c = self.symbol_lay(format!("menu#{i}#check"), "checkmark", MENU_CHECK.0, MENU_CHECK.1, primary);
                let (cw2, chh) = (c.size.w, c.size.h);
                bk.push((MENU_CHECK_X - cw2 / 2.0, (MENU_ROW_H - chh) / 2.0, c));
            }
            kids.push((0.0, MENU_PAD_V + i as f64 * MENU_ROW_H, Lay::new(Size { w: cw, h: MENU_ROW_H }, Some(b), bk)));
        }
        let mut card = Node::new("menu", "menu");
        card.radius = MENU_RADIUS;
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { MENU_GLASS.1 } else { MENU_GLASS.0 }), shadow: Some(POPOVER_SHADOW), ..self.glass_spec(GlassVariant::Regular, None, false) });
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        // Trailing and bottom edges line up with the picker's button; it hangs down from it when there is no room above.
        let x = (a.x + a.w - cw + MENU_DX).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
        let (top, bottom) = (a.center().1 - MENU_DY, a.center().1 + MENU_DY);
        let y = if bottom - ch >= g.safe_top { bottom - ch } else { top.min(h - g.dev_bottom - ch) };
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("menu#scrim", (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, Lay::new(Size { w: cw, h: ch }, Some(card), kids))])
    }

    fn scroll(&mut self, v: &ViewNode, vertical: bool, child: &ViewNode, p: Prop, env: Env) -> Lay {
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let child_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, ..env };
        let cp = if vertical {
            Prop { w: p.w, h: None }
        } else {
            Prop { w: None, h: p.h }
        };
        let c = self.view(child, cp, child_env);
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (cw, ch) = if vertical { (c.size.w, c.size.h + top + bottom) } else { (c.size.w, c.size.h) };
        let w = fin(p.w).unwrap_or(cw);
        let h = fin(p.h).unwrap_or(ch);
        let max = ((cw - w).max(0.0), (ch - h).max(0.0));
        let off = self.scroll.get(&v.id).copied().unwrap_or((0.0, 0.0));
        let off = if vertical { (0.0, self.clamp_offset(off.1, max.1, h)) } else { (self.clamp_offset(off.0, max.0, w), 0.0) };
        let mut n = Node::new(&v.id, "scroll");
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None, content: None });
        n.inspect = true;
        let cx = if vertical { (w - c.size.w) / 2.0 } else { 0.0 };
        Lay::new(Size { w, h }, Some(n), vec![(cx - off.0, top - off.1, c)])
    }

    fn list(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], p: Prop, env: Env) -> Lay {
        let inset = style == ListStyle::InsetGrouped;
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let margin = if inset { self.margin } else { 0.0 };
        let gw = w - 2.0 * margin;
        let inner_w = gw - 2.0 * self.margin;
        let row_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, fg: None, in_row: true, ..env };
        let sec_col = if self.floating { let c = if self.dark { FLOAT_CARD.1 } else { FLOAT_CARD.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::SecondaryGroupedBackground) };
        let sep_col = if self.floating { let c = if self.dark { FLOAT_SEP.1 } else { FLOAT_SEP.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::Separator) };
        let mut y = top;
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        for (si, sec) in sections.iter().enumerate() {
            let sid = format!("{}#s{si}", v.id);
            if let Some(h) = &sec.header {
                let hs = simcore_protocol::TextStyle::Headline.size_at(self.dts) as f32;
                let spec = FontSpec::new(hs, self.line_h(hs), 600, TextAlign::Leading);
                let mut t = self.text_lay(&format!("{sid}#header"), h, spec, self.semantic(Semantic::Secondary), Some(inner_w as f32));
                if let Some(n) = t.node.as_mut() {
                    n.role = Role::Text;
                }
                let hh = t.size.h + HEADER_PAD;
                kids.push((margin + self.margin, y + (hh - t.size.h) / 2.0, t));
                y += hh;
            }
            let mut sec_kids: Vec<(f64, f64, Lay)> = vec![];
            let mut ry = 0.0;
            for (ri, row) in sec.rows.iter().enumerate() {
                let mut c = self.view(row, Prop { w: Some(inner_w), h: None }, row_env);
                // Measured: 34.3pt buttons and 44pt content get 15pt vertical padding, 32pt steppers 10pt.
                let vpad = if has_bordered(row) || c.size.h > 33.0 { ROW_VPAD_BORDERED } else { ROW_VPAD };
                let rh = ROW_MIN_H.max(c.size.h + 2.0 * vpad);
                let yo = (rh - c.size.h) / 2.0;
                expand_button(&mut c, 0.0, yo, (-self.margin, 0.0, gw - self.margin, rh));
                let tx = first_text_x(&c, 0.0).unwrap_or(0.0);
                let mut rn = Node::new(format!("{sid}#r{ri}"), "row");
                rn.inspect = false;
                let last = ri + 1 == sec.rows.len();
                let mut rkids = vec![(self.margin, yo, c)];
                if !last {
                    let mut sp = Node::new(format!("{sid}#r{ri}#sep"), "separator");
                    sp.fill = Some(sep_col);
                    sp.inspect = false;
                    let x0 = self.margin + tx;
                    // Measured: 1pt (3px @3x) solid line ending at the row boundary.
                    rkids.push((x0, rh - 1.0, Lay::new(Size { w: (gw - self.margin - x0).max(0.0), h: 1.0 }, Some(sp), vec![])));
                }
                sec_kids.push((0.0, ry, Lay::new(Size { w: gw, h: rh }, Some(rn), rkids)));
                ry += rh;
            }
            let mut sn = Node::new(&sid, "section");
            if inset {
                sn.fill = Some(sec_col);
                sn.radius = CARD_RADIUS;
            }
            sn.inspect = true;
            kids.push((margin, y, Lay::new(Size { w: gw, h: ry }, Some(sn), sec_kids)));
            y += ry;
            if let Some(f) = &sec.footer {
                let spec = FontSpec::new(13.0, self.line_h(13.0), 400, TextAlign::Leading);
                let t = self.text_lay(&format!("{sid}#footer"), f, spec, self.semantic(Semantic::Secondary), Some(inner_w as f32));
                y += 8.0;
                let th = t.size.h;
                kids.push((margin + self.margin, y, t));
                y += th;
            }
            // Between sections 17.7; after the last one the list ends 20pt above the bottom inset (measured).
            y += if si + 1 == sections.len() { 20.0 } else { SECTION_GAP };
            // Measured: a header-less section after another sits 35pt below it (header-ful ones: 17.7 + header).
            if sections.get(si + 1).is_some_and(|n| n.header.is_none()) {
                y += HEADERLESS_EXTRA_GAP;
            }
        }
        let ch = y + bottom;
        let fin_h = p.h.filter(|h| h.is_finite());
        let h = fin_h.unwrap_or(ch);
        let max = (0.0, (ch - h).max(0.0));
        let off = self.clamp_offset(self.scroll.get(&v.id).map_or(0.0, |o| o.1), max.1, h);
        let mut n = Node::new(&v.id, "list");
        n.fill = (!self.floating).then(|| self.semantic(if inset { Semantic::GroupedBackground } else { Semantic::Background }));
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max, snap: None, content: None });
        let kids = kids.into_iter().map(|(x, ky, k)| (x, ky - off, k)).collect();
        Lay::new(Size { w, h }, Some(n), kids)
    }

    #[allow(clippy::too_many_arguments)]
    // Out of line: inlined into the recursive `view` dispatcher it inflates every stack frame.
    #[inline(never)]
    fn navigation(
        &mut self,
        v: &ViewNode,
        title: Option<&str>,
        mode: DisplayMode,
        back: Option<&simcore_protocol::BackButton>,
        content: &ViewNode,
        p: Prop,
        env: Env,
    ) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(874.0);
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let large = mode == DisplayMode::Large && title.is_some();
        let (large_title, title_h, band) = self.nav_large(&v.id, w, title, large);
        let bar_h = if title.is_none() && back.is_none() { 0.0 } else { NAV_INLINE_H + band };
        let ctop = top + bar_h;
        let scroller = is_scroller(content);
        let edge_bg = self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background });
        let mut bg = Node::new(&v.id, "navigation");
        bg.fill = Some(self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background }));
        bg.label = title.map(str::to_string);
        let cenv = Env { safe_top: 0.0, bottom_inset: bottom, fg: None, ..env };
        let mut kids = vec![];
        let mut offset = 0.0;
        if scroller {
            // Content scrolls under the bar: the scroller spans the screen with the bar as a top inset.
            let mut c = self.view(content, Prop { w: Some(w), h: Some(h) }, Env { safe_top: ctop, ..cenv });
            if let Some(s) = c.node.as_mut().and_then(|n| n.scroll.as_mut()).filter(|_| large) {
                s.snap = Some(band);
            }
            offset = c.node.as_ref().and_then(|n| n.scroll).map_or(0.0, |s| s.offset.1);
            kids.push((0.0, 0.0, c));
            // Content reaches under the bar once the large title has collapsed.
            if offset > band {
                let mut e = Node::new(format!("{}#edge", v.id), "scrolledge");
                e.edge = Some(crate::glass::ScrollEdge {
                    bar: (top + NAV_INLINE_H) as f32,
                    color: edge_bg,
                    tint: if self.dark { 0.0 } else { 3.0 / 255.0 },
                });
                e.inspect = false;
                kids.push((0.0, 0.0, Lay::new(Size { w, h: ctop + 33.0 }, Some(e), vec![])));
            }
        } else {
            let avail = (h - ctop - bottom).max(0.0);
            let c = self.view(content, Prop { w: Some(w), h: Some(avail) }, Env { bottom_inset: 0.0, ..cenv });
            kids.push(((w - c.size.w) / 2.0, ctop + (avail - c.size.h) / 2.0, c));
        }
        let back = back.map(|b| NavBack { action: Some(b.action), label: b.label.clone(), pill: false });
        let bar = NavBar { id: &v.id, w, h, top, bar_h, title, large, large_title, title_h, band, offset, back };
        kids.push((0.0, top, self.nav_bar(bar)));
        Lay::new(Size { w, h }, Some(bg), kids)
    }

    /// Large title lay (when `large`), its height and the band it occupies below the inline bar.
    /// Large title: LargeTitle bold, growing with Dynamic Type. Its band below the inline bar is
    /// 52pt, or the title's height + 1.33 when taller (measured: 106pt bar at large, 127 at AX5).
    fn nav_large(&mut self, id: &str, w: f64, title: Option<&str>, large: bool) -> (Option<Lay>, f64, f64) {
        let primary = self.semantic(Semantic::Primary);
        let large_title = title.filter(|_| large).map(|t| {
            let sz = simcore_protocol::TextStyle::LargeTitle.size_at(self.dts) as f32;
            let spec = FontSpec::new(sz, self.line_h(sz), 700, TextAlign::Leading);
            self.text_lay(&format!("{id}#title"), t, spec, primary, Some((w - 32.0) as f32))
        });
        let title_h = large_title.as_ref().map_or(0.0, |l| l.size.h);
        let band = if large { (title_h + 1.33).max(52.0) } else { 0.0 };
        (large_title, title_h, band)
    }

    /// The navigation bar (no background at rest on iOS 26): large + inline title and the glass back
    /// button. Shared by SwiftUI `NavigationStack` and React Native's native stack header.
    fn nav_bar(&mut self, nb: NavBar) -> Lay {
        let NavBar { id, w, h, top, bar_h, title, large, large_title, title_h, band, offset, back } = nb;
        let primary = self.semantic(Semantic::Primary);
                // The large title scrolls with the content and disappears under the inline bar; the bar
        // shrinks with it and the inline title fades in over the last INLINE_FADE points.
        let collapse = offset.clamp(0.0, band);
        let mut bar = Node::new(format!("{}#bar", id), "navbar");
        bar.label = title.map(str::to_string);
        let mut bar_kids = vec![];
        if let Some(tl) = large_title.filter(|_| offset < band) {
            // Title top: 54pt under the bar's top, lowered when the band has spare room (57.7 at large).
            let ty = NAV_INLINE_H + (band - title_h - 7.6).max(0.0) - offset;
            if offset > 0.0 {
                // Mid-collapse: cut off where it slides under the inline bar.
                let mut clip = Node::new(format!("{}#largetitle", id), "clip");
                clip.clip_radius = Some(0.0);
                clip.inspect = false;
                let clip_h = h - top - NAV_INLINE_H;
                bar_kids.push((0.0, NAV_INLINE_H, Lay::new(Size { w, h: clip_h }, Some(clip), vec![(self.margin, ty - NAV_INLINE_H, tl)])));
            } else {
                bar_kids.push((self.margin, ty, tl));
            }
        }
        if let Some(t) = title {
            let alpha = if large { ((collapse - (band - INLINE_FADE)) / INLINE_FADE).clamp(0.0, 1.0) as f32 } else { 1.0 };
            if alpha > 0.0 {
                // Inline titles grow with Dynamic Type but cap at x1.225 (measured at accessibility5).
                let nsz = 17.0 * (simcore_protocol::TextStyle::Body.size_at(self.dts) as f32 / 17.0).min(1.225);
                let spec = FontSpec::new(nsz, self.line_h(nsz), 600, TextAlign::Leading);
                let id = if large { format!("{}#inline", id) } else { format!("{}#title", id) };
                let mut tl = self.text_lay(&id, t, spec, primary, Some((w - 120.0) as f32));
                if let Some(n) = tl.node.as_mut() {
                    n.opacity = alpha;
                }
                let (tw, th) = (tl.size.w, tl.size.h);
                bar_kids.push(((w - tw) / 2.0, 22.0 - th / 2.0, tl));
            }
        }
        if let Some(b) = back {
            let mut n = Node::new(format!("{id}#back"), "button");
            n.role = Role::Button;
            n.action = b.action;
            n.label = Some(if b.label.is_empty() { "Back".into() } else { b.label.clone() });
            if self.sheet_up {
                // Under a sheet the glass goes flat: a barely visible disc remains.
                let a = if self.dark { BACK_FLAT.1 } else { BACK_FLAT.0 };
                n.fill = Some(Color::rgba(1.0, 1.0, 1.0, a));
            } else {
                n.glass = Some(self.glass_spec(GlassVariant::Regular, None, true));
            }
            n.radius = PILL;
            // Under a presentation the chevron desaturates like the rest of the presenter.
            let primary = if self.desat {
                let g = if self.dark { ALERT_DESAT.1 } else { ALERT_DESAT.0 };
                rgba(g, g, g, 1.0)
            } else {
                primary
            };
            if b.pill {
                // Pill with chevron and the previous title (measured iOS 26.5: chevron centre 19pt in, text at 35, 17.7 trailing).
                let spec = FontSpec::new(17.0, self.line_h(17.0), 400, TextAlign::Leading);
                let mut tl = self.text_lay(&format!("{id}#backtitle"), &b.label, spec, primary, None);
                if let Some(t) = tl.node.as_mut() {
                    t.inspect = false;
                }
                let mut chev = Node::new(format!("{id}#chevron"), "chevron");
                chev.glyph = Some((Glyph::BackChevron, primary));
                chev.inspect = false;
                let (tw, th) = (tl.size.w, tl.size.h);
                let kids = vec![(0.0, 0.0, Lay::new(Size { w: 38.0, h: 44.0 }, Some(chev), vec![])), (35.0, (44.0 - th) / 2.0, tl)];
                bar_kids.push((self.margin, 0.0, Lay::new(Size { w: 35.0 + tw + 17.7, h: 44.0 }, Some(n), kids)));
            } else {
                n.glyph = Some((Glyph::BackChevron, primary));
                bar_kids.push((self.margin, 0.0, Lay::new(Size { w: 44.0, h: 44.0 }, Some(n), vec![])));
            }
        }
        Lay::new(Size { w, h: bar_h - collapse }, Some(bar), bar_kids)
    }

    /// The iOS 26 floating Liquid Glass tab bar over a `w` x `h` screen (measured: 188 x 61 pt capsule for two tabs,
    /// 22 pt above the bottom, items 94 x 54 pt at an 86 pt pitch, 4 pt inside the capsule; badge an 18 pt red disc
    /// centred 18 pt right of the item centre, its top at the item's).
    fn tab_bar(&mut self, t: &TabBar, host: &str, w: f64, h: f64) -> Lay {
        let n = t.items.len().max(1) as f64;
        let bar_w = 8.0 + 94.0 + (n - 1.0) * 86.0;
        let mut bar = Node::new(format!("{host}#tabbar"), "tabbar");
        bar.radius = PILL;
        bar.glass = Some(crate::glass::GlassSpec { variant: GlassVariant::Regular, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None });
        let (label, accent) = if self.dark { (rgba(255, 255, 255, 1.0), rgba(10, 132, 255, 1.0)) } else { (rgba(0, 0, 0, 1.0), rgba(0, 136, 255, 1.0)) };
        let mut kids = vec![];
        for (i, tab) in t.items.iter().enumerate() {
            let sel = i == t.selected;
            let mut item = Node::new(format!("{host}#tab-{}", tab.key), "button");
            item.role = Role::Button;
            item.label = Some(tab.title.clone());
            // React Native items have no action: the host's numeric tag makes them hit-testable (the host sends `SelectTab`).
            item.action = tab.action.or_else(|| host.parse().ok());
            item.radius = PILL;
            if sel {
                item.fill = Some(if self.dark { rgba(58, 58, 60, 1.0) } else { rgba(233, 233, 235, 1.0) });
            }
            let tint = if sel { t.tint.unwrap_or(accent) } else { label };
            let path = if sel { tab.selected_icon.as_ref().or(tab.icon.as_ref()) } else { tab.icon.as_ref() };
            let mut icon = Node::new(format!("{host}#tab-{}#icon", tab.key), "image");
            icon.inspect = false;
            let mut isz = (0.0, 0.0);
            if let Some(pm) = path.and_then(|p| crate::image::load(p)) {
                isz = (pm.width() as f64 / self.scale, pm.height() as f64 / self.scale);
                let dest = Rect { x: 0.0, y: 0.0, w: isz.0, h: isz.1 };
                icon.image = super::rn::tinted(&pm, tint).map(|pixmap| crate::ImageRun { pixmap, dest, rotation: 0.0 });
            } else if let Some(name) = &tab.symbol {
                let spec = crate::SymbolSpec { name: name.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium };
                isz = crate::symbols::metrics(&spec).map_or((22.0, 22.0), |m| (m.w, m.h));
                icon.symbol = Some(crate::SymbolRun { spec, color: tint, size: isz });
            }
            let mut lb = Node::new(format!("{host}#tab-{}#label", tab.key), "text");
            lb.inspect = false;
            lb.text = Some(TextRun { text: tab.title.clone(), font: FontSpec::new(10.0, 12.0, if sel { 600 } else { 500 }, TextAlign::Center), color: label, strike: false, offset_x: 0.0, spans: vec![] });
            let mut ikids = vec![
                ((94.0 - isz.0) / 2.0, 6.0, Lay::new(Size { w: isz.0, h: isz.1 }, Some(icon), vec![])),
                (0.0, 31.0, Lay::new(Size { w: 94.0, h: 14.0 }, Some(lb), vec![])),
            ];
            if let Some(b) = &tab.badge {
                let spec = FontSpec::new(12.0, 14.0, 400, TextAlign::Center);
                let bw = (self.fonts.measure(b, &spec, None, &[]).0 + 10.0).max(18.0);
                let mut bn = Node::new(format!("{host}#tab-{}#badge", tab.key), "badge");
                bn.inspect = false;
                bn.radius = PILL;
                bn.fill = Some(if self.dark { rgba(255, 66, 69, 1.0) } else { rgba(255, 56, 60, 1.0) });
                let mut bt = Node::new(format!("{host}#tab-{}#badge#text", tab.key), "text");
                bt.inspect = false;
                bt.text = Some(TextRun { text: b.clone(), font: spec, color: rgba(255, 255, 255, 1.0), strike: false, offset_x: 0.0, spans: vec![] });
                let text = Lay::new(Size { w: bw, h: 14.0 }, Some(bt), vec![]);
                ikids.push((65.0 - bw / 2.0, 0.0, Lay::new(Size { w: bw, h: 18.0 }, Some(bn), vec![(0.0, 2.0, text)])));
            }
            kids.push((4.0 + i as f64 * 86.0, 4.0, Lay::new(Size { w: 94.0, h: 54.0 }, Some(item), ikids)));
        }
        let bar = Lay::new(Size { w: bar_w, h: 61.0 }, Some(bar), kids);
        Lay::new(Size { w, h }, None, vec![((w - bar_w) / 2.0, h - 20.0 - 61.0, bar)])
    }

    /// `TabView`: the selected tab's content placed like a root above the tab bar's inset, then the bar.
    fn tab_view(&mut self, v: &ViewNode, selected: usize, tabs: &[simcore_protocol::TabItem], content: &ViewNode, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(874.0);
        let g = Geo { w, h, safe_top: env.safe_top, safe_bottom: env.bottom_inset.max(TAB_BAR_INSET), dev_bottom: TAB_BAR_INSET, body: env.size };
        let inner = self.place_root(content, &g);
        let items = tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                // Tab bars draw the filled variant of a symbol when there is one.
                let symbol = t.system_image.as_ref().map(|s| {
                    let fill = format!("{s}.fill");
                    let spec = crate::SymbolSpec { name: fill.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium };
                    if s.ends_with(".fill") || crate::symbols::metrics(&spec).is_none() { s.clone() } else { fill }
                });
                TabBarItem { key: i.to_string(), title: t.title.clone(), icon: None, selected_icon: None, symbol, badge: t.badge.clone(), action: Some(t.action) }
            })
            .collect();
        let bar = self.tab_bar(&TabBar { items, selected, tint: None }, &v.id, w, h);
        let mut n = Node::new(&v.id, "tabview");
        n.inspect = false;
        Lay::new(Size { w, h }, Some(n), vec![(0.0, 0.0, inner), (0.0, 0.0, bar)])
    }

    /// Scale a laid-out subtree by `k` about its origin: geometry, fonts, symbols, strokes, radii, shadows.
    fn scale_lay(&mut self, l: &mut Lay, k: f64) {
        let one_line = l.node.as_ref().and_then(|n| n.text.as_ref()).is_some_and(|t| l.size.h <= t.font.line_height as f64 * 1.5);
        l.size = Size { w: l.size.w * k, h: l.size.h * k };
        l.infl = (l.infl.0 * k, l.infl.1 * k, l.infl.2 * k, l.infl.3 * k);
        if let Some(n) = l.node.as_mut() {
                        n.radius = scaled_radius(n.radius, k);
            n.clip_radius = n.clip_radius.map(|x| scaled_radius(x, k));
            n.stroke = n.stroke.map(|(c, w)| (c, w * k));
            if let Some(sh) = n.shadow.as_mut() {
                (sh.radius, sh.dx, sh.dy) = (sh.radius * k, sh.dx * k, sh.dy * k);
            }
            if let Some(t) = n.text.as_mut() {
                let kf = k as f32;
                (t.font.size, t.font.pt, t.font.line_height, t.font.pitch) = (t.font.size * kf, t.font.pt * kf, t.font.line_height * kf, t.font.pitch * kf);
                t.offset_x *= kf;
                for sp in t.spans.iter_mut() {
                    sp.pt *= kf;
                }
                // Smaller type tracks differently: re-measure a single line so it never wraps at its own width.
                if one_line {
                    let (w, _) = self.fonts.measure(&t.text, &FontSpec { align: TextAlign::Leading, ..t.font }, None, &t.spans);
                    l.size.w = (w * self.scale).ceil() / self.scale;
                }
            }
            if let Some(sy) = n.symbol.as_mut() {
                sy.spec.point_size *= k;
                sy.size = (sy.size.0 * k, sy.size.1 * k);
            }
            n.caret = n.caret.map(|c| c * k);
            if let Some(sc) = n.scroll.as_mut() {
                sc.offset = (sc.offset.0 * k, sc.offset.1 * k);
                sc.max = (sc.max.0 * k, sc.max.1 * k);
                sc.snap = sc.snap.map(|s| s * k);
            }
        }
        for (dx, dy, c) in l.kids.iter_mut() {
            *dx *= k;
            *dy *= k;
            self.scale_lay(c, k);
        }
    }

    fn scrim(&self, id: &str, (w, h): (f64, f64), alpha: f32, tap: Option<u64>) -> Node {
        let mut n = Node::new(id, "scrim");
        n.action = tap;
        n.fill = Some(Color::rgba(0.0, 0.0, 0.0, alpha));
        n.bounds = Rect { x: 0.0, y: 0.0, w, h };
        n.inspect = false;
        n
    }

    /// The root view: full-bleed scrollers span the screen, anything else is centered in the safe area.
    fn place_root(&mut self, root: &ViewNode, g: &Geo) -> Lay {
        let full_bleed = matches!(root.kind, ViewKind::Navigation { .. } | ViewKind::List { .. } | ViewKind::Scroll { .. } | ViewKind::TabView { .. });
        if full_bleed {
            let env = Env { safe_top: g.safe_top, bottom_inset: g.safe_bottom, size: g.body, ..Env::new() };
            let lay = self.view(root, Prop { w: Some(g.w), h: Some(g.h) }, env);
            Lay::new(Size { w: g.w, h: g.h }, None, vec![(0.0, 0.0, lay)])
        } else {
            let sh = (g.h - g.safe_top - g.safe_bottom).max(0.0);
            let lay = self.view(root, Prop { w: Some(g.w), h: Some(sh) }, Env { size: g.body, ..Env::new() });
            let (x, y) = ((g.w - lay.size.w) / 2.0, g.safe_top + (sh - lay.size.h) / 2.0);
            Lay::new(Size { w: g.w, h: g.h }, None, vec![(x, y, lay)])
        }
    }

    /// `.fullScreenCover`: an opaque screen above everything (status bar aside).
    fn cover(&mut self, content: &ViewNode, g: &Geo) -> Lay {
        let g = Geo { safe_bottom: g.dev_bottom, ..*g };
        let inner = self.place_root(content, &g);
        let mut card = Node::new("cover", "cover");
        card.fill = Some(self.semantic(Semantic::Background));
        card.inspect = false;
        let scrim = Lay::new(Size { w: g.w, h: g.h }, Some(self.scrim("cover#scrim", (g.w, g.h), 0.0, None)), vec![]);
        let cardl = Lay::new(Size { w: g.w, h: g.h }, Some(card), vec![(0.0, 0.0, inner)]);
        Lay::new(Size { w: g.w, h: g.h }, None, vec![(0.0, 0.0, scrim), (0.0, 0.0, cardl)])
    }

    /// Height of a detent (points). `max` is the large detent's height.
    fn detent_h(d: &Detent, h: f64, max: f64) -> f64 {
        match d {
            Detent::Medium => h * SHEET_MEDIUM,
            Detent::Large => max,
            Detent::Fraction { value } => value * max,
            Detent::Height { value } => *value,
        }
        .clamp(1.0, max)
    }

    /// `.sheet`: dimmed screen, a card from its top edge to the bottom. Detents (ascending) set the
    /// resting heights; `pos` is the interaction state (which detent, or a drag in progress). Below its
    /// largest detent the card floats (inset, glass); `depth` is the number of sheets under this one.
    #[allow(clippy::too_many_arguments)]
    fn sheet(&mut self, id: &str, c: &ViewNode, detents: &[Detent], indicator: bool, dismiss: Option<u64>, pos: crate::SheetPos, depth: usize, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let large_top = g.safe_top + SHEET_TOP_GAP + depth as f64 * SHEET_STACK_STEP;
        let max = (h - large_top).max(1.0);
        let mut hs: Vec<f64> = if detents.is_empty() { vec![max] } else { detents.iter().map(|d| Self::detent_h(d, h, max)).collect() };
        hs.sort_by(f64::total_cmp);
        hs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        let tops: Vec<f64> = hs.iter().map(|x| h - x).collect();
        let detent = pos.detent.min(tops.len() - 1);
        let top = pos.top.unwrap_or(tops[detent]).clamp(large_top, h);
        let float = top > large_top + 0.5;
        let (cx, cw) = if float { (SHEET_FLOAT_INSET, w - 2.0 * SHEET_FLOAT_INSET) } else { (0.0, w) };
        let sh = (h - top - if float { SHEET_FLOAT_INSET } else { 0.0 }).max(0.0);
        let radius = if float { SHEET_FLOAT_RADIUS } else { SHEET_RADIUS };
        let list = is_list(c);
        let scroller = is_scroller(c);
        let ctop = SHEET_CONTENT_TOP;
        let env = Env { size: g.body, safe_top: if scroller { ctop } else { 0.0 }, bottom_inset: if float { 0.0 } else { g.dev_bottom }, ..Env::new() };
        (self.elevated, self.floating) = (true, float);
        // Floating: lay the content out as in the large sheet (full width), then shrink it onto the card.
        let k = if float { SHEET_FLOAT_SCALE } else { 1.0 };
        let mut cl = self.view(c, Prop { w: Some(if float { w } else { cw }), h: Some(sh / k) }, env);
        if float {
            self.scale_lay(&mut cl, k);
        }
        let mut card = Node::new(id, "sheet");
        if float {
            card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { SHEET_GLASS.1 } else { SHEET_GLASS.0 }), ..self.glass_spec(GlassVariant::Regular, None, false) });
        } else {
            card.fill = Some(self.semantic(if list { Semantic::GroupedBackground } else { Semantic::Background }));
        }
        (self.elevated, self.floating) = (false, false);
        card.radius = radius;
        card.clip_radius = Some(radius);
        card.sheet = Some(crate::SheetInfo { tops, detent, top, dismiss });
        let (dx, dy) = ((cw - cl.size.w) / 2.0, if scroller { 0.0 } else { (sh - cl.size.h) / 2.0 });
        let mut kids = vec![(dx, dy, cl)];
        if indicator {
            let mut gr = Node::new(format!("{id}#grabber"), "grabber");
            let c = if self.dark { GRABBER_FG.1 } else { GRABBER_FG.0 };
            gr.fill = Some(rgba(c.0, c.1, c.2, 1.0));
            gr.radius = PILL;
            gr.inspect = false;
            kids.push(((cw - GRABBER.0) / 2.0, GRABBER_Y, Lay::new(Size { w: GRABBER.0, h: GRABBER.1 }, Some(gr), vec![])));
        }
        // A full-width card runs past the screen's bottom edge so only its top corners are rounded on screen.
        let ch = if float { sh } else { sh + 2.0 * radius };
        let cardl = Lay::new(Size { w: cw, h: ch }, Some(card), kids);
        // A single dim for stacked sheets (the lower one is covered anyway).
        let dim = if depth > 0 { 0.0 } else if self.dark { SHEET_SCRIM.1 } else { SHEET_SCRIM.0 };
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), dim, None)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (cx, top, cardl)])
    }

    /// The card of an alert or confirmation popover: title, message (left-aligned) and capsule buttons.
    /// Alerts put two buttons side by side (cancel first) and stack more (cancel last); a popover stacks
    /// them and has no cancel button (tapping outside cancels).
    fn alert_card(&mut self, kind: &'static str, title: Option<&str>, message: Option<&ViewNode>, actions: &[ViewNode], card_w: f64) -> Lay {
        let popover = kind == "popover";
        let inner = card_w - 2.0 * ALERT_TEXT_PAD;
        let primary = self.semantic(Semantic::Primary);
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        let mut y = ALERT_TOP;
        let mut gap = 0.0;
        if let Some(t) = title {
            let tsz = simcore_protocol::TextStyle::Headline.size_at(self.dts) as f32;
            let mut tl = self.text_lay("alert#title", t, FontSpec::new(tsz, self.line_h(tsz), 600, TextAlign::Leading), primary, Some(inner as f32));
            if let Some(n) = tl.node.as_mut() {
                n.inspect = false;
            }
            let th = tl.size.h;
            kids.push((ALERT_TEXT_PAD, y, tl));
            y += th;
            gap = ALERT_TITLE_MESSAGE_GAP;
        }
        if let Some(m) = message {
            let fg = if self.dark { ALERT_MESSAGE_FG.1 } else { ALERT_MESSAGE_FG.0 };
            let env = Env { size: ALERT_MESSAGE_PT, align: TextAlign::Leading, fg: Some(rgba(fg.0, fg.1, fg.2, 1.0)), ..Env::new() };
            let ml = self.view(m, Prop { w: Some(inner), h: None }, env);
            y += gap;
            let mh = ml.size.h;
            kids.push((ALERT_TEXT_PAD, y, ml));
            y += mh;
        }
        y += ALERT_TEXT_BUTTON_GAP;
        let is_cancel = |b: &ViewNode| matches!(b.kind, ViewKind::Button { role: Some(simcore_protocol::ButtonRole::Cancel), .. });
        let mut acts: Vec<&ViewNode> = actions.iter().filter(|b| !(popover && is_cancel(b))).collect();
        let side_by_side = !popover && acts.len() == 2;
        acts.sort_by_key(|b| is_cancel(b) != side_by_side); // stable: cancel first when side by side, last when stacked
        let n = acts.len().max(1);
        let bw = if side_by_side { (card_w - 2.0 * ALERT_BTN_PAD - ALERT_BTN_GAP) / 2.0 } else { card_w - 2.0 * ALERT_BTN_PAD };
        let bh = ALERT_BTN_H;
        let accent = self.semantic(Semantic::Accent);
        let red = self.color(&simcore_protocol::Color::Rgba { r: 1.0, g: 56.0 / 255.0, b: 60.0 / 255.0, a: 1.0, dark: Some([1.0, 66.0 / 255.0, 69.0 / 255.0]) });
        let fills = if popover { POPOVER_BTN_FILL } else { ALERT_BTN_FILL };
        for (i, b) in acts.iter().enumerate() {
            let ViewKind::Button { action, label, disabled, role, .. } = &b.kind else { continue };
            let (fg, weight) = match role {
                Some(simcore_protocol::ButtonRole::Destructive) => (red, 600),
                Some(simcore_protocol::ButtonRole::Cancel) => (primary, 600),
                None => (if popover { primary } else { accent }, 600),
            };
            let env = Env { fg: Some(if *disabled { self.semantic(Semantic::Tertiary) } else { fg }), base_weight: weight, align: TextAlign::Center, ..Env::new() };
            let l = self.view(label, Prop { w: Some(bw - 24.0), h: None }, env);
            let mut bn = Node::new(&b.id, "button");
            bn.role = Role::Button;
            bn.action = (!disabled).then_some(*action);
            bn.disabled = *disabled;
            bn.label = first_text(label);
            let f = if self.dark { fills.1 } else { fills.0 };
            bn.fill = Some(rgba(f.0, f.1, f.2, 1.0));
            bn.radius = PILL;
            let (lw, lh) = (l.size.w, l.size.h);
            let bl = Lay::new(Size { w: bw, h: bh }, Some(bn), vec![((bw - lw) / 2.0, (bh - lh) / 2.0, l)]);
            let (bx, by) = if side_by_side {
                (ALERT_BTN_PAD + i as f64 * (bw + ALERT_BTN_GAP), y)
            } else {
                (ALERT_BTN_PAD, y + i as f64 * (bh + ALERT_BTN_GAP))
            };
            kids.push((bx, by, bl));
        }
        let rows = if side_by_side { 1 } else { n };
        y += rows as f64 * bh + (rows - 1) as f64 * ALERT_BTN_GAP + ALERT_BTN_PAD;
        let mut card = Node::new(if popover { "dialog" } else { "alert" }, kind);
        card.label = title.map(str::to_string);
        card.radius = if popover { POPOVER_RADIUS } else { ALERT_RADIUS };
        let (mix, shadow) = if popover { (Some(POPOVER_GLASS), Some(POPOVER_SHADOW)) } else { (None, None) };
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: mix.filter(|_| !self.dark), shadow, ..self.glass_spec(GlassVariant::Regular, None, false) });
        Lay::new(Size { w: card_w, h: y }, Some(card), kids)
    }

    /// `.alert`: dimmed screen, a centered Liquid Glass card.
    fn alert(&mut self, title: &str, message: Option<&ViewNode>, actions: &[ViewNode], g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.alert_card("alert", Some(title), message, actions, ALERT_W);
        let y = cardl.size.h;
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("alert#scrim", (w, h), if self.dark { ALERT_SCRIM.1 } else { ALERT_SCRIM.0 }, None)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), ((w - ALERT_W) / 2.0, h / 2.0 + ALERT_DY - y / 2.0, cardl)])
    }

    /// `.confirmationDialog`: a popover pointing at `anchor` (the label of the presenting view): to its
    /// right when there is room, else above, else below. No dim; a tap outside dismisses it.
    fn popover(&mut self, title: Option<&str>, message: Option<&ViewNode>, actions: &[ViewNode], anchor: Option<Rect>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.alert_card("popover", title, message, actions, POPOVER_W);
        let ch = cardl.size.h;
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let (acx, acy) = a.center();
        #[derive(PartialEq)]
        enum Side { Right, Above, Below }
        let side = if a.x + a.w + POPOVER_SIDE_ARROW + POPOVER_W + POPOVER_MARGIN <= w {
            Side::Right
        } else if a.y + POPOVER_TIP_INSET - POPOVER_ARROW_H - ch >= g.safe_top {
            Side::Above
        } else {
            Side::Below
        };
        let y_clamp = |y: f64| y.clamp(g.safe_top, h - g.dev_bottom - ch);
        let (x, y, glyph, (aw, ah), (ax, ay)) = match side {
            Side::Right => {
                let (x, y) = (a.x + a.w + POPOVER_SIDE_ARROW, y_clamp(acy - ch / 2.0));
                (x, y, Glyph::ArrowLeft, (POPOVER_SIDE_ARROW, POPOVER_ARROW_W), (x - POPOVER_SIDE_ARROW + 0.5, (acy - POPOVER_ARROW_W / 2.0).clamp(y + POPOVER_RADIUS, y + ch - POPOVER_RADIUS - POPOVER_ARROW_W)))
            }
            _ => {
                let x = (acx - POPOVER_W / 2.0).clamp(POPOVER_MARGIN, w - POPOVER_W - POPOVER_MARGIN);
                let ax = (acx - POPOVER_ARROW_W / 2.0).clamp(x + POPOVER_RADIUS, x + POPOVER_W - POPOVER_RADIUS - POPOVER_ARROW_W);
                if side == Side::Above {
                    let y = a.y + POPOVER_TIP_INSET - POPOVER_ARROW_H - ch;
                    (x, y, Glyph::ArrowDown, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y + ch - 0.5))
                } else {
                    let y = a.y + a.h - POPOVER_TIP_INSET + POPOVER_ARROW_H;
                    (x, y, Glyph::ArrowUp, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y - POPOVER_ARROW_H + 0.5))
                }
            }
        };
        let mut arrow = Node::new("dialog#arrow", "arrow");
        arrow.glyph = Some((glyph, { let c = if self.dark { POPOVER_ARROW.1 } else { POPOVER_ARROW.0 }; rgba(c.0, c.1, c.2, 1.0) }));
        arrow.inspect = false;
        let arrowl = Lay::new(Size { w: aw, h: ah }, Some(arrow), vec![]);
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("dialog#scrim", (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, cardl), (ax, ay, arrowl)])
    }

    /// iOS status bar: "9:41" at the left, signal / wifi / battery at the right.
    fn status_bar(&mut self, w: f64, dev: &Device) -> Lay {
        let sb = dev.status_bar;
        let primary = self.semantic(Semantic::Primary);
        let k = sb.icon_scale;
        let text = |cx: &mut Self, id: &str, s: &str, weight: u16, align: TextAlign| {
            let fs = sb.font as f32;
            let spec = FontSpec::new(fs, cx.line_h(fs), weight, align);
            let mut t = cx.text_lay(id, s, spec, primary, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            t
        };
        let glyph = |id: &str, g: Glyph, c: Color, sz: Size| {
            let mut n = Node::new(id, "statusbar");
            n.glyph = Some((g, c));
            n.inspect = false;
            Lay::new(sz, Some(n), vec![])
        };
        let wifi = glyph("statusbar#wifi", Glyph::Wifi, primary, Size { w: 16.0 * k, h: 12.0 * k });
        let batt = glyph("statusbar#battery", Glyph::Battery, primary, Size { w: 27.0 * k, h: 13.0 * k });
        let right = sb.icons.0 + (w - dev.viewport.width); // anchored to the trailing edge
        let dy = sb.icons.1;
        let mut root = Node::new("statusbar", "statusbar");
        root.inspect = false;
        let t = text(self, "statusbar#time", if dev.home_button { "9:41 AM" } else { "9:41" }, 600, TextAlign::Leading);
        let kids = if dev.home_button {
            // Classic layout (home-button phones): carrier + wifi left, centred time, battery right.
            let carrier = text(self, "statusbar#carrier", "Carrier", 400, TextAlign::Leading);
            let ch = carrier.size.h;
            vec![
                (6.5, sb.time.1 - ch / 2.0, carrier),
                (49.7, 4.5 + dy, wifi),
                (sb.time.0 + (w - dev.viewport.width) / 2.0 - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (right, 4.6 + dy, batt),
            ]
        } else {
            let dots = glyph("statusbar#signal", Glyph::SignalDots, self.semantic(Semantic::Tertiary), Size { w: 20.0 * k, h: 4.0 * k });
            let (wx, wy) = (right - 24.7 * k, 26.2 + dy);
            vec![
                (sb.time.0 - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (wx - 27.3 * k, wy + 8.5 * k, dots),
                (wx, wy, wifi),
                (wx + 24.1 * k, wy - 0.2 * k, batt),
            ]
        };
        Lay::new(Size { w, h: dev.safe_top }, Some(root), kids)
    }

    /// SwiftUI stack algorithm: least flexible children are laid out first, each
    /// proposed an equal share of the space still remaining.
    #[allow(clippy::too_many_arguments)]
    fn stack(
        &mut self,
        v: &ViewNode,
        kind: &'static str,
        children: &[ViewNode],
        spacing: Option<f64>,
        align: Alignment,
        vertical: bool,
        p: Prop,
        env: Env,
    ) -> Lay {
        let env = Env { axis: Some(vertical), ..env };
        let (pm, pc) = if vertical { (p.h, p.w) } else { (p.w, p.h) };
        let mk = |m: Option<f64>| if vertical { Prop { w: pc, h: m } } else { Prop { w: m, h: pc } };
        let main_cross = |s: Size| if vertical { (s.h, s.w) } else { (s.w, s.h) };
        let spacer = |c: &ViewNode| matches!(c.kind, ViewKind::Spacer { .. });
        let sp = spacing.unwrap_or(DEFAULT_SPACING);
        let gaps: Vec<f64> = children
            .windows(2)
            .map(|w| if spacer(&w[0]) || spacer(&w[1]) { 0.0 } else { sp })
            .collect();
        let total_gap: f64 = gaps.iter().sum();

        let mut lays: Vec<Option<Lay>> = children.iter().map(|_| None).collect();
        match pm.filter(|m| m.is_finite()) {
            // Unconstrained (or probing with infinity): children take their ideal size.
            None => {
                for (i, c) in children.iter().enumerate() {
                    lays[i] = Some(self.view(c, mk(pm), env));
                }
            }
            Some(m) => {
                let mut order: Vec<(usize, f64)> = children
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let lo = main_cross(self.view(c, mk(Some(0.0)), env).size).0;
                        let hi = main_cross(self.view(c, mk(Some(INF)), env).size).0;
                        (i, hi - lo)
                    })
                    .collect();
                // SwiftUI sizes spacers last: other children share the space among
                // themselves first, spacers split whatever is left.
                order.sort_by(|a, b| {
                    (spacer(&children[a.0]), a.1)
                        .partial_cmp(&(spacer(&children[b.0]), b.1))
                        .unwrap_or(Ordering::Equal)
                });
                let mut remaining = m - total_gap;
                let n_spacers = children.iter().filter(|c| spacer(c)).count();
                let mut left = [children.len() - n_spacers, n_spacers];
                for (i, _) in order {
                    let group = spacer(&children[i]) as usize;
                    let share = (remaining / left[group] as f64).max(0.0);
                    let l = self.view(&children[i], mk(Some(share)), env);
                    remaining -= main_cross(l.size).0;
                    left[group] -= 1;
                    lays[i] = Some(l);
                }
            }
        }

        let lays: Vec<Lay> = lays.into_iter().map(Option::unwrap).collect();
        let cross = lays.iter().map(|l| main_cross(l.size).1).fold(0.0, f64::max);
        let frac = if vertical { h_frac(align) } else { v_frac(align) };
        let mut pos = 0.0;
        let mut kids = Vec::with_capacity(lays.len());
        for (i, l) in lays.into_iter().enumerate() {
            let (m, c) = main_cross(l.size);
            let off = (cross - c) * frac;
            kids.push(if vertical { (off, pos, l) } else { (pos, off, l) });
            pos += m + gaps.get(i).copied().unwrap_or(0.0);
        }
        let size = if vertical { Size { w: cross, h: pos } } else { Size { w: pos, h: cross } };
        Lay::new(size, Some(Node::new(&v.id, kind)), kids)
    }
}

/// Color / Rectangle: take whatever is proposed (10pt when unspecified).
fn greedy(p: Prop) -> Size {
    Size { w: p.w.unwrap_or(10.0), h: p.h.unwrap_or(10.0) }
}

/// Proposal passed to the child of a frame modifier along one axis.
fn child_prop(prop: Option<f64>, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> Option<f64> {
    match (fixed, prop) {
        (Some(f), _) => Some(f),
        (None, Some(p)) => Some(p.clamp(min.unwrap_or(0.0), max.unwrap_or(INF).max(min.unwrap_or(0.0)))),
        (None, None) => None,
    }
}

/// Resulting frame length along one axis. A `max: .infinity` frame fills the proposal.
fn frame_len(prop: Option<f64>, child: f64, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> f64 {
    if let Some(f) = fixed {
        return f;
    }
    let mut r = child;
    match (max, prop) {
        (Some(m), Some(p)) if m == INF => r = p.max(child),
        (Some(m), _) if m.is_finite() => r = r.min(m),
        _ => {}
    }
    if let Some(mn) = min {
        r = r.max(mn);
    }
    r
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const HOME: &str = include_str!("../../tests/data/home.json");
    const CONTROLS: &str = include_str!("../../tests/data/controls.json");
    const TODOS: &str = include_str!("../../tests/data/todos.json");
    const TYPOGRAPHY: &str = include_str!("../../tests/data/typography.json");

    fn scene_with(json: &str, vp: (f64, f64), dark: bool, safe: (f64, f64), scroll: &HashMap<String, (f64, f64)>, focus: Option<&str>) -> Scene {
        scene_sheets(json, vp, dark, safe, scroll, focus, &HashMap::new())
    }

    fn scene_sheets(json: &str, vp: (f64, f64), dark: bool, safe: (f64, f64), scroll: &HashMap<String, (f64, f64)>, focus: Option<&str>, sheets: &HashMap<String, crate::SheetPos>) -> Scene {
        let v: ViewNode = serde_json::from_str(json).unwrap();
        let dev = Device { safe_top: safe.0, safe_bottom: safe.1, island: None, ..*crate::device::device(crate::device::DEFAULT_DEVICE).unwrap() };
        let params = SceneParams {
            viewport: vp,
            device: &dev,
            scale: 3.0,
            appearance: if dark { Appearance::Dark } else { Appearance::Light },
            dynamic_type: DynamicTypeSize::Large,
            scroll,
            focus,
            sheets,
            overscroll: false,
            status_bar: safe.0 > 0.0,
            chrome: Default::default(),
        };
        layout(&v, &params, &mut Fonts::new())
    }

    fn scene(json: &str, vp: (f64, f64)) -> Scene {
        scene_with(json, vp, false, (0.0, 0.0), &HashMap::new(), None)
    }

    fn phone(json: &str) -> Scene {
        scene_with(json, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), None)
    }

    fn by_id<'a>(s: &'a Scene, id: &str) -> &'a Node {
        s.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 0.2, "{a} != {b}");
    }

    #[test]
    fn tab_view_bar_badge_and_inset() {
        let s = phone(
            r#"{"kind":"tabview","id":"t","selected":1,"tabs":[{"title":"Home","systemImage":"house","action":1},{"title":"Counter","action":2},{"title":"Settings","systemImage":"gearshape","badge":"3","action":3}],
               "content":{"kind":"text","id":"c","text":"Hi"}}"#,
        );
        // Measured on iOS 26.5 (3 tabs, PNG pixels): items 94 x 54 at x 68 / 154 / 240 (y 797; the a11y frame says 795); badge 18 pt disc centred at (305, 806).
        let item = |i: usize| by_id(&s, &format!("t#tab-{i}"));
        for (i, x) in [68.0, 154.0, 240.0].into_iter().enumerate() {
            near(item(i).bounds.x, x);
            near(item(i).bounds.y, 797.0);
            assert_eq!(item(i).action, Some(i as u64 + 1));
        }
        assert!(item(1).fill.is_some() && item(0).fill.is_none(), "selected item has the capsule");
        let b = by_id(&s, "t#tab-2#badge").bounds;
        near(b.x + b.w / 2.0, 305.0);
        near(b.y + b.h / 2.0, 806.0);
        // Non-scrolling content is centred between the status bar and the bar's 83 pt inset.
        let c = by_id(&s, "c").bounds;
        near(c.y + c.h / 2.0, 62.0 + (874.0 - 62.0 - TAB_BAR_INSET) / 2.0);
    }

    #[test]
    fn spacer_pushes_and_padding_applies() {
        let s = scene(
            r#"{"kind":"vstack","id":"v","spacing":0,"modifiers":[{"padding":{"top":10,"leading":10,"bottom":10,"trailing":10}},{"frame":{"maxWidth":"inf","maxHeight":"inf"}}],
                "children":[{"kind":"color","id":"a","color":{"r":1,"g":0,"b":0,"a":1},"modifiers":[{"frame":{"height":50}}]},
                            {"kind":"spacer","id":"sp"},
                            {"kind":"color","id":"b","color":{"r":0,"g":0,"b":1,"a":1},"modifiers":[{"frame":{"width":30,"height":50}}]}]}"#,
            (200.0, 400.0),
        );
        let a = by_id(&s, "a").bounds;
        let b = by_id(&s, "b").bounds;
        assert_eq!((a.x, a.y, a.h), (10.0, 10.0, 50.0));
        assert_eq!(a.w, 180.0);
        assert_eq!(b.y, 340.0); // pushed to bottom: 400 - 10 - 50
        assert_eq!(b.w, 30.0);
    }

    #[test]
    fn text_beats_spacer_for_space() {
        // HStack { Text(long); Spacer() } in 300pt: text must not wrap at half width.
        let json = r#"{"kind":"hstack","id":"h","alignment":"center","children":[
            {"kind":"text","id":"t","text":"Ship the SwiftUI subset"},
            {"kind":"spacer","id":"s"}]}"#;
        let s = scene(json, (300.0, 400.0));
        let t = by_id(&s, "t");
        assert!(t.bounds.h < 30.0, "text wrapped: {:?}", t.bounds);
    }

    #[test]
    fn deep_nesting_is_fast() {
        let mut json = r#"{"kind":"text","id":"t","text":"leaf"}"#.to_string();
        for i in 0..30 {
            json = format!(r#"{{"kind":"vstack","id":"v{i}","alignment":"center","children":[{json},{{"kind":"spacer","id":"s{i}"}}]}}"#);
        }
        let t = std::time::Instant::now();
        let s = scene(&json, (393.0, 852.0));
        assert!(by_id(&s, "t").bounds.w > 0.0);
        assert!(t.elapsed().as_secs() < 5, "layout is exponential in depth");
    }

    #[test]
    fn button_hit_and_clip() {
        let s = scene(
            r#"{"kind":"button","id":"b","action":3,"label":{"kind":"text","id":"t","text":"Go",
                "modifiers":[{"padding":{"top":8,"leading":8,"bottom":8,"trailing":8}},{"background":{"kind":"color","id":"bg","color":{"semantic":"accent"}}}]},
                "modifiers":[{"cornerRadius":10}]}"#,
            (200.0, 200.0),
        );
        let b = by_id(&s, "b");
        assert_eq!(b.action, Some(3));
        let (cx, cy) = b.bounds.center();
        assert_eq!(s.hit_test(cx, cy).map(|i| s.nodes[i].id.as_str()), Some("b"));
        // the very corner is outside the rounded clip
        assert_eq!(s.hit_test(b.bounds.x + 0.2, b.bounds.y + 0.2), None);
        assert_eq!(s.hit_test(1.0, 1.0), None);
    }

    /// Frames measured on an iOS 26 simulator (tests/fixtures/showcase-ios/README.md).
    #[test]
    fn inset_grouped_list_matches_ios_frames() {
        let s = phone(HOME);
        let row = by_id(&s, "l.0");
        near(row.bounds.x, 16.0);
        near(row.bounds.y, 208.3);
        near(row.bounds.w, 370.0);
        near(row.bounds.h, 52.0);
        near(by_id(&s, "l.1").bounds.y, 260.3);
        near(by_id(&s, "l.3").bounds.y, 364.3);
        // header (17pt semibold text, 40.3pt cell) then card
        near(by_id(&s, "l#s0#header").bounds.h, 20.3);
        near(by_id(&s, "l#s1").bounds.y, 474.3);
        // card: 4 rows, radius 26, secondary grouped background
        let card = by_id(&s, "l#s0");
        near(card.bounds.h, 208.0);
        assert_eq!(card.radius, CARD_RADIUS);
        assert_eq!(card.fill, Some(rgba(255, 255, 255, 1.0)));
        assert_eq!(by_id(&s, "l").fill, Some(rgba(242, 242, 247, 1.0)));
        // chevron 7x12, right edge 367.7
        let ch = by_id(&s, "l.0#chevron").bounds;
        near(ch.x, 360.7);
        near(ch.w, 7.0);
        near(ch.h, 12.0);
        // 3 separators in the 4-row card, none after the last row; inset to text (x=32)
        let seps: Vec<_> = s.nodes.iter().filter(|n| n.kind == "separator").collect();
        assert_eq!(seps.len(), 3);
        near(seps[0].bounds.x, 32.0);
        near(seps[0].bounds.x + seps[0].bounds.w, 370.0);
        near(seps[0].bounds.y + seps[0].bounds.h, 260.3);
        // the whole row is the hit target, e.g. right next to the chevron
        let hit = s.hit_test(383.0, 234.0).map(|i| s.nodes[i].id.as_str());
        assert_eq!(hit, Some("l.0"));
        // text metrics: 17pt label is 20.3pt tall
        near(by_id(&s, "l.0.l").bounds.h, 20.33);
        near(by_id(&s, "l.0.l").bounds.x, 32.0);
    }

    #[test]
    fn dark_list_colors() {
        let v: ViewNode = serde_json::from_str(HOME).unwrap();
        let s = layout(
            &v,
            &SceneParams {
                viewport: (402.0, 874.0),
                device: crate::device::device(crate::device::DEFAULT_DEVICE).unwrap(),
                scale: 3.0,
                appearance: Appearance::Dark,
                dynamic_type: DynamicTypeSize::Large,
                scroll: &HashMap::new(),
                sheets: &HashMap::new(),
                focus: None,
                overscroll: false,
                status_bar: true,
                chrome: Default::default(),
            },
            &mut Fonts::new(),
        );
        assert_eq!(by_id(&s, "l#s0").fill, Some(rgba(28, 28, 30, 1.0)));
        assert_eq!(by_id(&s, "l").fill, Some(rgba(0, 0, 0, 1.0)));
        assert_eq!(by_id(&s, "root").fill, Some(rgba(0, 0, 0, 1.0)));
    }

    #[test]
    fn large_title_collapses_into_the_bar() {
        // The HOME list is short; a long one so it can scroll past the 52pt collapse.
        let rows: Vec<String> = (0..30).map(|i| format!(r#"{{"kind":"text","id":"r{i}","text":"Row {i}"}}"#)).collect();
        let json = format!(
            r#"{{"kind":"navigation","id":"n","title":"Long","displayMode":"large","content":{{"kind":"list","id":"l","style":"insetGrouped","sections":[{{"rows":[{}]}}]}}}}"#,
            rows.join(",")
        );
        let at = |off: f64| {
            let mut m = HashMap::new();
            m.insert("l".to_string(), (0.0, off));
            scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &m, None)
        };
        let s = at(0.0);
        near(by_id(&s, "n#title").bounds.y, 119.7);
        assert_eq!(by_id(&s, "l").scroll.unwrap().snap, Some(52.0));
        assert!(s.nodes.iter().all(|n| n.id != "n#inline"), "no inline title while expanded");
        // halfway: the title moved up with the content, the bar shrank, the inline title not yet in
        let s = at(26.0);
        near(by_id(&s, "n#title").bounds.y, 119.7 - 26.0);
        near(by_id(&s, "n#bar").bounds.h, 80.0);
        assert!(s.nodes.iter().all(|n| n.id != "n#inline"));
        // collapsed: large title gone, inline title fully in, inline-height bar
        let s = at(52.0);
        assert!(s.nodes.iter().all(|n| n.id != "n#title"));
        assert_eq!(by_id(&s, "n#inline").opacity, 1.0);
        near(by_id(&s, "n#bar").bounds.h, 54.0);
    }

    #[test]
    fn navigation_bars() {
        let s = phone(HOME);
        // large title below the status bar, left aligned at 16
        let t = by_id(&s, "n#title");
        near(t.bounds.x, 16.0);
        near(t.bounds.y, 119.7);
        near(t.bounds.h, 40.67);
        assert_eq!(t.text.as_ref().unwrap().font.weight, 700);
        near(by_id(&s, "n#bar").bounds.h, 106.0);
        // nav + list are full-bleed
        assert_eq!(by_id(&s, "n").bounds.h, 874.0);
        // the list scrolls under the bar: it spans the screen, content starts below the bar
        near(by_id(&s, "l").bounds.y, 0.0);
        assert!(by_id(&s, "l#s0").bounds.y >= 168.0);
        // inline title is centered at 17pt semibold, back button is a 44pt circle
        let s = phone(TYPOGRAPHY);
        let t = by_id(&s, "n#title");
        near(t.bounds.x + t.bounds.w / 2.0, 201.0);
        near(t.bounds.y, 73.7);
        assert_eq!(t.text.as_ref().unwrap().font.weight, 600);
        near(by_id(&s, "n#bar").bounds.h, 54.0);
        let b = by_id(&s, "n#back");
        assert_eq!((b.bounds.x, b.bounds.y, b.bounds.w, b.bounds.h), (16.0, 62.0, 44.0, 44.0));
        assert_eq!(b.action, Some(99));
        // scroll content starts 16pt below the bar (padding)
        near(by_id(&s, "v.0").bounds.y, 132.0);
        near(by_id(&s, "v.3").bounds.h, 42.33); // 2 wrapped lines: 20.3 + 22 pitch
        // status bar chrome is drawn
        assert!(s.nodes.iter().any(|n| n.id == "statusbar#time"));
    }

    #[test]
    fn toggle_stepper_and_buttons() {
        let s = phone(CONTROLS);
        let sw = by_id(&s, "l.0#switch");
        near(sw.bounds.w, 63.0);
        near(sw.bounds.h, 28.0);
        near(sw.bounds.x, 309.0);
        near(sw.bounds.y, 180.0);
        assert_eq!(sw.fill, Some(rgba(52, 199, 89, 1.0)));
        assert_eq!(sw.value.as_deref(), Some("on"));
        // switch hit target; label area does not toggle
        assert_eq!(s.hit_test(340.0, 194.0).map(|i| s.nodes[i].id.as_str()), Some("l.0#switch"));
        assert_eq!(s.hit_test(60.0, 194.0), None);
        let st = by_id(&s, "l.1#control");
        near(st.bounds.h, 32.0);
        near(st.bounds.x, 276.0);
        near(by_id(&s, "l.1#increment").bounds.w, 47.0);
        // bordered buttons: 34.3pt tall in a 64.3pt row
        near(by_id(&s, "l.2.0").bounds.h, 34.33);
        near(by_id(&s, "l.2.2").bounds.h, 34.33);
        near(by_id(&s, "l#s0#r2").bounds.h, 64.33);
        assert_eq!(by_id(&s, "l.2.2").fill, Some(rgba(0, 136, 255, 1.0)));
    }

    #[test]
    fn tinted_button_label_stays_accent_and_disabled_is_gray() {
        let s = phone(TODOS);
        // done todo: strikethrough text in accent at half strength even with .secondary
        let t = by_id(&s, "l.0.t").text.as_ref().unwrap();
        assert!(t.strike);
        assert!((t.color.b - 1.0).abs() < 1e-6 && (t.color.a - 0.5).abs() < 1e-6);
        // the whole todo row is the button; separators start after the 22pt circle (x=66)
        let r = by_id(&s, "l.0");
        near(r.bounds.w, 370.0);
        let sep = s.nodes.iter().find(|n| n.kind == "separator").unwrap();
        near(sep.bounds.x, 66.0);
        // disabled Add: no action, tertiary text
        let add = by_id(&s, "l.a.b");
        assert!(add.action.is_none() && add.disabled);
        assert_eq!(s.nodes.iter().find(|n| n.id == "l.a.bl").unwrap().text.as_ref().unwrap().color.a, 0.3);
        // text field fills the rest of the row: 299.3 wide at x=32
        let f = by_id(&s, "l.a.f");
        near(f.bounds.x, 32.0);
        assert!((f.bounds.w - 299.3).abs() < 1.0, "field width {}", f.bounds.w); // text widths are within ~1px of iOS
        near(f.bounds.h, 22.0);
        assert_eq!(f.field, Some(1));
        // first headerless section starts right under the nav bar
        near(by_id(&s, "l#s0").bounds.y, 168.0);
        near(by_id(&s, "l#s1#header").bounds.y, 237.7 + 10.0); // 40.3pt header cell, text centred
    }

    #[test]
    fn text_field_focus_draws_caret_and_value() {
        let json = TODOS.replace(r#""text": """#, r#""text": "hello""#);
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), Some("l.a.f"));
        let f = by_id(&s, "l.a.f");
        assert!(f.focused);
        assert_eq!(f.value.as_deref(), Some("hello"));
        let caret = by_id(&s, "l.a.f#caret");
        assert!(caret.bounds.x > f.bounds.x + 20.0, "caret follows the text end");
        // unfocused: no caret
        let s = phone(&json);
        assert!(s.nodes.iter().all(|n| n.id != "l.a.f#caret"));
    }

    #[test]
    fn scroll_offsets_move_and_clip_content() {
        let none = phone(TYPOGRAPHY);
        let y0 = by_id(&none, "v.2").bounds.y;
        let mut off = HashMap::new();
        off.insert("sc".to_string(), (0.0, 40.0));
        let s = scene_with(TYPOGRAPHY, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        // content is short: the offset clamps to the scrollable range (none here)
        let sc = by_id(&s, "sc");
        assert_eq!(sc.scroll.unwrap().max, (0.0, 0.0));
        near(by_id(&s, "v.2").bounds.y, y0);
    }

    #[test]
    fn long_list_scrolls_and_clips_hit_tests() {
        let rows: Vec<String> = (0..30)
            .map(|i| format!(r#"{{"kind":"button","id":"r{i}","action":{i},"style":"navigationLink","label":{{"kind":"text","id":"r{i}.t","text":"Row {i}"}}}}"#))
            .collect();
        let json = format!(
            r#"{{"kind":"navigation","id":"n","title":"Long","displayMode":"large","content":{{"kind":"list","id":"l","style":"insetGrouped","sections":[{{"rows":[{}]}}]}}}}"#,
            rows.join(",")
        );
        let s = phone(&json);
        let sc = by_id(&s, "l").scroll.unwrap();
        assert!(sc.max.1 > 500.0, "content taller than the list: {:?}", sc);
        // row 20 is below the screen: clipped, cannot be hit
        assert_eq!(s.hit_test(100.0, by_id(&s, "r20").bounds.y + 10.0), None);
        // scrolled by 300: row 6 (y=168+312) moves to ~180
        let mut off = HashMap::new();
        off.insert("l".to_string(), (0.0, 300.0));
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        near(by_id(&s, "r6").bounds.y, 168.0 + 6.0 * 52.0 - 300.0);
        assert_eq!(s.scroll_target(200.0, 500.0).map(|i| s.nodes[i].id.as_str()), Some("l"));
        // rows scrolled under the nav bar are clipped: no hit at y=100
        assert_eq!(s.hit_test(100.0, 100.0), None);
        // bottom: last row clears the home indicator inset
        off.insert("l".to_string(), (0.0, 1.0e6));
        let s = scene_with(&json, (402.0, 874.0), false, (62.0, 34.0), &off, None);
        let last = by_id(&s, "r29").bounds;
        assert!(last.y + last.h <= 874.0 - 34.0 + 0.1);
    }

    #[test]
    fn shapes_stroke_overlay_shadow() {
        let s = phone(include_str!("../../tests/data/shapes.json"));
        let c = by_id(&s, "h.0");
        assert_eq!((c.bounds.w, c.bounds.h), (60.0, 60.0));
        assert!(c.radius >= 1.0e5);
        let r = by_id(&s, "r");
        assert_eq!(r.stroke.map(|s| s.1), Some(4.0));
        assert!(r.fill.is_none());
        // overlay text is centred over the rect
        let o = by_id(&s, "r.o");
        near(o.bounds.x + o.bounds.w / 2.0, r.bounds.x + r.bounds.w / 2.0);
        // shadow follows the 12pt corner radius of the card
        let sh = s.nodes.iter().find(|n| n.shadow.is_some() && n.id.starts_with("c#shadow")).unwrap();
        assert_eq!(sh.radius, 12.0);
        // divider hairline in a vstack spans the width
        let t = phone(TYPOGRAPHY);
        let d = by_id(&t, "v.8").bounds;
        near(d.w, 370.0);
        near(d.h, 1.0 / 3.0);
    }

    #[test]
    fn centered_text_aligns_within_its_box() {
        let s = phone(TYPOGRAPHY);
        let t = by_id(&s, "v.9");
        let run = t.text.as_ref().unwrap();
        assert_eq!(run.font.align, TextAlign::Center);
        near(t.bounds.h, 42.33);
        let mid = t.bounds.x + t.bounds.w / 2.0;
        near(mid, 201.0);
    }

    const SYMBOLS: &str = r#"{"kind":"navigation","id":"n","title":"S","displayMode":"large","content":{"kind":"list","id":"l","style":"insetGrouped","sections":[
        {"rows":[{"kind":"label","id":"lab","title":{"kind":"text","id":"lab.t","text":"Favorites"},"icon":{"kind":"image","id":"lab.i","systemName":"star.fill","resizable":false}},
                 {"kind":"label","id":"bad","title":{"kind":"text","id":"bad.t","text":"Bad"},"icon":{"kind":"image","id":"bad.i","systemName":"no.such.symbol.xyz","resizable":false},
                  "modifiers":[{"foregroundColor":{"semantic":"secondary"}}]}]},
        {"rows":[{"kind":"hstack","id":"h","spacing":20,"alignment":"center","children":[
            {"kind":"image","id":"h.0","systemName":"heart.fill","resizable":false},
            {"kind":"image","id":"h.1","systemName":"cloud.sun.fill","resizable":true,"contentMode":"fit","modifiers":[{"frame":{"width":44,"height":44}}]},
            {"kind":"image","id":"h.2","systemName":"checkmark.circle.fill","resizable":false,"modifiers":[{"imageScale":"large"}]}]}]}]}}"#;

    /// Frames measured on iOS 26 (tests/fixtures/showcase-ios/symbols.json): Label icons are
    /// imageScale large, centred 12pt into the row, title at +40; the tall HStack row gets 15pt padding.
    #[test]
    fn symbols_size_like_ios_and_unknown_ones_are_reported() {
        let s = phone(SYMBOLS);
        let cap = crate::symbols::capability();
        let system = cap == "system";
        // Unknown names fall back to an outlined placeholder and are reported.
        let bad = by_id(&s, "bad.i");
        assert!(bad.symbol.is_none() && bad.stroke.is_some());
        assert!(s.unsupported_symbols.contains(&"no.such.symbol.xyz".to_string()));
        assert_eq!(s.unsupported_symbols.len(), if cap == "placeholder" { 5 } else { 1 });
        if !system {
            return; // placeholder host: no glyph metrics to check
        }
        let star = by_id(&s, "lab.i");
        assert!(star.symbol.is_some());
        let c = star.bounds.center();
        near(c.0, 44.0);
        near(star.bounds.w, 28.0); // large star.fill box; ink is 24 wide
        near(by_id(&s, "lab.t").bounds.x, 72.0);
        // accent icon, primary title; an explicit foreground colors both (here secondary on the bad row)
        assert_eq!(star.symbol.as_ref().unwrap().color, rgba(0, 136, 255, 1.0));
        assert_eq!(by_id(&s, "lab.t").text.as_ref().unwrap().color, rgba(0, 0, 0, 1.0));
        assert_eq!(by_id(&s, "bad.i").stroke.map(|s| s.0), by_id(&s, "bad.t").text.as_ref().map(|t| t.color));
        // heart.fill at 17pt: 21x18 box (16.7x15.3 ink on iOS); large is bigger than medium
        let heart = by_id(&s, "h.0").bounds;
        near(heart.w, 21.0);
        near(heart.h, 18.0);
        assert!(by_id(&s, "h.2").bounds.w > 24.0);
        // resizable scaledToFit keeps the symbol's aspect inside the 44pt frame
        let cloud = by_id(&s, "h.1").bounds;
        near(cloud.w, 44.0);
        assert!(cloud.h < 40.0 && cloud.h > 30.0, "{cloud:?}");
        // 44pt content: 74pt row (15pt padding)
        near(by_id(&s, "l#s1#r0").bounds.h, 74.0);
    }

    #[test]
    fn runs_share_a_line_and_alert_blocks_what_is_under_it() {
        let runs = r#"{"kind":"text","id":"t","text":"ab","runs":[{"text":"a"},{"text":"b","fontWeight":"bold"}]}"#;
        let plain = phone(r#"{"kind":"text","id":"t","text":"ab"}"#);
        let mixed = phone(runs);
        assert!((by_id(&mixed, "t").bounds.h - by_id(&plain, "t").bounds.h).abs() < 0.01);
        assert!(by_id(&mixed, "t").bounds.w > by_id(&plain, "t").bounds.w - 1.0);
        let btn = |id: &str, a: u64, l: &str, role: &str| format!(r#"{{"kind":"button","id":"{id}","action":{a},"role":"{role}","label":{{"kind":"text","id":"{id}l","text":"{l}"}}}}"#);
        let j = format!(
            r#"{{"kind":"presentation","id":"p","content":{{"kind":"button","id":"under","action":1,"label":{{"kind":"text","id":"ul","text":"Under"}}}},
              "layers":[{{"kind":"alert","title":"T","actions":[{},{}]}}]}}"#,
            btn("ok", 2, "OK", "destructive"),
            btn("cx", 3, "Cancel", "cancel")
        );
        let s = phone(&j);
        let (ok, cx) = (by_id(&s, "ok").bounds, by_id(&s, "cx").bounds);
        assert!(cx.x < ok.x, "cancel is left of the destructive button");
        let (ux, uy) = by_id(&s, "under").bounds.center();
        assert!(s.hit_test(ux, uy).is_none_or(|i| s.nodes[i].id != "under"), "scrim blocks the presenting screen");
        let (x, y) = ok.center();
        assert_eq!(s.hit_test(x, y).and_then(|i| s.nodes[i].action), Some(2));
    }

    #[test]
    fn sheet_swipes_dismiss_and_snap_between_detents() {
        use crate::{DragPhase, Effect, Interaction};
        let json = |dismiss: &str| {
            format!(
                r#"{{"kind":"presentation","id":"p","content":{{"kind":"text","id":"u","text":"under"}},
                "layers":[{{"kind":"sheet","detents":[{{"kind":"medium"}},{{"kind":"large"}}]{dismiss},
                  "content":{{"kind":"vstack","id":"c","children":[{{"kind":"text","id":"t","text":"hi"}}]}}}}]}}"#
            )
        };
        let show = |inter: &Interaction, j: &str| scene_sheets(j, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), None, &inter.sheets);
        let j = json(r#","dismiss":7"#);
        let top = |s: &Scene| s.nodes.iter().find(|n| n.id == "layer0").unwrap().bounds.y;
        let mut inter = Interaction::default();
        let s = show(&inter, &j);
        assert!((top(&s) - 415.0).abs() < 1.0, "medium detent is the initial one: {}", top(&s));
        // Dragging up (on content that does not scroll) expands to large and the card stops floating.
        let swipe = |inter: &mut Interaction, s: &Scene, dy: f64| {
            inter.drag(s, DragPhase::Begin, 200.0, 700.0, 0.0);
            inter.drag(s, DragPhase::Move, 200.0, 700.0 + dy, 16.0);
            inter.drag(s, DragPhase::End, 200.0, 700.0 + dy, 1000.0)
        };
        assert_eq!(swipe(&mut inter, &s, -200.0), Effect::Redraw);
        let s = show(&inter, &j);
        assert!((top(&s) - 62.0).abs() < 1.0, "large: {}", top(&s));
        // A moderate pull down snaps back to medium; a long one dismisses.
        assert_eq!(swipe(&mut inter, &s, 250.0), Effect::Redraw);
        assert!((top(&show(&inter, &j)) - 415.0).abs() < 1.0);
        let s = show(&inter, &j);
        let e = swipe(&mut inter, &s, 400.0);
        assert!(matches!(e, Effect::Action { id: 7, .. }), "{e:?}");
        // interactiveDismissDisabled: no action, it settles back.
        let mut inter = Interaction::default();
        let s = show(&inter, &json(""));
        assert_eq!(swipe(&mut inter, &s, 400.0), Effect::Redraw);
        assert!((top(&show(&inter, &json(""))) - 415.0).abs() < 1.0);
    }

    #[test]
    fn form_controls_layout_and_slider_input() {
        use crate::{Effect, Interaction};
        use simcore_protocol::{PointerEvent, PointerKind};
        let s = scene(
            r#"{"kind":"vstack","id":"v","alignment":"leading","children":[
                {"kind":"picker","id":"pm","label":{"kind":"text","id":"pl","text":"Size"},"options":[{"text":"S","action":1},{"text":"M","action":2}],"selected":1,"open":9},
                {"kind":"picker","id":"ps","style":"segmented","label":{"kind":"text","id":"sl","text":"x"},"options":[{"text":"A","action":3},{"text":"B","action":4}],"selected":0},
                {"kind":"slider","id":"sd","value":25,"min":0,"max":100,"field":5},
                {"kind":"progress","id":"pv","value":0.5},
                {"kind":"progress","id":"sp"}]}"#,
            (300.0, 600.0),
        );
        let pm = by_id(&s, "pm");
        assert_eq!((pm.action, pm.value.as_deref()), (Some(9), Some("M")), "menu picker opens its menu, shows the selection");
        let ps = by_id(&s, "ps");
        near(ps.bounds.h, SEGMENTED_H);
        assert_eq!(ps.value.as_deref(), Some("A"));
        assert_eq!(by_id(&s, "ps#seg0").action, None, "selected segment is inert");
        assert_eq!(by_id(&s, "ps#seg1").action, Some(4));
        let sd = by_id(&s, "sd");
        near(sd.bounds.w, 300.0);
        near(sd.bounds.h, SLIDER_H);
        near(by_id(&s, "sd#thumb").bounds.x, 0.25 * (300.0 - SLIDER_THUMB.0));
        near(by_id(&s, "pv#bar").bounds.w, 150.0);
        near(by_id(&s, "pv").bounds.h, PROGRESS_H);
        assert!(matches!(by_id(&s, "sp#spinner").glyph, Some((Glyph::Spinner, _))));

        // A tap at the track's end sets max; a drag ends where the finger lifts.
        let mut it = Interaction::default();
        let (y, x0, w) = (sd.bounds.center().1, sd.bounds.x, sd.bounds.w);
        let text = |e: Effect| match e {
            Effect::TextInput { field: 5, text, .. } => text.parse::<f64>().unwrap(),
            e => panic!("{e:?}"),
        };
        assert_eq!(text(it.pointer(&s, PointerEvent::new(PointerKind::Click, x0 + w - 1.0, y))), 100.0);
        let mid = x0 + SLIDER_THUMB.0 / 2.0 + (w - SLIDER_THUMB.0) / 2.0;
        let drag = PointerEvent { dx: mid - (x0 + 5.0), ..PointerEvent::new(PointerKind::Drag, x0 + 5.0, y) };
        assert!((text(it.pointer(&s, drag)) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn weights_are_distinct() {
        let mut f = Fonts::new();
        let w = |f: &mut Fonts, wt| f.measure("Hamburgefonstiv", &FontSpec::new(17.0, 20.33, wt, TextAlign::Leading), None, &[]).0;
        let (r, sb, b) = (w(&mut f, 400), w(&mut f, 600), w(&mut f, 700));
        if f.has_sf() {
            assert!(r < sb && sb < b, "variable wght axis applies: {r} {sb} {b}");
        }
    }
}
