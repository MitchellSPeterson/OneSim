//! SwiftUI-semantics layout: parent proposes, child chooses, parent places.
//!
//! Stacks probe each child at 0 and infinity before the final proposal; `Ctx::memo`
//! caches results per (node, proposal) so nesting stays polynomial instead of 3^depth.
//!
//! Visual constants (row height, card radius, nav bar heights, control sizes) are measured from
//! an iOS 26 / iPhone 17 Pro simulator, see `tests/fixtures/showcase-ios/README.md`.

use super::rn::{ModalLayers, Placement};
use crate::device::Device;
use crate::scene::{Color, CustomShapeEval, Glyph, Node, Pager, Rect, Role, Scene, ScrollInfo, Shadow, SymbolRun, TextRun, VectorFill, VectorGeom, VectorShape, VectorStroke};
use crate::symbols::{self, SymbolSpec};
use crate::text::{FontSpec, Fonts, Span, TextAlign};
use onesim_protocol::{
    Alignment, AnimationSpec, Appearance, Axis, ButtonStyle, ConcentricCorners, ContainerShapeSpec, ContentMode, ContentTransitionKind, CornerStyleSpec, Detent, DisplayMode, DynamicTypeSize, Edges, FitAxes, Font, GaugeStyle, GlassShapeKind, GlassVariant, GridTrack, ImageScale, IosVersion, Layer, LineCap, LineJoin, ListStyle, MatchedGeometryMod, MatchedProps, Modifier, PageIndexBackground, PageIndexDisplay, PickerOption, PickerStyle, RoundedCornerStyle, ScrollSnap, Section, Semantic, ShapeKind, ShapeSizeHint, TabPage, TrackSize, TransitionSpec, UniformCorners, ViewKind, ViewNode,
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
/// iOS 27 dark alert (alert-dark.png, ios27): lighter buttons, dimmer message, pinker destructive label.
const ALERT_27_DARK_BTN: (u8, u8, u8) = (71, 71, 73);
const ALERT_27_DARK_MESSAGE: (u8, u8, u8) = (120, 121, 124);
const ALERT_27_DARK_DESTRUCTIVE: (u8, u8, u8) = (255, 103, 106);
/// iOS 27 dark confirmation popover (dialog-dark.png, ios27).
const POPOVER_27_DARK_BTN: (u8, u8, u8) = (80, 80, 82);
const POPOVER_27_DARK_MESSAGE: (u8, u8, u8) = (128, 130, 134);
const POPOVER_27_DARK_DESTRUCTIVE: (u8, u8, u8) = (255, 109, 112);
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
/// iOS 27 dark alert / popover cards: darker than plain dark glass (alert-dark.png: card ~(35..43) over the dimmed screen).
const PRESENT_GLASS_27_DARK: (f32, f32) = (0.49, 0.125);
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
/// Label-to-bar gap of a linear progress view (iOS 27.2, swiftui-controls indicators).
const PROGRESS_BAR_LABEL_GAP: f64 = 4.67;
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
/// `Menu` card (centred on its button): icon centre and text x when items carry icons, the gap a divider
/// opens between items, separator side inset, and how far the card's top sits above the button's centre.
const MENU_ICON_X: f64 = 39.5;
const MENU_ICON_TEXT_X: f64 = 63.2;
const MENU_DIVIDER_GAP: f64 = 21.0;
const MENU_SEP_INSET: f64 = 24.0;
const MENU_CENTER_DY: f64 = 10.5;

// Section Q controls (iOS 27.2, measured on tests/fixtures/ios27/swiftui-controls-ios).
/// `.toggleStyle(.button)` when on: accent alpha of its capsule.
const TOGGLE_BUTTON_ON_A: f32 = 0.16;
/// Gap between a linear progress bar and its currentValueLabel (caption, secondary).
const PROGRESS_VALUE_GAP: f64 = 4.0;
const PROGRESS_VALUE_PT: f32 = 12.0;
/// Gauge: capacity bar and accessory bar heights; label-to-bar and bar-to-value gaps.
const GAUGE_BAR: f64 = 16.0;
const GAUGE_ACC_BAR: f64 = 8.0;
const GAUGE_LABEL_GAP: f64 = 10.4;
const GAUGE_VALUE_GAP: f64 = 6.3;
const GAUGE_ACC_LABEL_GAP: f64 = 4.0;
/// Accessory circular gauges: box, stroke, track alpha, open arc half-angle (degrees from the top),
/// marker cut-out and dot diameters (linear / circular), value and label sizes.
const GAUGE_RING: f64 = 58.0;
const GAUGE_RING_W: f64 = 5.7;
const GAUGE_RING_TRACK_A: f32 = 0.35;
const GAUGE_ARC_ANGLE: f64 = 140.0;
const GAUGE_MARKER: (f64, f64) = (16.0, 7.33);
const GAUGE_ARC_MARKER: (f64, f64) = (11.0, 5.5);
const GAUGE_RING_PT: f32 = 25.0;
const GAUGE_ARC_LABEL_PT: f32 = 11.0;
/// Wheel picker: height, (offset from the centre, vertical squash) per row away from the selection,
/// font size, highlight band (height, side inset, fill), alpha of the unselected rows.
const WHEEL_H: f64 = 216.0;
const WHEEL_ROWS: [(f64, f64); 4] = [(0.0, 1.0), (30.0, 0.915), (56.0, 0.745), (75.2, 0.45)];
const WHEEL_PT: f32 = 22.0;
const WHEEL_BAND: (f64, f64) = (32.0, 16.67);
const WHEEL_BAND_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((244, 244, 245), (58, 58, 60));
const WHEEL_DIM_A: f32 = 0.45;

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

/// The optional views of a `Gauge`.
#[derive(Clone, Copy)]
struct GaugeLabels<'a> {
    label: Option<&'a ViewNode>,
    current: Option<&'a ViewNode>,
    min: Option<&'a ViewNode>,
    max: Option<&'a ViewNode>,
}

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
    embedded: Option<super::rn::Embedded>,
}

impl Lay {
    fn new(size: Size, node: Option<Node>, kids: Vec<(f64, f64, Lay)>) -> Self {
        Self { size, node: node.map(Box::new), kids, infl: (0.0, 0.0, 0.0, 0.0), embedded: None }
    }
}

#[derive(Clone, Copy, PartialEq)]
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
    /// Active animation index into `Ctx::anims` (section P); `None` = no animation.
    anim: Option<u32>,
    /// Inherited `.contentTransition` (section P).
    content_transition: ContentTransitionKind,
    /// Kerning + tracking, in points.
    extra: f32,
    /// 0 default, 1 serif, 2 rounded, 3 monospaced.
    design: u8,
    /// -2 compressed, -1 condensed, 0 standard, 1 expanded.
    width: i8,
    mono: bool,
    digits: bool,
    /// `Text.Scale.secondary`.
    secondary: bool,
    /// 0 tail, 1 head, 2 middle.
    trunc: u8,
    baseline: f32,
    italic: bool,
    underline: bool,
    under_color: Option<Color>,
    under_pat: u8,
    strike_color: Option<Color>,
    strike_pat: u8,
    /// 0 none, 1 uppercase, 2 lowercase.
    case_fold: u8,
    tighten: bool,
    min_scale: f32,
    max_lines: Option<usize>,
    reserve_lines: Option<usize>,
    /// 0 monochrome, 1 hierarchical, 2 palette, 3 multicolor.
    sym_mode: u8,
    /// 0 automatic, 1 color, 2 draw.
    sym_var: u8,
    sym_grad: bool,
    /// `.renderingMode(.original)`.
    sym_original: bool,
    /// 0 nearest, 1 bilinear, 2 bicubic.
    img_filter: u8,
}

/// Prepared layout for one static SwiftUI list. Clear it before replacing or mutating the view tree
/// or replacing its font system. Layout inputs are checked separately on each pass.
#[derive(Default)]
pub struct SwiftUiLayoutCache {
    list: Option<(ListLayoutKey, ListContent)>,
}

impl SwiftUiLayoutCache {
    /// Release the retained list before starting a new tree or font-system lease.
    pub fn clear(&mut self) {
        self.list = None;
    }
}

#[derive(PartialEq)]
struct ListLayoutKey {
    node: usize,
    width: f64,
    env: Env,
    scale: f64,
    margin: f64,
    row_pad: f64,
    dark: bool,
    dynamic_type: DynamicTypeSize,
    ios: IosVersion,
    system_fonts: bool,
}

#[derive(Clone)]
struct ListContent {
    bottom: f64,
    kids: Vec<(f64, f64, Lay)>,
}

