use super::*;

pub(super) const INF: f64 = f64::INFINITY;
pub(super) const DEFAULT_SPACING: f64 = 8.0;
/// Radius that always clamps to a capsule / circle.
pub(super) const PILL: f64 = 1.0e6;

// Presentation layer (alert / sheet). Estimated from iOS 26 behavior, to be tuned against the simulator
// references (`alert`, `alert-dark`, `sheet`, `sheet-dark` in tests/fixtures/showcase-ios).
/// Black dimming over the presenting screen (light, dark): measured on the simulator.
pub(super) const ALERT_SCRIM: (f32, f32) = (0.20, 0.46);
pub(super) const SHEET_SCRIM: (f32, f32) = (0.20, 0.46);
pub(super) const ALERT_W: f64 = 320.0;
pub(super) const ALERT_RADIUS: f64 = 34.0;
/// Card center sits this far below the screen center.
pub(super) const ALERT_DY: f64 = 14.0;
/// Text side inset, title top, gaps, button metrics.
pub(super) const ALERT_TEXT_PAD: f64 = 30.0;
pub(super) const ALERT_TOP: f64 = 22.0;
pub(super) const ALERT_TITLE_MESSAGE_GAP: f64 = 7.33;
pub(super) const ALERT_TEXT_BUTTON_GAP: f64 = 20.33;
pub(super) const ALERT_BTN_H: f64 = 48.0;
pub(super) const ALERT_BTN_GAP: f64 = 8.0;
pub(super) const ALERT_BTN_PAD: f64 = 16.0;
/// Alert message size (pt, scales with nothing: measured 18pt line).
pub(super) const ALERT_MESSAGE_PT: f32 = 15.0;
/// Button fill (light, dark) and gray the presenter's prominent buttons turn while an alert is up.
// Alert message: iOS draws a vibrant gray, darker than .secondary over the card (sampled from alert(-dark).png).
/// React Native `ActionSheetIOS` message, light (measured on iOS 27.2; dark: `ALERT_MESSAGE_FG`).
pub(super) const SHEET_MESSAGE_FG: (u8, u8, u8) = (135, 135, 135);
pub(super) const ALERT_MESSAGE_FG: ((u8, u8, u8), (u8, u8, u8)) = ((106, 107, 108), (162, 162, 162));
pub(super) const ALERT_BTN_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((209, 209, 210), (47, 47, 48));
pub(super) const ALERT_DESAT: (u8, u8) = (160, 115);
/// iOS 27 dark alert (alert-dark.png, ios27): lighter buttons, dimmer message, pinker destructive label.
pub(super) const ALERT_27_DARK_BTN: (u8, u8, u8) = (71, 71, 73);
pub(super) const ALERT_27_DARK_MESSAGE: (u8, u8, u8) = (120, 121, 124);
pub(super) const ALERT_27_DARK_DESTRUCTIVE: (u8, u8, u8) = (255, 103, 106);
/// iOS 27 dark confirmation popover (dialog-dark.png, ios27).
pub(super) const POPOVER_27_DARK_BTN: (u8, u8, u8) = (80, 80, 82);
pub(super) const POPOVER_27_DARK_MESSAGE: (u8, u8, u8) = (128, 130, 134);
pub(super) const POPOVER_27_DARK_DESTRUCTIVE: (u8, u8, u8) = (255, 109, 112);
/// Confirmation popover (measured on the simulator).
/// Glass (mul, add) of the popover on a light backdrop, and its wide soft shadow (alpha, sigma, dy).
pub(super) const POPOVER_GLASS: (f32, f32) = (0.196, 0.76);
pub(super) const POPOVER_SHADOW: (f32, f32, f32) = (0.16, 40.0, 12.0);
/// Glass (mul, add) of a floating sheet (light, dark).
pub(super) const SHEET_GLASS: ((f32, f32), (f32, f32)) = ((0.43, 0.585), (0.35, 0.118));
pub(super) const POPOVER_W: f64 = 240.0;
pub(super) const POPOVER_RADIUS: f64 = 40.0;
pub(super) const POPOVER_ARROW_W: f64 = 30.0;
pub(super) const POPOVER_ARROW_H: f64 = 12.5;
/// Arrow tip sits this far inside the anchor.
pub(super) const POPOVER_TIP_INSET: f64 = 5.0;
pub(super) const POPOVER_MARGIN: f64 = 8.0;
/// Length of the arrow when the popover sits beside its anchor.
pub(super) const POPOVER_SIDE_ARROW: f64 = 12.5;
pub(super) const POPOVER_BTN_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((214, 214, 215), (48, 49, 49));
pub(super) const POPOVER_ARROW: ((u8, u8, u8), (u8, u8, u8)) = ((242, 242, 244), (17, 20, 18));
/// Medium detent height as a fraction of the screen (floating card top at y 415 on 874).
pub(super) const SHEET_MEDIUM: f64 = 0.525;
/// A sheet below its large detent floats: inset from the sides and bottom, glass fill.
pub(super) const SHEET_FLOAT_INSET: f64 = 8.0;
pub(super) const SHEET_FLOAT_RADIUS: f64 = 36.0;
/// White alpha (light, dark) of the back button's disc while a sheet is up (measured: 195 on 194, 13 on 0).
pub(super) const BACK_FLAT: (f32, f32) = (0.017, 0.05);
/// A floating sheet shows the large-sheet layout (402 wide) scaled by this, about its top center (the card is 386 wide).
pub(super) const SHEET_FLOAT_SCALE: f64 = 0.96;
/// Inset-grouped card color inside a floating sheet (light, dark).
pub(super) const FLOAT_CARD: ((u8, u8, u8), (u8, u8, u8)) = ((222, 222, 222), (46, 46, 47));
/// Row separator inside a floating sheet's card (light, dark).
pub(super) const FLOAT_SEP: ((u8, u8, u8), (u8, u8, u8)) = ((189, 189, 189), (74, 74, 76));
/// A sheet presented from a sheet sits this much lower per level.
pub(super) const SHEET_STACK_STEP: f64 = 10.0;
/// Grabber colors (light, dark).
pub(super) const GRABBER_FG: ((u8, u8, u8), (u8, u8, u8)) = ((177, 177, 180), (103, 103, 108));
/// Drag indicator: size and distance from the sheet's top.
pub(super) const GRABBER: (f64, f64) = (34.7, 5.0);
pub(super) const GRABBER_Y: f64 = 4.67;
/// Gap between the safe-area top and the sheet's top edge (large detent).
pub(super) const SHEET_TOP_GAP: f64 = 0.0;
pub(super) const SHEET_RADIUS: f64 = 40.0;
/// Backdrop blur (sigma, pt) under floating sheets, alert and dialog glass.
pub(super) const PRESENT_BLUR: f32 = 12.0;
/// iOS 27 dark alert / popover cards: darker than plain dark glass (alert-dark.png: card ~(35..43) over the dimmed screen).
pub(super) const PRESENT_GLASS_27_DARK: (f32, f32) = (0.49, 0.125);
/// Space above the sheet content (a list starts this far below the sheet's top edge).
pub(super) const SHEET_CONTENT_TOP: f64 = 35.0;