fn cacheable_list(v: &ViewNode, sections: &[Section]) -> bool {
    // Bound retained nodes and strings even if a guest submits a single very large list.
    const MAX_SECTIONS: usize = 64;
    const MAX_ROWS: usize = 512;
    const MAX_TEXT_BYTES: usize = 64 * 1024;
    if v.id.len() > 128 || sections.len() > MAX_SECTIONS || sections.iter().map(|s| s.rows.len()).sum::<usize>() > MAX_ROWS {
        return false;
    }
    let mut bytes = 0;
    for section in sections {
        bytes += section.header.as_ref().map_or(0, String::len) + section.footer.as_ref().map_or(0, String::len);
        for row in &section.rows {
            let ViewKind::Text { text, runs } = &row.kind else { return false };
            if !runs.is_empty() || !row.modifiers.is_empty() {
                return false;
            }
            bytes += row.id.len() + text.len();
        }
    }
    bytes <= MAX_TEXT_BYTES
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
            anim: None,
            content_transition: ContentTransitionKind::Identity,
            extra: 0.0,
            design: 0,
            width: 0,
            mono: false,
            digits: false,
            secondary: false,
            trunc: 0,
            baseline: 0.0,
            italic: false,
            underline: false,
            under_color: None,
            under_pat: 0,
            strike_color: None,
            strike_pat: 0,
            case_fold: 0,
            tighten: false,
            min_scale: 1.0,
            max_lines: None,
            reserve_lines: None,
            sym_mode: 0,
            sym_var: 0,
            sym_grad: false,
            sym_original: false,
            img_filter: 2,
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
    params: &'a SceneParams<'a>,
    dark: bool,
    dts: DynamicTypeSize,
    scale: f64,
    /// Side margin of lists, cards and titles: 20pt on Plus/Max-width screens (>= 414pt), else 16.
    margin: f64,
    /// Horizontal padding of list rows inside their card (`margin`, at least the release's minimum).
    row_pad: f64,
    ios: IosVersion,
    /// Compact title bar (iPhone Duo): 28pt title in a 58pt bar.
    compact_nav: bool,
    /// Back button in the right status column (iPhone Duo): top-left of its 48pt circle, screen points.
    column_back: Option<(f64, f64)>,
    scroll: &'a HashMap<String, (f64, f64)>,
    overscroll: bool,
    focus: Option<&'a str>,
    /// Keyed by (node address, proposal bits). A node's Env is fixed by its ancestors,
    /// so node + proposal fully determines its layout within one pass.
    memo: HashMap<(usize, u64, u64), Lay>,
    list_cache: Option<&'a mut SwiftUiLayoutCache>,
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
    /// Animation specs referenced by `Env::anim` (keeps `Env` `Copy` and small).
    anims: Vec<AnimationSpec>,
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

fn ctx<'a>(params: &'a SceneParams, fonts: &'a mut Fonts) -> Ctx<'a> {
    Ctx {
        fonts,
        params,
        dark: params.appearance == Appearance::Dark,
        dts: params.dynamic_type,
        scale: params.scale.max(1.0),
        margin: if params.viewport.0 >= 414.0 { 20.0 } else { 16.0 },
        row_pad: f64::max(if params.viewport.0 >= 414.0 { 20.0 } else { 16.0 }, crate::look::look(params.ios).row_pad_min),
        ios: params.ios,
        compact_nav: params.device.compact_nav,
        column_back: (params.device.safe_right > 0.0)
            .then_some((params.viewport.0 - 72.25, 123.75)),
        scroll: params.scroll,
        overscroll: params.overscroll,
        focus: params.focus,
        memo: HashMap::new(),
        list_cache: None,
        unsupported: BTreeSet::new(),
        elevated: false,
        floating: false,
        desat: false,
        sheet_up: false,
        anims: Vec::new(),
    }
}

/// Status bar and Dynamic Island on top of `nodes`, for layouts that draw their own content (React Native).
pub(crate) fn add_chrome(nodes: &mut Vec<Node>, params: &SceneParams, fonts: &mut Fonts) {
    if params.status_bar {
        let lay = ctx(params, fonts).status_bar(params.viewport.0, params.device, params.ios);
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

pub(super) fn embedded(
    root: &ViewNode,
    available: (Option<f64>, Option<f64>),
    params: &SceneParams,
    fonts: &mut Fonts,
    place: Option<Placement<'_>>,
) -> (f64, f64) {
    let mut cx = ctx(params, fonts);
    let env = Env { size: onesim_protocol::TextStyle::Body.size_at(params.dynamic_type) as f32, ..Env::new() };
    let laid = cx.view(root, Prop { w: available.0, h: available.1 }, env);
    let size = (laid.size.w, laid.size.h);
    if let Some(Placement { bounds, parent, opacity, nodes, modals }) = place {
        flatten_tree(laid, bounds.x, bounds.y, Some(parent), opacity, nodes, modals);
    }
    size
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    layout_inner(root, params, fonts, None)
}

/// Layout with a cache leased to this tree and font system.
pub fn layout_with_cache(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts, cache: &mut SwiftUiLayoutCache) -> Scene {
    layout_inner(root, params, fonts, Some(cache))
}

fn layout_inner<'a>(root: &ViewNode, params: &'a SceneParams<'a>, fonts: &'a mut Fonts, cache: Option<&'a mut SwiftUiLayoutCache>) -> Scene {
    let (w, h) = params.viewport;
    // Keyboard avoidance: the keyboard top replaces the bottom safe-area inset.
    let kb = params.chrome.keyboard && params.focus.is_some();
    let kb_h = crate::look::keyboard_height(params.device, params.ios);
    let (safe_top, dev_bottom) = (params.device.safe_top, params.device.safe_bottom);
    let safe_bottom = if kb { kb_h } else { dev_bottom };
    let mut cx = ctx(params, fonts);
    // Unstyled text is `.body`, which scales with Dynamic Type.
    let body = onesim_protocol::TextStyle::Body.size_at(params.dynamic_type) as f32;
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(cx.semantic(Semantic::Background));
    let mut nodes = vec![screen];
    let (layers, root): (&[Layer], &ViewNode) = match &root.kind {
        ViewKind::Presentation { content, layers } => (layers, &**content),
        _ => (&[], root),
    };
    if layers.is_empty() {
        cx.list_cache = cache;
    }
    cx.desat = !layers.is_empty();
    cx.sheet_up = layers.iter().any(|l| matches!(l, Layer::Sheet { .. }));
    // A right status column (iPhone Duo) narrows the content; full-width backgrounds still reach the edge.
    let wc = w - params.device.safe_right;
    let geo = Geo { w: wc, h, safe_top, safe_bottom, dev_bottom, body };
    let lay = cx.place_root(root, &geo);
    flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    apply_matched_geometry(&mut nodes);
    if wc < w {
        for n in nodes.iter_mut().skip(1) {
            if n.fill.is_some()
                && n.bounds.x.abs() < 0.5
                && (n.bounds.x + n.bounds.w - wc).abs() < 0.5
            {
                n.bounds.w = w;
            }
        }
    }
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
            Layer::Menu { anchor, items, dismiss, centered } => {
                let rect = anchor.as_ref().and_then(|a| nodes.iter().find(|n| &n.id == a)).map(|n| n.bounds);
                cx.menu(items, rect, *dismiss, *centered, &geo)
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
    resolve_container_shapes(&mut nodes);
    if params.status_bar {
        let lay = cx.status_bar(w, params.device, params.ios);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    }
    let dark = params.appearance == Appearance::Dark;
    if kb {
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift, params.device,
            crate::look::look(params.ios));
    }
    if let Some(isl) = params.device.island {
        crate::chrome::island(&mut nodes, isl);
    }
    if let Some((x, y, hw, hh)) = params.device.home_indicator {
        let mut bar = Node::new("home_indicator", "home_indicator");
        bar.bounds = Rect { x, y, w: hw, h: hh };
        bar.parent = Some(0);
        bar.radius = hh / 2.0;
        bar.fill = Some(if dark {
            Color::rgba(1.0, 1.0, 1.0, 0.6)
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.6)
        });
        bar.inspect = false;
        nodes.push(bar);
    }
    Scene { size: params.viewport, appearance: params.appearance,
        ios: params.ios, nodes, unsupported_symbols: cx.unsupported.into_iter().collect(), notes: vec![] }
}

fn flatten(lay: Lay, x: f64, y: f64, parent: Option<usize>, opacity: f32, out: &mut Vec<Node>) {
    let mut modals = ModalLayers::default();
    flatten_tree(lay, x, y, parent, opacity, out, &mut modals);
    modals.paint(out);
}

fn flatten_tree(lay: Lay, x: f64, y: f64, mut parent: Option<usize>, mut opacity: f32, out: &mut Vec<Node>, modals: &mut ModalLayers) {
    if let Some(mut n) = lay.node {
        let (l, t, r, b) = lay.infl;
        n.bounds = Rect { x: x - l, y: y - t, w: lay.size.w + l + r, h: lay.size.h + t + b };
        // Bitmap dest is local to the node. RN tab icons overwrite it with the absolute bounds afterwards.
        if let Some(im) = n.image.as_mut() {
            im.dest.x += x;
            im.dest.y += y;
        }
        n.parent = parent;
        opacity *= n.opacity;
        n.opacity = opacity;
        out.push(*n);
        parent = Some(out.len() - 1);
    }
    if let Some(embedded) = lay.embedded {
        embedded.place((x, y), parent, opacity, out, modals);
    }
    for (dx, dy, k) in lay.kids {
        flatten_tree(k, x + dx, y + dy, parent, opacity, out, modals);
    }
}

fn design_code(d: onesim_protocol::FontDesign) -> u8 {
    match d {
        onesim_protocol::FontDesign::Default => 0,
        onesim_protocol::FontDesign::Serif => 1,
        onesim_protocol::FontDesign::Rounded => 2,
        onesim_protocol::FontDesign::Monospaced => 3,
    }
}

fn width_code(w: onesim_protocol::FontWidth) -> i8 {
    match w {
        onesim_protocol::FontWidth::Compressed => -2,
        onesim_protocol::FontWidth::Condensed => -1,
        onesim_protocol::FontWidth::Standard => 0,
        onesim_protocol::FontWidth::Expanded => 1,
    }
}

fn trunc_code(m: onesim_protocol::Truncation) -> u8 {
    match m {
        onesim_protocol::Truncation::Tail => 0,
        onesim_protocol::Truncation::Head => 1,
        onesim_protocol::Truncation::Middle => 2,
    }
}

fn pattern_code(p: onesim_protocol::LinePattern) -> u8 {
    match p {
        onesim_protocol::LinePattern::Solid => 0,
        onesim_protocol::LinePattern::Dot => 1,
        onesim_protocol::LinePattern::Dash => 2,
        onesim_protocol::LinePattern::DashDot => 3,
        onesim_protocol::LinePattern::DashDotDot => 4,
    }
}

fn case_code(c: onesim_protocol::TextCase) -> u8 {
    match c {
        onesim_protocol::TextCase::Uppercase => 1,
        onesim_protocol::TextCase::Lowercase => 2,
    }
}

fn symbol_mode_code(m: onesim_protocol::SymbolMode) -> u8 {
    match m {
        onesim_protocol::SymbolMode::Monochrome => 0,
        onesim_protocol::SymbolMode::Hierarchical => 1,
        onesim_protocol::SymbolMode::Palette => 2,
        onesim_protocol::SymbolMode::Multicolor => 3,
    }
}

fn pull_tracking(spec: &FontSpec, spans: &[Span], pull: f32) -> (FontSpec, Vec<Span>) {
    let mut spec = *spec;
    spec.extra -= pull;
    let spans = spans.iter().map(|s| Span { extra: s.extra - pull, ..s.clone() }).collect();
    (spec, spans)
}

fn fold_label(text: &str, spans: Vec<Span>, case_fold: u8) -> (String, Vec<Span>) {
    let fold = |s: &str| match case_fold {
        1 => s.to_uppercase(),
        2 => s.to_lowercase(),
        _ => s.to_string(),
    };
    if case_fold == 0 {
        return (text.to_string(), spans);
    }
    let spans = spans.into_iter().map(|s| Span { text: fold(&s.text), ..s }).collect();
    (fold(text), spans)
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
        ViewKind::Grid { rows, .. } => rows.iter().find_map(|r| r.cells.iter().find_map(first_text)),
        ViewKind::LazyGrid { children, .. } | ViewKind::ViewThatFits { children, .. } => children.iter().find_map(first_text),
        ViewKind::Geometry { child, .. } => first_text(child),
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

/// Wrap `inner` in a layout-only node (clip, opacity, shadow, glass) set up by `f`. Out of line, like `gesture_layer`,
/// so `mods` keeps a small stack frame: deep view trees recurse through it once per modifier.
#[inline(never)]
fn layer(id: String, kind: &'static str, inner: Lay, f: impl FnOnce(&mut Node)) -> Lay {
    let mut n = Node::new(id, kind);
    f(&mut n);
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Wrap `inner` in a gesture view's layout-only node. Out of line so `mods`, which recurses once per modifier,
/// keeps a small stack frame.
#[inline(never)]
fn gesture_layer(id: String, g: &onesim_protocol::GestureMod, inner: Lay) -> Lay {
    let mut n = Node::new(id, "gesture");
    n.gesture = Some(Box::new(g.clone()));
    n.inspect = false;
    // Touches land where an `.offset` moved the content, so the gesture frame moves with it.
    let (dx, dy) = moved_by(&inner);
    let size = inner.size;
    Lay::new(size, None, vec![(dx, dy, Lay::new(size, Some(n), vec![(-dx, -dy, inner)]))])
}

/// Wrap `inner` in a layout-only transition or matchedGeometryEffect node (out of line for stack budget).
#[inline(never)]
fn anim_layer(
    id: String,
    kind: &'static str,
    inner: Lay,
    animation: Option<AnimationSpec>,
    transition: Option<TransitionSpec>,
    matched: Option<MatchedGeometryMod>,
) -> Lay {
    let mut n = Node::new(id, kind);
    n.anim = Some(Box::new(crate::scene::NodeAnim {
        animation,
        transition,
        matched,
        content_transition: ContentTransitionKind::Identity,
    }));
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Wrap `inner` in a `.containerShape(_:)` layout-only node (out of line to keep `mods` stack small).
#[inline(never)]
fn container_shape_layer(id: String, spec: ContainerShapeSpec, inner: Lay) -> Lay {
    let mut n = Node::new(id, "container_shape");
    n.container_shape = Some(Box::new(spec));
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Same-frame `.matchedGeometryEffect(isSource: false)`: align each non-source subtree to its
/// source `(namespace, id)` according to `properties` and `anchor`.
fn apply_matched_geometry(nodes: &mut [Node]) {
    let mut sources: HashMap<(String, String), Rect> = HashMap::new();
    for n in nodes.iter() {
        if let Some(m) = n.anim.as_ref().and_then(|a| a.matched.as_ref()) {
            if m.is_source {
                sources.entry((m.namespace.clone(), m.id.clone())).or_insert(n.bounds);
            }
        }
    }
    if sources.is_empty() {
        return;
    }
    let targets: Vec<(usize, Rect, MatchedProps, [f64; 2])> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let m = n.anim.as_ref()?.matched.as_ref()?;
            if m.is_source {
                return None;
            }
            let src = *sources.get(&(m.namespace.clone(), m.id.clone()))?;
            Some((i, src, m.properties, m.anchor))
        })
        .collect();

    for (root_i, src, props, anchor) in targets {
        let cur = nodes[root_i].bounds;
        let new_w = match props {
            MatchedProps::Frame | MatchedProps::Size => src.w,
            MatchedProps::Position => cur.w,
        };
        let new_h = match props {
            MatchedProps::Frame | MatchedProps::Size => src.h,
            MatchedProps::Position => cur.h,
        };
        let (new_x, new_y) = match props {
            MatchedProps::Frame | MatchedProps::Position => {
                let sax = src.x + src.w * anchor[0];
                let say = src.y + src.h * anchor[1];
                (sax - new_w * anchor[0], say - new_h * anchor[1])
            }
            MatchedProps::Size => {
                let cax = cur.x + cur.w * anchor[0];
                let cay = cur.y + cur.h * anchor[1];
                (cax - new_w * anchor[0], cay - new_h * anchor[1])
            }
        };
        let dx = new_x - cur.x;
        let dy = new_y - cur.y;
        let dw = new_w - cur.w;
        let dh = new_h - cur.h;

        // Collect subtree rooted at `root_i`.
        let mut in_sub = vec![false; nodes.len()];
        in_sub[root_i] = true;
        for j in (root_i + 1)..nodes.len() {
            if nodes[j].parent.is_some_and(|p| in_sub[p]) {
                in_sub[j] = true;
            }
        }
        for (j, n) in nodes.iter_mut().enumerate() {
            if !in_sub[j] {
                continue;
            }
            let fills = (n.bounds.x - cur.x).abs() < 0.5
                && (n.bounds.y - cur.y).abs() < 0.5
                && (n.bounds.w - cur.w).abs() < 0.5
                && (n.bounds.h - cur.h).abs() < 0.5;
            if j == root_i || fills {
                n.bounds = Rect { x: new_x, y: new_y, w: new_w, h: new_h };
            } else {
                n.bounds.x += dx + dw * anchor[0];
                n.bounds.y += dy + dh * anchor[1];
            }
            if let Some(im) = n.image.as_mut() {
                im.dest = n.bounds;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ResolvedContainer {
    Rectangle,
    Circle,
    Capsule { continuous: bool },
    Ellipse,
    RoundedRect { radii: [(f64, f64); 4], continuous: bool },
}

fn resolve_container_spec(
    spec: ContainerShapeSpec,
    bounds: Rect,
    ancestor: Option<(Rect, ResolvedContainer)>,
) -> ResolvedContainer {
    match spec {
        ContainerShapeSpec::Rectangle => ResolvedContainer::Rectangle,
        ContainerShapeSpec::Circle => ResolvedContainer::Circle,
        ContainerShapeSpec::Capsule { corner_style } => ResolvedContainer::Capsule { continuous: corner_style == RoundedCornerStyle::Continuous },
        ContainerShapeSpec::Ellipse => ResolvedContainer::Ellipse,
        ContainerShapeSpec::RoundedRectangle { corner_radius, corner_size, corner_style } => {
            let (rx, ry) = corner_size.map(|[w, h]| (w, h)).unwrap_or_else(|| {
                let r = corner_radius.unwrap_or(0.0);
                (r, r)
            });
            ResolvedContainer::RoundedRect { radii: [(rx, ry); 4], continuous: corner_style == RoundedCornerStyle::Continuous }
        }
        ContainerShapeSpec::UnevenRoundedRectangle { corner_radii, corner_style } => ResolvedContainer::RoundedRect {
            radii: corner_radii.map(|r| (r, r)),
            continuous: corner_style == RoundedCornerStyle::Continuous,
        },
        ContainerShapeSpec::ContainerRelative => resolve_relative_container(bounds, ancestor),
        ContainerShapeSpec::ConcentricRectangle { concentric } => {
            let radii = resolve_concentric_radii(concentric, bounds, ancestor);
            ResolvedContainer::RoundedRect { radii: radii.map(|r| (r, r)), continuous: true }
        }
    }
}

fn edge_distances(inner: Rect, outer: Rect) -> (f64, f64, f64, f64) {
    let left = (inner.x - outer.x).max(0.0);
    let top = (inner.y - outer.y).max(0.0);
    let right = ((outer.x + outer.w) - (inner.x + inner.w)).max(0.0);
    let bottom = ((outer.y + outer.h) - (inner.y + inner.h)).max(0.0);
    (left, top, right, bottom)
}

fn container_corner_radii(outer_rect: Rect, outer: ResolvedContainer) -> [f64; 4] {
    let half = (outer_rect.w.min(outer_rect.h) / 2.0).max(0.0);
    match outer {
        ResolvedContainer::Rectangle | ResolvedContainer::Ellipse => [0.0; 4],
        ResolvedContainer::Circle | ResolvedContainer::Capsule { .. } => [half; 4],
        ResolvedContainer::RoundedRect { radii, .. } => radii.map(|(rx, ry)| rx.min(ry).min(half).max(0.0)),
    }
}

fn resolve_relative_container(bounds: Rect, ancestor: Option<(Rect, ResolvedContainer)>) -> ResolvedContainer {
    let Some((outer_rect, outer)) = ancestor else {
        return ResolvedContainer::Rectangle;
    };
    let (dl, dt, dr, db) = edge_distances(bounds, outer_rect);
    let pads = [(dl, dt), (dr, dt), (dr, db), (dl, db)];
    match outer {
        ResolvedContainer::Rectangle => ResolvedContainer::Rectangle,
        ResolvedContainer::Circle => ResolvedContainer::Circle,
        ResolvedContainer::Capsule { continuous } => ResolvedContainer::Capsule { continuous },
        ResolvedContainer::Ellipse => ResolvedContainer::Ellipse,
        ResolvedContainer::RoundedRect { radii, continuous } => {
            let mut out = [(0.0, 0.0); 4];
            for i in 0..4 {
                let (rx, ry) = radii[i];
                let (dx, dy) = pads[i];
                // Uniform inset reduces both axes by the corner inset; asymmetric padding keeps concentric curve.
                let pad = dx.max(dy);
                out[i] = ((rx - pad).max(0.0), (ry - pad).max(0.0));
            }
            ResolvedContainer::RoundedRect { radii: out, continuous }
        }
    }
}

fn resolve_concentric_radii(
    spec: ConcentricCorners,
    bounds: Rect,
    ancestor: Option<(Rect, ResolvedContainer)>,
) -> [f64; 4] {
    let (outer_r, pads) = match ancestor {
        Some((outer_rect, outer)) => {
            let (dl, dt, dr, db) = edge_distances(bounds, outer_rect);
            (container_corner_radii(outer_rect, outer), [dl.max(dt), dr.max(dt), dr.max(db), dl.max(db)])
        }
        None => ([0.0; 4], [0.0; 4]),
    };
    let mut r = [0.0f64; 4];
    for i in 0..4 {
        r[i] = match spec.corners[i] {
            CornerStyleSpec::Fixed(v) => v.max(0.0),
            CornerStyleSpec::Concentric(min_r) => (outer_r[i] - pads[i]).max(min_r.unwrap_or(0.0)).max(0.0),
        };
    }
    let unify = |r: &mut [f64; 4], idxs: &[usize]| {
        let mut val = 0.0f64;
        for &k in idxs {
            val = val.max(r[k]);
        }
        for &k in idxs {
            if matches!(spec.corners[k], CornerStyleSpec::Concentric(_)) {
                r[k] = val;
            }
        }
    };
    match spec.uniform {
        Some(UniformCorners::All) => unify(&mut r, &[0, 1, 2, 3]),
        Some(UniformCorners::Top) => unify(&mut r, &[0, 1]),
        Some(UniformCorners::Bottom) => unify(&mut r, &[2, 3]),
        Some(UniformCorners::Leading) => unify(&mut r, &[0, 3]),
        Some(UniformCorners::Trailing) => unify(&mut r, &[1, 2]),
        Some(UniformCorners::TopAndBottom) => {
            unify(&mut r, &[0, 1]);
            unify(&mut r, &[2, 3]);
        }
        Some(UniformCorners::LeadingAndTrailing) => {
            unify(&mut r, &[0, 3]);
            unify(&mut r, &[1, 2]);
        }
        None => {}
    }
    r
}

fn resolved_to_geom(rc: ResolvedContainer) -> VectorGeom {
    match rc {
        ResolvedContainer::Rectangle => VectorGeom::RoundedRect { radii: [(0.0, 0.0); 4], continuous: false },
        ResolvedContainer::Circle => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous: false },
        ResolvedContainer::Capsule { continuous } => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous },
        ResolvedContainer::Ellipse => VectorGeom::Ellipse,
        ResolvedContainer::RoundedRect { radii, continuous } => VectorGeom::RoundedRect { radii, continuous },
    }
}

/// Resolve `ContainerRelativeShape` and `ConcentricRectangle` nodes against their nearest enclosing `.containerShape`.
fn resolve_container_shapes(nodes: &mut [Node]) {
    let mut resolved: Vec<Option<(Rect, ResolvedContainer)>> = vec![None; nodes.len()];
    for i in 0..nodes.len() {
        let ancestor = {
            let mut p = nodes[i].parent;
            let mut found = None;
            while let Some(idx) = p {
                if let Some(rc) = resolved[idx] {
                    found = Some(rc);
                    break;
                }
                p = nodes[idx].parent;
            }
            found
        };
        if let Some(spec) = nodes[i].container_shape.as_deref().copied() {
            let rc = resolve_container_spec(spec, nodes[i].bounds, ancestor);
            resolved[i] = Some((nodes[i].bounds, rc));
        }
        let bounds = nodes[i].bounds;
        if let Some(vec) = nodes[i].vector.as_mut() {
            match vec.geom {
                VectorGeom::ContainerRelative => {
                    vec.geom = resolved_to_geom(resolve_relative_container(bounds, ancestor));
                }
                VectorGeom::Concentric(spec) => {
                    let r = resolve_concentric_radii(spec, bounds, ancestor);
                    vec.geom = VectorGeom::RoundedRect { radii: r.map(|v| (v, v)), continuous: true };
                }
                _ => {}
            }
        }
    }
}

/// How far `.offset` wrappers (node-less, one same-sized child) move `l`'s content.
fn moved_by(l: &Lay) -> (f64, f64) {
    match l.kids.as_slice() {
        [(dx, dy, k)] if l.node.is_none() && (k.size.w, k.size.h) == (l.size.w, l.size.h) => {
            let (x, y) = moved_by(k);
            (dx + x, dy + y)
        }
        _ => (0.0, 0.0),
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
        ViewKind::Grid { rows, .. } => rows.iter().any(|r| r.cells.iter().any(has_bordered)),
        ViewKind::LazyGrid { children, .. } | ViewKind::ViewThatFits { children, .. } => children.iter().any(has_bordered),
        ViewKind::Geometry { child, .. } => has_bordered(child),
        _ => false,
    }
}

fn is_list(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { style: ListStyle::InsetGrouped, .. })
}

fn is_scroller(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { .. } | ViewKind::Scroll { .. })
}

/// First scene node of `l`: itself, or the single child of a node-less wrapper (frame, padding).
fn first_node(l: &mut Lay) -> Option<&mut Node> {
    if let Some(n) = l.node.as_deref_mut() {
        return Some(n);
    }
    if l.kids.len() == 1 {
        first_node(&mut l.kids[0].2)
    } else {
        None
    }
}

/// Scene node of a direct child, walking one node-less wrapper (a frame).
fn direct_target(l: &mut Lay) -> Option<&mut Node> {
    if let Some(n) = l.node.as_deref_mut() {
        return Some(n);
    }
    if l.kids.len() == 1 {
        l.kids[0].2.node.as_deref_mut()
    } else {
        None
    }
}

fn content_margins(mods: &[Modifier]) -> Edges {
    let mut e = Edges::default();
    for m in mods {
        if let Modifier::ContentMargins(c) = m {
            e.top += c.top;
            e.leading += c.leading;
            e.bottom += c.bottom;
            e.trailing += c.trailing;
        }
    }
    e
}

fn cell_columns(cell: &ViewNode) -> usize {
    let mut span = 1usize;
    for m in &cell.modifiers {
        if let Modifier::GridCellColumns(n) = m {
            span = (*n).max(1) as usize;
        }
    }
    span
}

fn slot_width(cols: &[f64], col: usize, span: usize, gap: f64) -> f64 {
    let end = (col + span).min(cols.len());
    if col >= end {
        return 0.0;
    }
    cols[col..end].iter().sum::<f64>() + gap * (end - col - 1) as f64
}

struct PinHit {
    path: Vec<usize>,
    natural: f64,
    h: f64,
}

fn find_pins(l: &Lay, main: f64, path: &[usize], vertical: bool, pins: &mut Vec<PinHit>) {
    if !path.is_empty() && l.node.as_ref().is_some_and(|n| n.pinned.as_deref() == Some("header")) {
        let extent = if vertical { l.size.h } else { l.size.w };
        pins.push(PinHit { path: path.to_vec(), natural: main, h: extent });
    }
    for (i, (dx, dy, k)) in l.kids.iter().enumerate() {
        let mut child = path.to_vec();
        child.push(i);
        find_pins(k, main + if vertical { *dy } else { *dx }, &child, vertical, pins);
    }
}

/// Index in `path` whose parent lay has a scene node: the stack-child offset, including a
/// node-less frame sitting above the pinned node.
fn place_depth(root: &Lay, path: &[usize]) -> usize {
    let mut lay = root;
    let mut place = 0;
    for (depth, &i) in path.iter().enumerate() {
        if lay.node.is_some() {
            place = depth;
        }
        if depth + 1 == path.len() {
            break;
        }
        lay = &lay.kids[i].2;
    }
    place
}

fn add_main(l: &mut Lay, path: &[usize], at: usize, delta: f64, vertical: bool) {
    let Some(&i) = path.first() else { return };
    let Some(slot) = l.kids.get_mut(i) else { return };
    if at == 0 {
        if vertical { slot.1 += delta; } else { slot.0 += delta; }
    } else {
        add_main(&mut slot.2, &path[1..], at - 1, delta, vertical);
    }
}

/// Sticky section headers on the scroll's main axis. Content position becomes
/// `max(natural, offset)` but not past the next header.
fn pin_headers(root: &mut Lay, offset: f64, vertical: bool) {
    let mut pins = Vec::new();
    find_pins(root, 0.0, &[], vertical, &mut pins);
    pins.sort_by(|a, b| a.natural.total_cmp(&b.natural));
    let targets: Vec<f64> = pins
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut t = p.natural.max(offset);
            if let Some(next) = pins.get(i + 1) {
                t = t.min(next.natural - p.h);
            }
            t
        })
        .collect();
    for (p, target) in pins.iter().zip(targets) {
        let delta = target - p.natural;
        if delta.abs() < 1e-9 {
            continue;
        }
        let at = place_depth(root, &p.path);
        add_main(root, &p.path, at, delta, vertical);
    }
}

struct TrackSlot {
    w: f64,
    gap: f64,
    align: Option<Alignment>,
}

fn track_gap(t: &GridTrack) -> f64 {
    t.spacing.unwrap_or(DEFAULT_SPACING)
}

fn track_min(t: &GridTrack) -> f64 {
    t.min.filter(|v| v.is_finite()).unwrap_or(10.0)
}

fn track_max(t: &GridTrack) -> f64 {
    match t.max {
        Some(m) if m.is_finite() => m,
        _ => INF,
    }
}

fn expand_adaptive(t: &GridTrack, available: f64) -> Vec<TrackSlot> {
    let g = track_gap(t);
    let min = track_min(t);
    let denom = min + g;
    let n = if denom <= 0.0 { 1 } else { ((available + g) / denom).floor().max(1.0) as usize };
    let n = n.max(1);
    let w = (available - g * (n as f64 - 1.0)) / n as f64;
    let w = w.max(0.0);
    (0..n)
        .map(|i| TrackSlot { w, gap: if i + 1 == n { 0.0 } else { g }, align: t.alignment })
        .collect()
}

fn resolve_tracks(tracks: &[GridTrack], available: f64) -> Vec<TrackSlot> {
    if tracks.len() == 1 && tracks[0].size == TrackSize::Adaptive {
        return expand_adaptive(&tracks[0], available);
    }
    let mut slots: Vec<TrackSlot> = Vec::with_capacity(tracks.len());
    let mut flex_at = Vec::new();
    let mut fixed_sum = 0.0;
    for (i, t) in tracks.iter().enumerate() {
        let gap = if i + 1 == tracks.len() { 0.0 } else { track_gap(t) };
        match t.size {
            TrackSize::Fixed => {
                let w = t.value.unwrap_or(10.0);
                fixed_sum += w;
                slots.push(TrackSlot { w, gap, align: t.alignment });
            }
            TrackSize::Flexible | TrackSize::Adaptive => {
                flex_at.push(slots.len());
                slots.push(TrackSlot { w: 0.0, gap, align: t.alignment });
            }
        }
    }
    let gap_sum: f64 = slots.iter().map(|s| s.gap).sum();
    if !flex_at.is_empty() {
        let share = ((available - fixed_sum - gap_sum) / flex_at.len() as f64).max(0.0);
        for i in flex_at {
            let t = &tracks[i];
            let min = track_min(t);
            let max = track_max(t).max(min);
            slots[i].w = share.clamp(min, max);
        }
    }
    slots
}

fn fits_axis(size: f64, prop: Option<f64>) -> bool {
    prop.filter(|p| p.is_finite()).is_none_or(|p| size <= p + 0.5)
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

    fn glass_spec(&self, variant: GlassVariant, tint: Option<&onesim_protocol::Color>, interactive: bool) -> crate::glass::GlassSpec {
        crate::glass::GlassSpec { variant, tint: tint.map(|c| self.color(c)), interactive, blur: None, mix: None, shadow: None, flat: None }
    }

    /// System red: destructive menu items and tab badges.
    fn destructive(&self) -> Color {
        if self.dark { rgba(255, 66, 69, 1.0) } else { rgba(255, 56, 60, 1.0) }
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

    fn color(&self, c: &onesim_protocol::Color) -> Color {
        match c {
            onesim_protocol::Color::Semantic { semantic } => self.semantic(*semantic),
            onesim_protocol::Color::Rgba { r, g, b, a, dark } => {
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
        let mut spec = FontSpec::new(env.size, self.line_h(env.size), env.weight.unwrap_or(env.base_weight), env.align);
        if env.secondary {
            // Glyphs shrink. The line box stays the body's, so a secondary row keeps the body pitch.
            let k = crate::text::SECONDARY_TEXT_SCALE;
            spec.size *= k;
            spec.pt *= k;
            spec.tracking = crate::text::tracking_em(spec.pt, spec.weight);
        }
        spec.extra = env.extra;
        spec.design = env.design;
        spec.width = env.width;
        spec.mono = env.mono || env.design == 3;
        spec.digits = env.digits;
        spec.trunc = env.trunc;
        spec.baseline = env.baseline;
        spec.case_fold = env.case_fold;
        spec.tighten = env.tighten;
        spec.min_scale = env.min_scale;
        spec.lines = env.max_lines;
        spec.underline = env.underline;
        spec.italic = env.italic;
        spec.line_pat = env.under_pat;
        spec.under_color = env.under_color;
        spec.strike_pat = env.strike_pat;
        spec.strike_color = env.strike_color;
        spec.reserve = env.reserve_lines;
        spec
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
        let (text, spans) = fold_label(text, spans, spec.case_fold);
        let (spec, spans) = self.fit_text(&text, spec, wrap, spans);
        let (w, mut h) = self.fonts.measure(&text, &spec, wrap, &spans);
        // A positive baselineOffset lengthens the line downward; the glyphs stay at the top.
        // A negative one lengthens it and the draw shifts the glyphs down into that space.
        let (rise, sink) = crate::text::baseline_extent(&spec, &spans);
        h += f64::from(rise - sink);
        if let Some(n) = spec.reserve.filter(|n| *n > 1) {
            h = h.max(spec.line_height as f64 + (n - 1) as f64 * spec.pitch as f64);
        }
        // SwiftUI rounds text sizes up to the device pixel.
        let w = (w * self.scale).ceil() / self.scale;
        let mut n = Node::new(id, "text");
        n.label = Some(text.clone());
        n.text = Some(TextRun { text, font: spec, color, strike: false, offset_x: 0.0, spans });
        n.role = Role::Text;
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

    fn push_anim(&mut self, spec: &AnimationSpec) -> u32 {
        let idx = self.anims.len() as u32;
        self.anims.push(spec.clone());
        idx
    }

    fn stamp_node(&self, n: &mut Node, env: Env) {
        let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
        if anim.is_none() && env.content_transition == ContentTransitionKind::Identity {
            return;
        }
        let slot = n.anim.get_or_insert_with(|| Box::new(crate::scene::NodeAnim::default()));
        if slot.animation.is_none() {
            slot.animation = anim;
        }
        if slot.content_transition == ContentTransitionKind::Identity {
            slot.content_transition = env.content_transition;
        }
    }

    /// Applies modifiers outermost-first by recursion (array order is innermost first).
    fn mods(&mut self, v: &ViewNode, mods: &[Modifier], p: Prop, env: Env) -> Lay {
        let Some((last, rest)) = mods.split_last() else {
            let mut out = self.base(v, p, env);
            if let Some(n) = out.node.as_deref_mut() {
                self.stamp_node(n, env);
            }
            return out;
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
                if let (Some(t), onesim_protocol::Color::Semantic { semantic }) = (env.tint, c) {
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
                layer(format!("{}#clip{}", v.id, rest.len()), "clip", inner, |n| n.clip_radius = Some(*r))
            }
            Modifier::Opacity(o) => {
                let inner = self.mods(v, rest, p, env);
                let mut out = layer(format!("{}#opacity{}", v.id, rest.len()), "opacity", inner, |n| n.opacity = *o as f32);
                if let Some(n) = out.node.as_deref_mut() {
                    self.stamp_node(n, env);
                }
                out
            }
            Modifier::Shadow(s) => {
                let inner = self.mods(v, rest, p, env);
                let (radius, shadow) = (lay_radius(&inner), Shadow { color: self.color(&s.color), radius: s.radius, dx: s.x, dy: s.y });
                layer(format!("{}#shadow{}", v.id, rest.len()), "shadow", inner, |n| {
                    n.radius = radius;
                    n.shadow = Some(shadow);
                })
            }
            Modifier::Font(f) => {
                let (size, weight, design) = self.font_size_weight(f);
                let mut env = Env { size: size as f32, base_weight: weight.value(), ..env };
                if let Some(d) = design {
                    env.design = d;
                    env.mono = d == 3;
                }
                self.mods(v, rest, p, env)
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
                let radius = match (g.shape.shape, g.shape.corner_radius) {
                    (GlassShapeKind::Rect, None) => 0.0,
                    (GlassShapeKind::Rect | GlassShapeKind::RoundedRectangle, r) => r.unwrap_or(0.0),
                    _ => PILL,
                };
                let spec = self.glass_spec(g.variant, g.tint.as_ref(), g.interactive);
                layer(format!("{}#glass{}", v.id, rest.len()), "glass", inner, |n| {
                    n.radius = radius;
                    n.glass = Some(spec);
                })
            }
            // Layout keeps the original place; drawing and hit testing use the moved one.
            Modifier::Offset(o) => {
                let inner = self.mods(v, rest, p, env);
                Lay::new(inner.size, None, vec![(o.x, o.y, inner)])
            }
            Modifier::Gesture(g) => {
                let inner = self.mods(v, rest, p, env);
                gesture_layer(format!("{}#gesture{}", v.id, rest.len()), g, inner)
            }
            Modifier::ContainerShape(spec) => {
                let inner = self.mods(v, rest, p, env);
                container_shape_layer(format!("{}#cshape{}", v.id, rest.len()), *spec, inner)
            }
            Modifier::Identity(id) => {
                let mut inner = self.mods(v, rest, p, env);
                if let Some(n) = first_node(&mut inner) {
                    n.anchor = Some(id.clone());
                }
                inner
            }
            Modifier::Pin(pin) => {
                let mut inner = self.mods(v, rest, p, env);
                if let Some(n) = first_node(&mut inner) {
                    n.pinned = Some(pin.clone());
                }
                inner
            }
            Modifier::ScrollTargetLayout(enabled) => {
                let mut inner = self.mods(v, rest, p, env);
                if *enabled {
                    for (_, _, kid) in &mut inner.kids {
                        if let Some(n) = direct_target(kid) {
                            n.scroll_target = true;
                        }
                    }
                }
                inner
            }
            // Read by the owning container (grid columns, scroll margins and snap).
            Modifier::GridCellColumns(_)
            | Modifier::ContentMargins(_)
            | Modifier::ScrollIndicators(_)
            | Modifier::ScrollTargetBehavior(_)
            | Modifier::ScrollPosition => self.mods(v, rest, p, env),
            Modifier::Animation(opt) => {
                let anim = opt.as_deref().map(|s| self.push_anim(s));
                self.mods(v, rest, p, Env { anim, ..env })
            }
            Modifier::Transition(t) => {
                let inner = self.mods(v, rest, p, env);
                let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
                anim_layer(format!("{}#transition{}", v.id, rest.len()), "transition", inner, anim, Some((**t).clone()), None)
            }
            Modifier::MatchedGeometry(m) => {
                let inner = self.mods(v, rest, p, env);
                let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
                anim_layer(format!("{}#match{}", v.id, rest.len()), "matched", inner, anim, None, Some((**m).clone()))
            }
            Modifier::ContentTransition(ct) => self.mods(v, rest, p, Env { content_transition: *ct, ..env }),
            Modifier::Kerning(k) => self.mods(v, rest, p, Env { extra: env.extra + *k as f32, ..env }),
            Modifier::Tracking(k) => self.mods(v, rest, p, Env { extra: env.extra + *k as f32, ..env }),
            Modifier::BaselineOffset(k) => self.mods(v, rest, p, Env { baseline: *k as f32, ..env }),
            Modifier::FontDesign(d) => {
                let design = d.map(design_code).unwrap_or(0);
                self.mods(v, rest, p, Env { design, mono: design == 3 || (d.is_none() && env.mono), ..env })
            }
            Modifier::FontWidth(w) => self.mods(v, rest, p, Env { width: w.map(width_code).unwrap_or(0), ..env }),
            Modifier::Monospaced(on) => self.mods(v, rest, p, Env { mono: *on, ..env }),
            Modifier::MonospacedDigit(on) => self.mods(v, rest, p, Env { digits: *on, ..env }),
            Modifier::TextScale(s) => self.mods(v, rest, p, Env { secondary: *s == onesim_protocol::TextScale::Secondary, ..env }),
            Modifier::TruncationMode(m) => self.mods(v, rest, p, Env { trunc: trunc_code(*m), ..env }),
            Modifier::LineLimit(lim) => {
                let (max_lines, reserve_lines) = lim.map(|l| {
                    let max = l.max.map(|n| n as usize);
                    let reserve = if l.reserves { l.max.or(l.min).map(|n| n as usize) } else { l.min.map(|n| n as usize) };
                    (max, reserve)
                }).unwrap_or((None, None));
                self.mods(v, rest, p, Env { max_lines, reserve_lines, ..env })
            }
            Modifier::Underline(s) => self.mods(v, rest, p, Env {
                underline: s.active,
                under_pat: pattern_code(s.pattern),
                under_color: s.color.as_ref().map(|c| self.color(c)),
                ..env
            }),
            Modifier::Italic(on) => self.mods(v, rest, p, Env { italic: *on, ..env }),
            Modifier::TextCase(c) => self.mods(v, rest, p, Env { case_fold: c.map(case_code).unwrap_or(0), ..env }),
            Modifier::AllowsTightening(on) => self.mods(v, rest, p, Env { tighten: *on, ..env }),
            Modifier::MinimumScaleFactor(k) => self.mods(v, rest, p, Env { min_scale: (*k as f32).clamp(0.01, 1.0), ..env }),
            Modifier::SymbolRenderingMode(m) => self.mods(v, rest, p, Env { sym_mode: m.map(symbol_mode_code).unwrap_or(0), ..env }),
            Modifier::SymbolVariableValueMode(m) => self.mods(v, rest, p, Env { sym_var: m.map(|v| if v == onesim_protocol::SymbolVariable::Draw { 2 } else { 1 }).unwrap_or(0), ..env }),
            Modifier::SymbolColorRenderingMode(m) => self.mods(v, rest, p, Env { sym_grad: m == &Some(onesim_protocol::SymbolColorMode::Gradient), ..env }),
            Modifier::AllowedDynamicRange(_) => self.mods(v, rest, p, env),
            Modifier::StrikethroughStyle(s) => self.mods(v, rest, p, Env {
                strike: s.active,
                strike_pat: pattern_code(s.pattern),
                strike_color: s.color.as_ref().map(|c| self.color(c)),
                ..env
            }),
        }
    }

    /// Point size (at the current Dynamic Type size), weight and design of a `Font`.
    fn font_size_weight(&self, f: &Font) -> (f64, onesim_protocol::Weight, Option<u8>) {
        match f {
            Font::Style { style, weight } => (style.size_at(self.dts), weight.unwrap_or(style.spec().1), None),
            Font::System { size, weight, design } => (*size, *weight, design.map(design_code)),
        }
    }

    #[inline(never)]
    fn shape_view(&self, v: &ViewNode, p: Prop, fg: Color) -> Lay {
        let ViewKind::Shape { shape, extra } = &v.kind else {
            unreachable!()
        };
        let onesim_protocol::ShapeExtra {
            corner_radius,
            corner_size,
            corner_radii,
            corner_style,
            concentric,
            path,
            path_size,
            custom,
            size_hint,
            mirror_rtl,
            fill,
            fill_style,
            stroke,
        } = &**extra;
        let square = |p: Prop| {
            let side = match (p.w, p.h) {
                (Some(w), Some(h)) => w.min(h),
                (Some(w), None) => w,
                (None, Some(h)) => h,
                _ => 10.0,
            };
            Size { w: side, h: side }
        };
        let size = match size_hint {
            Some(ShapeSizeHint::Square) => square(p),
            Some(ShapeSizeHint::Fixed([w, h])) => Size { w: *w, h: *h },
            Some(ShapeSizeHint::Aspect(ratio)) if *ratio > 0.0 => match (p.w, p.h) {
                (Some(w), Some(h)) if w.is_finite() && h.is_finite() => {
                    if w / h > *ratio {
                        Size { w: h * ratio, h }
                    } else {
                        Size { w, h: w / ratio }
                    }
                }
                (Some(w), _) if w.is_finite() => Size { w, h: w / ratio },
                (_, Some(h)) if h.is_finite() => Size { w: h * ratio, h },
                _ => Size { w: 10.0 * ratio, h: 10.0 },
            },
            Some(ShapeSizeHint::Ideal([iw, ih])) => Size {
                w: p.w.filter(|x| x.is_finite()).unwrap_or(*iw),
                h: p.h.filter(|x| x.is_finite()).unwrap_or(*ih),
            },
            _ if *shape == ShapeKind::Circle => square(p),
            _ => greedy(p),
        };
        let mut n = Node::new(&v.id, "shape");
        let fs = fill_style.unwrap_or_default();
        let simple_fill = !fs.eo_fill && fs.antialiased;
        let simple_stroke = stroke.as_ref().is_none_or(|st| {
            st.line_cap == LineCap::Butt
                && st.line_join == LineJoin::Miter
                && (st.miter_limit - 10.0).abs() < 1e-6
                && st.dash.is_empty()
                && st.antialiased
                && st.inset == 0.0
        });
        let continuous = *corner_style == Some(RoundedCornerStyle::Continuous);
        let stroke_color = |st: &onesim_protocol::Stroke| st.color.as_ref().map_or(fg, |c| self.color(c));

        // Fast path for plain rectangular/circular/pill shapes with default fill & stroke.
        if simple_fill && simple_stroke && !*custom {
            let fast_radius = match shape {
                ShapeKind::Rectangle => Some(0.0),
                ShapeKind::Circle => Some(PILL),
                ShapeKind::Capsule if !continuous => Some(PILL),
                ShapeKind::RoundedRectangle if corner_size.is_none() && !continuous => Some(corner_radius.unwrap_or(0.0)),
                _ => None,
            };
            if let Some(radius) = fast_radius {
                n.radius = radius;
                n.fill = fill.as_ref().map(|c| self.color(c));
                n.stroke = stroke.as_ref().map(|st| (stroke_color(st), st.line_width));
                if n.fill.is_none() && n.stroke.is_none() {
                    n.fill = Some(fg);
                }
                return Lay::new(size, Some(n), vec![]);
            }
        }

        let geom = match shape {
            ShapeKind::Rectangle => VectorGeom::RoundedRect { radii: [(0.0, 0.0); 4], continuous: false },
            ShapeKind::Circle => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous: false },
            ShapeKind::Capsule => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous },
            ShapeKind::RoundedRectangle => {
                let (rx, ry) = corner_size.map(|[w, h]| (w, h)).unwrap_or_else(|| {
                    let r = corner_radius.unwrap_or(0.0);
                    (r, r)
                });
                VectorGeom::RoundedRect { radii: [(rx, ry); 4], continuous }
            }
            ShapeKind::Ellipse => VectorGeom::Ellipse,
            ShapeKind::UnevenRoundedRectangle => {
                let r = corner_radii.unwrap_or([0.0; 4]);
                VectorGeom::RoundedRect { radii: r.map(|v| (v, v)), continuous }
            }
            ShapeKind::ContainerRelative => VectorGeom::ContainerRelative,
            ShapeKind::ConcentricRectangle => VectorGeom::Concentric(concentric.unwrap_or(ConcentricCorners {
                corners: [CornerStyleSpec::Concentric(None); 4],
                uniform: None,
            })),
            ShapeKind::Path => VectorGeom::Path {
                ops: path.clone().unwrap_or_default(),
                mirror_x: *mirror_rtl,
            },
        };
        let fill_col = fill.as_ref().map(|c| self.color(c)).or_else(|| stroke.is_none().then_some(fg));
        let vec_fill = fill_col.map(|color| VectorFill { color, eo_fill: fs.eo_fill, antialiased: fs.antialiased });
        let vec_stroke = stroke.as_ref().map(|st| VectorStroke {
            color: stroke_color(st),
            line_width: st.line_width,
            line_cap: st.line_cap,
            line_join: st.line_join,
            miter_limit: st.miter_limit,
            dash: st.dash.clone(),
            dash_phase: st.dash_phase,
            antialiased: st.antialiased,
            inset: st.inset,
        });
        n.vector = Some(Box::new(VectorShape { geom, fill: vec_fill, stroke: vec_stroke }));
        if *custom {
            n.custom_shape = Some(match *path_size {
                Some(sz) => CustomShapeEval::Evaluated(sz),
                None => CustomShapeEval::Pending,
            });
        }
        Lay::new(size, Some(n), vec![])
    }

    fn base(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let child_env = Env { axis: None, ..env };
        match &v.kind {
            ViewKind::Text { text, runs } => {
                let wrap = p.w.filter(|w| w.is_finite()).map(|w| w.max(1.0) as f32);
                let mut spec = self.spec(&env);
                let spans: Vec<Span> = runs.iter().map(|r| self.span_of(r, &env, fg)).collect();
                // Line metrics come from the largest run. Keep the paragraph's truncation and tracking.
                if let Some(big) = spans.iter().max_by(|a, b| a.pt.total_cmp(&b.pt)) {
                    let keep = spec;
                    spec = FontSpec::new(big.pt, self.line_h(big.pt), big.weight, env.align);
                    spec.extra = keep.extra;
                    spec.trunc = keep.trunc;
                    spec.baseline = keep.baseline;
                    spec.case_fold = keep.case_fold;
                    spec.tighten = keep.tighten;
                    spec.min_scale = keep.min_scale;
                    spec.lines = keep.lines;
                    spec.underline = keep.underline;
                    spec.italic = keep.italic;
                    spec.line_pat = keep.line_pat;
                    spec.under_color = keep.under_color;
                    spec.strike_pat = keep.strike_pat;
                    spec.strike_color = keep.strike_color;
                    spec.reserve = keep.reserve;
                    spec.mono = big.mono;
                    spec.digits = big.digits;
                    spec.design = big.design;
                    spec.width = big.width;
                }
                let mut l = self.text_lay_runs(&v.id, text, spec, fg, wrap, spans);
                if let Some(t) = l.node.as_mut().and_then(|n| n.text.as_mut()) {
                    t.strike = env.strike;
                }
                l
            }
            ViewKind::VStack { spacing, alignment, children, .. } => {
                self.stack(v, "vstack", children, *spacing, *alignment, true, p, child_env)
            }
            ViewKind::HStack { spacing, alignment, children, .. } => {
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
            ViewKind::Button { action, label, style, disabled, selected, .. } => match selected {
                Some(on) => self.toggle_button(v, *action, label, *on, *disabled, p, env),
                None => self.button(v, *action, label, *style, *disabled, p, env),
            },
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
            ViewKind::Shape { .. } => self.shape_view(v, p, fg),
            ViewKind::Divider => {
                let hl = self.hairline();
                let len = |x: Option<f64>| x.filter(|x| x.is_finite()).unwrap_or(0.0);
                let size = if env.axis == Some(false) { Size { w: hl, h: len(p.h) } } else { Size { w: len(p.w), h: hl } };
                let mut n = Node::new(&v.id, "divider");
                // UIKit's translucent separator (measured 198 on white).
                n.fill = Some(if self.dark { rgba(84, 84, 88, 0.6) } else { rgba(60, 60, 67, 0.29) });
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Rn { component, .. } if component == onesim_protocol::EXPO_UI_RN_HOST => {
                let limit = |axis: Option<f64>| axis.filter(|n| n.is_finite());
                let embedded = super::rn::embedded(v, (limit(p.w), limit(p.h)), self.params, self.fonts);
                let mut laid = Lay::new(Size { w: embedded.size.0, h: embedded.size.1 }, None, vec![]);
                laid.embedded = Some(embedded);
                laid
            }
            ViewKind::Empty | ViewKind::Rn { .. } | ViewKind::Compose { .. } => Lay::new(Size::default(), Some(Node::new(&v.id, "empty")), vec![]),
            // Only the root's presentation layer is drawn (see `layout`); nested, it is transparent.
            ViewKind::Presentation { content, .. } => self.view(content, p, env),
            ViewKind::Scroll { axis, child, .. } => self.scroll(v, *axis == Axis::Vertical, child, p, env),
            ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows } => {
                self.grid(v, *alignment, *horizontal_spacing, *vertical_spacing, rows, p, child_env)
            }
            ViewKind::LazyGrid { axis, alignment, spacing, tracks, children } => {
                self.lazy_grid(v, *axis, *alignment, *spacing, tracks, children, p, child_env)
            }
            ViewKind::Geometry { proposal, child } => self.geometry(v, *proposal, child, p, child_env),
            ViewKind::ViewThatFits { axes, children } => self.view_that_fits(v, *axes, children, p, child_env),
            ViewKind::List { style, sections } => self.list(v, *style, sections, p, env),
            ViewKind::TabView { selected, tabs, content } => self.tab_view(v, *selected, tabs, content, p, env),
            ViewKind::PageTabView { selected, pages, index_display, index_background } => {
                self.page_tab_view(v, *selected, pages, (*index_display, *index_background), p, child_env)
            }
            ViewKind::Navigation { title, display_mode, back, content } => {
                self.navigation(v, title.as_deref(), *display_mode, back.as_ref(), content, p, env)
            }
            ViewKind::Toggle { is_on, action, label, disabled } => self.toggle(v, *is_on, *action, label, *disabled, p, env),
            ViewKind::Stepper { label, decrement, increment } => self.stepper(v, label, *decrement, *increment, p, env),
            ViewKind::TextField { placeholder, text, field, secure, disabled } => {
                let mut l = self.text_field(v, placeholder, if *secure { "" } else { text }, *field, p, env);
                if *disabled {
                    if let Some(n) = l.node.as_mut() {
                        n.field = None;
                        n.disabled = true;
                    }
                }
                if !*secure || text.is_empty() {
                    return l;
                }
                // SecureField: one bullet (a dot) per character.
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
                match style {
                    PickerStyle::Segmented => self.segmented(v, options, *selected, *disabled, p),
                    PickerStyle::Wheel => self.wheel(v, label, options, *selected, *disabled, p),
                    PickerStyle::Menu => self.menu_picker(v, label, options, *selected, *open, *disabled, p, child_env),
                }
            }
            ViewKind::Slider { value, min, max, field, disabled, .. } => self.slider(v, *value, (*min, *max), *field, *disabled, p),
            ViewKind::Progress { value, label, current_value_label } => {
                self.progress(v, *value, label.as_deref(), current_value_label.as_deref(), p, child_env)
            }
            ViewKind::Gauge { style, value, label, current_value_label, minimum_value_label, maximum_value_label } => {
                let labels = GaugeLabels {
                    label: label.as_deref(),
                    current: current_value_label.as_deref(),
                    min: minimum_value_label.as_deref(),
                    max: maximum_value_label.as_deref(),
                };
                self.gauge(v, *style, *value, labels, p, child_env)
            }
            ViewKind::Image {
                system_name, resizable, content_mode, scale, rendering_mode, symbol_rendering_mode, interpolation, antialiased,
                symbol_color_rendering_mode, symbol_variable_value_mode, bitmap, ..
            } => {
                let mut env = env;
                if let Some(m) = symbol_rendering_mode {
                    env.sym_mode = symbol_mode_code(*m);
                }
                if let Some(m) = rendering_mode {
                    match m {
                        onesim_protocol::TemplateMode::Template => {
                            env.sym_mode = 0;
                            env.sym_grad = false;
                            env.sym_original = false;
                        }
                        onesim_protocol::TemplateMode::Original => env.sym_original = true,
                    }
                }
                if let Some(m) = interpolation {
                    env.img_filter = match m {
                        onesim_protocol::Interpolation::None => 0,
                        onesim_protocol::Interpolation::Low => 1,
                        onesim_protocol::Interpolation::Medium | onesim_protocol::Interpolation::High => 2,
                    };
                }
                if *antialiased == Some(false) {
                    env.img_filter = 0;
                }
                if let Some(m) = symbol_color_rendering_mode {
                    env.sym_grad = *m == onesim_protocol::SymbolColorMode::Gradient;
                }
                if let Some(m) = symbol_variable_value_mode {
                    env.sym_var = if *m == onesim_protocol::SymbolVariable::Draw { 2 } else { 1 };
                }
                if let Some(bm) = bitmap {
                    self.bitmap_image(v, bm, *resizable, *content_mode, env.img_filter, p)
                } else {
                    self.image(v, system_name, *resizable, *content_mode, scale.unwrap_or(env.img), p, env, fg)
                }
            }
            ViewKind::Label { title, icon } => self.label(v, title, icon, p, child_env),
        }
    }

    fn span_of(&self, r: &onesim_protocol::Run, env: &Env, fg: Color) -> Span {
        let (mut pt, weight, design) = match &r.font {
            Some(f) => {
                let (size, w, d) = self.font_size_weight(f);
                (size as f32, r.font_weight.map(|w| w.value()).unwrap_or(w.value()), d.unwrap_or(env.design))
            }
            None => (env.size, r.font_weight.map(|w| w.value()).unwrap_or(env.weight.unwrap_or(env.base_weight)), env.design),
        };
        let secondary = match r.text_scale {
            Some(onesim_protocol::TextScale::Secondary) => true,
            Some(onesim_protocol::TextScale::Default) => false,
            None => env.secondary,
        };
        if secondary {
            pt *= crate::text::SECONDARY_TEXT_SCALE;
        }
        let design = r.font_design.map(design_code).unwrap_or(design);
        let width = r.font_width.map(width_code).unwrap_or(env.width);
        // A run's kerning and tracking add to the parent's. They do not replace it.
        let extra = env.extra + r.kerning.unwrap_or(0.0) as f32 + r.tracking.unwrap_or(0.0) as f32;
        let color = r.foreground_color.as_ref().map(|c| self.color(c)).unwrap_or(fg);
        Span {
            italic: r.italic || env.italic,
            underline: r.underline || env.underline,
            strike: r.strikethrough || env.strike,
            mono: r.monospaced || env.mono || design == 3,
            extra,
            design,
            width,
            digits: r.monospaced_digit || env.digits,
            baseline: r.baseline_offset.map(|b| b as f32).unwrap_or(env.baseline),
            line_pat: r.underline_pattern.map(pattern_code).unwrap_or(env.under_pat),
            under_color: r.underline_color.as_ref().map(|c| self.color(c)).or(env.under_color),
            strike_pat: r.strikethrough_pattern.map(pattern_code).unwrap_or(env.strike_pat),
            strike_color: r.strikethrough_color.as_ref().map(|c| self.color(c)).or(env.strike_color),
            ..Span::basic(&r.text, pt, weight, color)
        }
    }

    /// Tighten tracking, then scale down to `min_scale`, when the text is wider than the wrap or uses more lines than the cap.
    fn fit_text(&mut self, text: &str, spec: FontSpec, wrap: Option<f32>, spans: Vec<Span>) -> (FontSpec, Vec<Span>) {
        let Some(limit) = wrap else { return (spec, spans) };
        let overflows = |fonts: &mut Fonts, spec: &FontSpec, spans: &[Span]| {
            let probe = FontSpec { lines: None, ..*spec };
            let (w, h) = fonts.measure(text, &probe, Some(limit), spans);
            if w > limit as f64 + 0.51 {
                return true;
            }
            let Some(max) = spec.lines else { return false };
            let cap = f64::from(spec.line_height) + (max.saturating_sub(1) as f64) * f64::from(spec.pitch);
            h > cap + 0.51
        };
        if !overflows(self.fonts, &spec, &spans) {
            return (spec, spans);
        }
        let (mut spec, mut spans) = (spec, spans);
        if spec.tighten {
            let max_pull = 0.05 * spec.pt;
            let mut lo = 0.0f32;
            let mut hi = max_pull;
            for _ in 0..7 {
                let mid = (lo + hi) / 2.0;
                let (ts, tsp) = pull_tracking(&spec, &spans, mid);
                if overflows(self.fonts, &ts, &tsp) { lo = mid; } else { hi = mid; }
            }
            (spec, spans) = pull_tracking(&spec, &spans, hi);
        }
        if spec.min_scale < 0.999 && overflows(self.fonts, &spec, &spans) {
            let floor = spec.min_scale;
            let mut lo = floor;
            let mut hi = 1.0f32;
            for _ in 0..7 {
                let mid = (lo + hi) / 2.0;
                let (ts, tsp) = self.scale_type(&spec, &spans, mid);
                if overflows(self.fonts, &ts, &tsp) { hi = mid; } else { lo = mid; }
            }
            let (trial, trial_spans) = self.scale_type(&spec, &spans, lo);
            let k = if overflows(self.fonts, &trial, &trial_spans) { floor } else { lo };
            (spec, spans) = self.scale_type(&spec, &spans, k);
        }
        (spec, spans)
    }

    fn scale_type(&self, spec: &FontSpec, spans: &[Span], k: f32) -> (FontSpec, Vec<Span>) {
        let mut s = *spec;
        let pt = (spec.pt * k).max(1.0);
        let ratio = if spec.pt > 0.0 { spec.size / spec.pt } else { 1.0 };
        s.pt = pt;
        s.size = pt * ratio;
        s.line_height = self.line_h(pt);
        s.pitch = if pt > 34.0 { pt * 1.12 + 2.64 } else { s.line_height.max(pt * 22.0 / 17.0) };
        s.extra *= k;
        s.baseline *= k;
        s.tracking = crate::text::tracking_em(pt, s.weight);
        let spans = spans.iter().map(|sp| Span { pt: sp.pt * k, extra: sp.extra * k, baseline: sp.baseline * k, ..sp.clone() }).collect();
        (s, spans)
    }

    fn bitmap_image(&mut self, v: &ViewNode, bm: &onesim_protocol::Bitmap, resizable: bool, mode: Option<ContentMode>, filter: u8, p: Prop) -> Lay {
        let pixmap = crate::image::load(&bm.path);
        let scale = if bm.scale > 0.0 { bm.scale } else { 1.0 };
        let (pw, ph) = pixmap.as_ref().map(|m| (m.width() as f64 / scale, m.height() as f64 / scale)).unwrap_or((1.0, 1.0));
        let (nw, nh) = (if bm.width > 0.0 { bm.width } else { pw }, if bm.height > 0.0 { bm.height } else { ph });
        let ratio = if nh > 0.0 { nw / nh } else { 1.0 };
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (pw, ph) = (fin(p.w), fin(p.h));
        let (size, dest) = if !resizable {
            (Size { w: nw, h: nh }, Rect { x: 0.0, y: 0.0, w: nw, h: nh })
        } else {
            match mode {
                None => {
                    let s = Size { w: pw.unwrap_or(nw), h: ph.unwrap_or(nh) };
                    (s, Rect { x: 0.0, y: 0.0, w: s.w, h: s.h })
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
                        ContentMode::Fit => (Size { w: fit.0, h: fit.1 }, Rect { x: 0.0, y: 0.0, w: fit.0, h: fit.1 }),
                        ContentMode::Fill => {
                            let dx = (bw - fill.0) / 2.0;
                            let dy = (bh - fill.1) / 2.0;
                            (Size { w: bw, h: bh }, Rect { x: dx, y: dy, w: fill.0, h: fill.1 })
                        }
                    }
                }
            }
        };
        let mut n = Node::new(&v.id, "image");
        n.role = Role::Image;
        if let Some(pixmap) = pixmap {
            n.image = Some(crate::scene::ImageRun { pixmap, dest, rotation: 0.0, filter });
        }
        Lay::new(size, Some(n), vec![])
    }

    /// SF Symbol. Non-resizable images are glyph-sized from the font; resizable ones stretch to
    /// the proposal (or fit / fill it keeping the symbol's aspect ratio).
    #[allow(clippy::too_many_arguments)]
    fn image(&mut self, v: &ViewNode, name: &str, resizable: bool, mode: Option<ContentMode>, scale: ImageScale, p: Prop, env: Env, fg: Color) -> Lay {
        let sym_mode = if env.sym_original && env.sym_mode == 0 { 3 } else { env.sym_mode };
        let spec = SymbolSpec {
            name: name.to_string(),
            point_size: env.size as f64,
            weight: env.weight.unwrap_or(env.base_weight),
            scale,
            mode: sym_mode,
            variable: env.sym_var,
            gradient: env.sym_grad,
        };
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
                n.glass = Some(self.glass_spec(GlassVariant::Regular, prominent.then_some(onesim_protocol::Color::Semantic { semantic: Semantic::Accent }).as_ref(), true));
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::NavigationLink => {
                let fg = if disabled { dim } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
                // 7x12 at body size, growing with Dynamic Type (21.7x37.3, 8.6pt inset at AX5; measured).
                let k = (onesim_protocol::TextStyle::Body.size_at(self.dts) / 17.0).max(1.0);
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
        let spec = SymbolSpec { name: name.into(), point_size: pt, weight, scale: ImageScale::Medium, ..SymbolSpec::default() };
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
    fn progress(&mut self, v: &ViewNode, value: Option<f64>, label: Option<&ViewNode>, current: Option<&ViewNode>, p: Prop, env: Env) -> Lay {
        let fin = p.w.filter(|w| w.is_finite());
        let ll = label.map(|l| self.view(l, Prop { w: fin, h: None }, env));
        let gap = if value.is_some() { PROGRESS_BAR_LABEL_GAP } else { PROGRESS_LABEL_GAP };
        let lh = ll.as_ref().map_or(0.0, |l| l.size.h + gap);
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
            let mut h = lh + PROGRESS_H;
            if let Some(c) = current {
                // currentValueLabel: caption, secondary, under the bar.
                let cenv = Env { size: PROGRESS_VALUE_PT, weight: None, fg: Some(self.semantic(Semantic::Secondary)), ..env };
                let cl = self.view(c, Prop { w: fin, h: None }, cenv);
                h += PROGRESS_VALUE_GAP;
                let ch = cl.size.h;
                kids.push((0.0, h, cl));
                h += ch;
            }
            Size { w, h }
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
    /// edge, one row per option, a checkmark before the selected one. A `Menu`'s card (`centered`) is
    /// centred on its button, items may carry a leading icon, red when destructive, and a divider opens a gap
    /// with a separator. A tap outside closes it.
    fn menu(&mut self, items: &[PickerOption], anchor: Option<Rect>, dismiss: Option<u64>, centered: bool, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let primary = self.semantic(Semantic::Primary);
        let red = self.destructive();
        let dim = self.semantic(Semantic::Tertiary);
        let icons = items.iter().any(|o| o.system_image.is_some());
        let text_x = if icons { MENU_ICON_TEXT_X } else { MENU_TEXT_X };
        let spec = FontSpec::new(17.0, self.line_h(17.0), 400, TextAlign::Leading);
        let texts: Vec<Lay> = items.iter().enumerate().map(|(i, o)| {
            let fg = if o.disabled { dim } else if o.destructive { red } else { primary };
            let mut t = self.text_lay(&format!("menu#{i}#text"), &o.text, spec, fg, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            t
        }).collect();
        let cw = (texts.iter().map(|t| t.size.w).fold(0.0, f64::max) + text_x + MENU_PAD_R).clamp(MENU_MIN_W, w - 2.0 * POPOVER_MARGIN);
        let gaps = items.iter().skip(1).filter(|o| o.divider).count() as f64;
        let ch = items.len() as f64 * MENU_ROW_H + gaps * MENU_DIVIDER_GAP + 2.0 * MENU_PAD_V;
        let mut kids = vec![];
        let mut y = MENU_PAD_V;
        for (i, (o, t)) in items.iter().zip(texts).enumerate() {
            if o.divider && i > 0 {
                let mut sep = Node::new(format!("menu#sep{i}"), "separator");
                sep.fill = Some(self.semantic(Semantic::Separator));
                sep.inspect = false;
                kids.push((MENU_SEP_INSET, y + (MENU_DIVIDER_GAP - 1.0) / 2.0, Lay::new(Size { w: cw - 2.0 * MENU_SEP_INSET, h: 1.0 }, Some(sep), vec![])));
                y += MENU_DIVIDER_GAP;
            }
            let fg = if o.disabled { dim } else if o.destructive { red } else { primary };
            let mut b = Node::new(format!("menu#{i}"), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!o.disabled).then_some(o.action);
            b.disabled = o.disabled;
            b.value = o.checked.then(|| "checked".into());
            let th = t.size.h;
            let mut bk = vec![(text_x, (MENU_ROW_H - th) / 2.0, t)];
            if let Some(name) = &o.system_image {
                let c = self.symbol_lay(format!("menu#{i}#icon"), name, 17.0, 400, fg);
                let (iw, ih) = (c.size.w, c.size.h);
                bk.push((MENU_ICON_X - iw / 2.0, (MENU_ROW_H - ih) / 2.0, c));
            }
            if o.checked {
                let c = self.symbol_lay(format!("menu#{i}#check"), "checkmark", MENU_CHECK.0, MENU_CHECK.1, primary);
                let (cw2, chh) = (c.size.w, c.size.h);
                bk.push((MENU_CHECK_X - cw2 / 2.0, (MENU_ROW_H - chh) / 2.0, c));
            }
            kids.push((0.0, y, Lay::new(Size { w: cw, h: MENU_ROW_H }, Some(b), bk)));
            y += MENU_ROW_H;
        }
        let mut card = Node::new("menu", "menu");
        card.radius = MENU_RADIUS;
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { MENU_GLASS.1 } else { MENU_GLASS.0 }), shadow: Some(POPOVER_SHADOW), ..self.glass_spec(GlassVariant::Regular, None, false) });
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let (x, y) = if centered {
            // Centred on the button, its top just above the button's centre; kept on screen.
            let x = (a.center().0 - cw / 2.0).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
            (x, (a.center().1 - MENU_CENTER_DY).min(h - g.dev_bottom - ch).max(g.safe_top))
        } else {
            // Trailing and bottom edges line up with the picker's button; it hangs down from it when there is no room above.
            let x = (a.x + a.w - cw + MENU_DX).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
            let (top, bottom) = (a.center().1 - MENU_DY, a.center().1 + MENU_DY);
            (x, if bottom - ch >= g.safe_top { bottom - ch } else { top.min(h - g.dev_bottom - ch) })
        };
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("menu#scrim", (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, Lay::new(Size { w: cw, h: ch }, Some(card), kids))])
    }

    /// `.toggleStyle(.button)`: bordered-button metrics, accent label; when on, an accent-tinted capsule.
    #[allow(clippy::too_many_arguments)]
    fn toggle_button(&mut self, v: &ViewNode, action: u64, label: &ViewNode, on: bool, disabled: bool, p: Prop, env: Env) -> Lay {
        let accent = self.semantic(Semantic::Accent);
        let fg = if disabled { self.semantic(Semantic::Tertiary) } else { accent };
        let mut n = Node::new(&v.id, "button");
        n.role = Role::Toggle;
        n.action = (!disabled).then_some(action);
        n.disabled = disabled;
        n.label = first_text(label);
        n.value = Some(if on { "on" } else { "off" }.into());
        let (hp, vp) = (12.0, 7.0);
        let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
        let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, axis: None, ..env });
        let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
        if on {
            n.fill = Some(Color { a: fg.a * TOGGLE_BUTTON_ON_A, ..fg });
        }
        n.radius = PILL;
        Lay::new(size, Some(n), vec![(hp, vp, l)])
    }

    /// `Gauge`: linear styles are a capsule bar (accent fill on a gray track, or a primary bar with a marker),
    /// circular ones a ring or an open arc with the current value inside.
    fn gauge(&mut self, v: &ViewNode, style: GaugeStyle, value: f64, l: GaugeLabels, p: Prop, env: Env) -> Lay {
        let f = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        let mut n = Node::new(&v.id, "gauge");
        n.value = Some(format!("{f}"));
        n.label = l.label.and_then(first_text);
        if matches!(style, GaugeStyle::AccessoryCircular | GaugeStyle::AccessoryCircularCapacity) {
            return self.gauge_circular(v, n, style == GaugeStyle::AccessoryCircular, f, l, env);
        }
        let fin = p.w.filter(|w| w.is_finite());
        let w = fin.unwrap_or(200.0);
        let primary = self.semantic(Semantic::Primary);
        let accent = self.semantic(Semantic::Accent);
        let (bar_h, track_c, gap, leading) = match style {
            GaugeStyle::AccessoryLinearCapacity => (PROGRESS_H, self.pick(PROGRESS_TRACK_FILL), GAUGE_ACC_LABEL_GAP, true),
            GaugeStyle::AccessoryLinear => (GAUGE_ACC_BAR, primary, GAUGE_ACC_LABEL_GAP, true),
            _ => (GAUGE_BAR, self.control_fill(0.12), GAUGE_LABEL_GAP, false),
        };
        let marker = style == GaugeStyle::AccessoryLinear;
        let mut kids = vec![];
        let mut y = 0.0;
        // accessoryLinear shows no label (like iOS).
        if let Some(lv) = l.label.filter(|_| !marker) {
            let ll = self.view(lv, Prop { w: fin, h: None }, env);
            let (lw, lh) = (ll.size.w, ll.size.h);
            kids.push((if leading { 0.0 } else { (w - lw) / 2.0 }, 0.0, ll));
            y += lh + gap;
        }
        let side = |cx: &mut Self, lv: Option<&ViewNode>| lv.map(|lv| cx.view(lv, Prop { w: None, h: None }, env));
        let (minl, maxl) = (side(self, l.min), side(self, l.max));
        let lead = minl.as_ref().map_or(0.0, |m| m.size.w + DEFAULT_SPACING);
        let trail = maxl.as_ref().map_or(0.0, |m| m.size.w + DEFAULT_SPACING);
        let bw = (w - lead - trail).max(0.0);
        let row_h = [bar_h, minl.as_ref().map_or(0.0, |m| m.size.h), maxl.as_ref().map_or(0.0, |m| m.size.h)].into_iter().fold(0.0, f64::max);
        if let Some(m) = minl {
            let mh = m.size.h;
            kids.push((0.0, y + (row_h - mh) / 2.0, m));
        }
        if let Some(m) = maxl {
            let (mw, mh) = (m.size.w, m.size.h);
            kids.push((w - mw, y + (row_h - mh) / 2.0, m));
        }
        let mut track = Node::new(format!("{}#track", v.id), "gauge-track");
        track.fill = Some(track_c);
        track.radius = PILL;
        track.inspect = false;
        let mut tk = vec![];
        if marker {
            // A primary dot in a cut-out of the background color.
            let (cut, dot) = GAUGE_MARKER;
            let cx = f * bw;
            let mut m = Node::new(format!("{}#marker", v.id), "gauge-marker");
            m.fill = Some(self.semantic(Semantic::Background));
            m.radius = PILL;
            m.inspect = false;
            let mut d = Node::new(format!("{}#dot", v.id), "gauge-dot");
            d.fill = Some(primary);
            d.radius = PILL;
            d.inspect = false;
            let dl = Lay::new(Size { w: dot, h: dot }, Some(d), vec![]);
            tk.push((cx - cut / 2.0, (bar_h - cut) / 2.0, Lay::new(Size { w: cut, h: cut }, Some(m), vec![((cut - dot) / 2.0, (cut - dot) / 2.0, dl)])));
        } else {
            let mut bar = Node::new(format!("{}#bar", v.id), "gauge-bar");
            bar.fill = Some(accent);
            bar.radius = PILL;
            bar.inspect = false;
            let fw = (bw * f).max(if f > 0.0 { bar_h } else { 0.0 });
            tk.push((0.0, 0.0, Lay::new(Size { w: fw, h: bar_h }, Some(bar), vec![])));
        }
        kids.push((lead, y + (row_h - bar_h) / 2.0, Lay::new(Size { w: bw, h: bar_h }, Some(track), tk)));
        y += row_h;
        if let Some(cv) = l.current.filter(|_| !marker) {
            let cl = self.view(cv, Prop { w: fin, h: None }, env);
            let (cw, ch) = (cl.size.w, cl.size.h);
            y += GAUGE_VALUE_GAP;
            kids.push((if leading { 0.0 } else { (w - cw) / 2.0 }, y, cl));
            y += ch;
        }
        Lay::new(Size { w, h: y }, Some(n), kids)
    }

    /// `accessoryCircularCapacity`: a ring filled clockwise from the top. `accessoryCircular`: an arc open at the
    /// bottom with a dot at the value and the label in the opening. Both show the current value inside.
    fn gauge_circular(&mut self, v: &ViewNode, n: Node, open: bool, f: f64, l: GaugeLabels, env: Env) -> Lay {
        let s = GAUGE_RING;
        let primary = self.semantic(Semantic::Primary);
        let width = (GAUGE_RING_W * 100.0).round() as u16;
        let arc = |id: String, from: f64, to: f64, c: Color| {
            let mut a = Node::new(id, "gauge-arc");
            a.glyph = Some((Glyph::Arc { from: from.round() as i16, to: to.round() as i16, width }, c));
            a.inspect = false;
            Lay::new(Size { w: s, h: s }, Some(a), vec![])
        };
        let mut kids = vec![];
        if open {
            kids.push((0.0, 0.0, arc(format!("{}#track", v.id), -GAUGE_ARC_ANGLE, GAUGE_ARC_ANGLE, primary)));
            let (cut, dot) = GAUGE_ARC_MARKER;
            let a = (-GAUGE_ARC_ANGLE + 2.0 * GAUGE_ARC_ANGLE * f).to_radians();
            let r = (s - GAUGE_RING_W) / 2.0;
            let (mx, my) = (s / 2.0 + r * a.sin(), s / 2.0 - r * a.cos());
            let mut m = Node::new(format!("{}#marker", v.id), "gauge-marker");
            m.fill = Some(self.semantic(Semantic::Background));
            m.radius = PILL;
            m.inspect = false;
            let mut d = Node::new(format!("{}#dot", v.id), "gauge-dot");
            d.fill = Some(primary);
            d.radius = PILL;
            d.inspect = false;
            let dl = Lay::new(Size { w: dot, h: dot }, Some(d), vec![]);
            kids.push((mx - cut / 2.0, my - cut / 2.0, Lay::new(Size { w: cut, h: cut }, Some(m), vec![((cut - dot) / 2.0, (cut - dot) / 2.0, dl)])));
        } else {
            kids.push((0.0, 0.0, arc(format!("{}#track", v.id), -180.0, 180.0, Color { a: primary.a * GAUGE_RING_TRACK_A, ..primary })));
            if f > 0.0 {
                kids.push((0.0, 0.0, arc(format!("{}#bar", v.id), 0.0, 360.0 * f, primary)));
            }
        }
        if let Some(cv) = l.current {
            let cl = self.view(cv, Prop { w: Some(s - 2.0 * GAUGE_RING_W), h: None }, Env { size: GAUGE_RING_PT, weight: None, align: TextAlign::Center, ..env });
            let (cw, ch) = (cl.size.w, cl.size.h);
            let cy = if open { s / 2.0 - 0.9 } else { s / 2.0 };
            kids.push(((s - cw) / 2.0, cy - ch / 2.0, cl));
        }
        if let Some(lv) = l.label.filter(|_| open) {
            let ll = self.view(lv, Prop { w: None, h: None }, Env { size: GAUGE_ARC_LABEL_PT, weight: Some(600), ..env });
            let (lw, lh) = (ll.size.w, ll.size.h);
            kids.push(((s - lw) / 2.0, s / 2.0 + 19.1 - lh / 2.0, ll));
        }
        Lay::new(Size { w: s, h: s }, Some(n), kids)
    }

    /// Wheel `Picker`: the options on a drum around a highlight band, the selection in it. Rows further from
    /// the centre are squashed and dimmed; tapping one selects it.
    fn wheel(&mut self, v: &ViewNode, label: &ViewNode, options: &[PickerOption], selected: Option<usize>, disabled: bool, p: Prop) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(320.0);
        let (h, cy) = (WHEEL_H, WHEEL_H / 2.0);
        let mut root = Node::new(&v.id, "picker");
        root.label = first_text(label);
        root.disabled = disabled;
        root.value = selected.and_then(|i| options.get(i)).map(|o| o.text.clone());
        let mut band = Node::new(format!("{}#band", v.id), "wheel-band");
        band.fill = Some(self.pick(WHEEL_BAND_FILL));
        band.radius = PILL;
        band.inspect = false;
        let (bh, inset) = WHEEL_BAND;
        let mut kids = vec![(inset, cy - bh / 2.0, Lay::new(Size { w: (w - 2.0 * inset).max(0.0), h: bh }, Some(band), vec![]))];
        let primary = self.semantic(Semantic::Primary);
        let sel = selected.unwrap_or(0) as isize;
        for (i, o) in options.iter().enumerate() {
            let d = i as isize - sel;
            let Some(&(off, sq)) = WHEEL_ROWS.get(d.unsigned_abs()) else { continue };
            let pt = WHEEL_PT * sq as f32;
            let color = if d == 0 { primary } else { Color { a: primary.a * WHEEL_DIM_A, ..primary } };
            let mut t = self.text_lay(&format!("{}#opt{i}#text", v.id), &o.text, FontSpec::new(pt, self.line_h(pt), 400, TextAlign::Leading), color, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            let rh = WHEEL_ROWS[1].0 * sq;
            let mut b = Node::new(format!("{}#opt{i}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!disabled && d != 0).then_some(o.action);
            b.disabled = disabled;
            let (tw, th) = (t.size.w, t.size.h);
            let y = cy + off * d.signum() as f64 - rh / 2.0;
            kids.push((0.0, y, Lay::new(Size { w, h: rh }, Some(b), vec![((w - tw) / 2.0, (rh - th) / 2.0, t)])));
        }
        Lay::new(Size { w, h }, Some(root), kids)
    }

    /// `Grid`: column widths come from the cells. The grid stays content-sized.
    #[allow(clippy::too_many_arguments)]
    fn grid(&mut self, v: &ViewNode, alignment: Alignment, hs: Option<f64>, vs: Option<f64>, rows: &[onesim_protocol::GridLine], _p: Prop, env: Env) -> Lay {
        let hgap = hs.unwrap_or(DEFAULT_SPACING);
        let vgap = vs.unwrap_or(DEFAULT_SPACING);
        let spans: Vec<Vec<usize>> = rows.iter().map(|r| r.cells.iter().map(cell_columns).collect()).collect();
        let cols = spans.iter().map(|r| r.iter().sum::<usize>()).max().unwrap_or(0);
        let mut ideals = Vec::with_capacity(rows.len());
        for row in rows {
            let mut ws = Vec::with_capacity(row.cells.len());
            for cell in &row.cells {
                ws.push(self.view(cell, Prop { w: None, h: None }, env).size.w);
            }
            ideals.push(ws);
        }
        let mut col_w = vec![0.0_f64; cols];
        for (ri, row) in rows.iter().enumerate() {
            let mut col = 0;
            for (ci, _) in row.cells.iter().enumerate() {
                let span = spans[ri][ci];
                if span == 1 && col < cols {
                    col_w[col] = col_w[col].max(ideals[ri][ci]);
                }
                col += span;
            }
        }
        for _ in 0..4 {
            let mut changed = false;
            for (ri, row) in rows.iter().enumerate() {
                let mut col = 0;
                for (ci, _) in row.cells.iter().enumerate() {
                    let span = spans[ri][ci];
                    if span > 1 && col + span <= cols {
                        let sum: f64 = col_w[col..col + span].iter().sum();
                        let gaps = hgap * (span - 1) as f64;
                        let need = ideals[ri][ci];
                        if need > sum + gaps + 1e-6 {
                            let add = (need - sum - gaps) / span as f64;
                            for w in &mut col_w[col..col + span] {
                                *w += add;
                            }
                            changed = true;
                        }
                    }
                    col += span;
                }
            }
            if !changed {
                break;
            }
        }
        let mut col_x = vec![0.0; cols];
        let mut x = 0.0;
        for i in 0..cols {
            col_x[i] = x;
            x += col_w[i];
            if i + 1 != cols {
                x += hgap;
            }
        }
        let grid_w = x;
        let mut y = 0.0;
        let mut kids = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            let align_v = row.alignment.unwrap_or(alignment);
            let mut laid_row = Vec::new();
            let mut col = 0;
            let mut rh = 0.0_f64;
            for (ci, cell) in row.cells.iter().enumerate() {
                let span = spans[ri][ci];
                let slot = slot_width(&col_w, col, span, hgap);
                let laid = self.view(cell, Prop { w: Some(slot), h: None }, env);
                rh = rh.max(laid.size.h);
                laid_row.push((col, slot, laid));
                col += span;
            }
            for (col, slot, laid) in laid_row {
                let dx = col_x.get(col).copied().unwrap_or(0.0) + (slot - laid.size.w) * h_frac(alignment);
                let dy = y + (rh - laid.size.h) * v_frac(align_v);
                kids.push((dx, dy, laid));
            }
            y += rh;
            if ri + 1 != rows.len() {
                y += vgap;
            }
        }
        Lay::new(Size { w: grid_w, h: y }, Some(Node::new(&v.id, "grid")), kids)
    }

    /// `LazyVGrid` / `LazyHGrid`. Tracks run along the cross axis; `spacing` separates lines.
    #[allow(clippy::too_many_arguments)]
    fn lazy_grid(&mut self, v: &ViewNode, axis: Axis, alignment: Alignment, spacing: Option<f64>, tracks: &[GridTrack], children: &[ViewNode], p: Prop, env: Env) -> Lay {
        let vertical = axis == Axis::Vertical;
        let cross_prop = if vertical { p.w } else { p.h };
        let available = cross_prop.filter(|x| x.is_finite()).unwrap_or(10.0);
        let slots = resolve_tracks(tracks, available);
        let n = slots.len();
        if n == 0 {
            return Lay::new(Size::default(), Some(Node::new(&v.id, "lazygrid")), vec![]);
        }
        let line_gap = spacing.unwrap_or(DEFAULT_SPACING);
        let mut items = Vec::with_capacity(children.len());
        for (i, child) in children.iter().enumerate() {
            let col = i % n;
            let line = i / n;
            let prop = if vertical { Prop { w: Some(slots[col].w), h: None } } else { Prop { w: None, h: Some(slots[col].w) } };
            items.push((col, line, self.view(child, prop, env)));
        }
        let nlines = items.iter().map(|(_, line, _)| *line + 1).max().unwrap_or(0);
        let mut line_main = vec![0.0_f64; nlines];
        for (_, line, laid) in &items {
            let main = if vertical { laid.size.h } else { laid.size.w };
            line_main[*line] = line_main[*line].max(main);
        }
        let mut cross_at = vec![0.0; n];
        let mut acc = 0.0;
        for (i, slot) in slots.iter().enumerate() {
            cross_at[i] = acc;
            acc += slot.w + slot.gap;
        }
        let cross_total = acc;
        let mut line_at = vec![0.0; nlines];
        let mut acc = 0.0;
        for i in 0..nlines {
            line_at[i] = acc;
            acc += line_main[i];
            if i + 1 != nlines {
                acc += line_gap;
            }
        }
        let main_total = acc;
        let kids = items
            .into_iter()
            .map(|(col, line, laid)| {
                let align = slots[col].align.unwrap_or(alignment);
                if vertical {
                    let dx = cross_at[col] + (slots[col].w - laid.size.w) * h_frac(align);
                    let dy = line_at[line] + (line_main[line] - laid.size.h) * v_frac(align);
                    (dx, dy, laid)
                } else {
                    let dx = line_at[line] + (line_main[line] - laid.size.w) * h_frac(align);
                    let dy = cross_at[col] + (slots[col].w - laid.size.h) * v_frac(align);
                    (dx, dy, laid)
                }
            })
            .collect();
        let size = if vertical { Size { w: cross_total, h: main_total } } else { Size { w: main_total, h: cross_total } };
        Lay::new(size, Some(Node::new(&v.id, "lazygrid")), kids)
    }

    /// `GeometryReader`: greedy on each finite parent axis, otherwise the guest's proposal.
    fn geometry(&mut self, v: &ViewNode, proposal: [f64; 2], child: &ViewNode, p: Prop, env: Env) -> Lay {
        let axis = |prop: Option<f64>, guest: f64| prop.filter(|x| x.is_finite()).unwrap_or(guest);
        let (w, h) = (axis(p.w, proposal[0]), axis(p.h, proposal[1]));
        let child = self.view(child, Prop { w: Some(w), h: Some(h) }, env);
        let mut n = Node::new(&v.id, "geometry");
        n.proposal = Some(proposal);
        Lay::new(Size { w, h }, Some(n), vec![(0.0, 0.0, child)])
    }

    /// `ViewThatFits`: the first child whose ideal size fits the checked axes, else the last.
    /// The probe is unconstrained on those axes, so wrapping text is not treated as fitting.
    /// The chosen child is then laid out with the real proposal.
    fn view_that_fits(&mut self, v: &ViewNode, axes: FitAxes, children: &[ViewNode], p: Prop, env: Env) -> Lay {
        let fits = |size: Size| match axes {
            FitAxes::Both => fits_axis(size.w, p.w) && fits_axis(size.h, p.h),
            FitAxes::Horizontal => fits_axis(size.w, p.w),
            FitAxes::Vertical => fits_axis(size.h, p.h),
        };
        let probe = match axes {
            FitAxes::Both => Prop { w: None, h: None },
            FitAxes::Horizontal => Prop { w: None, h: p.h },
            FitAxes::Vertical => Prop { w: p.w, h: None },
        };
        let mut index = None;
        for (i, c) in children.iter().enumerate() {
            let laid = self.view(c, probe, env);
            if fits(laid.size) || i + 1 == children.len() {
                index = Some(i);
                break;
            }
        }
        let inner = match index {
            Some(i) => self.view(&children[i], p, env),
            None => Lay::new(Size::default(), None, vec![]),
        };
        let size = inner.size;
        Lay::new(size, Some(Node::new(&v.id, "viewThatFits")), vec![(0.0, 0.0, inner)])
    }

    fn scroll(&mut self, v: &ViewNode, vertical: bool, child: &ViewNode, p: Prop, env: Env) -> Lay {
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let margins = content_margins(&v.modifiers);
        let child_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, ..env };
        let cp = if vertical {
            Prop { w: p.w.map(|w| (w - margins.leading - margins.trailing).max(0.0)), h: None }
        } else {
            Prop { w: None, h: p.h.map(|h| (h - margins.top - margins.bottom).max(0.0)) }
        };
        let cache = self.list_cache.take();
        let mut c = self.view(child, cp, child_env);
        self.list_cache = cache;
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (cw, ch) = if vertical {
            (c.size.w, c.size.h + margins.top + margins.bottom + top + bottom)
        } else {
            (c.size.w + margins.leading + margins.trailing, c.size.h)
        };
        let w = fin(p.w).unwrap_or(cw);
        let h = fin(p.h).unwrap_or(ch);
        let max = ((cw - w).max(0.0), (ch - h).max(0.0));
        let off = self.scroll.get(&v.id).copied().unwrap_or((0.0, 0.0));
        let off = if vertical { (0.0, self.clamp_offset(off.1, max.1, h)) } else { (self.clamp_offset(off.0, max.0, w), 0.0) };
        pin_headers(&mut c, if vertical { off.1 } else { off.0 }, vertical);
        let mut page = None;
        let mut aligned = false;
        for m in &v.modifiers {
            if let Modifier::ScrollTargetBehavior(behavior) = m {
                match behavior {
                    ScrollSnap::Paging => {
                        page = Some(if vertical { h } else { w });
                        aligned = false;
                    }
                    ScrollSnap::ViewAligned => {
                        aligned = true;
                        page = None;
                    }
                }
            }
        }
        let mut n = Node::new(&v.id, "scroll");
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None, content: None, page, aligned });
        if v.modifiers.iter().any(|m| matches!(m, Modifier::ScrollPosition)) {
            n.value = Some("position".to_string());
        }
        n.inspect = true;
        // No margins: keep the old cross-axis placement (centered when vertical).
        let (x0, y0) = if vertical {
            let cx = if margins.leading == 0.0 && margins.trailing == 0.0 { (w - c.size.w) / 2.0 } else { margins.leading };
            (cx, top + margins.top)
        } else {
            let cy = if margins.top == 0.0 && margins.bottom == 0.0 { top } else { top + margins.top };
            (margins.leading, cy)
        };
        Lay::new(Size { w, h }, Some(n), vec![(x0 - off.0, y0 - off.1, c)])
    }

    fn list(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let fin_h = p.h.filter(|h| h.is_finite());
        let key = (self.list_cache.is_some() && !env.in_row && w > 0.0 && fin_h.is_some()
            && env.anim.is_none() && env.content_transition == ContentTransitionKind::Identity).then(|| ListLayoutKey {
            node: v as *const ViewNode as usize,
            width: w,
            env,
            scale: self.scale,
            margin: self.margin,
            row_pad: self.row_pad,
            dark: self.dark,
            dynamic_type: self.dts,
            ios: self.ios,
            system_fonts: self.fonts.is_sf(),
        });
        let cached = key.as_ref().and_then(|key| self.list_cache.as_deref()?.list.as_ref().filter(|(cached_key, _)| cached_key == key));
        let mut content = if let Some((_, content)) = cached {
            content.clone()
        } else {
            let content = self.list_content(v, style, sections, w, env);
            if let Some((key, cache)) = key.zip(self.list_cache.as_deref_mut()) {
                // A sibling list follows the normal layout path while this tree owns the cache.
                if cache.list.as_ref().is_none_or(|(cached_key, _)| cached_key.node == key.node) && cacheable_list(v, sections) {
                    cache.list = Some((key, content.clone()));
                }
            }
            content
        };
        let ch = content.bottom + env.bottom_inset;
        let h = fin_h.unwrap_or(ch);
        let max = (0.0, (ch - h).max(0.0));
        let off = self.clamp_offset(self.scroll.get(&v.id).map_or(0.0, |o| o.1), max.1, h);
        let mut n = Node::new(&v.id, "list");
        n.fill = (!self.floating).then(|| self.semantic(if style == ListStyle::InsetGrouped { Semantic::GroupedBackground } else { Semantic::Background }));
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max, snap: None, content: None, page: None, aligned: false });
        for (_, y, _) in &mut content.kids {
            *y -= off;
        }
        Lay::new(Size { w, h }, Some(n), content.kids)
    }

    fn list_content(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], w: f64, env: Env) -> ListContent {
        let inset = style == ListStyle::InsetGrouped;
        let margin = if inset { self.margin } else { 0.0 };
        let gw = w - 2.0 * margin;
        let inner_w = gw - 2.0 * self.row_pad;
        let row_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, fg: None, in_row: true, ..env };
        let sec_col = if self.floating { let c = if self.dark { FLOAT_CARD.1 } else { FLOAT_CARD.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::SecondaryGroupedBackground) };
        let sep_col = if self.floating { let c = if self.dark { FLOAT_SEP.1 } else { FLOAT_SEP.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::Separator) };
        let mut y = env.safe_top;
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        for (si, sec) in sections.iter().enumerate() {
            let sid = format!("{}#s{si}", v.id);
            if let Some(h) = &sec.header {
                let hs = onesim_protocol::TextStyle::Headline.size_at(self.dts) as f32;
                let spec = FontSpec::new(hs, self.line_h(hs), 600, TextAlign::Leading);
                let mut t = self.text_lay(&format!("{sid}#header"), h, spec, self.semantic(Semantic::Secondary), Some(inner_w as f32));
                if let Some(n) = t.node.as_mut() {
                    n.role = Role::Text;
                }
                let hh = t.size.h + HEADER_PAD;
                kids.push((margin + self.row_pad, y + (hh - t.size.h) / 2.0, t));
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
                expand_button(&mut c, 0.0, yo, (-self.row_pad, 0.0, gw - self.row_pad, rh));
                let tx = first_text_x(&c, 0.0).unwrap_or(0.0);
                let mut rn = Node::new(format!("{sid}#r{ri}"), "row");
                rn.inspect = false;
                let last = ri + 1 == sec.rows.len();
                let mut rkids = vec![(self.row_pad, yo, c)];
                if !last {
                    let mut sp = Node::new(format!("{sid}#r{ri}#sep"), "separator");
                    sp.fill = Some(sep_col);
                    sp.inspect = false;
                    let x0 = self.row_pad + tx;
                    // Measured: 1pt (3px @3x) solid line ending at the row boundary.
                    rkids.push((x0, rh - 1.0, Lay::new(Size { w: (gw - self.row_pad - x0).max(0.0), h: 1.0 }, Some(sp), vec![])));
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
                kids.push((margin + self.row_pad, y, t));
                y += th;
            }
            // Between sections 17.7; after the last one the list ends 20pt above the bottom inset (measured).
            y += if si + 1 == sections.len() { 20.0 } else { SECTION_GAP };
            // Measured: a header-less section after another sits 35pt below it (header-ful ones: 17.7 + header).
            if sections.get(si + 1).is_some_and(|n| n.header.is_none()) {
                y += HEADERLESS_EXTRA_GAP;
            }
        }
        ListContent { bottom: y, kids }
    }

    #[allow(clippy::too_many_arguments)]
    // Out of line: inlined into the recursive `view` dispatcher it inflates every stack frame.
    #[inline(never)]
    fn navigation(
        &mut self,
        v: &ViewNode,
        title: Option<&str>,
        mode: DisplayMode,
        back: Option<&onesim_protocol::BackButton>,
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
            let sz = if self.compact_nav {
                28.0
            } else { onesim_protocol::TextStyle::LargeTitle.size_at(self.dts) as f32
            };
            let spec = FontSpec::new(sz, self.line_h(sz), 700, TextAlign::Leading);
            self.text_lay(&format!("{id}#title"), t, spec, primary, Some((w - 32.0) as f32))
        });
        let title_h = large_title.as_ref().map_or(0.0, |l| l.size.h);
        // Compact bar: 58pt in all, the title inside it (no band to collapse).
        let band = if self.compact_nav {
            4.0
        } else if large { (title_h + 1.33).max(52.0) } else { 0.0 };
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
        let compact = self.compact_nav;
        if let Some(tl) = large_title.filter(|_| compact || offset < band) {
            if compact {
                // Compact bar (iPhone Duo): the title stays at the bar's top-left (measured y 31 in a bar from 24).
                bar_kids.push((self.margin, 7.0, tl));
            } else {
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
        }
        if let Some(t) = title.filter(|_| !(self.compact_nav && large)) {
            let alpha = if large { ((collapse - (band - INLINE_FADE)) / INLINE_FADE).clamp(0.0, 1.0) as f32 } else { 1.0 };
            if alpha > 0.0 {
                // Inline titles grow with Dynamic Type but cap at x1.225 (measured at accessibility5).
                let nsz = 17.0 * (onesim_protocol::TextStyle::Body.size_at(self.dts) as f32 / 17.0).min(1.225);
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
            // Under a presentation the chevron desaturates like the rest of the presenter (iOS 26; iOS 27 keeps it).
            let primary = if self.desat && self.ios < IosVersion::Ios27 {
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
                if let Some((bx, by)) = self.column_back {
                    // In the status column (iPhone Duo): a 48pt circle, screen-placed (the bar starts at the top inset).
                    bar_kids.push((
                        bx,
                        by - top,
                        Lay::new(Size { w: 48.0, h: 48.0 }, Some(n), vec![]),
                    ));
                } else {
                bar_kids.push((self.margin, 0.0, Lay::new(Size { w: 44.0, h: 44.0 }, Some(n), vec![])));
                }
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
            item.disabled = tab.action == Some(0);
            item.action = tab.action.or_else(|| host.parse().ok()).filter(|_| !item.disabled);
            item.radius = PILL;
            if sel {
                // iOS 27 sinks the selection into the bar (tabs-home(-dark).png, ios27); iOS 26 raises it.
                item.fill = Some(match (self.dark, self.ios >= IosVersion::Ios27) {
                    (true, true) => rgba(10, 10, 10, 1.0),
                    (false, true) => rgba(225, 225, 231, 1.0),
                    (true, false) => rgba(58, 58, 60, 1.0),
                    (false, false) => rgba(233, 233, 235, 1.0), });
            }
            let tint = if sel { t.tint.unwrap_or(accent) } else { label };
            let path = if sel { tab.selected_icon.as_ref().or(tab.icon.as_ref()) } else { tab.icon.as_ref() };
            let mut icon = Node::new(format!("{host}#tab-{}#icon", tab.key), "image");
            icon.inspect = false;
            let mut isz = (0.0, 0.0);
            if let Some(pm) = path.and_then(|p| crate::image::load(p)) {
                isz = (pm.width() as f64 / self.scale, pm.height() as f64 / self.scale);
                let dest = Rect { x: 0.0, y: 0.0, w: isz.0, h: isz.1 };
                icon.image = super::rn::tinted(&pm, tint).map(|pixmap| crate::ImageRun { pixmap, dest, rotation: 0.0, filter: 2 });
            } else if let Some(name) = &tab.symbol {
                let spec = crate::SymbolSpec { name: name.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium, ..Default::default() };
                isz = crate::symbols::metrics(&spec).map_or((22.0, 22.0), |m| (m.w, m.h));
                icon.symbol = Some(crate::SymbolRun { spec, color: tint, size: isz });
            }
            let mut lb = Node::new(format!("{host}#tab-{}#label", tab.key), "text");
            lb.inspect = false;
            lb.text = Some(TextRun { text: tab.title.clone(), font: FontSpec::new(10.0, 12.0, if sel { 600 } else { 500 }, TextAlign::Center), color: label, strike: false, offset_x: 0.0, spans: vec![] });
            let mut ikids = vec![
                ((94.0 - isz.0) / 2.0,
                    if tab.symbol.is_some() { 3.5 } else { 6.0 }, Lay::new(Size { w: isz.0, h: isz.1 }, Some(icon), vec![])),
                // Label line: ink at 832.7..839.3 in the 17/18 Pro captures (both releases).
                (0.0, 33.3, Lay::new(Size { w: 94.0, h: 14.0 }, Some(lb), vec![])),
            ];
            if let Some(b) = &tab.badge {
                let spec = FontSpec::new(12.0, 14.0, 400, TextAlign::Center);
                let bw = (self.fonts.measure(b, &spec, None, &[]).0 + 10.0).max(18.0);
                let mut bn = Node::new(format!("{host}#tab-{}#badge", tab.key), "badge");
                bn.inspect = false;
                bn.radius = PILL;
                bn.fill = Some(self.destructive());
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
    fn tab_view(&mut self, v: &ViewNode, selected: usize, tabs: &[onesim_protocol::TabItem], content: &ViewNode, p: Prop, env: Env) -> Lay {
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
                    let spec = crate::SymbolSpec { name: fill.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium, ..Default::default() };
                    if s.ends_with(".fill") || crate::symbols::metrics(&spec).is_none() { s.clone() } else { fill }
                });
                TabBarItem { key: i.to_string(), title: t.title.clone(), icon: None, selected_icon: None, symbol, badge: t.badge.clone(), action: Some(t.action) }
            })
            .collect();
        let mut n = Node::new(&v.id, "tabview");
        n.inspect = false;
        let mut kids = vec![(0.0, 0.0, inner)];
        if !tabs.is_empty() {
            kids.push((0.0, 0.0, self.tab_bar(&TabBar { items, selected, tint: None }, &v.id, w, h)));
        }
        Lay::new(Size { w, h }, Some(n), kids)
    }

    fn page_tab_view(&mut self, v: &ViewNode, selected: usize, pages: &[TabPage], index: (PageIndexDisplay, PageIndexBackground), p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(10.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(10.0);
        let size = Size { w, h };
        let mut n = Node::new(&v.id, "pagetabview");
        n.clip_radius = Some(0.0);
        if pages.is_empty() || w <= 0.0 || h <= 0.0 {
            return Lay::new(size, Some(n), vec![]);
        }
        let selected = selected.min(pages.len() - 1);
        let idle = selected as f64 * w;
        let max = (pages.len() - 1) as f64 * w;
        let drag = self.scroll.get(&v.id).map_or(idle, |off| off.0);
        let offset = self.clamp_offset(drag, max, w);
        if pages.len() > 1 && pages.iter().any(|page| page.action.is_some()) {
            n.scroll = Some(ScrollInfo { vertical: false, offset: (offset, 0.0), max: (max, 0.0), snap: None, content: None, page: Some(w), aligned: false });
            n.pager = Some(Box::new(Pager { selected, actions: pages.iter().map(|page| page.action).collect() }));
        }

        let mut kids = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let x = i as f64 * w - offset;
            if x <= -w || x >= w {
                continue;
            }
            let content = self.view(&page.content, Prop { w: Some(w), h: Some(h) }, Env { axis: None, ..env });
            let position = ((w - content.size.w) / 2.0, (h - content.size.h) / 2.0, content);
            let mut page = Node::new(format!("{}#page-{i}", v.id), "page");
            page.inspect = false;
            page.clip_radius = Some(0.0);
            kids.push((x, 0.0, Lay::new(size, Some(page), vec![position])));
        }
        if index.0 == PageIndexDisplay::Always || (index.0 == PageIndexDisplay::Automatic && pages.len() > 1) {
            let dots = self.page_index(v, pages, selected, index.1, w);
            kids.push(((w - dots.size.w) / 2.0, (h - dots.size.h).max(0.0), dots));
        }
        Lay::new(size, Some(n), kids)
    }

    fn page_index(&self, v: &ViewNode, pages: &[TabPage], selected: usize, background: PageIndexBackground, width: f64) -> Lay {
        let pitch = ((width - 16.0) / pages.len() as f64).clamp(7.0, 16.0);
        let w = pages.len() as f64 * pitch + 16.0;
        let h = 26.0;
        let mut index = Node::new(format!("{}#index", v.id), "page-index");
        index.label = Some(format!("Page {} of {}", selected + 1, pages.len()));
        let material = background == PageIndexBackground::Always;
        index.radius = PILL;
        index.fill = material.then(|| rgba(118, 118, 128, 0.35));
        let kids = pages.iter().enumerate().map(|(i, page)| {
            let mut dot = Node::new(format!("{}#index-{i}", v.id), "page-dot");
            dot.role = Role::Button;
            dot.label = Some(format!("Page {}", i + 1));
            dot.action = page.action;
            dot.disabled = page.action.is_none();
            let mut mark = Node::new(format!("{}#index-{i}#mark", v.id), "circle");
            mark.inspect = false;
            mark.radius = PILL;
            let alpha = if i == selected { 1.0 } else { 0.3 };
            mark.fill = Some(rgba(255, 255, 255, alpha));
            let mark = Lay::new(Size { w: 7.0, h: 7.0 }, Some(mark), vec![]);
            let child = Lay::new(Size { w: pitch, h }, Some(dot), vec![((pitch - 7.0) / 2.0, (h - 7.0) / 2.0, mark)]);
            (8.0 + i as f64 * pitch, 0.0, child)
        }).collect();
        Lay::new(Size { w, h }, Some(index), kids)
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
                t.font.extra *= kf;
                t.font.baseline *= kf;
                for sp in t.spans.iter_mut() {
                    sp.pt *= kf;
                    sp.extra *= kf;
                    sp.baseline *= kf;
                }
                // Smaller type tracks differently: re-measure a single line so it never wraps at its own width.
                if one_line {
                    let (w, _) = self.fonts.measure(&t.text, &FontSpec { align: TextAlign::Leading, ..t.font }, None, &t.spans);
                    l.size.w = (w * self.scale).ceil() / self.scale;
                }
            }
            if let Some(im) = n.image.as_mut() {
                im.dest.x *= k;
                im.dest.y *= k;
                im.dest.w *= k;
                im.dest.h *= k;
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
                sc.page = sc.page.map(|p| p * k);
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
            let tsz = onesim_protocol::TextStyle::Headline.size_at(self.dts) as f32;
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
            let fg = match (self.dark, self.ios >= IosVersion::Ios27, popover) {
                (true, true, false) => ALERT_27_DARK_MESSAGE,
                (true, true, true) => POPOVER_27_DARK_MESSAGE,
                (true, false, _) => ALERT_MESSAGE_FG.1,
                _ => ALERT_MESSAGE_FG.0, };
            let env = Env { size: ALERT_MESSAGE_PT, align: TextAlign::Leading, fg: Some(rgba(fg.0, fg.1, fg.2, 1.0)), ..Env::new() };
            let ml = self.view(m, Prop { w: Some(inner), h: None }, env);
            y += gap;
            let mh = ml.size.h;
            kids.push((ALERT_TEXT_PAD, y, ml));
            y += mh;
        }
        y += ALERT_TEXT_BUTTON_GAP;
        let is_cancel = |b: &ViewNode| matches!(b.kind, ViewKind::Button { role: Some(onesim_protocol::ButtonRole::Cancel), .. });
        let mut acts: Vec<&ViewNode> = actions.iter().filter(|b| !(popover && is_cancel(b))).collect();
        let side_by_side = !popover && acts.len() == 2;
        acts.sort_by_key(|b| is_cancel(b) != side_by_side); // stable: cancel first when side by side, last when stacked
        let n = acts.len().max(1);
        let bw = if side_by_side { (card_w - 2.0 * ALERT_BTN_PAD - ALERT_BTN_GAP) / 2.0 } else { card_w - 2.0 * ALERT_BTN_PAD };
        let bh = ALERT_BTN_H;
        let accent = self.semantic(Semantic::Accent);
        let dark27 = self.dark && self.ios >= IosVersion::Ios27;
        let (red27, btn27) = if popover {
            (POPOVER_27_DARK_DESTRUCTIVE, POPOVER_27_DARK_BTN)
        } else {
            (ALERT_27_DARK_DESTRUCTIVE, ALERT_27_DARK_BTN)
        };
        let red = if dark27 {
            self.pick((red27, red27))
        } else { self.color(&onesim_protocol::Color::Rgba { r: 1.0, g: 56.0 / 255.0, b: 60.0 / 255.0, a: 1.0, dark: Some([1.0, 66.0 / 255.0, 69.0 / 255.0]) })
        };
        let fills = if popover { POPOVER_BTN_FILL } else { ALERT_BTN_FILL
        };
        let fills = if dark27 { (fills.0, btn27) } else { fills };
        for (i, b) in acts.iter().enumerate() {
            let ViewKind::Button { action, label, disabled, role, .. } = &b.kind else { continue };
            let (fg, weight) = match role {
                Some(onesim_protocol::ButtonRole::Destructive) => (red, 600),
                Some(onesim_protocol::ButtonRole::Cancel) => (primary, 600),
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
        let mix = if self.dark && self.ios >= IosVersion::Ios27 {
            Some(PRESENT_GLASS_27_DARK)
        } else {
            mix.filter(|_| !self.dark)
        };
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix, shadow, ..self.glass_spec(GlassVariant::Regular, None, false) });
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
    fn status_bar(&mut self, w: f64, dev: &Device, ios: IosVersion) -> Lay {
        let sb = dev.status_bar_for(ios);
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
        let ios27 = ios >= IosVersion::Ios27;
        let wifi = glyph("statusbar#wifi",
            if ios27 { Glyph::Wifi27 } else { Glyph::Wifi }, primary, Size { w: 16.0 * k, h: 12.0 * k });
        let batt = glyph("statusbar#battery",
            if ios27 {
                Glyph::Battery27
            } else { Glyph::Battery
            }, primary, Size { w: 27.0 * k, h: 13.0 * k });
        let right = sb.icons.0 + (w - dev.viewport.width); // anchored to the trailing edge
        let dy = sb.icons.1;
        let mut root = Node::new("statusbar", "statusbar");
        root.inspect = false;
        let t = text(self, "statusbar#time", if dev.home_button { "9:41 AM" } else { "9:41" }, 600, TextAlign::Leading);
        let kids = if dev.safe_right > 0.0 {
            // Status column (iPhone Duo): clock, then the battery ring around the wifi glyph, then the dot arc.
            let cx = sb.time.0 + (w - dev.viewport.width);
            let ring = glyph(
                "statusbar#battery",
                Glyph::BatteryRing,
                primary,
                Size { w: 41.0, h: 41.0 },
            );
            let dots = glyph(
                "statusbar#signal",
                Glyph::DotsArc,
                self.semantic(Semantic::Tertiary),
                Size { w: 24.0, h: 7.0 },
            );
            vec![
                (cx - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (cx + 0.4 - 20.5, 73.75 - 20.5, ring),
                (cx - 10.0, 70.4, wifi),
                (cx - 11.6, 91.5, dots),
            ]
        } else if dev.home_button {
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
            let dots = glyph("statusbar#signal",
                if ios27 {
                    Glyph::SignalDots27
                } else { Glyph::SignalDots
                }, self.semantic(Semantic::Tertiary), Size { w: 20.0 * k, h: 4.0 * k });
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
        // Frames below were measured on iOS 26.5 / iPhone 17 Pro (tests/fixtures/showcase-ios).
        let dev = Device { safe_top: safe.0, safe_bottom: safe.1, island: None, ..*crate::device::device("iphone-17-pro").unwrap() };
        let params = SceneParams {
            viewport: vp,
            device: &dev,
            scale: 3.0,
            appearance: if dark { Appearance::Dark } else { Appearance::Light },
            dynamic_type: DynamicTypeSize::Large,
            ios: IosVersion::Ios26,
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
    fn retained_list_reuses_scroll_layout_and_matches_a_fresh_pass() {
        let tree: ViewNode = serde_json::from_str(r#"{"kind":"list","id":"list","sections":[{"header":"Rows","rows":[
            {"kind":"text","id":"a","text":"First row"}, {"kind":"text","id":"b","text":"Second row"},
            {"kind":"text","id":"c","text":"Third row"}]}]}"#).unwrap();
        let mut fonts = Fonts::with_fallback(true);
        let mut cache = SwiftUiLayoutCache::default();
        let no_scroll = HashMap::new();
        let scrolled = HashMap::from([("list".into(), (0.0, 35.0))]);
        let mut params = SceneParams {
            viewport: (180.0, 100.0), device: crate::device::device("iphone-17-pro").unwrap(), scale: 2.0,
            appearance: Appearance::Light, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios27,
            scroll: &no_scroll, sheets: &HashMap::new(), focus: None, overscroll: false, status_bar: false, chrome: Default::default(),
        };
        let initial = layout_with_cache(&tree, &params, &mut fonts, &mut cache);
        let retained_kids = cache.list.as_ref().unwrap().1.kids.as_ptr();
        params.scroll = &scrolled;
        let scroll = layout_with_cache(&tree, &params, &mut fonts, &mut cache);
        let fresh = layout(&tree, &params, &mut fonts);
        assert_eq!(cache.list.as_ref().unwrap().1.kids.as_ptr(), retained_kids);
        assert_eq!(by_id(&initial, "a").bounds.y - by_id(&scroll, "a").bounds.y, 35.0);
        assert_eq!(scroll.ui_tree(), fresh.ui_tree());
        assert_eq!(crate::render(&scroll, &mut fonts, params.scale).data(), crate::render(&fresh, &mut fonts, params.scale).data());

        params.appearance = Appearance::Dark;
        params.viewport.0 = 160.0;
        let dark = layout_with_cache(&tree, &params, &mut fonts, &mut cache);
        let fresh = layout(&tree, &params, &mut fonts);
        assert_ne!(cache.list.as_ref().unwrap().1.kids.as_ptr(), retained_kids);
        assert_eq!(dark.ui_tree(), fresh.ui_tree());
        assert_eq!(crate::render(&dark, &mut fonts, params.scale).data(), crate::render(&fresh, &mut fonts, params.scale).data());
    }

    #[test]
    fn type_style_changes_measure_case_and_scale() {
        let plain = scene(r#"{"kind":"text","id":"t","text":"Tracking","modifiers":[{"font":{"size":17,"weight":"regular"}}]}"#, (400.0, 80.0));
        let wide = scene(r#"{"kind":"text","id":"t","text":"Tracking","modifiers":[{"tracking":4},{"font":{"size":17,"weight":"regular"}}]}"#, (400.0, 80.0));
        assert!(by_id(&wide, "t").bounds.w > by_id(&plain, "t").bounds.w + 3.0, "tracking should widen the word");
        let tracked = scene(
            r#"{"kind":"text","id":"t","text":"WW","modifiers":[{"tracking":4},{"font":{"size":17,"weight":"regular"}}]}"#,
            (400.0, 80.0),
        );
        let run = scene(
            r#"{"kind":"text","id":"t","text":"WW","runs":[{"text":"WW","kerning":4}],"modifiers":[{"tracking":4},{"font":{"size":17,"weight":"regular"}}]}"#,
            (400.0, 80.0),
        );
        assert!(
            by_id(&run, "t").bounds.w > by_id(&tracked, "t").bounds.w + 2.0,
            "a run's kerning adds to the parent's tracking"
        );
        let upper = scene(r#"{"kind":"text","id":"t","text":"Hello","modifiers":[{"textCase":"uppercase"}]}"#, (200.0, 80.0));
        assert_eq!(by_id(&upper, "t").label.as_deref(), Some("HELLO"));
        let word = "Supercalifragilistic";
        let full = scene(&format!(r#"{{"kind":"text","id":"t","text":"{word}","modifiers":[{{"font":{{"size":17,"weight":"regular"}}}}]}}"#), (400.0, 80.0));
        let shrunk = scene(
            &format!(r#"{{"kind":"text","id":"t","text":"{word}","modifiers":[{{"font":{{"size":17,"weight":"regular"}}}},{{"minimumScaleFactor":0.5}},{{"frame":{{"width":40,"height":30}}}}]}}"#),
            (400.0, 80.0),
        );
        assert!(by_id(&shrunk, "t").bounds.w < by_id(&full, "t").bounds.w * 0.75, "minimumScaleFactor shrinks a word that does not fit");
        let sentence = "alpha beta gamma delta";
        let open = scene(
            &format!(r#"{{"kind":"text","id":"t","text":"{sentence}","modifiers":[{{"font":{{"size":17,"weight":"regular"}}}},{{"lineLimit":{{"max":1}}}},{{"minimumScaleFactor":0.5}},{{"frame":{{"width":220,"height":40}}}}]}}"#),
            (400.0, 80.0),
        );
        let tight = scene(
            &format!(r#"{{"kind":"text","id":"t","text":"{sentence}","modifiers":[{{"font":{{"size":17,"weight":"regular"}}}},{{"lineLimit":{{"max":1}}}},{{"minimumScaleFactor":0.5}},{{"frame":{{"width":80,"height":40}}}}]}}"#),
            (400.0, 80.0),
        );
        let open_pt = by_id(&open, "t").text.as_ref().unwrap().font.pt;
        let tight_pt = by_id(&tight, "t").text.as_ref().unwrap().font.pt;
        assert!(tight_pt < open_pt - 0.5, "lineLimit 1 scales a sentence that wraps: {tight_pt} vs {open_pt}");
        let one = scene(r#"{"kind":"text","id":"t","text":"Line","modifiers":[{"font":{"size":17,"weight":"regular"}}]}"#, (200.0, 80.0));
        let two = scene(r#"{"kind":"text","id":"t","text":"Line","modifiers":[{"font":{"size":17,"weight":"regular"}},{"lineLimit":{"min":2}}]}"#, (200.0, 120.0));
        assert!(by_id(&two, "t").bounds.h > by_id(&one, "t").bounds.h * 1.6, "a lower line limit reserves a second line");
    }

    #[test]
    fn baseline_offset_grows_the_line_and_pushes_the_next_row() {
        let plain = scene(
            r#"{"kind":"vstack","id":"v","spacing":0,"children":[
                {"kind":"text","id":"a","text":"Before"},
                {"kind":"text","id":"b","text":"Up"},
                {"kind":"text","id":"c","text":"After"}]}"#,
            (200.0, 200.0),
        );
        let raised = scene(
            r#"{"kind":"vstack","id":"v","spacing":0,"children":[
                {"kind":"text","id":"a","text":"Before"},
                {"kind":"text","id":"b","text":"Up","modifiers":[{"baselineOffset":6}]},
                {"kind":"text","id":"c","text":"After"}]}"#,
            (200.0, 200.0),
        );
        let pb = by_id(&plain, "b").bounds;
        let rb = by_id(&raised, "b").bounds;
        let pa = by_id(&plain, "a").bounds;
        let ra = by_id(&raised, "a").bounds;
        // The scene centers the stack, so compare gaps, not absolute y.
        near(rb.y - ra.y, pb.y - pa.y);
        near(rb.h, pb.h + 6.0);
        near(by_id(&raised, "c").bounds.y - rb.y, by_id(&plain, "c").bounds.y - pb.y + 6.0);
        let row = scene(
            r#"{"kind":"hstack","id":"h","alignment":"center","spacing":8,"children":[
                {"kind":"text","id":"up","text":"Up","modifiers":[{"baselineOffset":6}]},
                {"kind":"text","id":"base","text":"Base"}]}"#,
            (300.0, 80.0),
        );
        let up = by_id(&row, "up").bounds;
        let base = by_id(&row, "base").bounds;
        near(base.y - up.y, (up.h - base.h) / 2.0);
    }

    #[test]
    fn secondary_text_scale_keeps_the_body_line_box() {
        let body = scene(r#"{"kind":"text","id":"t","text":"Secondary","modifiers":[{"font":{"size":17,"weight":"regular"}}]}"#, (400.0, 80.0));
        let sec = scene(
            r#"{"kind":"text","id":"t","text":"Secondary","modifiers":[{"font":{"size":17,"weight":"regular"}},{"textScale":"secondary"}]}"#,
            (400.0, 80.0),
        );
        near(by_id(&sec, "t").bounds.h, by_id(&body, "t").bounds.h);
        let bw = by_id(&body, "t").bounds.w;
        let sw = by_id(&sec, "t").bounds.w;
        assert!(sw < bw * 0.92 && sw > bw * 0.78, "secondary width {sw} vs body {bw}");
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
    fn page_tab_view_uses_the_selected_page_and_sends_a_selection_action_on_swipe() {
        let s = phone(r#"{"kind":"pagetabview","id":"p","selected":1,"indexDisplay":"never","pages":[
            {"action":10,"content":{"kind":"text","id":"first","text":"First"}},
            {"action":11,"content":{"kind":"text","id":"second","text":"Second"}},
            {"action":12,"content":{"kind":"text","id":"third","text":"Third"}}]}"#);
        let pager = by_id(&s, "p");
        let second = by_id(&s, "second").bounds;
        near(second.x + second.w / 2.0, 201.0);
        near(second.y + second.h / 2.0, 451.0);
        assert!(s.nodes.iter().all(|n| n.kind != "page-index"));

        let mut inter = crate::Interaction::default();
        inter.version = 7;
        let drag = |dx, dy| onesim_protocol::PointerEvent {
            kind: onesim_protocol::PointerKind::Drag, x: pager.bounds.x + 250.0, y: 300.0, dx, dy,
            duration: None, scale: None, degrees: None,
        };
        assert_eq!(inter.pointer(&s, drag(-260.0, 0.0)), crate::Effect::Action { id: 12, version: 7 });
        assert!(inter.scroll.is_empty(), "the guest selection determines the next resting page");
        assert!(!matches!(inter.pointer(&s, drag(-15.0, -260.0)), crate::Effect::Action { .. }));
        assert_eq!(inter.pointer(&s, drag(260.0, 0.0)), crate::Effect::Action { id: 10, version: 7 });
    }

    #[test]
    fn page_tab_view_index_taps_and_visibility() {
        let tree = |count, display, background| {
            let pages: Vec<_> = (0..count).map(|i| serde_json::json!({
                "content": {"kind":"text","id": format!("page-{i}"),"text": format!("Page {i}")}, "action": i + 1,
            })).collect();
            serde_json::json!({"kind":"pagetabview","id":"p","pages":pages,"indexDisplay":display,"indexBackground":background}).to_string()
        };
        let automatic = phone(&tree(1, "automatic", "always"));
        assert!(automatic.nodes.iter().all(|n| n.kind != "page-index"));
        let always = phone(&tree(1, "always", "always"));
        assert!(by_id(&always, "p#index").fill.is_some());
        let several = phone(&tree(3, "automatic", "never"));
        assert!(by_id(&several, "p#index").fill.is_none());
        let dot = by_id(&several, "p#index-2").bounds;
        let click = onesim_protocol::PointerEvent::new(onesim_protocol::PointerKind::Click, dot.x + dot.w / 2.0, dot.y + dot.h / 2.0);
        let mut inter = crate::Interaction::default();
        assert_eq!(inter.pointer(&several, click), crate::Effect::Action { id: 3, version: 0 });
    }

    #[test]
    fn horizontal_page_swipe_inside_a_sheet_selects_the_next_page() {
        let s = phone(r#"{"kind":"presentation","id":"modal","content":{"kind":"empty","id":"under"},"layers":[
            {"kind":"sheet","detents":[{"kind":"medium"}],"content":{"kind":"pagetabview","id":"p","pages":[
                {"action":10,"content":{"kind":"text","id":"a","text":"First"}},
                {"action":11,"content":{"kind":"text","id":"b","text":"Second"}}]}}]}"#);
        let bounds = by_id(&s, "p").bounds;
        let (x, y) = (bounds.x + bounds.w * 0.8, bounds.y + bounds.h * 0.5);
        let mut inter = crate::Interaction::default();
        inter.drag(&s, crate::DragPhase::Begin, x, y, 0.0);
        assert_eq!(inter.drag(&s, crate::DragPhase::Move, x - bounds.w * 0.6, y, 16.0), crate::Effect::Redraw);
        assert_eq!(inter.drag(&s, crate::DragPhase::End, x - bounds.w * 0.6, y, 32.0), crate::Effect::Action { id: 11, version: 0 });
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
        // Debug layout frames are large (the match keeps every arm's locals). 30 stacks overflow the
        // test thread's 2 MB stack; the check is time, not that limit.
        let t = std::time::Instant::now();
        let s = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || scene(&json, (393.0, 852.0)))
            .unwrap()
            .join()
            .unwrap();
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
                device: crate::device::device("iphone-17-pro").unwrap(),
                scale: 3.0,
                appearance: Appearance::Dark,
                dynamic_type: DynamicTypeSize::Large,
                ios: IosVersion::Ios26,
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
        // Unknown names fall back to an outlined placeholder and are reported.
        let bad = by_id(&s, "bad.i");
        assert!(bad.symbol.is_none() && bad.stroke.is_some());
        assert!(s.unsupported_symbols.contains(&"no.such.symbol.xyz".to_string()));
        assert_eq!(s.unsupported_symbols.len(), if cap == "placeholder" { 5 } else { 1 });
        if cap == "placeholder" {
            return;
        }
        // AppKit symbol boxes vary with the host macOS release. Check layout against the active
        // symbol metrics, using the expected point size, weight and scale for each image.
        let metrics = |name: &str, scale| {
            symbols::metrics(&SymbolSpec { name: name.into(), point_size: 17.0, weight: 400, scale, ..SymbolSpec::default() }).unwrap()
        };
        let star = by_id(&s, "lab.i");
        assert!(star.symbol.is_some());
        let c = star.bounds.center();
        near(c.0, 44.0);
        near(star.bounds.w, metrics("star.fill", ImageScale::Large).w);
        near(by_id(&s, "lab.t").bounds.x, 72.0);
        // accent icon, primary title; an explicit foreground colors both (here secondary on the bad row)
        assert_eq!(star.symbol.as_ref().unwrap().color, rgba(0, 136, 255, 1.0));
        assert_eq!(by_id(&s, "lab.t").text.as_ref().unwrap().color, rgba(0, 0, 0, 1.0));
        assert_eq!(by_id(&s, "bad.i").stroke.map(|s| s.0), by_id(&s, "bad.t").text.as_ref().map(|t| t.color));
        let heart = by_id(&s, "h.0").bounds;
        let heart_metrics = metrics("heart.fill", ImageScale::Medium);
        near(heart.w, heart_metrics.w);
        near(heart.h, heart_metrics.h);
        near(by_id(&s, "h.2").bounds.w, metrics("checkmark.circle.fill", ImageScale::Large).w);
        // resizable scaledToFit keeps the symbol's aspect inside the 44pt frame
        let cloud = by_id(&s, "h.1").bounds;
        let cloud_metrics = metrics("cloud.sun.fill", ImageScale::Medium);
        near(cloud.w.max(cloud.h), 44.0);
        assert!((cloud.w / cloud.h - cloud_metrics.w / cloud_metrics.h).abs() < 0.001, "{cloud:?}");
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
        use onesim_protocol::{PointerEvent, PointerKind};
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
    fn section_q_controls_layout() {
        let t = |id: &str, s: &str| format!(r#"{{"kind":"text","id":"{id}","text":"{s}"}}"#);
        let json = format!(
            r#"{{"kind":"vstack","id":"v","spacing":0,"children":[
                {{"kind":"button","id":"on","action":1,"selected":true,"label":{}}},
                {{"kind":"button","id":"off","action":2,"selected":false,"label":{}}},
                {{"kind":"gauge","id":"lc","style":"linearCapacity","value":0.5,"label":{}}},
                {{"kind":"gauge","id":"al","style":"accessoryLinear","value":0.5}},
                {{"kind":"gauge","id":"alc","style":"accessoryLinearCapacity","value":0.25,"label":{}}},
                {{"kind":"gauge","id":"acc","style":"accessoryCircularCapacity","value":0.6,"currentValueLabel":{}}},
                {{"kind":"gauge","id":"ac","style":"accessoryCircular","value":0.3,"currentValueLabel":{},"label":{}}},
                {{"kind":"gauge","id":"dg","value":0.4,"label":{},"minimumValueLabel":{},"maximumValueLabel":{},"currentValueLabel":{}}},
                {{"kind":"progress","id":"pv","value":0.3,"label":{},"currentValueLabel":{}}},
                {{"kind":"picker","id":"wh","style":"wheel","label":{},"options":[
                    {{"text":"6:00","action":10}},{{"text":"7:00","action":11}},{{"text":"8:00","action":12}},{{"text":"9:00","action":13}},
                    {{"text":"10:00","action":14}},{{"text":"11:00","action":15}},{{"text":"12:00","action":16}}],"selected":3}}
            ]}}"#,
            t("on.l", "Bold"), t("off.l", "Bold"), t("lc.l", "Disk"), t("alc.l", "CPU"), t("acc.c", "60"), t("ac.c", "30"), t("ac.l", "Arc"),
            t("dg.l", "Battery"), t("dg.min", "0"), t("dg.max", "100"), t("dg.c", "40%"), t("pv.l", "Download"), t("pv.c", "30%"), t("wh.l", "Hour"),
        );
        let s = scene(&json, (362.0, 1400.0));
        // `.toggleStyle(.button)`: the same capsule metrics on and off; only the selected one is tinted.
        let (on, off) = (by_id(&s, "on"), by_id(&s, "off"));
        near(on.bounds.h, off.bounds.h);
        assert!(on.fill.is_some() && off.fill.is_none());
        assert_eq!((on.role, on.value.as_deref(), off.value.as_deref()), (Role::Toggle, Some("on"), Some("off")));
        // Bars measured on iOS 27.2 (swiftui-controls indicators): capacity 16pt, accessory 8pt / 4pt, full width.
        near(by_id(&s, "lc#track").bounds.h, 16.0);
        near(by_id(&s, "lc#track").bounds.w, 362.0);
        near(by_id(&s, "lc#bar").bounds.w, 181.0);
        near(by_id(&s, "al#track").bounds.h, 8.0);
        near(by_id(&s, "al#marker").bounds.center().0, 181.0);
        near(by_id(&s, "alc#track").bounds.h, 4.0);
        near(by_id(&s, "acc").bounds.w, 58.0);
        near(by_id(&s, "acc").bounds.h, 58.0);
        near(by_id(&s, "ac").bounds.w, 58.0);
        assert_eq!(by_id(&s, "acc").value.as_deref(), Some("0.6"));
        // Default gauge: min / max labels beside a capacity bar (8pt gaps), current value under it.
        let dg = by_id(&s, "dg#track");
        near(dg.bounds.h, 16.0);
        assert!(by_id(&s, "dg.c").bounds.y > dg.bounds.y + dg.bounds.h);
        assert!(by_id(&s, "dg.min").bounds.x + by_id(&s, "dg.min").bounds.w + 8.0 <= dg.bounds.x + 0.5);
        // currentValueLabel sits under a linear progress bar.
        assert!(by_id(&s, "pv.c").bounds.y > by_id(&s, "pv#track").bounds.y);
        // Wheel: the selected row sits in the highlight band at the centre; other rows are tappable.
        let wh = by_id(&s, "wh");
        near(by_id(&s, "wh#band").bounds.center().1, wh.bounds.center().1);
        near(by_id(&s, "wh#opt3").bounds.center().1, wh.bounds.center().1);
        assert_eq!((by_id(&s, "wh#opt3").action, by_id(&s, "wh#opt4").action), (None, Some(14)));
        assert_eq!(wh.value.as_deref(), Some("9:00"));
    }

    #[test]
    fn menu_layer_centres_on_its_anchor() {
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"vstack","id":"v","children":[
                {"kind":"button","id":"m","action":1,"label":{"kind":"text","id":"m.l","text":"Actions"}}]},
              "layers":[{"kind":"menu","anchor":"m","centered":true,"dismiss":7,"items":[
                {"text":"Duplicate","action":2,"systemImage":"plus.square.on.square"},
                {"text":"Rename","action":3,"systemImage":"pencil"},
                {"text":"Delete","action":4,"systemImage":"trash","destructive":true,"divider":true}]}]}"#,
        );
        let (m, card) = (by_id(&s, "m"), by_id(&s, "menu"));
        near(card.bounds.center().0, m.bounds.center().0);
        // iOS 27.2: three rows of 42pt, a 21pt gap at the divider, 10pt padding (166.5pt), top 10pt above the anchor's centre.
        near(card.bounds.h, 3.0 * 42.0 + 21.0 + 20.0);
        near(card.bounds.y, m.bounds.center().1 - 10.5);
        assert_eq!(by_id(&s, "menu#2").action, Some(4));
        assert!(by_id(&s, "menu#2#icon").symbol.is_some());
        assert!(s.nodes.iter().any(|n| n.id == "menu#sep2"));
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

    // --- SwiftUI gestures (protocol section L)

    use crate::{DragPhase, Effect, Interaction};
    use onesim_protocol::{GestureTarget, GestureTouch, PointerEvent, PointerKind, TouchPhase};

    fn touches(e: Effect) -> Vec<GestureTouch> {
        match e {
            Effect::Gesture(t) => t,
            e => panic!("expected touches, got {e:?}"),
        }
    }

    fn phases(t: &[GestureTouch]) -> Vec<TouchPhase> {
        t.iter().map(|t| t.phase).collect()
    }

    fn keys(t: &GestureTouch) -> Vec<&str> {
        t.targets.iter().map(|t| t.key.as_str()).collect()
    }

    /// A 100 x 100 square moved right by `.offset(x: 20)`, with gestures (`pans`), centred in 300 x 300.
    fn square(pans: bool) -> Scene {
        scene(
            &format!(r#"{{"kind":"shape","id":"sq","shape":"rectangle","modifiers":[{{"frame":{{"width":100,"height":100,"alignment":"center"}}}},{{"offset":{{"x":20,"y":0}}}},{{"gesture":{{"key":"g","pans":{pans}}}}}]}}"#),
            (300.0, 300.0),
        )
    }

    #[test]
    fn offset_moves_the_view_and_its_gesture_frame() {
        let s = square(true);
        let b = by_id(&s, "sq").bounds;
        assert_eq!((b.x, b.y, b.w, b.h), (120.0, 100.0, 100.0, 100.0));
        let g = s.nodes.iter().find(|n| n.gesture.is_some()).expect("gesture node");
        assert_eq!((g.bounds.x, g.bounds.y), (120.0, 100.0));
        assert!(s.ui_tree().children.iter().all(|c| c.kind != "gesture"), "layout-only node stays out of inspection");
    }

    #[test]
    fn drag_on_a_gesture_view_is_one_touch_sequence() {
        let s = square(true);
        let mut it = Interaction::default();
        let t = touches(it.pointer(&s, PointerEvent::moved(PointerKind::Drag, 170.0, 150.0, 30.0, 0.0)));
        let p = phases(&t);
        assert_eq!((p[0], *p.last().unwrap()), (TouchPhase::Start, TouchPhase::End));
        assert!(p[1..p.len() - 1].iter().all(|&p| p == TouchPhase::Move) && p.len() >= 3);
        assert_eq!(t[0].points, vec![[170.0, 150.0]]);
        assert_eq!(t.last().unwrap().points, vec![[200.0, 150.0]]);
        assert!(t.windows(2).all(|w| w[1].time > w[0].time), "time runs forward");
        for e in &t {
            assert_eq!(e.targets, vec![GestureTarget { key: "g".into(), frame: [120.0, 100.0, 100.0, 100.0] }]);
        }
        assert_eq!(it.hit.as_deref(), Some("sq#gesture2"));
        // A click is a start and an end at one point; a miss sends nothing.
        let t = touches(it.pointer(&s, PointerEvent::new(PointerKind::Click, 130.0, 110.0)));
        assert_eq!(phases(&t), vec![TouchPhase::Start, TouchPhase::End]);
        assert_eq!(it.pointer(&s, PointerEvent::new(PointerKind::Click, 10.0, 10.0)), Effect::None);
    }

    #[test]
    fn control_inside_a_gesture_view_owns_its_touches() {
        let s = scene(
            r#"{"kind":"zstack","id":"z","alignment":"center","modifiers":[{"gesture":{"key":"o","pans":true}}],"children":[
                {"kind":"color","id":"bg","color":{"semantic":"accent"},"modifiers":[{"frame":{"width":100,"height":100,"alignment":"center"}}]},
                {"kind":"shape","id":"in","shape":"circle","modifiers":[{"frame":{"width":20,"height":20,"alignment":"center"}},{"offset":{"x":-30,"y":-30}},{"gesture":{"key":"i","pans":false}}]},
                {"kind":"button","id":"b","action":7,"label":{"kind":"color","id":"bl","color":{"semantic":"primary"},"modifiers":[{"frame":{"width":40,"height":40,"alignment":"center"}}]}}]}"#,
            (300.0, 300.0),
        );
        let mut it = Interaction::default();
        it.version = 2;
        assert_eq!(it.pointer(&s, PointerEvent::new(PointerKind::Click, 150.0, 150.0)), Effect::Action { id: 7, version: 2 });
        let t = touches(it.pointer(&s, PointerEvent::new(PointerKind::Click, 190.0, 190.0)));
        assert_eq!(keys(&t[0]), vec!["o"]);
        // Nested gesture views: innermost first.
        let t = touches(it.pointer(&s, PointerEvent::new(PointerKind::Click, 120.0, 120.0)));
        assert_eq!(keys(&t[0]), vec!["i", "o"]);
        assert_eq!(t[0].targets[0].frame, [110.0, 110.0, 20.0, 20.0]);
    }

    #[test]
    fn pinch_rotate_and_long_press_synthesize_fingers() {
        let s = square(true);
        let mut it = Interaction::default();
        let pinch = PointerEvent { scale: Some(2.0), ..PointerEvent::new(PointerKind::Pinch, 170.0, 150.0) };
        let t = touches(it.pointer(&s, pinch));
        assert_eq!(t[0].points, vec![[120.0, 150.0], [220.0, 150.0]]);
        assert_eq!(t.last().unwrap().points, vec![[70.0, 150.0], [270.0, 150.0]]);
        assert_eq!(t.last().unwrap().phase, TouchPhase::End);

        let rotate = PointerEvent { degrees: Some(90.0), ..PointerEvent::new(PointerKind::Rotate, 170.0, 150.0) };
        let t = touches(it.pointer(&s, rotate));
        let end = &t.last().unwrap().points;
        for (got, want) in end.iter().zip([[170.0, 100.0], [170.0, 200.0]]) {
            assert!((got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9, "{end:?}");
        }

        let press = PointerEvent { duration: Some(0.8), ..PointerEvent::new(PointerKind::LongPress, 150.0, 150.0) };
        let t = touches(it.pointer(&s, press));
        assert_eq!((t[0].phase, t.last().unwrap().phase), (TouchPhase::Start, TouchPhase::End));
        assert!((t.last().unwrap().time - t[0].time - 800.0).abs() < 1e-9);
        assert!(t.iter().all(|e| e.points == vec![[150.0, 150.0]]), "the finger never moves");
        assert!(t.len() > 3, "held fingers keep reporting, so a long press can fire before the lift");
    }

    #[test]
    fn live_touch_holds_and_moves_with_the_finger() {
        let s = square(true);
        let mut it = Interaction::default();
        assert_eq!(phases(&touches(it.drag(&s, DragPhase::Begin, 150.0, 150.0, 1000.0))), vec![TouchPhase::Start]);
        let held = touches(it.hold(0.1));
        assert_eq!((held[0].phase, held[0].time, held[0].points.clone()), (TouchPhase::Move, 1100.0, vec![[150.0, 150.0]]));
        let t = touches(it.drag(&s, DragPhase::Move, 160.0, 150.0, 1116.0));
        assert_eq!((t[0].phase, t[0].points.clone()), (TouchPhase::Move, vec![[160.0, 150.0]]));
        assert_eq!(touches(it.drag(&s, DragPhase::End, 160.0, 150.0, 1130.0))[0].phase, TouchPhase::End);
        assert_eq!(it.hold(0.1), Effect::None, "no finger, no hold");
    }

    #[test]
    fn scroller_takes_a_moving_touch_from_a_tap_only_gesture() {
        let json = |pans: bool| {
            format!(
                r#"{{"kind":"scroll","id":"sc","axis":"vertical","child":{{"kind":"vstack","id":"v","alignment":"center","spacing":0,"children":[
                    {{"kind":"color","id":"c","color":{{"semantic":"accent"}},"modifiers":[{{"frame":{{"height":2000,"alignment":"center"}}}},{{"gesture":{{"key":"c","pans":{pans}}}}}]}}]}}}}"#
            )
        };
        let s = scene(&json(false), (300.0, 300.0));
        let mut it = Interaction::default();
        touches(it.drag(&s, DragPhase::Begin, 150.0, 200.0, 0.0));
        let t = touches(it.drag(&s, DragPhase::Move, 150.0, 170.0, 16.0));
        assert_eq!(phases(&t), vec![TouchPhase::Cancel]);
        it.drag(&s, DragPhase::Move, 150.0, 150.0, 32.0);
        assert_eq!(it.scroll.get("sc").map(|o| o.1), Some(50.0), "the scroller follows from where the touch began");

        let s = scene(&json(true), (300.0, 300.0));
        let mut it = Interaction::default();
        touches(it.drag(&s, DragPhase::Begin, 150.0, 200.0, 0.0));
        assert_eq!(phases(&touches(it.drag(&s, DragPhase::Move, 150.0, 150.0, 16.0))), vec![TouchPhase::Move]);
        assert!(it.scroll.get("sc").is_none_or(|o| o.1 == 0.0), "a drag gesture keeps the touch");
    }

    fn color(id: &str, frame: &str, extra: &str) -> String {
        format!(
            r#"{{"kind":"color","id":"{id}","color":{{"r":1,"g":0,"b":0,"a":1}},"modifiers":[{frame}{extra}]}}"#
        )
    }

    #[test]
    fn grid_sizes_columns_from_cells() {
        let cell = |id, w| color(id, &format!(r#"{{"frame":{{"width":{w},"height":36}}}}"#), "");
        let e = color("e", r#"{"frame":{"width":100,"height":36}}"#, r#",{"gridCellColumns":2}"#);
        let json = format!(
            r#"{{"kind":"grid","id":"g","alignment":"leading","horizontalSpacing":8,"verticalSpacing":8,"rows":[
                {{"cells":[{},{},{}]}},
                {{"cells":[{},{}]}}]}}"#,
            cell("a", 64),
            cell("b", 64),
            cell("c", 64),
            cell("d", 64),
            e
        );
        let s = scene(&json, (400.0, 400.0));
        let (a, b, c, d, e) = (by_id(&s, "a").bounds, by_id(&s, "b").bounds, by_id(&s, "c").bounds, by_id(&s, "d").bounds, by_id(&s, "e").bounds);
        assert_eq!(b.x - a.x, 72.0);
        assert_eq!(c.x - b.x, 72.0);
        assert_eq!(e.x - d.x, 72.0);
        assert_eq!(e.w, 100.0);
        assert_eq!(d.w, 64.0);
        assert_eq!(d.y - a.y, 44.0);
        assert_eq!(by_id(&s, "g").bounds.w, 208.0);
    }

    #[test]
    fn lazy_vgrid_flexible_tracks() {
        let kids: Vec<String> = (0..4).map(|i| color(&format!("c{i}"), r#"{"frame":{"height":36}}"#, "")).collect();
        let json = format!(
            r#"{{"kind":"lazyGrid","id":"g","axis":"vertical","spacing":8,"tracks":[{{"size":"flexible"}},{{"size":"flexible"}}],"children":[{}]}}"#,
            kids.join(",")
        );
        let s = scene(&json, (200.0, 400.0));
        let (c0, c1, c2) = (by_id(&s, "c0").bounds, by_id(&s, "c1").bounds, by_id(&s, "c2").bounds);
        assert_eq!(c1.x - c0.x, 104.0);
        assert_eq!(c2.y - c0.y, 44.0);
        assert_eq!(c0.w, 96.0);
        assert_eq!(c0.h, 36.0);
    }

    #[test]
    fn lazy_vgrid_adaptive_track() {
        let kids: Vec<String> = (0..3).map(|i| color(&format!("c{i}"), r#"{"frame":{"height":10}}"#, "")).collect();
        let json = format!(
            r#"{{"kind":"lazyGrid","id":"g","axis":"vertical","tracks":[{{"size":"adaptive","min":50}}],"children":[{}]}}"#,
            kids.join(",")
        );
        let s = scene(&json, (200.0, 400.0));
        let (c0, c1, c2) = (by_id(&s, "c0").bounds, by_id(&s, "c1").bounds, by_id(&s, "c2").bounds);
        let w = 184.0 / 3.0;
        near(c0.w, w);
        near(c1.x - c0.x, w + 8.0);
        near(c2.x - c1.x, w + 8.0);
        near(c1.y, c0.y);
        near(c2.y, c0.y);
    }

    #[test]
    fn view_that_fits_picks_the_first_child_that_fits() {
        let json = format!(
            r#"{{"kind":"viewThatFits","id":"f","axes":"both","children":[{},{}]}}"#,
            color("wide", r#"{"frame":{"width":200,"height":10}}"#, ""),
            color("narrow", r#"{"frame":{"width":40,"height":10}}"#, "")
        );
        let s = scene(&json, (100.0, 200.0));
        assert!(s.nodes.iter().all(|n| n.id != "wide"));
        assert!(s.nodes.iter().any(|n| n.id == "narrow"));
        assert_eq!(by_id(&s, "f").bounds.w, 40.0);
    }

    #[test]
    fn view_that_fits_rejects_text_that_only_fits_by_wrapping() {
        let json = r#"{"kind":"viewThatFits","id":"f","axes":"horizontal","children":[
            {"kind":"text","id":"long","text":"Wide layout does not fit this row"},
            {"kind":"text","id":"short","text":"Compact"}
        ]}"#;
        let s = scene(json, (80.0, 40.0));
        assert!(s.nodes.iter().all(|n| n.id != "long"), "wrapped text was treated as fitting");
        assert!(s.nodes.iter().any(|n| n.id == "short"));
    }

    #[test]
    fn geometry_reader_fills_a_finite_proposal() {
        let json = format!(
            r#"{{"kind":"geometry","id":"g","proposal":[10.0,10.0],"child":{}}}"#,
            color("c", "", "")
        );
        let s = scene(&json, (120.0, 80.0));
        let g = by_id(&s, "g");
        assert_eq!((g.bounds.x, g.bounds.y, g.bounds.w, g.bounds.h), (0.0, 0.0, 120.0, 80.0));
        assert_eq!(g.proposal, Some([10.0, 10.0]));
        let c = by_id(&s, "c").bounds;
        assert_eq!((c.x, c.y, c.w, c.h), (0.0, 0.0, 120.0, 80.0));
    }

    #[test]
    fn geometry_reader_in_a_scroll_keeps_the_guest_height() {
        let json = format!(
            r#"{{"kind":"scroll","id":"s","child":{{"kind":"geometry","id":"g","proposal":[10.0,40.0],"child":{}}}}}"#,
            color("c", "", "")
        );
        let s = scene(&json, (200.0, 100.0));
        let g = by_id(&s, "g");
        assert_eq!((g.bounds.w, g.bounds.h), (200.0, 40.0));
        assert_eq!(g.proposal, Some([10.0, 40.0]));
    }

    #[test]
    fn scroll_content_margins_inset_the_child() {
        let json = r#"{"kind":"scroll","id":"s","modifiers":[{"contentMargins":{"top":5,"leading":10,"bottom":7,"trailing":10}}],
            "child":{"kind":"color","id":"c","color":{"r":1,"g":0,"b":0,"a":1}}}"#;
        let s = scene(json, (200.0, 100.0));
        let c = by_id(&s, "c").bounds;
        assert_eq!((c.x, c.y, c.w, c.h), (10.0, 5.0, 180.0, 10.0));
    }

    #[test]
    fn paging_scroll_records_the_page_length() {
        let json = format!(
            r#"{{"kind":"scroll","id":"s","modifiers":[{{"scrollTargetBehavior":"paging"}}],"child":{}}}"#,
            color("c", r#"{"frame":{"height":350}}"#, "")
        );
        let s = scene(&json, (100.0, 100.0));
        let info = by_id(&s, "s").scroll.unwrap();
        assert_eq!(info.page, Some(100.0));
        assert_eq!(info.max.1, 250.0);
    }

    #[test]
    fn view_aligned_marks_direct_children() {
        let kids: Vec<String> = ["a", "b", "c"].iter().map(|id| color(id, r#"{"frame":{"width":80,"height":20}}"#, "")).collect();
        let json = format!(
            r#"{{"kind":"scroll","id":"s","axis":"horizontal","modifiers":[{{"scrollTargetBehavior":"viewAligned"}}],
                "child":{{"kind":"hstack","id":"h","spacing":0,"modifiers":[{{"scrollTargetLayout":true}}],"children":[{}]}}}}"#,
            kids.join(",")
        );
        let s = scene(&json, (200.0, 80.0));
        let info = by_id(&s, "s").scroll.unwrap();
        assert!(info.aligned);
        assert!(info.page.is_none());
        assert!(by_id(&s, "a").scroll_target && by_id(&s, "b").scroll_target && by_id(&s, "c").scroll_target);
    }

    #[test]
    fn identity_sets_the_anchor() {
        let s = scene(r#"{"kind":"text","id":"t","text":"Row","modifiers":[{"id":"row"}]}"#, (200.0, 80.0));
        assert_eq!(by_id(&s, "t").anchor.as_deref(), Some("row"));
    }

    #[test]
    fn pinned_headers_stick_and_push() {
        let header = |id| {
            color(id, r#"{"pin":"header"}"#, r#",{"frame":{"height":20,"maxWidth":"inf"}}"#)
        };
        let json = format!(
            r#"{{"kind":"scroll","id":"s","child":{{"kind":"vstack","id":"v","spacing":0,"alignment":"leading","children":[{},{},{},{}]}}}}"#,
            header("h1"),
            color("c1", r#"{"frame":{"height":400}}"#, ""),
            header("h2"),
            color("c2", r#"{"frame":{"height":400}}"#, "")
        );
        let at = |off: f64| {
            let mut m = HashMap::new();
            m.insert("s".to_string(), (0.0, off));
            scene_with(&json, (200.0, 300.0), false, (50.0, 0.0), &m, None)
        };
        let s = at(0.0);
        assert_eq!(by_id(&s, "h1").bounds.y, 50.0);
        assert_eq!(by_id(&s, "h2").bounds.y, 470.0);
        let s = at(30.0);
        assert_eq!(by_id(&s, "h1").bounds.y, 50.0);
        assert_eq!(by_id(&s, "h2").bounds.y, 440.0);
        let s = at(410.0);
        assert_eq!(by_id(&s, "h1").bounds.y, 40.0);
    }

    #[test]
    fn pinned_headers_stick_on_a_horizontal_scroll() {
        let header = |id| color(id, r#"{"pin":"header"}"#, r#",{"frame":{"width":20,"height":20}}"#);
        let json = format!(
            r#"{{"kind":"scroll","id":"s","axis":"horizontal","child":{{"kind":"hstack","id":"h","spacing":0,"alignment":"top","children":[{},{},{},{}]}}}}"#,
            header("h1"),
            color("c1", r#"{"frame":{"width":400,"height":20}}"#, ""),
            header("h2"),
            color("c2", r#"{"frame":{"width":400,"height":20}}"#, "")
        );
        let at = |off: f64| {
            let mut m = HashMap::new();
            m.insert("s".to_string(), (off, 0.0));
            scene_with(&json, (300.0, 80.0), false, (0.0, 0.0), &m, None)
        };
        let s = at(0.0);
        assert_eq!(by_id(&s, "h1").bounds.x, 0.0);
        assert_eq!(by_id(&s, "h2").bounds.x, 420.0);
        let s = at(30.0);
        assert_eq!(by_id(&s, "h1").bounds.x, 0.0);
        assert_eq!(by_id(&s, "h2").bounds.x, 390.0);
        let s = at(410.0);
        assert_eq!(by_id(&s, "h1").bounds.x, -10.0);
    }

    #[test]
    fn matched_geometry_aligns_non_source_to_source() {
        let json = r#"{
            "kind": "zstack", "id": "z", "alignment": "topLeading",
            "children": [
                {"kind": "shape", "id": "pill", "shape": "capsule",
                 "modifiers": [{"matchedGeometry": {"id": "tab", "namespace": "ns", "properties": "frame", "anchor": [0.5, 0.5], "isSource": false}}]},
                {"kind": "hstack", "id": "h", "spacing": 10, "alignment": "center", "children": [
                    {"kind": "color", "id": "a", "color": {"r": 1, "g": 0, "b": 0, "a": 1}, "modifiers": [{"frame": {"width": 60, "height": 30}}]},
                    {"kind": "color", "id": "b", "color": {"r": 0, "g": 1, "b": 0, "a": 1},
                     "modifiers": [{"frame": {"width": 80, "height": 30}}, {"matchedGeometry": {"id": "tab", "namespace": "ns", "isSource": true}}]}
                ]}
            ]
        }"#;
        let s = scene(json, (200.0, 100.0));
        assert_eq!(by_id(&s, "pill").bounds, by_id(&s, "b").bounds);
    }

    #[test]
    fn animation_and_content_transition_propagate_and_disable() {
        let json = r#"{
            "kind": "vstack", "id": "v",
            "modifiers": [{"animation": {"curve": "linear", "duration": 0.4, "token": 3}}, {"contentTransition": "numericTextUp"}],
            "children": [
                {"kind": "text", "id": "a", "text": "1"},
                {"kind": "text", "id": "b", "text": "2", "modifiers": [{"animation": null}]}
            ]
        }"#;
        let s = scene(json, (200.0, 100.0));
        let a = by_id(&s, "a").anim.as_ref().expect("a inherits animation");
        assert_eq!(a.animation.as_ref().unwrap().token, 3);
        assert_eq!(a.content_transition, ContentTransitionKind::NumericTextUp);
        let b = by_id(&s, "b").anim.as_ref().expect("b inherits contentTransition");
        assert!(b.animation.is_none(), "animation(nil) disables outer animation");
        assert_eq!(b.content_transition, ContentTransitionKind::NumericTextUp);
    }
}