// Form controls (iOS 26.5, measured on `form*.png` in tests/fixtures/showcase-ios).
/// Menu picker: chevron.up.chevron.down (pt, weight), gap after the value, trailing inset.
pub(super) const PICKER_CHEVRON: (f64, u16) = (13.0, 600);
pub(super) const PICKER_CHEVRON_GAP: f64 = 5.0;
pub(super) const PICKER_TRAIL_INSET: f64 = 0.0;
/// Segmented control: height, thumb inset, track fill alpha, font (pt, weight, selected weight).
pub(super) const SEGMENTED_H: f64 = 31.0;
pub(super) const SEGMENTED_INSET: f64 = 2.0;
pub(super) const SEGMENTED_TRACK_A: f32 = 0.12;
pub(super) const SEGMENTED_FONT: (f32, u16, u16) = (13.0, 500, 600);
pub(super) const SEGMENTED_THUMB_DARK: (u8, u8, u8) = (105, 105, 111);
/// SecureField bullet: diameter, pitch, first bullet's x in the field (pt).
pub(super) const SECURE_BULLET: (f64, f64, f64) = (7.33, 10.22, 1.67);
/// Slider: thumb (w, h), track thickness and fill alpha, thumb shadow (alpha, radius, dy).
pub(super) const SLIDER_THUMB: (f64, f64) = (37.0, 24.0);
pub(super) const SLIDER_H: f64 = 31.0;
pub(super) const SLIDER_TRACK: f64 = 6.0;
/// Unfilled slider track (light, dark).
pub(super) const SLIDER_TRACK_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((230, 230, 230), (50, 50, 52));
pub(super) const SLIDER_SHADOW: (f32, f64, f64) = (0.15, 3.0, 1.0);
/// Linear progress: bar thickness, track alpha, gap under a label; spinner box.
pub(super) const PROGRESS_H: f64 = 4.0;
/// Linear progress track (light, dark).
pub(super) const PROGRESS_TRACK_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((228, 228, 230), (61, 61, 65));
pub(super) const PROGRESS_LABEL_GAP: f64 = 8.0;
/// Label-to-bar gap of a linear progress view (iOS 27.2, swiftui-controls indicators).
pub(super) const PROGRESS_BAR_LABEL_GAP: f64 = 4.67;
pub(super) const SPINNER: f64 = 20.0;
/// Picker menu: row height, vertical padding, text x, right padding, min width, radius,
/// checkmark (pt, weight) and centre x, offset from the anchor, glass mix (light, dark).
pub(super) const MENU_ROW_H: f64 = 42.0;
pub(super) const MENU_PAD_V: f64 = 10.0;
pub(super) const MENU_TEXT_X: f64 = 43.7;
pub(super) const MENU_PAD_R: f64 = 24.0;
pub(super) const MENU_MIN_W: f64 = 250.0;
pub(super) const MENU_RADIUS: f64 = 26.0;
pub(super) const MENU_CHECK: (f64, u16) = (13.0, 600);
pub(super) const MENU_CHECK_X: f64 = 27.67;
pub(super) const MENU_DX: f64 = 0.0;
/// Half the height of the picker's hit area: the card's bottom lines up with it.
pub(super) const MENU_DY: f64 = 17.17;
pub(super) const MENU_GLASS: ((f32, f32), (f32, f32)) = ((0.196, 0.76), (0.35, 0.118));
/// `Menu` card (centred on its button): icon centre and text x when items carry icons, the gap a divider
/// opens between items, separator side inset, and how far the card's top sits above the button's centre.
pub(super) const MENU_ICON_X: f64 = 39.5;
pub(super) const MENU_ICON_TEXT_X: f64 = 63.2;
pub(super) const MENU_DIVIDER_GAP: f64 = 21.0;
pub(super) const MENU_SEP_INSET: f64 = 24.0;
pub(super) const MENU_CENTER_DY: f64 = 10.5;

// Section Q controls (iOS 27.2, measured on tests/fixtures/ios27/swiftui-controls-ios).
/// `.toggleStyle(.button)` when on: accent alpha of its capsule.
pub(super) const TOGGLE_BUTTON_ON_A: f32 = 0.16;
/// Gap between a linear progress bar and its currentValueLabel (caption, secondary).
pub(super) const PROGRESS_VALUE_GAP: f64 = 4.0;
pub(super) const PROGRESS_VALUE_PT: f32 = 12.0;
/// Gauge: capacity bar and accessory bar heights; label-to-bar and bar-to-value gaps.
pub(super) const GAUGE_BAR: f64 = 16.0;
pub(super) const GAUGE_ACC_BAR: f64 = 8.0;
pub(super) const GAUGE_LABEL_GAP: f64 = 10.4;
pub(super) const GAUGE_VALUE_GAP: f64 = 6.3;
pub(super) const GAUGE_ACC_LABEL_GAP: f64 = 4.0;
/// Accessory circular gauges: box, stroke, track alpha, open arc half-angle (degrees from the top),
/// marker cut-out and dot diameters (linear / circular), value and label sizes.
pub(super) const GAUGE_RING: f64 = 58.0;
pub(super) const GAUGE_RING_W: f64 = 5.7;
pub(super) const GAUGE_RING_TRACK_A: f32 = 0.35;
pub(super) const GAUGE_ARC_ANGLE: f64 = 140.0;
pub(super) const GAUGE_MARKER: (f64, f64) = (16.0, 7.33);
pub(super) const GAUGE_ARC_MARKER: (f64, f64) = (11.0, 5.5);
pub(super) const GAUGE_RING_PT: f32 = 25.0;
pub(super) const GAUGE_ARC_LABEL_PT: f32 = 11.0;
/// Wheel picker: height, (offset from the centre, vertical squash) per row away from the selection,
/// font size, highlight band (height, side inset, fill), alpha of the unselected rows.
pub(super) const WHEEL_H: f64 = 216.0;
pub(super) const WHEEL_ROWS: [(f64, f64); 4] = [(0.0, 1.0), (30.0, 0.915), (56.0, 0.745), (75.2, 0.45)];
pub(super) const WHEEL_PT: f32 = 22.0;
pub(super) const WHEEL_BAND: (f64, f64) = (32.0, 16.67);
pub(super) const WHEEL_BAND_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((244, 244, 245), (58, 58, 60));
pub(super) const WHEEL_DIM_A: f32 = 0.45;
/// Multi-column wheel (`DatePicker`, section S): a fourth row each side, the band inset from the wheel's edges,
/// columns this far apart, and out-of-range options at this alpha (measured on iOS 27.2, swiftui-pickers).
/// Rows sit on a drum: radius `h * k + c` (89.5 pt for a 216 pt wheel, 74 for 172), one row per `pitch` of arc;
/// a row at angle t is `r sin t` from the centre and squashed to `cos t`. Alpha by distance from the selection.
/// The selection is 23.5 pt, the other rows 20.5 pt.
pub(super) const WHEEL_DRUM: (f64, f64, f64) = (0.352, 12.6, 32.0);
pub(super) const WHEEL_COLUMN_ALPHA: [f32; 5] = [1.0, 0.29, 0.265, 0.125, 0.06];
pub(super) const WHEEL_COLUMN_PT: (f32, f32) = (23.5, 20.5);
pub(super) const WHEEL_POPOVER_BAND_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((229, 229, 231), (70, 70, 74));
pub(super) const WHEEL_COLUMN_BAND_INSET: f64 = 9.0;
pub(super) const WHEEL_COLUMN_GAP: f64 = 5.0;
pub(super) const WHEEL_OUT_OF_RANGE_A: f32 = 0.2;
/// Popover holding a view (compact `DatePicker`): its top this far under the anchor, at least this far from the
/// screen's sides, corner radius (iOS 27.2, swiftui-pickers pickers-date / pickers-time).
pub(super) const CONTENT_POPOVER_GAP: f64 = 8.67;
pub(super) const CONTENT_POPOVER_MARGIN: (f64, f64) = (16.0, 32.33);
pub(super) const CONTENT_POPOVER_RADIUS: f64 = 30.0;
/// Its glass is nearly white: (mul, add) of the backdrop.
pub(super) const CONTENT_POPOVER_GLASS: ((f32, f32), (f32, f32)) = ((0.2, 0.795), (0.35, 0.118));
/// The card's trailing edge lines up with the anchor's (this far past it) when the card fits that way.
pub(super) const CONTENT_POPOVER_TRAILING: f64 = 0.67;
/// `.popover` bubble (iOS 27.2 iPhone 18 Pro, `popover-bubble`): content plus 7.83 pt above and below, radius 26,
/// the arrow's tip 0.33 pt past the anchor.
pub(super) const BUBBLE_PAD_V: f64 = 7.83;
pub(super) const BUBBLE_RADIUS: f64 = 26.0;
pub(super) const BUBBLE_TIP_GAP: f64 = 0.33;
/// Share sheet (`share`; per-release frames in `look::ShareLook`).
pub(super) const SHARE_RADIUS: f64 = 44.0;
pub(super) const SHARE_GLASS: ((f32, f32), (f32, f32)) = ((0.45, 0.55), (0.35, 0.118));
pub(super) const SHARE_BLUR: f32 = 20.0;
/// (alpha, radius, dy)
pub(super) const SHARE_TILE_SHADOW: (f32, f64, f64) = (0.07, 8.0, 2.0);
pub(super) const SHARE_TILE: ((u8, u8, u8), (u8, u8, u8)) = ((255, 255, 255), (58, 58, 60));
pub(super) const SHARE_TILE_SIZE: f64 = 64.0;
pub(super) const SHARE_TILE_RADIUS: f64 = 16.0;
pub(super) const SHARE_TILE_GLYPH: f64 = 30.0;
pub(super) const SHARE_TITLE_PT: f32 = 15.0;
pub(super) const SHARE_APP_RADIUS: f64 = 15.0;
pub(super) const SHARE_MORE_GLYPH: f64 = 32.0;
pub(super) const SHARE_MORE_FG: ((u8, u8, u8), (u8, u8, u8)) = ((99, 99, 102), (174, 174, 178));
pub(super) const SHARE_ACTION_FILL: ((u8, u8, u8), (u8, u8, u8)) = ((230, 230, 232), (58, 58, 60));
pub(super) const SHARE_ACTION_GLYPH: f64 = 26.0;
pub(super) const SHARE_LABEL_PT: f32 = 12.0;
pub(super) const SHARE_SAFARI_GLYPH: f64 = 30.0;
pub(super) const SHARE_GLASSES_GLYPH: f64 = 30.0;
/// A `SharePreview` title sits this much lower (negative: higher) than an item's own text (`share-link`).
pub(super) const SHARE_TITLED_DY: f64 = -1.0;
/// `PasteButton` (`popovers`): 13.33 pt each side of the label, 34.67 tall, radius 5.
pub(super) const PASTE_PAD: (f64, f64) = (13.33, 34.67);
pub(super) const PASTE_RADIUS: f64 = 5.0;
/// Its gray under a presented layer (light, dark; `popover-bubble`).
pub(super) const PASTE_DESAT: (u8, u8) = (133, 140);
/// `.contextMenu` (`context-menu`): the view lifts by 1.15 about its centre, the menu card hangs 16 pt under it,
/// the screen dims (light / dark).
pub(super) const CONTEXT_LIFT: f64 = 1.15;
pub(super) const CONTEXT_MENU_GAP: f64 = 16.0;
pub(super) const CONTEXT_DIM: (f32, f32) = (0.20, 0.46);
/// A custom `preview` sits on a system-background platter (`context-preview`).
pub(super) const CONTEXT_PLATTER_RADIUS: f64 = 28.0;

// iOS 26 measurements (points).
pub(super) const ROW_MIN_H: f64 = 52.0;
pub(super) const ROW_VPAD: f64 = 10.0;
/// Rows holding bordered buttons get extra vertical padding (measured: 34.3pt button -> 64.3pt row).
pub(super) const ROW_VPAD_BORDERED: f64 = 15.0;
pub(super) const CARD_RADIUS: f64 = 26.0;
pub(super) const SECTION_GAP: f64 = 17.7;
/// Extra gap above a header-less section that follows another (35pt in total).
pub(super) const HEADERLESS_EXTRA_GAP: f64 = 17.3;
pub(super) const HEADER_PAD: f64 = 20.0;
/// Footer text this far under its section, and the footer cell this far under the text (29.67pt for one line).
pub(super) const FOOTER_TOP: f64 = 8.0;
pub(super) const FOOTER_BOTTOM: f64 = 6.0;
/// Plain and inset lists: gap above each section (and below the last) (iOS 27.2, swiftui-forms plain / inset).
pub(super) const PLAIN_SECTION_GAP: f64 = 22.0;
/// Collapsible (sidebar) section header chevron: point size and weight (iOS 27.2, swiftui-forms sidebar).
pub(super) const SECTION_CHEVRON: (f64, u16) = (15.0, 600);
/// Section footer text size.
pub(super) const FOOTER_PT: f32 = 13.0;
/// Edit mode: width of the reorder strip at a row's trailing edge (over the handle).
pub(super) const REORDER_STRIP: f64 = 56.0;
/// Swipe actions (iOS 27.2, swiftui-forms swipe-trailing / swipe-leading): pills inset 4.33pt from the row's top and
/// bottom, 10.33pt from the card's edge and apart, labels 13pt medium; every pill on an edge is as wide as the widest
/// label plus 13.17pt a side.
pub(super) const SWIPE_PILL_INSET: f64 = 4.33;
pub(super) const SWIPE_EDGE: f64 = 10.33;
pub(super) const SWIPE_GAP: f64 = 10.33;
pub(super) const SWIPE_FONT: f32 = 13.0;
pub(super) const SWIPE_PILL_PAD: f64 = 13.17;
pub(super) const SWIPE_PILL_RADIUS: f64 = 16.0;
/// Gap between a swipe action's symbol and its title.
pub(super) const SWIPE_ICON_GAP: f64 = 5.0;
/// Swipe action fills without a `.tint` (light, dark): `Color.red` for a destructive role, else `Color.gray`.
pub(super) const SWIPE_DESTRUCTIVE: ((u8, u8, u8), (u8, u8, u8)) = ((255, 56, 60), (255, 66, 69));
pub(super) const SWIPE_DEFAULT: ((u8, u8, u8), (u8, u8, u8)) = ((142, 142, 147), (142, 142, 147));
/// A swiped row's highlight (light, dark).
pub(super) const SWIPED_ROW: ((u8, u8, u8), (u8, u8, u8)) = ((229, 229, 234), (44, 44, 46));
pub(super) const NAV_INLINE_H: f64 = 54.0;
/// Scroll distance over which the inline title fades in as the large title finishes collapsing.
pub(super) const INLINE_FADE: f64 = 12.0;
/// Switch knob size (inset 2pt from the track edge).
pub(super) const STEPPER: (f64, f64) = (94.0, 32.0);
/// `Label` in a list row: icon centred in a leading column of this width, title starts after it.
/// Measured: icon centred 12pt in, title at +40 (column 24 + gap 16), icons at imageScale large.
pub(super) const LABEL_ICON_COL: f64 = 24.0;
pub(super) const LABEL_ROW_GAP: f64 = 16.0;
/// Icon-to-title gap of a `Label` outside a list (not measured).
pub(super) const LABEL_GAP: f64 = 8.0;

/// The optional views of a `Gauge`.
#[derive(Clone, Copy)]
pub(super) struct GaugeLabels<'a> {
    pub(super) label: Option<&'a ViewNode>,
    pub(super) current: Option<&'a ViewNode>,
    pub(super) min: Option<&'a ViewNode>,
    pub(super) max: Option<&'a ViewNode>,
}

/// Screen geometry shared by the root and the presentation layers.
#[derive(Clone, Copy)]
pub(super) struct Geo {
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) safe_top: f64,
    /// Bottom inset including the keyboard (the root avoids it).
    pub(super) safe_bottom: f64,
    /// Device bottom inset (presented content ignores the keyboard).
    pub(super) dev_bottom: f64,
    pub(super) body: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Prop {
    pub(super) w: Option<f64>,
    pub(super) h: Option<f64>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Size {
    pub(super) w: f64,
    pub(super) h: f64,
}

/// Relative-positioned layout result. `node` (if any) becomes a scene node.
#[derive(Clone)]
pub(super) struct Lay {
    pub(super) size: Size,
    /// Boxed: layout recursion holds many `Lay`s per frame and `Node` is large.
    pub(super) node: Option<Box<Node>>,
    pub(super) kids: Vec<(f64, f64, Lay)>,
    /// Hit/paint bounds growth (left, top, right, bottom) beyond `size`.
    pub(super) infl: (f64, f64, f64, f64),
    pub(super) embedded: Option<Embedded>,
    pub(super) shared_kids: Option<(f64, Arc<ListContent>)>,
    /// Child that is the modified view when `kids` also holds a decoration (background, overlay, mask).
    pub(super) content_child: Option<u8>,
}

impl Lay {
    pub(super) fn new(size: Size, node: Option<Node>, kids: Vec<(f64, f64, Lay)>) -> Self {
        Self { size, node: node.map(Box::new), kids, infl: (0.0, 0.0, 0.0, 0.0), embedded: None, shared_kids: None, content_child: None }
    }
}

/// `child` is the modified view; the other children are decoration and paint around it.
pub(super) fn with_content(mut lay: Lay, child: u8) -> Lay {
    lay.content_child = Some(child);
    lay
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Env {
    pub(super) fg: Option<Color>,
    pub(super) foreground_paint: Option<usize>,
    pub(super) secondary_paint: Option<usize>,
    pub(super) tertiary_paint: Option<usize>,
    pub(super) background_paint: Option<usize>,
    pub(super) tint_paint: Option<usize>,
    /// Axis of the directly enclosing stack (true = vertical), for Spacer/Divider.
    pub(super) axis: Option<bool>,
    pub(super) size: f32,
    pub(super) base_weight: u16,
    pub(super) weight: Option<u16>,
    pub(super) strike: bool,
    pub(super) align: TextAlign,
    /// Accent tint in effect (automatic buttons recolour their label content).
    pub(super) tint: Option<Color>,
    /// Top/bottom safe-area insets for full-bleed scrollers (consumed by list/scroll).
    pub(super) safe_top: f64,
    pub(super) bottom_inset: f64,
    /// `imageScale` in effect for symbols.
    pub(super) img: ImageScale,
    /// Laid out as (part of) a list row: `Label` uses the fixed icon column and accent icon.
    pub(super) in_row: bool,
    pub(super) in_toolbar: bool,
    /// Active animation index into `Ctx::anims` (section P); `None` = no animation.
    pub(super) anim: Option<u32>,
    /// Inherited `.contentTransition` (section P).
    pub(super) content_transition: ContentTransitionKind,
    /// Kerning + tracking, in points.
    pub(super) extra: f32,
    /// 0 default, 1 serif, 2 rounded, 3 monospaced.
    pub(super) design: u8,
    /// -2 compressed, -1 condensed, 0 standard, 1 expanded.
    pub(super) width: i8,
    pub(super) mono: bool,
    pub(super) digits: bool,
    /// `Text.Scale.secondary`.
    pub(super) secondary: bool,
    /// 0 tail, 1 head, 2 middle.
    pub(super) trunc: u8,
    pub(super) baseline: f32,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) under_color: Option<Color>,
    pub(super) under_pat: u8,
    pub(super) strike_color: Option<Color>,
    pub(super) strike_pat: u8,
    /// 0 none, 1 uppercase, 2 lowercase.
    pub(super) case_fold: u8,
    pub(super) tighten: bool,
    pub(super) min_scale: f32,
    pub(super) max_lines: Option<usize>,
    pub(super) reserve_lines: Option<usize>,
    /// 0 monochrome, 1 hierarchical, 2 palette, 3 multicolor.
    pub(super) sym_mode: u8,
    /// 0 automatic, 1 color, 2 draw.
    pub(super) sym_var: u8,
    pub(super) sym_grad: bool,
    /// `.renderingMode(.original)`.
    pub(super) sym_original: bool,
    /// 0 nearest, 1 bilinear, 2 bicubic.
    pub(super) img_filter: u8,
}


pub(super) fn design_code(d: onesim_protocol::FontDesign) -> u8 {
    match d {
        onesim_protocol::FontDesign::Default => 0,
        onesim_protocol::FontDesign::Serif => 1,
        onesim_protocol::FontDesign::Rounded => 2,
        onesim_protocol::FontDesign::Monospaced => 3,
    }
}

pub(super) fn width_code(w: onesim_protocol::FontWidth) -> i8 {
    match w {
        onesim_protocol::FontWidth::Compressed => -2,
        onesim_protocol::FontWidth::Condensed => -1,
        onesim_protocol::FontWidth::Standard => 0,
        onesim_protocol::FontWidth::Expanded => 1,
    }
}

pub(super) fn trunc_code(m: onesim_protocol::Truncation) -> u8 {
    match m {
        onesim_protocol::Truncation::Tail => 0,
        onesim_protocol::Truncation::Head => 1,
        onesim_protocol::Truncation::Middle => 2,
    }
}

pub(super) fn pattern_code(p: onesim_protocol::LinePattern) -> u8 {
    match p {
        onesim_protocol::LinePattern::Solid => 0,
        onesim_protocol::LinePattern::Dot => 1,
        onesim_protocol::LinePattern::Dash => 2,
        onesim_protocol::LinePattern::DashDot => 3,
        onesim_protocol::LinePattern::DashDotDot => 4,
    }
}

pub(super) fn case_code(c: onesim_protocol::TextCase) -> u8 {
    match c {
        onesim_protocol::TextCase::Uppercase => 1,
        onesim_protocol::TextCase::Lowercase => 2,
    }
}

pub(super) fn symbol_mode_code(m: onesim_protocol::SymbolMode) -> u8 {
    match m {
        onesim_protocol::SymbolMode::Monochrome => 0,
        onesim_protocol::SymbolMode::Hierarchical => 1,
        onesim_protocol::SymbolMode::Palette => 2,
        onesim_protocol::SymbolMode::Multicolor => 3,
    }
}

pub(super) fn pull_tracking(spec: &FontSpec, spans: &[Span], pull: f32) -> (FontSpec, Vec<Span>) {
    let mut spec = *spec;
    spec.extra -= pull;
    let spans = spans.iter().map(|s| Span { extra: s.extra - pull, ..s.clone() }).collect();
    (spec, spans)
}

pub(super) fn fold_label(text: &str, spans: Vec<Span>, case_fold: u8) -> (String, Vec<Span>) {
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

pub(super) fn h_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Leading | TopLeading | BottomLeading => 0.0,
        Trailing | TopTrailing | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

pub(super) fn v_frac(a: Alignment) -> f64 {
    use Alignment::*;
    match a {
        Top | TopLeading | TopTrailing => 0.0,
        Bottom | BottomLeading | BottomTrailing => 1.0,
        _ => 0.5,
    }
}

pub(super) fn first_text(v: &ViewNode) -> Option<String> {
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
        ViewKind::Geometry { child, .. } | ViewKind::VisualEffect { child, .. } => first_text(child),
        _ => None,
    }
}

/// Corner radius of whatever a modifier chain painted last (for shadows).
pub(super) fn lay_radius(l: &Lay) -> f64 {
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
pub(super) fn layer(id: String, kind: &'static str, inner: Lay, f: impl FnOnce(&mut Node)) -> Lay {
    let mut n = Node::new(id, kind);
    f(&mut n);
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Wrap `inner` in a gesture view's layout-only node. Out of line so `mods`, which recurses once per modifier,
/// keeps a small stack frame.
#[inline(never)]
pub(super) fn gesture_layer(id: String, g: &onesim_protocol::GestureMod, inner: Lay) -> Lay {
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
pub(super) fn anim_layer(
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
        layout: false,
        removal: None,
    }));
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Wrap `inner` in a `.containerShape(_:)` layout-only node (out of line to keep `mods` stack small).
/// How far `.offset` wrappers (node-less, one same-sized child) move `l`'s content.
pub(super) fn moved_by(l: &Lay) -> (f64, f64) {
    match l.kids.as_slice() {
        [(dx, dy, k)] if l.node.is_none() && (k.size.w, k.size.h) == (l.size.w, l.size.h) => {
            let (x, y) = moved_by(k);
            (dx + x, dy + y)
        }
        _ => (0.0, 0.0),
    }
}

/// x offset of the first text inside `l` (separator inset), relative to `l`'s origin.
pub(super) fn first_text_x(l: &Lay, ox: f64) -> Option<f64> {
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
pub(super) fn expand_button(l: &mut Lay, ox: f64, oy: f64, row: (f64, f64, f64, f64)) -> bool {
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
pub(super) fn has_bordered(v: &ViewNode) -> bool {
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
        ViewKind::Geometry { child, .. } | ViewKind::VisualEffect { child, .. } => has_bordered(child),
        _ => false,
    }
}

/// Lists drawn on the grouped background (the navigation bar matches it).
pub(super) fn is_list(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { style, .. } if on_grouped_background(style))
}

/// List styles drawn on the grouped background (the others on the plain one).
pub(super) fn on_grouped_background(style: ListStyle) -> bool {
    matches!(style, ListStyle::InsetGrouped | ListStyle::Grouped | ListStyle::Sidebar)
}

/// First SF Symbol name inside a view (a swipe action's `Label` icon).
pub(super) fn first_symbol(v: &ViewNode) -> Option<String> {
    match &v.kind {
        ViewKind::Image { system_name, .. } if !system_name.is_empty() => Some(system_name.clone()),
        ViewKind::Label { icon, .. } => first_symbol(icon),
        ViewKind::HStack { children, .. } | ViewKind::VStack { children, .. } | ViewKind::ZStack { children, .. } => children.iter().find_map(first_symbol),
        _ => None,
    }
}

/// A list row's `listRow` modifier (section T).
pub(super) fn list_row(row: &ViewNode) -> Option<&onesim_protocol::ListRowMod> {
    row.modifiers.iter().rev().find_map(|m| if let Modifier::ListRow(l) = m { Some(&**l) } else { None })
}

pub(super) fn is_scroller(v: &ViewNode) -> bool {
    matches!(v.kind, ViewKind::List { .. } | ViewKind::Scroll { .. })
}

/// First scene node of `l`: itself, or the single child of a node-less wrapper (frame, padding).
pub(super) fn first_node(l: &mut Lay) -> Option<&mut Node> {
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
pub(super) fn direct_target(l: &mut Lay) -> Option<&mut Node> {
    if let Some(n) = l.node.as_deref_mut() {
        return Some(n);
    }
    if l.kids.len() == 1 {
        l.kids[0].2.node.as_deref_mut()
    } else {
        None
    }
}

pub(super) fn content_margins(mods: &[Modifier]) -> Edges {
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

/// A row's own minimum height (`rowMinHeight`), else the list default.
pub(super) fn row_min_height(row: &ViewNode) -> f64 {
    row.modifiers.iter().rev().find_map(|m| if let Modifier::RowMinHeight(h) = m { Some(*h) } else { None }).unwrap_or(ROW_MIN_H)
}

pub(super) fn cell_columns(cell: &ViewNode) -> usize {
    let mut span = 1usize;
    for m in &cell.modifiers {
        if let Modifier::GridCellColumns(n) = m {
            span = (*n).max(1) as usize;
        }
    }
    span
}

pub(super) fn slot_width(cols: &[f64], col: usize, span: usize, gap: f64) -> f64 {
    let end = (col + span).min(cols.len());
    if col >= end {
        return 0.0;
    }
    cols[col..end].iter().sum::<f64>() + gap * (end - col - 1) as f64
}


pub(super) fn fits_axis(size: f64, prop: Option<f64>) -> bool {
    prop.filter(|p| p.is_finite()).is_none_or(|p| size <= p + 0.5)
}

/// A corner radius scaled by `k` (pill/circle radii stay pills).
pub(super) fn scaled_radius(x: f64, k: f64) -> f64 {
    if x >= PILL / 2.0 {
        x
    } else {
        x * k
    }
}



pub(super) fn greedy(p: Prop) -> Size {
    Size { w: p.w.unwrap_or(10.0), h: p.h.unwrap_or(10.0) }
}

/// Proposal passed to the child of a frame modifier along one axis.
pub(super) fn child_prop(prop: Option<f64>, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> Option<f64> {
    match (fixed, prop) {
        (Some(f), _) => Some(f),
        (None, Some(p)) => Some(p.clamp(min.unwrap_or(0.0), max.unwrap_or(INF).max(min.unwrap_or(0.0)))),
        (None, None) => None,
    }
}

/// Resulting frame length along one axis. A `max: .infinity` frame fills the proposal.
pub(super) fn frame_len(prop: Option<f64>, child: f64, fixed: Option<f64>, min: Option<f64>, max: Option<f64>) -> f64 {
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

impl Env {
    pub(super) fn new() -> Self {
        Env {
            fg: None,
            foreground_paint: None,
            secondary_paint: None,
            tertiary_paint: None,
            background_paint: None,
            tint_paint: None,
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
            in_toolbar: false,
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

