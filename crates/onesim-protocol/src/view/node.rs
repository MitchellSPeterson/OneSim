use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use crate::{ShapePaint, VisualOp};
use super::color::Color;
use super::modifier::{
    Alignment, ContentMode, DynamicRange, GestureMod, ImageScale, Interpolation, Modifier, SymbolColorMode,
    SymbolMode, SymbolVariable, TemplateMode,
};
use super::shape::{ShapeExtra, ShapeKind};
use super::text::{Run, TextInputBorderShape, TextInputOptions, TextInputStyle};
use super::{inf, is_false, is_zero, one};

/// A bitmap `Image` (AsyncImage success, or a file the guest already loaded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bitmap {
    pub path: String,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    /// Pixels per point. `0` means 1.
    #[serde(default)]
    pub scale: f64,
}

/// `ButtonRole`: `cancel` / `destructive` (alerts order and color buttons by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonRole {
    Cancel,
    Destructive,
}

/// `PresentationDetent`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Detent {
    Medium,
    Large,
    /// Fraction of the large detent's height.
    Fraction { value: f64 },
    /// Height in points.
    Height { value: f64 },
}

/// One presented layer, bottom to top in `presentation.layers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Layer {
    /// `.sheet`. `detents` ascending by height (empty = large only); the first is the initial one.
    Sheet {
        content: Box<ViewNode>,
        #[serde(default)]
        detents: Vec<Detent>,
        #[serde(default, rename = "dragIndicator")]
        drag_indicator: bool,
        /// Guest action that clears the binding (swipe-to-dismiss); `None` when dismissal is disabled.
        #[serde(default)]
        dismiss: Option<u64>,
        /// `presentationBackground`: drawn behind the content, filling the card (replaces the system background).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<Box<ViewNode>>,
        /// `presentationCornerRadius` (points); `None` keeps the system radius.
        #[serde(default, rename = "cornerRadius", skip_serializing_if = "Option::is_none")]
        corner_radius: Option<f64>,
        /// `presentationBackgroundInteraction`.
        #[serde(default, rename = "backgroundInteraction", skip_serializing_if = "Option::is_none")]
        background_interaction: Option<BackgroundInteraction>,
        /// `presentationContentInteraction`.
        #[serde(default, rename = "contentInteraction", skip_serializing_if = "Option::is_none")]
        content_interaction: Option<ContentInteraction>,
    },
    /// `.fullScreenCover`.
    Cover { content: Box<ViewNode> },
    /// `.alert`; `actions` are `button` nodes in source order.
    Alert {
        title: String,
        #[serde(default)]
        message: Option<Box<ViewNode>>,
        #[serde(default)]
        actions: Vec<ViewNode>,
    },
    /// `.confirmationDialog`: a popover on iPhone, anchored to the view it is attached to.
    Dialog {
        #[serde(default)]
        title: String,
        #[serde(default, rename = "titleVisible")]
        title_visible: bool,
        #[serde(default)]
        message: Option<Box<ViewNode>>,
        #[serde(default)]
        actions: Vec<ViewNode>,
        /// Id of the node the popover points at.
        #[serde(default)]
        anchor: Option<String>,
        /// Guest action that clears the binding (tap outside the popover).
        #[serde(default)]
        dismiss: Option<u64>,
        /// The dialog's own appearance (React Native `ActionSheetIOS` `userInterfaceStyle`); `None` follows the screen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        appearance: Option<crate::Appearance>,
    },
    /// A glass card holding `content`, hanging under the anchor (a compact `DatePicker`'s calendar or time wheel,
    /// section S); `dismiss` closes it (tap outside).
    Popover {
        #[serde(default)]
        anchor: Option<String>,
        content: Box<ViewNode>,
        #[serde(default)]
        dismiss: Option<u64>,
        /// `.popover` with `presentationCompactAdaptation(.popover)`: a bubble with an arrow at the anchor.
        #[serde(default, skip_serializing_if = "is_false")]
        arrow: bool,
        /// `attachmentAnchor` other than the anchor's bounds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attachment: Option<Attachment>,
        /// `arrowEdge`: the popover's edge that points at the anchor (`top`, `leading`, `bottom`, `trailing`).
        #[serde(default, rename = "arrowEdge", skip_serializing_if = "Option::is_none")]
        arrow_edge: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<Box<ViewNode>>,
    },
    /// `.contextMenu`: the anchor lifted (or `preview`) over a blurred screen, with the menu by it; `dismiss`
    /// closes it (tap outside).
    #[serde(rename = "contextMenu")]
    ContextMenu {
        #[serde(default)]
        anchor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview: Option<Box<ViewNode>>,
        #[serde(default)]
        items: Vec<PickerOption>,
        #[serde(default)]
        dismiss: Option<u64>,
    },
    /// `ShareLink`'s share sheet. `copy` puts the items on the pasteboard and closes it; `dismiss` closes it.
    Share {
        #[serde(default)]
        anchor: Option<String>,
        #[serde(default)]
        items: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// `SharePreview` image and icon, as views (drawn in the header tile when they are bitmaps).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<Box<ViewNode>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<Box<ViewNode>>,
        /// Every item is a `URL`: the sheet offers the link activities (Safari, Add to Reading List).
        #[serde(default, skip_serializing_if = "is_false")]
        url: bool,
        #[serde(default)]
        dismiss: Option<u64>,
        #[serde(default)]
        copy: Option<u64>,
        /// Guest actions of the other activities by name (`Reminders`, `More`, `Save to Files`): each reports the
        /// items as a `module.called` event and closes the sheet.
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        activities: std::collections::BTreeMap<String, u64>,
        /// The sheet's own appearance (React Native `userInterfaceStyle`); `None` follows the screen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        appearance: Option<crate::Appearance>,
    },
    /// Menu of a menu-style `Picker` or a `Menu`, anchored to it; `dismiss` closes it (tap outside).
    Menu {
        #[serde(default)]
        anchor: Option<String>,
        /// `Menu`: the card is centred on the anchor instead of hanging from its trailing edge (`Picker`).
        #[serde(default, skip_serializing_if = "is_false")]
        centered: bool,
        #[serde(default)]
        items: Vec<PickerOption>,
        #[serde(default)]
        dismiss: Option<u64>,
    },
}

/// `PresentationBackgroundInteraction`: `enabled` leaves the screen under a sheet undimmed and interactive, up
/// through `up_through` (every detent when `None`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BackgroundInteraction {
    Automatic,
    Enabled {
        #[serde(default, rename = "upThrough", skip_serializing_if = "Option::is_none")]
        up_through: Option<Detent>,
    },
    Disabled,
}

/// `PresentationContentInteraction`: `scrolls` makes a drag on a sheet's scrollable content scroll it before
/// resizing the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentInteraction {
    Automatic,
    Resizes,
    Scrolls,
}

/// `PopoverAttachmentAnchor`: a rect in the anchor's space, or a unit point of it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Attachment {
    Rect([f64; 4]),
    Point([f64; 2]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PickerStyle {
    #[default]
    Menu,
    Segmented,
    /// Wheel: the options on a drum, the selected one in a highlight band (section Q).
    Wheel,
}

/// `Gauge` styles (section Q). `Automatic` is the iOS default (a linear capacity bar with its labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GaugeStyle {
    #[default]
    Automatic,
    LinearCapacity,
    AccessoryLinear,
    AccessoryLinearCapacity,
    AccessoryCircular,
    AccessoryCircularCapacity,
}

/// One `Picker` choice or `Menu` item: its label text and the action that selects or runs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PickerOption {
    pub text: String,
    pub action: u64,
    /// Menu layers only: the selected item (drawn with a checkmark).
    #[serde(default, skip_serializing_if = "is_false")]
    pub checked: bool,
    /// Menu layers only: SF Symbol drawn before the text.
    #[serde(default, rename = "systemImage", skip_serializing_if = "Option::is_none")]
    pub system_image: Option<String>,
    /// Menu layers only: drawn in red (`Button(role: .destructive)`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub destructive: bool,
    /// Menu layers only: a separator (`Divider` or a new `Section`) sits above this item.
    #[serde(default, skip_serializing_if = "is_false")]
    pub divider: bool,
    /// Menu layers and wheel columns: drawn dimmed and not tappable (a disabled button, a date out of range).
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

/// One drum of a `wheel` (section S). `options` are the visible rows, `selected` the one in the band; an option
/// with `disabled` is drawn dimmer and does nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WheelColumn {
    #[serde(default)]
    pub options: Vec<PickerOption>,
    #[serde(default)]
    pub selected: usize,
    pub width: f64,
    #[serde(default)]
    pub align: WheelAlign,
    /// Gap between the text and the column edge it is aligned to (leading / trailing alignment only).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub inset: f64,
    /// `inset` of the selected (magnified) row when it differs.
    #[serde(default, rename = "selectedInset", skip_serializing_if = "Option::is_none")]
    pub selected_inset: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WheelAlign {
    Leading,
    #[default]
    Center,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonStyle {
    #[default]
    Automatic,
    Plain,
    Bordered,
    BorderedProminent,
    NavigationLink,
    Glass,
    GlassProminent,
    /// `PasteButton`: the system Paste button.
    PasteButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    #[default]
    Vertical,
    Horizontal,
}

/// Which section views a lazy stack pins (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinMode {
    Headers,
    Footers,
    Both,
}

/// `GridItem` size (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackSize {
    Fixed,
    Flexible,
    Adaptive,
}

/// One `GridItem`. `value` is the fixed size; `min`/`max` bound flexible and adaptive tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridTrack {
    pub size: TrackSize,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "inf")]
    pub spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
}

/// One `GridRow`. `alignment` `None` inherits the grid's alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridLine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    #[serde(default)]
    pub cells: Vec<ViewNode>,
}

/// Axes `ViewThatFits` checks (section M).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitAxes {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    #[default]
    InsetGrouped,
    Grouped,
    Plain,
    Inset,
    Sidebar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// Plain-text header, drawn in the list style's header font.
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    /// Any other header or footer view (section T). Takes the place of `header` / `footer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_view: Option<Box<ViewNode>>,
    /// Any other footer view, in place of `footer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer_view: Option<Box<ViewNode>>,
    /// `Section(isExpanded:)`: collapsible. `false` hides the rows; the header shows a chevron and runs `toggle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    /// Guest action the collapsible header runs (flips `expanded`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<u64>,
    #[serde(default)]
    pub rows: Vec<ViewNode>,
}

/// `listRow` modifier on a list row (section T).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ListRowMod {
    /// Swipe actions and where the row rests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swipe: Option<RowSwipe>,
    /// Gesture on the whole row (its frame is the row's, padding included): the swipe pan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<GestureMod>,
    /// Edit mode: gesture on a strip at the row's trailing edge, the row's height, over its reorder handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorder: Option<GestureMod>,
    /// A row being reordered is drawn this far (pt) below its place, above the other rows.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lift: f64,
}

/// `.swipeActions` (and swipe-to-delete) state of a row. `open` is the edge whose actions show at rest (the row moved
/// by their width); `offset` is the finger's movement on top of that (negative = towards leading). `full`: dragged
/// past the full-swipe point, the outermost action stretches across the row.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RowSwipe {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<HEdge>,
    /// Outermost (leftmost) first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leading: Vec<SwipeAction>,
    /// Outermost (rightmost) first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trailing: Vec<SwipeAction>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub full: bool,
}

/// `HorizontalEdge`: a row's leading or trailing side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HEdge {
    Leading,
    Trailing,
}

/// One swipe action button: its label (text and/or symbol), tint (`role: destructive` is red, otherwise gray when unset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwipeAction {
    /// The button's label: its text, and a `label`'s symbol drawn before it.
    pub label: Box<ViewNode>,
    /// Runs the button (the guest also closes the row).
    pub action: u64,
    /// `.tint` on the button: the pill's fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Color>,
    /// `destructive` fills the pill red when there is no tint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ButtonRole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackButton {
    pub action: u64,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolbarRegion {
    Leading,
    Principal,
    Trailing,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarItem {
    pub region: ToolbarRegion,
    pub content: ViewNode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BarVisibility {
    #[default]
    Automatic,
    Visible,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationSearch {
    pub field: ViewNode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<ViewNode>,
    pub placement: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NavigationChrome {
    pub items: Vec<ToolbarItem>,
    pub navigation_bar_visibility: BarVisibility,
    pub bottom_bar_visibility: BarVisibility,
    pub navigation_bar_background: Option<ShapePaint>,
    pub bottom_bar_background: Option<ShapePaint>,
    pub navigation_bar_background_visibility: BarVisibility,
    pub bottom_bar_background_visibility: BarVisibility,
    pub title_menu: Option<ViewNode>,
    pub search: Option<NavigationSearch>,
}

/// A `tabview` tab bar item; running `action` selects the tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabItem {
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "systemImage", skip_serializing_if = "Option::is_none")]
    pub system_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    pub action: u64,
}

/// A page and the guest action that selects it, if enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabPage {
    pub content: ViewNode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageIndexDisplay {
    #[default]
    Automatic,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageIndexBackground {
    #[default]
    Automatic,
    Always,
    Interactive,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    #[default]
    Large,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ViewKind {
    /// Jetpack Compose layout primitive (section K): `Box`, `Column`, `Row` or `Text`, modifier chain in `props.modifier`.
    Compose {
        component: String,
        #[serde(default)]
        props: Box<serde_json::Value>,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    /// React Native Fabric node (`id` = reactTag). `props` is Fabric's flat attribute payload.
    Rn {
        component: String,
        #[serde(default)]
        props: Box<serde_json::Value>,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Text {
        text: String,
        /// Styled runs of a concatenation; `text` is their plain concatenation. Empty for a plain `Text`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        runs: Vec<Run>,
    },
    VStack {
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
        /// Lazy stack pinning. Absent on an ordinary stack.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pinned: Option<PinMode>,
    },
    HStack {
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pinned: Option<PinMode>,
    },
    ZStack {
        #[serde(default)]
        alignment: Alignment,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Spacer {
        #[serde(default, rename = "minLength")]
        min_length: Option<f64>,
    },
    Button {
        action: u64,
        label: Box<ViewNode>,
        #[serde(default)]
        style: ButtonStyle,
        #[serde(default)]
        disabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<ButtonRole>,
        /// `.toggleStyle(.button)`: the toggle's state. Selected buttons sit in an accent-tinted capsule.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<bool>,
    },
    /// Modal layer above the whole navigation tree; present only while an alert or sheet is shown.
    Presentation {
        content: Box<ViewNode>,
        #[serde(default)]
        layers: Vec<Layer>,
    },
    /// Drawing commands at the Canvas origin, evaluated by the guest for the laid-out size.
    Canvas {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proposal: Option<[f64; 2]>,
        children: Vec<ViewNode>,
        #[serde(default)]
        opaque: bool,
    },
    #[serde(rename = "canvasItem")]
    CanvasItem {
        child: Box<ViewNode>,
        origin: [f64; 2],
        anchor: [f64; 2],
        #[serde(default)]
        size: Option<[f64; 2]>,
    },
    /// Host geometry for a guest visualEffect closure. The child keeps its own layout size.
    #[serde(rename = "visualEffect")]
    VisualEffect {
        child: Box<ViewNode>,
        frame: [f64; 4],
        #[serde(default)]
        effects: Vec<VisualOp>,
    },
    Shape {
        shape: ShapeKind,
        #[serde(flatten)]
        extra: Box<ShapeExtra>,
    },
    Divider,
    Scroll {
        #[serde(default)]
        axis: Axis,
        child: Box<ViewNode>,
        /// `"automatic"` or `"hidden"`. Absent means automatic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        indicators: Option<String>,
    },
    List {
        #[serde(default)]
        style: ListStyle,
        #[serde(default)]
        sections: Vec<Section>,
    },
    Navigation {
        #[serde(default)]
        title: Option<String>,
        #[serde(default, rename = "displayMode")]
        display_mode: DisplayMode,
        #[serde(default)]
        back: Option<Box<BackButton>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chrome: Option<Box<NavigationChrome>>,
        content: Box<ViewNode>,
    },
    /// `TabView`: `content` is the selected tab's (the others are not sent); `selected` indexes `tabs`.
    TabView {
        #[serde(default)]
        selected: usize,
        #[serde(default)]
        tabs: Vec<TabItem>,
        content: Box<ViewNode>,
    },
    /// `.tabViewStyle(.page)`: all pages are sent, and `selected` is the resting page.
    PageTabView {
        #[serde(default)]
        selected: usize,
        #[serde(default)]
        pages: Vec<TabPage>,
        #[serde(default, rename = "indexDisplay")]
        index_display: PageIndexDisplay,
        #[serde(default, rename = "indexBackground")]
        index_background: PageIndexBackground,
    },
    Toggle {
        #[serde(rename = "isOn")]
        is_on: bool,
        action: u64,
        label: Box<ViewNode>,
        #[serde(default)]
        disabled: bool,
    },
    Stepper {
        label: Box<ViewNode>,
        #[serde(default)]
        decrement: Option<u64>,
        #[serde(default)]
        increment: Option<u64>,
    },
    #[serde(rename = "text_field")]
    TextField {
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        text: String,
        field: u64,
        /// `SecureField`: drawn as bullets.
        #[serde(default, skip_serializing_if = "is_false")]
        secure: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        disabled: bool,
        #[serde(flatten)]
        input: Box<TextInputOptions>,
    },
    /// `Picker`; `selected` indexes `options`. Menu style: `open` shows the options as a `menu` layer.
    Picker {
        label: Box<ViewNode>,
        #[serde(default)]
        style: PickerStyle,
        #[serde(default)]
        options: Vec<PickerOption>,
        #[serde(default)]
        selected: Option<usize>,
        #[serde(default)]
        open: Option<u64>,
        #[serde(default)]
        disabled: bool,
    },
    /// `Slider`: the host sets a new value with `text_input` on `field` (the number as text).
    Slider {
        value: f64,
        #[serde(default)]
        min: f64,
        #[serde(default = "one")]
        max: f64,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        field: Option<u64>,
        #[serde(default)]
        disabled: bool,
    },
    /// `ProgressView`: `value` is the fraction done (0...1); `None` = indeterminate spinner.
    Progress {
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        label: Option<Box<ViewNode>>,
        /// Shown under a linear bar (section Q).
        #[serde(default, rename = "currentValueLabel", skip_serializing_if = "Option::is_none")]
        current_value_label: Option<Box<ViewNode>>,
    },
    /// `Gauge` (section Q): `value` is the fraction of the range (0...1).
    Gauge {
        #[serde(default)]
        style: GaugeStyle,
        value: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<Box<ViewNode>>,
        #[serde(default, rename = "currentValueLabel", skip_serializing_if = "Option::is_none")]
        current_value_label: Option<Box<ViewNode>>,
        #[serde(default, rename = "minimumValueLabel", skip_serializing_if = "Option::is_none")]
        minimum_value_label: Option<Box<ViewNode>>,
        #[serde(default, rename = "maximumValueLabel", skip_serializing_if = "Option::is_none")]
        maximum_value_label: Option<Box<ViewNode>>,
    },
    /// Multi-column wheel (`DatePicker` wheels, section S): drums side by side behind one highlight band.
    /// `width` / `height` default to the proposal and 216.
    Wheel {
        #[serde(default)]
        columns: Vec<WheelColumn>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(default)]
        disabled: bool,
        /// Shown in a popover (a compact picker's time wheel): the band is darker over the glass.
        #[serde(default, skip_serializing_if = "is_false")]
        popover: bool,
    },
    /// SF Symbol (host capability).
    Image {
        #[serde(default, rename = "systemName", skip_serializing_if = "String::is_empty")]
        system_name: String,
        #[serde(default)]
        resizable: bool,
        #[serde(default, rename = "contentMode")]
        content_mode: Option<ContentMode>,
        #[serde(default)]
        scale: Option<ImageScale>,
        #[serde(default, rename = "renderingMode", skip_serializing_if = "Option::is_none")]
        rendering_mode: Option<TemplateMode>,
        #[serde(default, rename = "symbolRenderingMode", skip_serializing_if = "Option::is_none")]
        symbol_rendering_mode: Option<SymbolMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interpolation: Option<Interpolation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        antialiased: Option<bool>,
        #[serde(default, rename = "symbolColorRenderingMode", skip_serializing_if = "Option::is_none")]
        symbol_color_rendering_mode: Option<SymbolColorMode>,
        #[serde(default, rename = "symbolVariableValueMode", skip_serializing_if = "Option::is_none")]
        symbol_variable_value_mode: Option<SymbolVariable>,
        #[serde(default, rename = "dynamicRange", skip_serializing_if = "Option::is_none")]
        dynamic_range: Option<DynamicRange>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bitmap: Option<Box<Bitmap>>,
    },
    Label {
        title: Box<ViewNode>,
        icon: Box<ViewNode>,
    },
    Color {
        color: Color,
    },
    Rectangle,
    /// `Grid` (section M). Does not stretch columns to the proposed width.
    Grid {
        #[serde(default)]
        alignment: Alignment,
        #[serde(default, rename = "horizontalSpacing", skip_serializing_if = "Option::is_none")]
        horizontal_spacing: Option<f64>,
        #[serde(default, rename = "verticalSpacing", skip_serializing_if = "Option::is_none")]
        vertical_spacing: Option<f64>,
        #[serde(default)]
        rows: Vec<GridLine>,
    },
    /// `LazyVGrid` (`axis: vertical`) or `LazyHGrid` (`axis: horizontal`).
    #[serde(rename = "lazyGrid")]
    LazyGrid {
        #[serde(default)]
        axis: Axis,
        #[serde(default)]
        alignment: Alignment,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<f64>,
        #[serde(default)]
        tracks: Vec<GridTrack>,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    /// `GeometryReader`. `proposal` is the size the guest built the child with.
    Geometry {
        proposal: [f64; 2],
        child: Box<ViewNode>,
    },
    /// `ViewThatFits`. The host shows the first child that fits `axes`.
    #[serde(rename = "viewThatFits")]
    ViewThatFits {
        #[serde(default)]
        axes: FitAxes,
        #[serde(default)]
        children: Vec<ViewNode>,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewNode {
    pub id: String,
    pub modifiers: Vec<Modifier>,
    pub kind: ViewKind,
}

impl Serialize for ViewNode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        if !self.modifiers.is_empty() {
            map.serialize_entry("modifiers", &self.modifiers)?;
        }
        serialize_view_kind(&self.kind, &mut map)?;
        map.end()
    }
}

fn serialize_view_kind<M: SerializeMap>(kind: &ViewKind, map: &mut M) -> Result<(), M::Error> {
    match kind {
        ViewKind::Compose { component, props, children } => {
            map.serialize_entry("kind", "compose")?;
            map.serialize_entry("component", component)?;
            map.serialize_entry("props", props)?;
            map.serialize_entry("children", children)?;
        }
        ViewKind::Rn { component, props, children } => {
            map.serialize_entry("kind", "rn")?;
            map.serialize_entry("component", component)?;
            map.serialize_entry("props", props)?;
            map.serialize_entry("children", children)?;
        }
        ViewKind::Text { text, runs } => {
            map.serialize_entry("kind", "text")?;
            map.serialize_entry("text", text)?;
            if !runs.is_empty() {
                map.serialize_entry("runs", runs)?;
            }
        }
        ViewKind::VStack { spacing, alignment, children, pinned } => {
            map.serialize_entry("kind", "vstack")?;
            map.serialize_entry("spacing", spacing)?;
            map.serialize_entry("alignment", alignment)?;
            map.serialize_entry("children", children)?;
            if let Some(p) = pinned {
                map.serialize_entry("pinned", p)?;
            }
        }
        ViewKind::HStack { spacing, alignment, children, pinned } => {
            map.serialize_entry("kind", "hstack")?;
            map.serialize_entry("spacing", spacing)?;
            map.serialize_entry("alignment", alignment)?;
            map.serialize_entry("children", children)?;
            if let Some(p) = pinned {
                map.serialize_entry("pinned", p)?;
            }
        }
        ViewKind::ZStack { alignment, children } => {
            map.serialize_entry("kind", "zstack")?;
            map.serialize_entry("alignment", alignment)?;
            map.serialize_entry("children", children)?;
        }
        ViewKind::Spacer { min_length } => {
            map.serialize_entry("kind", "spacer")?;
            map.serialize_entry("minLength", min_length)?;
        }
        ViewKind::Button { action, label, style, disabled, role, selected } => {
            map.serialize_entry("kind", "button")?;
            map.serialize_entry("action", action)?;
            map.serialize_entry("label", label)?;
            map.serialize_entry("style", style)?;
            map.serialize_entry("disabled", disabled)?;
            if let Some(r) = role {
                map.serialize_entry("role", r)?;
            }
            if let Some(s) = selected {
                map.serialize_entry("selected", s)?;
            }
        }
        ViewKind::Presentation { content, layers } => {
            map.serialize_entry("kind", "presentation")?;
            map.serialize_entry("content", content)?;
            map.serialize_entry("layers", layers)?;
        }
        ViewKind::Canvas { proposal, children, opaque } => {
            map.serialize_entry("kind", "canvas")?;
            if let Some(p) = proposal {
                map.serialize_entry("proposal", p)?;
            }
            map.serialize_entry("children", children)?;
            map.serialize_entry("opaque", opaque)?;
        }
        ViewKind::CanvasItem { child, origin, anchor, size } => {
            map.serialize_entry("kind", "canvasItem")?;
            map.serialize_entry("child", child)?;
            map.serialize_entry("origin", origin)?;
            map.serialize_entry("anchor", anchor)?;
            map.serialize_entry("size", size)?;
        }
        ViewKind::VisualEffect { child, frame, effects } => {
            map.serialize_entry("kind", "visualEffect")?;
            map.serialize_entry("child", child)?;
            map.serialize_entry("frame", frame)?;
            map.serialize_entry("effects", effects)?;
        }
        ViewKind::Shape { shape, extra } => {
            map.serialize_entry("kind", "shape")?;
            map.serialize_entry("shape", shape)?;
            if let Some(v) = &extra.corner_radius {
                map.serialize_entry("cornerRadius", v)?;
            }
            if let Some(v) = &extra.corner_size {
                map.serialize_entry("cornerSize", v)?;
            }
            if let Some(v) = &extra.corner_radii {
                map.serialize_entry("cornerRadii", v)?;
            }
            if let Some(v) = &extra.corner_style {
                map.serialize_entry("cornerStyle", v)?;
            }
            if let Some(v) = &extra.concentric {
                map.serialize_entry("concentric", v)?;
            }
            if let Some(v) = &extra.path {
                map.serialize_entry("path", v)?;
            }
            if let Some(v) = &extra.path_size {
                map.serialize_entry("pathSize", v)?;
            }
            if extra.custom {
                map.serialize_entry("custom", &extra.custom)?;
            }
            if let Some(v) = &extra.size_hint {
                map.serialize_entry("sizeHint", v)?;
            }
            if extra.mirror_rtl {
                map.serialize_entry("mirrorRtl", &extra.mirror_rtl)?;
            }
            if let Some(v) = &extra.fill {
                map.serialize_entry("fill", v)?;
            }
            if let Some(v) = &extra.fill_style {
                map.serialize_entry("fillStyle", v)?;
            }
            if let Some(v) = &extra.stroke {
                map.serialize_entry("stroke", v)?;
            }
        }
        ViewKind::Divider => {
            map.serialize_entry("kind", "divider")?;
        }
        ViewKind::Scroll { axis, child, indicators } => {
            map.serialize_entry("kind", "scroll")?;
            map.serialize_entry("axis", axis)?;
            map.serialize_entry("child", child)?;
            if let Some(v) = indicators {
                map.serialize_entry("indicators", v)?;
            }
        }
        ViewKind::List { style, sections } => {
            map.serialize_entry("kind", "list")?;
            map.serialize_entry("style", style)?;
            map.serialize_entry("sections", sections)?;
        }
        ViewKind::Navigation { title, display_mode, back, chrome, content } => {
            map.serialize_entry("kind", "navigation")?;
            map.serialize_entry("title", title)?;
            map.serialize_entry("displayMode", display_mode)?;
            map.serialize_entry("back", back)?;
            if let Some(c) = chrome {
                map.serialize_entry("chrome", c)?;
            }
            map.serialize_entry("content", content)?;
        }
        ViewKind::TabView { selected, tabs, content } => {
            map.serialize_entry("kind", "tabview")?;
            map.serialize_entry("selected", selected)?;
            map.serialize_entry("tabs", tabs)?;
            map.serialize_entry("content", content)?;
        }
        ViewKind::PageTabView { selected, pages, index_display, index_background } => {
            map.serialize_entry("kind", "pagetabview")?;
            map.serialize_entry("selected", selected)?;
            map.serialize_entry("pages", pages)?;
            map.serialize_entry("indexDisplay", index_display)?;
            map.serialize_entry("indexBackground", index_background)?;
        }
        ViewKind::Toggle { is_on, action, label, disabled } => {
            map.serialize_entry("kind", "toggle")?;
            map.serialize_entry("isOn", is_on)?;
            map.serialize_entry("action", action)?;
            map.serialize_entry("label", label)?;
            map.serialize_entry("disabled", disabled)?;
        }
        ViewKind::Stepper { label, decrement, increment } => {
            map.serialize_entry("kind", "stepper")?;
            map.serialize_entry("label", label)?;
            map.serialize_entry("decrement", decrement)?;
            map.serialize_entry("increment", increment)?;
        }
        ViewKind::TextField { placeholder, text, field, secure, disabled, input } => {
            map.serialize_entry("kind", "text_field")?;
            map.serialize_entry("placeholder", placeholder)?;
            map.serialize_entry("text", text)?;
            map.serialize_entry("field", field)?;
            if *secure {
                map.serialize_entry("secure", secure)?;
            }
            if *disabled {
                map.serialize_entry("disabled", disabled)?;
            }
            if input.editor {
                map.serialize_entry("editor", &input.editor)?;
            }
            if let Some(v) = &input.input_style {
                map.serialize_entry("input_style", v)?;
            }
            if let Some(v) = &input.input_border_shape {
                map.serialize_entry("input_border_shape", v)?;
            }
            if let Some(v) = &input.axis {
                map.serialize_entry("axis", v)?;
            }
            if let Some(v) = &input.focused {
                map.serialize_entry("focused", v)?;
            }
            if let Some(v) = &input.focus_action {
                map.serialize_entry("focus_action", v)?;
            }
            if let Some(v) = &input.blur_action {
                map.serialize_entry("blur_action", v)?;
            }
            if let Some(v) = &input.submit_action {
                map.serialize_entry("submit_action", v)?;
            }
            if let Some(v) = &input.keyboard_type {
                map.serialize_entry("keyboard_type", v)?;
            }
            if let Some(v) = &input.autocapitalization {
                map.serialize_entry("autocapitalization", v)?;
            }
            if let Some(v) = &input.submit_label {
                map.serialize_entry("submit_label", v)?;
            }
            if input.search_field {
                map.serialize_entry("search_field", &input.search_field)?;
            }
            if let Some(v) = &input.content_type {
                map.serialize_entry("content_type", v)?;
            }
            if let Some(v) = &input.autocorrection_disabled {
                map.serialize_entry("autocorrection_disabled", v)?;
            }
            if let Some(v) = &input.prompt {
                map.serialize_entry("prompt", v)?;
            }
        }
        ViewKind::Picker { label, style, options, selected, open, disabled } => {
            map.serialize_entry("kind", "picker")?;
            map.serialize_entry("label", label)?;
            map.serialize_entry("style", style)?;
            map.serialize_entry("options", options)?;
            map.serialize_entry("selected", selected)?;
            map.serialize_entry("open", open)?;
            map.serialize_entry("disabled", disabled)?;
        }
        ViewKind::Slider { value, min, max, step, field, disabled } => {
            map.serialize_entry("kind", "slider")?;
            map.serialize_entry("value", value)?;
            map.serialize_entry("min", min)?;
            map.serialize_entry("max", max)?;
            map.serialize_entry("step", step)?;
            map.serialize_entry("field", field)?;
            map.serialize_entry("disabled", disabled)?;
        }
        ViewKind::Progress { value, label, current_value_label } => {
            map.serialize_entry("kind", "progress")?;
            map.serialize_entry("value", value)?;
            map.serialize_entry("label", label)?;
            if let Some(v) = current_value_label {
                map.serialize_entry("currentValueLabel", v)?;
            }
        }
        ViewKind::Gauge {
            style,
            value,
            label,
            current_value_label,
            minimum_value_label,
            maximum_value_label,
        } => {
            map.serialize_entry("kind", "gauge")?;
            map.serialize_entry("style", style)?;
            map.serialize_entry("value", value)?;
            if let Some(v) = label {
                map.serialize_entry("label", v)?;
            }
            if let Some(v) = current_value_label {
                map.serialize_entry("currentValueLabel", v)?;
            }
            if let Some(v) = minimum_value_label {
                map.serialize_entry("minimumValueLabel", v)?;
            }
            if let Some(v) = maximum_value_label {
                map.serialize_entry("maximumValueLabel", v)?;
            }
        }
        ViewKind::Wheel { columns, width, height, disabled, popover } => {
            map.serialize_entry("kind", "wheel")?;
            map.serialize_entry("columns", columns)?;
            if let Some(v) = width {
                map.serialize_entry("width", v)?;
            }
            if let Some(v) = height {
                map.serialize_entry("height", v)?;
            }
            map.serialize_entry("disabled", disabled)?;
            if *popover {
                map.serialize_entry("popover", popover)?;
            }
        }
        ViewKind::Image {
            system_name,
            resizable,
            content_mode,
            scale,
            rendering_mode,
            symbol_rendering_mode,
            interpolation,
            antialiased,
            symbol_color_rendering_mode,
            symbol_variable_value_mode,
            dynamic_range,
            bitmap,
        } => {
            map.serialize_entry("kind", "image")?;
            if !system_name.is_empty() {
                map.serialize_entry("systemName", system_name)?;
            }
            map.serialize_entry("resizable", resizable)?;
            map.serialize_entry("contentMode", content_mode)?;
            map.serialize_entry("scale", scale)?;
            if let Some(v) = rendering_mode {
                map.serialize_entry("renderingMode", v)?;
            }
            if let Some(v) = symbol_rendering_mode {
                map.serialize_entry("symbolRenderingMode", v)?;
            }
            if let Some(v) = interpolation {
                map.serialize_entry("interpolation", v)?;
            }
            if let Some(v) = antialiased {
                map.serialize_entry("antialiased", v)?;
            }
            if let Some(v) = symbol_color_rendering_mode {
                map.serialize_entry("symbolColorRenderingMode", v)?;
            }
            if let Some(v) = symbol_variable_value_mode {
                map.serialize_entry("symbolVariableValueMode", v)?;
            }
            if let Some(v) = dynamic_range {
                map.serialize_entry("dynamicRange", v)?;
            }
            if let Some(v) = bitmap {
                map.serialize_entry("bitmap", v)?;
            }
        }
        ViewKind::Label { title, icon } => {
            map.serialize_entry("kind", "label")?;
            map.serialize_entry("title", title)?;
            map.serialize_entry("icon", icon)?;
        }
        ViewKind::Color { color } => {
            map.serialize_entry("kind", "color")?;
            map.serialize_entry("color", color)?;
        }
        ViewKind::Rectangle => {
            map.serialize_entry("kind", "rectangle")?;
        }
        ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows } => {
            map.serialize_entry("kind", "grid")?;
            map.serialize_entry("alignment", alignment)?;
            if let Some(v) = horizontal_spacing {
                map.serialize_entry("horizontalSpacing", v)?;
            }
            if let Some(v) = vertical_spacing {
                map.serialize_entry("verticalSpacing", v)?;
            }
            map.serialize_entry("rows", rows)?;
        }
        ViewKind::LazyGrid { axis, alignment, spacing, tracks, children } => {
            map.serialize_entry("kind", "lazyGrid")?;
            map.serialize_entry("axis", axis)?;
            map.serialize_entry("alignment", alignment)?;
            if let Some(v) = spacing {
                map.serialize_entry("spacing", v)?;
            }
            map.serialize_entry("tracks", tracks)?;
            map.serialize_entry("children", children)?;
        }
        ViewKind::Geometry { proposal, child } => {
            map.serialize_entry("kind", "geometry")?;
            map.serialize_entry("proposal", proposal)?;
            map.serialize_entry("child", child)?;
        }
        ViewKind::ViewThatFits { axes, children } => {
            map.serialize_entry("kind", "viewThatFits")?;
            map.serialize_entry("axes", axes)?;
            map.serialize_entry("children", children)?;
        }
        ViewKind::Empty => {
            map.serialize_entry("kind", "empty")?;
        }
    }
    Ok(())
}

/// Source of remaining key-value pairs for single-pass `ViewNode` deserialization.
/// Replays any fields that appeared before `"kind"`, then reads directly from `MapAccess`.
struct FieldStream<'a, 'de, A: MapAccess<'de>> {
    buffered: std::vec::IntoIter<(String, serde_json::Value)>,
    pending_value: Option<serde_json::Value>,
    map: &'a mut A,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'a, 'de, A: MapAccess<'de>> FieldStream<'a, 'de, A> {
    fn new(buffered: Vec<(String, serde_json::Value)>, map: &'a mut A) -> Self {
        Self {
            buffered: buffered.into_iter(),
            pending_value: None,
            map,
            _marker: std::marker::PhantomData,
        }
    }

    fn next_key(&mut self) -> Result<Option<Cow<'de, str>>, A::Error> {
        if let Some((k, v)) = self.buffered.next() {
            self.pending_value = Some(v);
            Ok(Some(Cow::Owned(k)))
        } else {
            self.map.next_key::<Cow<'de, str>>()
        }
    }

    fn next_value<T: de::DeserializeOwned>(&mut self) -> Result<T, A::Error> {
        if let Some(v) = self.pending_value.take() {
            serde_json::from_value(v).map_err(de::Error::custom)
        } else {
            self.map.next_value()
        }
    }

    fn skip_value(&mut self) -> Result<(), A::Error> {
        if self.pending_value.take().is_some() {
            Ok(())
        } else {
            self.map.next_value::<de::IgnoredAny>()?;
            Ok(())
        }
    }
}

impl<'de> Deserialize<'de> for ViewNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ViewNodeVisitor;

        impl<'de> Visitor<'de> for ViewNodeVisitor {
            type Value = ViewNode;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a ViewNode object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut id: Option<String> = None;
                let mut modifiers: Vec<Modifier> = Vec::new();
                let mut buffered: Vec<(String, serde_json::Value)> = Vec::new();
                let mut kind_tag: Option<Cow<'de, str>> = None;

                while let Some(key) = map.next_key::<Cow<'de, str>>()? {
                    match key.as_ref() {
                        "id" => id = Some(map.next_value()?),
                        "modifiers" => modifiers = map.next_value()?,
                        "kind" => {
                            kind_tag = Some(map.next_value::<Cow<'de, str>>()?);
                            break;
                        }
                        _ => {
                            let val: serde_json::Value = map.next_value()?;
                            buffered.push((key.into_owned(), val));
                        }
                    }
                }

                let tag = kind_tag.ok_or_else(|| de::Error::missing_field("kind"))?;
                let mut stream = FieldStream::new(buffered, &mut map);
                let kind = deserialize_view_kind(&tag, &mut stream, &mut id, &mut modifiers)?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                Ok(ViewNode { id, modifiers, kind })
            }
        }

        deserializer.deserialize_map(ViewNodeVisitor)
    }
}

fn deserialize_view_kind<'de, A: MapAccess<'de>>(
    tag: &str,
    stream: &mut FieldStream<'_, 'de, A>,
    id: &mut Option<String>,
    modifiers: &mut Vec<Modifier>,
) -> Result<ViewKind, A::Error> {
    macro_rules! common_key {
        ($k:expr) => {
            match $k {
                "id" => {
                    *id = Some(stream.next_value()?);
                    continue;
                }
                "modifiers" => {
                    *modifiers = stream.next_value()?;
                    continue;
                }
                _ => {}
            }
        };
    }

    match tag {
        "compose" | "rn" => {
            let mut component: Option<String> = None;
            let mut props: Option<Box<serde_json::Value>> = None;
            let mut children: Vec<ViewNode> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "component" => component = Some(stream.next_value()?),
                    "props" => props = Some(stream.next_value()?),
                    "children" => children = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            let component = component.ok_or_else(|| de::Error::missing_field("component"))?;
            let props = props.unwrap_or_default();
            if tag == "compose" {
                Ok(ViewKind::Compose { component, props, children })
            } else {
                Ok(ViewKind::Rn { component, props, children })
            }
        }
        "text" => {
            let mut text: Option<String> = None;
            let mut runs: Vec<Run> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "text" => text = Some(stream.next_value()?),
                    "runs" => runs = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Text {
                text: text.ok_or_else(|| de::Error::missing_field("text"))?,
                runs,
            })
        }
        "vstack" | "hstack" => {
            let mut spacing: Option<f64> = None;
            let mut alignment: Alignment = Alignment::default();
            let mut children: Vec<ViewNode> = Vec::new();
            let mut pinned: Option<PinMode> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "spacing" => spacing = stream.next_value()?,
                    "alignment" => alignment = stream.next_value()?,
                    "children" => children = stream.next_value()?,
                    "pinned" => pinned = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            if tag == "vstack" {
                Ok(ViewKind::VStack { spacing, alignment, children, pinned })
            } else {
                Ok(ViewKind::HStack { spacing, alignment, children, pinned })
            }
        }
        "zstack" => {
            let mut alignment: Alignment = Alignment::default();
            let mut children: Vec<ViewNode> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "alignment" => alignment = stream.next_value()?,
                    "children" => children = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::ZStack { alignment, children })
        }
        "spacer" => {
            let mut min_length: Option<f64> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "minLength" => min_length = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Spacer { min_length })
        }
        "button" => {
            let mut action: Option<u64> = None;
            let mut label: Option<Box<ViewNode>> = None;
            let mut style: ButtonStyle = ButtonStyle::default();
            let mut disabled: bool = false;
            let mut role: Option<ButtonRole> = None;
            let mut selected: Option<bool> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "action" => action = Some(stream.next_value()?),
                    "label" => label = Some(stream.next_value()?),
                    "style" => style = stream.next_value()?,
                    "disabled" => disabled = stream.next_value()?,
                    "role" => role = stream.next_value()?,
                    "selected" => selected = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Button {
                action: action.ok_or_else(|| de::Error::missing_field("action"))?,
                label: label.ok_or_else(|| de::Error::missing_field("label"))?,
                style,
                disabled,
                role,
                selected,
            })
        }
        "presentation" => {
            let mut content: Option<Box<ViewNode>> = None;
            let mut layers: Vec<Layer> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "content" => content = Some(stream.next_value()?),
                    "layers" => layers = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Presentation {
                content: content.ok_or_else(|| de::Error::missing_field("content"))?,
                layers,
            })
        }
        "canvas" => {
            let mut proposal: Option<[f64; 2]> = None;
            let mut children: Option<Vec<ViewNode>> = None;
            let mut opaque: bool = false;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "proposal" => proposal = stream.next_value()?,
                    "children" => children = Some(stream.next_value()?),
                    "opaque" => opaque = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Canvas {
                proposal,
                children: children.ok_or_else(|| de::Error::missing_field("children"))?,
                opaque,
            })
        }
        "canvasItem" => {
            let mut child: Option<Box<ViewNode>> = None;
            let mut origin: Option<[f64; 2]> = None;
            let mut anchor: Option<[f64; 2]> = None;
            let mut size: Option<[f64; 2]> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "child" => child = Some(stream.next_value()?),
                    "origin" => origin = Some(stream.next_value()?),
                    "anchor" => anchor = Some(stream.next_value()?),
                    "size" => size = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::CanvasItem {
                child: child.ok_or_else(|| de::Error::missing_field("child"))?,
                origin: origin.ok_or_else(|| de::Error::missing_field("origin"))?,
                anchor: anchor.ok_or_else(|| de::Error::missing_field("anchor"))?,
                size,
            })
        }
        "visualEffect" => {
            let mut child: Option<Box<ViewNode>> = None;
            let mut frame: Option<[f64; 4]> = None;
            let mut effects: Vec<VisualOp> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "child" => child = Some(stream.next_value()?),
                    "frame" => frame = Some(stream.next_value()?),
                    "effects" => effects = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::VisualEffect {
                child: child.ok_or_else(|| de::Error::missing_field("child"))?,
                frame: frame.ok_or_else(|| de::Error::missing_field("frame"))?,
                effects,
            })
        }
        "shape" => {
            let mut shape: Option<ShapeKind> = None;
            let mut extra = Box::<ShapeExtra>::default();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "shape" => shape = Some(stream.next_value()?),
                    "cornerRadius" => extra.corner_radius = stream.next_value()?,
                    "cornerSize" => extra.corner_size = stream.next_value()?,
                    "cornerRadii" => extra.corner_radii = stream.next_value()?,
                    "cornerStyle" => extra.corner_style = stream.next_value()?,
                    "concentric" => extra.concentric = stream.next_value()?,
                    "path" => extra.path = stream.next_value()?,
                    "pathSize" => extra.path_size = stream.next_value()?,
                    "custom" => extra.custom = stream.next_value()?,
                    "sizeHint" => extra.size_hint = stream.next_value()?,
                    "mirrorRtl" => extra.mirror_rtl = stream.next_value()?,
                    "fill" => extra.fill = stream.next_value()?,
                    "fillStyle" => extra.fill_style = stream.next_value()?,
                    "stroke" => extra.stroke = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Shape {
                shape: shape.ok_or_else(|| de::Error::missing_field("shape"))?,
                extra,
            })
        }
        "divider" => {
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                stream.skip_value()?;
            }
            Ok(ViewKind::Divider)
        }
        "scroll" => {
            let mut axis: Axis = Axis::default();
            let mut child: Option<Box<ViewNode>> = None;
            let mut indicators: Option<String> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "axis" => axis = stream.next_value()?,
                    "child" => child = Some(stream.next_value()?),
                    "indicators" => indicators = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Scroll {
                axis,
                child: child.ok_or_else(|| de::Error::missing_field("child"))?,
                indicators,
            })
        }
        "list" => {
            let mut style: ListStyle = ListStyle::default();
            let mut sections: Vec<Section> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "style" => style = stream.next_value()?,
                    "sections" => sections = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::List { style, sections })
        }
        "navigation" => {
            let mut title: Option<String> = None;
            let mut display_mode: DisplayMode = DisplayMode::default();
            let mut back: Option<Box<BackButton>> = None;
            let mut chrome: Option<Box<NavigationChrome>> = None;
            let mut content: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "title" => title = stream.next_value()?,
                    "displayMode" => display_mode = stream.next_value()?,
                    "back" => back = stream.next_value()?,
                    "chrome" => chrome = stream.next_value()?,
                    "content" => content = Some(stream.next_value()?),
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Navigation {
                title,
                display_mode,
                back,
                chrome,
                content: content.ok_or_else(|| de::Error::missing_field("content"))?,
            })
        }
        "tabview" => {
            let mut selected: usize = 0;
            let mut tabs: Vec<TabItem> = Vec::new();
            let mut content: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "selected" => selected = stream.next_value()?,
                    "tabs" => tabs = stream.next_value()?,
                    "content" => content = Some(stream.next_value()?),
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::TabView {
                selected,
                tabs,
                content: content.ok_or_else(|| de::Error::missing_field("content"))?,
            })
        }
        "pagetabview" => {
            let mut selected: usize = 0;
            let mut pages: Vec<TabPage> = Vec::new();
            let mut index_display: PageIndexDisplay = PageIndexDisplay::default();
            let mut index_background: PageIndexBackground = PageIndexBackground::default();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "selected" => selected = stream.next_value()?,
                    "pages" => pages = stream.next_value()?,
                    "indexDisplay" => index_display = stream.next_value()?,
                    "indexBackground" => index_background = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::PageTabView { selected, pages, index_display, index_background })
        }
        "toggle" => {
            let mut is_on: Option<bool> = None;
            let mut action: Option<u64> = None;
            let mut label: Option<Box<ViewNode>> = None;
            let mut disabled: bool = false;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "isOn" => is_on = Some(stream.next_value()?),
                    "action" => action = Some(stream.next_value()?),
                    "label" => label = Some(stream.next_value()?),
                    "disabled" => disabled = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Toggle {
                is_on: is_on.ok_or_else(|| de::Error::missing_field("isOn"))?,
                action: action.ok_or_else(|| de::Error::missing_field("action"))?,
                label: label.ok_or_else(|| de::Error::missing_field("label"))?,
                disabled,
            })
        }
        "stepper" => {
            let mut label: Option<Box<ViewNode>> = None;
            let mut decrement: Option<u64> = None;
            let mut increment: Option<u64> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "label" => label = Some(stream.next_value()?),
                    "decrement" => decrement = stream.next_value()?,
                    "increment" => increment = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Stepper {
                label: label.ok_or_else(|| de::Error::missing_field("label"))?,
                decrement,
                increment,
            })
        }
        "text_field" => {
            let mut placeholder = String::new();
            let mut text = String::new();
            let mut field: Option<u64> = None;
            let mut secure = false;
            let mut disabled = false;
            let mut input = Box::<TextInputOptions>::default();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "placeholder" => placeholder = stream.next_value()?,
                    "text" => text = stream.next_value()?,
                    "field" => field = Some(stream.next_value()?),
                    "secure" => secure = stream.next_value()?,
                    "disabled" => disabled = stream.next_value()?,
                    "editor" => input.editor = stream.next_value()?,
                    "input_style" => input.input_style = stream.next_value::<Option<TextInputStyle>>()?,
                    "input_border_shape" => {
                        input.input_border_shape = stream.next_value::<Option<TextInputBorderShape>>()?
                    }
                    "axis" => input.axis = stream.next_value()?,
                    "focused" => input.focused = stream.next_value()?,
                    "focus_action" => input.focus_action = stream.next_value()?,
                    "blur_action" => input.blur_action = stream.next_value()?,
                    "submit_action" => input.submit_action = stream.next_value()?,
                    "keyboard_type" => input.keyboard_type = stream.next_value()?,
                    "autocapitalization" => input.autocapitalization = stream.next_value()?,
                    "submit_label" => input.submit_label = stream.next_value()?,
                    "search_field" => input.search_field = stream.next_value()?,
                    "content_type" => input.content_type = stream.next_value()?,
                    "autocorrection_disabled" => input.autocorrection_disabled = stream.next_value()?,
                    "prompt" => input.prompt = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::TextField {
                placeholder,
                text,
                field: field.ok_or_else(|| de::Error::missing_field("field"))?,
                secure,
                disabled,
                input,
            })
        }
        "picker" => {
            let mut label: Option<Box<ViewNode>> = None;
            let mut style: PickerStyle = PickerStyle::default();
            let mut options: Vec<PickerOption> = Vec::new();
            let mut selected: Option<usize> = None;
            let mut open: Option<u64> = None;
            let mut disabled: bool = false;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "label" => label = Some(stream.next_value()?),
                    "style" => style = stream.next_value()?,
                    "options" => options = stream.next_value()?,
                    "selected" => selected = stream.next_value()?,
                    "open" => open = stream.next_value()?,
                    "disabled" => disabled = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Picker {
                label: label.ok_or_else(|| de::Error::missing_field("label"))?,
                style,
                options,
                selected,
                open,
                disabled,
            })
        }
        "slider" => {
            let mut value: Option<f64> = None;
            let mut min: f64 = 0.0;
            let mut max: f64 = one();
            let mut step: Option<f64> = None;
            let mut field: Option<u64> = None;
            let mut disabled: bool = false;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "value" => value = Some(stream.next_value()?),
                    "min" => min = stream.next_value()?,
                    "max" => max = stream.next_value()?,
                    "step" => step = stream.next_value()?,
                    "field" => field = stream.next_value()?,
                    "disabled" => disabled = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Slider {
                value: value.ok_or_else(|| de::Error::missing_field("value"))?,
                min,
                max,
                step,
                field,
                disabled,
            })
        }
        "progress" => {
            let mut value: Option<f64> = None;
            let mut label: Option<Box<ViewNode>> = None;
            let mut current_value_label: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "value" => value = stream.next_value()?,
                    "label" => label = stream.next_value()?,
                    "currentValueLabel" => current_value_label = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Progress { value, label, current_value_label })
        }
        "gauge" => {
            let mut style: GaugeStyle = GaugeStyle::default();
            let mut value: Option<f64> = None;
            let mut label: Option<Box<ViewNode>> = None;
            let mut current_value_label: Option<Box<ViewNode>> = None;
            let mut minimum_value_label: Option<Box<ViewNode>> = None;
            let mut maximum_value_label: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "style" => style = stream.next_value()?,
                    "value" => value = Some(stream.next_value()?),
                    "label" => label = stream.next_value()?,
                    "currentValueLabel" => current_value_label = stream.next_value()?,
                    "minimumValueLabel" => minimum_value_label = stream.next_value()?,
                    "maximumValueLabel" => maximum_value_label = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Gauge {
                style,
                value: value.ok_or_else(|| de::Error::missing_field("value"))?,
                label,
                current_value_label,
                minimum_value_label,
                maximum_value_label,
            })
        }
        "wheel" => {
            let mut columns: Vec<WheelColumn> = Vec::new();
            let mut width: Option<f64> = None;
            let mut height: Option<f64> = None;
            let mut disabled: bool = false;
            let mut popover: bool = false;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "columns" => columns = stream.next_value()?,
                    "width" => width = stream.next_value()?,
                    "height" => height = stream.next_value()?,
                    "disabled" => disabled = stream.next_value()?,
                    "popover" => popover = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Wheel { columns, width, height, disabled, popover })
        }
        "image" => {
            let mut system_name = String::new();
            let mut resizable = false;
            let mut content_mode: Option<ContentMode> = None;
            let mut scale: Option<ImageScale> = None;
            let mut rendering_mode: Option<TemplateMode> = None;
            let mut symbol_rendering_mode: Option<SymbolMode> = None;
            let mut interpolation: Option<Interpolation> = None;
            let mut antialiased: Option<bool> = None;
            let mut symbol_color_rendering_mode: Option<SymbolColorMode> = None;
            let mut symbol_variable_value_mode: Option<SymbolVariable> = None;
            let mut dynamic_range: Option<DynamicRange> = None;
            let mut bitmap: Option<Box<Bitmap>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "systemName" => system_name = stream.next_value()?,
                    "resizable" => resizable = stream.next_value()?,
                    "contentMode" => content_mode = stream.next_value()?,
                    "scale" => scale = stream.next_value()?,
                    "renderingMode" => rendering_mode = stream.next_value()?,
                    "symbolRenderingMode" => symbol_rendering_mode = stream.next_value()?,
                    "interpolation" => interpolation = stream.next_value()?,
                    "antialiased" => antialiased = stream.next_value()?,
                    "symbolColorRenderingMode" => symbol_color_rendering_mode = stream.next_value()?,
                    "symbolVariableValueMode" => symbol_variable_value_mode = stream.next_value()?,
                    "dynamicRange" => dynamic_range = stream.next_value()?,
                    "bitmap" => bitmap = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Image {
                system_name,
                resizable,
                content_mode,
                scale,
                rendering_mode,
                symbol_rendering_mode,
                interpolation,
                antialiased,
                symbol_color_rendering_mode,
                symbol_variable_value_mode,
                dynamic_range,
                bitmap,
            })
        }
        "label" => {
            let mut title: Option<Box<ViewNode>> = None;
            let mut icon: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "title" => title = Some(stream.next_value()?),
                    "icon" => icon = Some(stream.next_value()?),
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Label {
                title: title.ok_or_else(|| de::Error::missing_field("title"))?,
                icon: icon.ok_or_else(|| de::Error::missing_field("icon"))?,
            })
        }
        "color" => {
            let mut color: Option<Color> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "color" => color = Some(stream.next_value()?),
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Color {
                color: color.ok_or_else(|| de::Error::missing_field("color"))?,
            })
        }
        "rectangle" => {
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                stream.skip_value()?;
            }
            Ok(ViewKind::Rectangle)
        }
        "grid" => {
            let mut alignment: Alignment = Alignment::default();
            let mut horizontal_spacing: Option<f64> = None;
            let mut vertical_spacing: Option<f64> = None;
            let mut rows: Vec<GridLine> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "alignment" => alignment = stream.next_value()?,
                    "horizontalSpacing" => horizontal_spacing = stream.next_value()?,
                    "verticalSpacing" => vertical_spacing = stream.next_value()?,
                    "rows" => rows = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows })
        }
        "lazyGrid" => {
            let mut axis: Axis = Axis::default();
            let mut alignment: Alignment = Alignment::default();
            let mut spacing: Option<f64> = None;
            let mut tracks: Vec<GridTrack> = Vec::new();
            let mut children: Vec<ViewNode> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "axis" => axis = stream.next_value()?,
                    "alignment" => alignment = stream.next_value()?,
                    "spacing" => spacing = stream.next_value()?,
                    "tracks" => tracks = stream.next_value()?,
                    "children" => children = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::LazyGrid { axis, alignment, spacing, tracks, children })
        }
        "geometry" => {
            let mut proposal: Option<[f64; 2]> = None;
            let mut child: Option<Box<ViewNode>> = None;
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "proposal" => proposal = Some(stream.next_value()?),
                    "child" => child = Some(stream.next_value()?),
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::Geometry {
                proposal: proposal.ok_or_else(|| de::Error::missing_field("proposal"))?,
                child: child.ok_or_else(|| de::Error::missing_field("child"))?,
            })
        }
        "viewThatFits" => {
            let mut axes: FitAxes = FitAxes::default();
            let mut children: Vec<ViewNode> = Vec::new();
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                match k.as_ref() {
                    "axes" => axes = stream.next_value()?,
                    "children" => children = stream.next_value()?,
                    _ => stream.skip_value()?,
                }
            }
            Ok(ViewKind::ViewThatFits { axes, children })
        }
        "empty" => {
            while let Some(k) = stream.next_key()? {
                common_key!(k.as_ref());
                stream.skip_value()?;
            }
            Ok(ViewKind::Empty)
        }
        other => Err(de::Error::unknown_variant(
            other,
            &[
                "compose", "rn", "text", "vstack", "hstack", "zstack", "spacer", "button",
                "presentation", "canvas", "canvasItem", "visualEffect", "shape", "divider",
                "scroll", "list", "navigation", "tabview", "pagetabview", "toggle", "stepper",
                "text_field", "picker", "slider", "progress", "gauge", "wheel", "image", "label",
                "color", "rectangle", "grid", "lazyGrid", "geometry", "viewThatFits", "empty",
            ],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::*;
    use crate::wire::GuestMessage;
    use serde_json::json;

    #[test]
    fn type_sizes_stay_compact() {
        assert!(std::mem::size_of::<Color>() <= 48, "Color was {}", std::mem::size_of::<Color>());
        assert!(std::mem::size_of::<Modifier>() <= 56, "Modifier was {}", std::mem::size_of::<Modifier>());
        assert!(std::mem::size_of::<ViewKind>() <= 80, "ViewKind was {}", std::mem::size_of::<ViewKind>());
        assert!(std::mem::size_of::<ViewNode>() <= 128, "ViewNode was {}", std::mem::size_of::<ViewNode>());
        assert!(std::mem::size_of::<GuestMessage>() <= 80, "GuestMessage was {}", std::mem::size_of::<GuestMessage>());
    }

    #[test]
    fn view_node_arbitrary_key_order_round_trip() {
        // Variant fields before "kind", and "id"/"modifiers" after "kind".
        let j = r#"{"text":"hello","runs":[{"text":"he","italic":true}],"kind":"text","id":"t1","modifiers":[{"opacity":0.5}]}"#;
        let n: ViewNode = serde_json::from_str(j).unwrap();
        assert_eq!(n.id, "t1");
        assert_eq!(n.modifiers, vec![Modifier::Opacity(0.5)]);
        let ViewKind::Text { text, runs } = &n.kind else { panic!() };
        assert_eq!(text, "hello");
        assert_eq!(runs.len(), 1);
        assert!(runs[0].italic);
        let again: ViewNode = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert_eq!(n, again);

        // Adaptive Color with boxed dark RGB round-trips identically on the wire.
        let c: Color = serde_json::from_str(r#"{"r":0.1,"g":0.2,"b":0.3,"a":1.0,"dark":[0.9,0.8,0.7]}"#).unwrap();
        assert_eq!(
            c,
            Color::Rgba { r: 0.1, g: 0.2, b: 0.3, a: 1.0, dark: Some(Box::new([0.9, 0.8, 0.7])) }
        );
        assert_eq!(
            serde_json::to_string(&c).unwrap(),
            r#"{"r":0.1,"g":0.2,"b":0.3,"a":1.0,"dark":[0.9,0.8,0.7]}"#
        );
    }

    #[test]
    fn counter_tree_round_trip() {
        let j = json!({
            "kind":"vstack","id":"0","spacing":12.0,"alignment":"center",
            "modifiers":[{"padding":{"top":16.0,"leading":16.0,"bottom":16.0,"trailing":16.0}},
                         {"frame":{"maxWidth":"inf","alignment":"center"}},
                         {"background":{"kind":"color","id":"0.bg","color":{"semantic":"background"}}},
                         {"cornerRadius":8.0},{"opacity":0.5},
                         {"foregroundColor":{"r":1.0,"g":0.0,"b":0.0,"a":1.0}}],
            "children":[
                {"kind":"text","id":"0.0","text":"Count: 1"},
                {"kind":"spacer","id":"0.1","minLength":null},
                {"kind":"button","id":"0.2","action":7,"label":{"kind":"text","id":"0.2.l","text":"Increment"}},
                {"kind":"rectangle","id":"0.3"},{"kind":"empty","id":"0.4"}]
        });
        let n: ViewNode = serde_json::from_value(j.clone()).unwrap();
        let ViewKind::VStack { children, .. } = &n.kind else { panic!() };
        assert_eq!(children.len(), 5);
        let Modifier::Frame(f) = &n.modifiers[1] else { panic!() };
        assert_eq!(f.max_width, Some(f64::INFINITY));
        assert_eq!(n.modifiers[5], Modifier::ForegroundColor(Color::Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0, dark: None }));
        // re-serialise and re-parse: stable
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
        assert_eq!(serde_json::to_value(&n).unwrap()["modifiers"][1]["frame"]["maxWidth"], "inf");
    }

    #[test]
    fn semantic_color() {
        let c: Color = serde_json::from_str(r#"{"semantic":"accent"}"#).unwrap();
        assert_eq!(c, Color::Semantic { semantic: Semantic::Accent });
    }

    #[test]
    fn section_d_nodes_parse() {
        let j = json!({"kind":"navigation","id":"n","title":"T","displayMode":"inline","back":{"action":9,"label":"B"},
          "modifiers":[{"font":{"style":"headline"}},{"font":{"size":22,"weight":"semibold"}},{"fontWeight":"bold"},
                       {"strikethrough":true},{"multilineTextAlignment":"center"},{"navigationTitle":"T"},
                       {"shadow":{"color":{"semantic":"separator"},"radius":6,"x":0,"y":2}},
                       {"overlay":{"view":{"kind":"shape","id":"o","shape":"circle","stroke":{"color":{"semantic":"tertiary"},"lineWidth":2}},"alignment":"topTrailing"}}],
          "content":{"kind":"list","id":"l","style":"insetGrouped","sections":[{"header":"H","rows":[
            {"kind":"toggle","id":"a","isOn":true,"action":1,"label":{"kind":"text","id":"a.l","text":"x"}},
            {"kind":"stepper","id":"b","label":{"kind":"text","id":"b.l","text":"y"},"decrement":null,"increment":3},
            {"kind":"text_field","id":"c","placeholder":"p","text":"","field":4},
            {"kind":"button","id":"d","action":5,"style":"borderedProminent","disabled":true,"label":{"kind":"divider","id":"d.l"}},
            {"kind":"scroll","id":"e","axis":"horizontal","child":{"kind":"shape","id":"e.c","shape":"roundedRectangle","cornerRadius":4,"fill":{"semantic":"groupedBackground"}}}]}]}});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers.len(), 8);
        assert_eq!(n.modifiers[1], Modifier::Font(Font::System { size: 22.0, weight: Weight::Semibold, design: None }));
        assert_eq!(n.modifiers[0], Modifier::Font(Font::Style { style: TextStyle::Headline, weight: None }));
        let ViewKind::Navigation { display_mode, back, content, .. } = &n.kind else { panic!() };
        assert_eq!(*display_mode, DisplayMode::Inline);
        assert_eq!(back.as_ref().unwrap().action, 9);
        let ViewKind::List { sections, .. } = &content.kind else { panic!() };
        assert!(matches!(sections[0].rows[2].kind, ViewKind::TextField { field: 4, .. }));
        assert!(matches!(sections[0].rows[3].kind, ViewKind::Button { style: ButtonStyle::BorderedProminent, disabled: true, .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
        assert_eq!(TextStyle::Body.spec(), (17.0, Weight::Regular));
        assert_eq!(Weight::Bold.value(), 700);
    }

    #[test]
    fn new_semantics_and_button_defaults() {
        let c: Color = serde_json::from_str(r#"{"semantic":"secondaryGroupedBackground"}"#).unwrap();
        assert_eq!(c, Color::Semantic { semantic: Semantic::SecondaryGroupedBackground });
        let b: ViewNode = serde_json::from_str(r#"{"kind":"button","id":"b","action":1,"label":{"kind":"empty","id":"e"}}"#).unwrap();
        assert!(matches!(b.kind, ViewKind::Button { style: ButtonStyle::Automatic, disabled: false, .. }));
    }

    #[test]
    fn runs_roles_and_presentation_parse() {
        let j = json!({"kind":"presentation","id":"p",
          "content":{"kind":"text","id":"t","text":"ab","runs":[{"text":"a","fontWeight":"bold","italic":true},{"text":"b","underline":true,"foregroundColor":{"semantic":"secondary"}}]},
          "layers":[
            {"kind":"sheet","content":{"kind":"empty","id":"s"},"detents":[{"kind":"medium"},{"kind":"fraction","value":0.8},{"kind":"large"}],"dragIndicator":true,"dismiss":4},
            {"kind":"alert","title":"T","message":{"kind":"text","id":"m","text":"M"},
             "actions":[{"kind":"button","id":"b","action":2,"role":"destructive","label":{"kind":"empty","id":"e"}}]},
            {"kind":"dialog","title":"D","titleVisible":true,"actions":[],"anchor":"x","dismiss":9},
            {"kind":"cover","content":{"kind":"empty","id":"c"}}]});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        let ViewKind::Presentation { content, layers } = &n.kind else { panic!() };
        let ViewKind::Text { runs, .. } = &content.kind else { panic!() };
        assert_eq!(runs[0].font_weight, Some(Weight::Bold));
        assert!(runs[0].italic && runs[1].underline && !runs[1].italic && runs[1].foreground_color.is_some());
        assert!(matches!(&layers[0], Layer::Sheet { detents, drag_indicator: true, dismiss: Some(4), .. } if detents[1] == Detent::Fraction { value: 0.8 }));
        let Layer::Alert { actions, .. } = &layers[1] else { panic!() };
        assert!(matches!(actions[0].kind, ViewKind::Button { role: Some(ButtonRole::Destructive), .. }));
        assert!(matches!(&layers[2], Layer::Dialog { title_visible: true, dismiss: Some(9), .. }) && matches!(&layers[3], Layer::Cover { .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn symbol_nodes_parse() {
        let j = json!({"kind":"label","id":"l","title":{"kind":"text","id":"l.t","text":"A"},
          "icon":{"kind":"image","id":"l.i","systemName":"star.fill","resizable":true,"contentMode":"fit"},
          "modifiers":[{"imageScale":"large"},{"font":{"style":"footnote","weight":"semibold"}}]});
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers[0], Modifier::ImageScale(ImageScale::Large));
        let g: ViewNode = serde_json::from_value(json!({"kind":"empty","id":"0","modifiers":[{"offset":{"x":3,"y":-4}},{"gesture":{"key":"0","pans":true}}]})).unwrap();
        assert_eq!(g.modifiers, vec![Modifier::Offset(Offset { x: 3.0, y: -4.0 }), Modifier::Gesture(GestureMod { key: "0".into(), pans: true, axis: None })]);
        assert_eq!(n.modifiers[1], Modifier::Font(Font::Style { style: TextStyle::Footnote, weight: Some(Weight::Semibold) }));
        let ViewKind::Label { icon, .. } = &n.kind else { panic!() };
        assert!(matches!(&icon.kind, ViewKind::Image { resizable: true, content_mode: Some(ContentMode::Fit), scale: None, .. }));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn section_t_lists_round_trip() {
        let t = |id: &str, s: &str| json!({"kind": "text", "id": id, "text": s});
        let list: ViewNode = serde_json::from_value(json!({
            "kind": "list", "id": "l", "style": "sidebar", "sections": [
                {"header": "Fruit", "expanded": false, "toggle": 4, "rows": []},
                {"headerView": t("h", "Custom"), "footerView": t("f", "Foot"), "rows": [
                    {"kind": "text", "id": "r", "text": "Row", "modifiers": [{"listRow": {
                        "swipe": {"offset": -12.5, "open": "trailing", "trailing": [
                            {"label": t("d", "Delete"), "action": 7, "role": "destructive"},
                            {"label": t("p", "Flag"), "action": 8, "tint": {"r": 1, "g": 0.5, "b": 0, "a": 1}}
                        ], "leading": [{"label": t("x", "Read"), "action": 9}], "full": true},
                        "gesture": {"key": "r#swipe", "pans": true, "axis": "horizontal"},
                        "reorder": {"key": "r#move", "pans": true, "axis": "vertical"}, "lift": 30
                    }}]}
                ]}
            ]
        })).unwrap();
        let ViewKind::List { style, sections } = &list.kind else { panic!() };
        assert_eq!(*style, ListStyle::Sidebar);
        assert_eq!((sections[0].expanded, sections[0].toggle), (Some(false), Some(4)));
        assert!(sections[1].header_view.is_some() && sections[1].footer_view.is_some() && sections[1].header.is_none());
        let row = &sections[1].rows[0];
        let Modifier::ListRow(lr) = &row.modifiers[0] else { panic!() };
        let swipe = lr.swipe.as_ref().unwrap();
        assert_eq!((swipe.offset, swipe.open, swipe.trailing.len(), swipe.leading.len(), swipe.full), (-12.5, Some(HEdge::Trailing), 2, 1, true));
        assert_eq!(swipe.trailing[0].role, Some(ButtonRole::Destructive));
        assert!(swipe.trailing[1].tint.is_some() && swipe.leading[0].tint.is_none());
        assert_eq!(lr.gesture.as_ref().unwrap().axis, Some(Axis::Horizontal));
        assert_eq!((lr.reorder.as_ref().unwrap().axis, lr.lift), (Some(Axis::Vertical), 30.0));
        for style in ["insetGrouped", "grouped", "plain", "inset", "sidebar"] {
            let l: ViewNode = serde_json::from_value(json!({"kind": "list", "id": "l", "style": style})).unwrap();
            assert_eq!(serde_json::to_value(&l).unwrap()["style"], style);
        }
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&list).unwrap()).unwrap();
        assert_eq!(list, again);
        // Older guests: no axis, string headers only.
        let g: GestureMod = serde_json::from_value(json!({"key": "k", "pans": true})).unwrap();
        assert_eq!(g.axis, None);
    }

    #[test]
    fn section_q_round_trip() {
        let t = |id: &str, s: &str| json!({"kind": "text", "id": id, "text": s});
        let gauge: ViewNode = serde_json::from_value(json!({
            "kind": "gauge", "id": "g", "style": "accessoryCircular", "value": 0.3,
            "label": t("g/l", "Arc"), "currentValueLabel": t("g/c", "30"),
            "minimumValueLabel": t("g/min", "0"), "maximumValueLabel": t("g/max", "100")
        })).unwrap();
        let ViewKind::Gauge { style, value, label, current_value_label, minimum_value_label, maximum_value_label } = &gauge.kind else { panic!() };
        assert_eq!((*style, *value), (GaugeStyle::AccessoryCircular, 0.3));
        assert!(label.is_some() && current_value_label.is_some() && minimum_value_label.is_some() && maximum_value_label.is_some());
        let plain: ViewNode = serde_json::from_value(json!({"kind": "gauge", "id": "g2", "value": 1.0})).unwrap();
        assert!(matches!(plain.kind, ViewKind::Gauge { style: GaugeStyle::Automatic, label: None, .. }));
        let wheel: ViewNode = serde_json::from_value(json!({
            "kind": "picker", "id": "p", "style": "wheel", "label": t("p/l", "Hour"),
            "options": [{"text": "9:00", "action": 3}], "selected": 0
        })).unwrap();
        assert!(matches!(wheel.kind, ViewKind::Picker { style: PickerStyle::Wheel, .. }));
        let progress: ViewNode = serde_json::from_value(json!({
            "kind": "progress", "id": "pr", "value": 0.3, "currentValueLabel": t("pr/c", "30%")
        })).unwrap();
        assert!(matches!(progress.kind, ViewKind::Progress { current_value_label: Some(_), .. }));
        let toggle: ViewNode = serde_json::from_value(json!({
            "kind": "button", "id": "b", "action": 1, "selected": true, "label": t("b/l", "Bold")
        })).unwrap();
        assert!(matches!(toggle.kind, ViewKind::Button { selected: Some(true), .. }));
        let menu: Layer = serde_json::from_value(json!({
            "kind": "menu", "anchor": "m", "centered": true, "dismiss": 9, "items": [
                {"text": "Duplicate", "action": 1, "systemImage": "plus.square.on.square"},
                {"text": "Delete", "action": 2, "systemImage": "trash", "destructive": true, "divider": true},
                {"text": "Off", "action": 0, "disabled": true}
            ]
        })).unwrap();
        let Layer::Menu { items, centered: true, .. } = &menu else { panic!() };
        assert_eq!(items[0].system_image.as_deref(), Some("plus.square.on.square"));
        assert!(items[1].destructive && items[1].divider && !items[0].divider);
        assert!(items[2].disabled && !items[1].disabled);
        for node in [&gauge, &plain, &wheel, &progress, &toggle] {
            let again: ViewNode = serde_json::from_value(serde_json::to_value(node).unwrap()).unwrap();
            assert_eq!(node, &again);
        }
        let again: Layer = serde_json::from_value(serde_json::to_value(&menu).unwrap()).unwrap();
        assert_eq!(menu, again);
        // A picker menu (no new fields) still parses.
        let old: Layer = serde_json::from_value(json!({"kind": "menu", "items": [{"text": "A", "action": 1, "checked": true}]})).unwrap();
        assert!(matches!(old, Layer::Menu { centered: false, .. }));
    }
    #[test]
    fn presentation_layers_section_u_round_trip() {
        let layers: Vec<Layer> = serde_json::from_value(json!([
            {"kind": "sheet", "content": {"kind": "empty", "id": "c"}, "cornerRadius": 0.0, "contentInteraction": "scrolls",
             "backgroundInteraction": {"kind": "enabled", "upThrough": {"kind": "medium"}}, "background": {"kind": "empty", "id": "bg"}},
            {"kind": "popover", "content": {"kind": "empty", "id": "c"}, "arrow": true, "arrowEdge": "top", "attachment": {"point": [0.5, 1.0]}},
            {"kind": "contextMenu", "anchor": "a", "preview": {"kind": "empty", "id": "p"}, "items": [{"text": "Copy", "action": 2}], "dismiss": 3},
            {"kind": "share", "items": ["x", "y"], "title": "T", "subject": "S", "message": "M", "dismiss": 4, "copy": 5, "url": true,
             "image": {"kind": "image", "id": "i", "systemName": "globe"}, "activities": {"More": 6}},
        ]))
        .unwrap();
        assert!(matches!(&layers[0], Layer::Sheet { corner_radius: Some(r), content_interaction: Some(ContentInteraction::Scrolls),
            background_interaction: Some(BackgroundInteraction::Enabled { up_through: Some(Detent::Medium) }), background: Some(_), .. } if *r == 0.0));
        assert!(matches!(&layers[1], Layer::Popover { arrow: true, attachment: Some(Attachment::Point([x, y])), arrow_edge: Some(e), .. } if *x == 0.5 && *y == 1.0 && e == "top"));
        assert!(matches!(&layers[2], Layer::ContextMenu { preview: Some(_), dismiss: Some(3), .. }));
        assert!(matches!(&layers[3], Layer::Share { copy: Some(5), dismiss: Some(4), url: true, image: Some(_), icon: None, activities, .. } if activities["More"] == 6));
        for l in &layers {
            let again: Layer = serde_json::from_value(serde_json::to_value(l).unwrap()).unwrap();
            assert_eq!(&again, l);
        }
        // Older hosts' popovers (a compact DatePicker's) still parse: no arrow, no attachment.
        let old: Layer = serde_json::from_value(json!({"kind": "popover", "content": {"kind": "empty", "id": "c"}})).unwrap();
        assert!(matches!(old, Layer::Popover { arrow: false, attachment: None, .. }));
        let paste: ViewNode = serde_json::from_value(json!({"kind": "button", "id": "b", "action": 1, "style": "pasteButton", "label": {"kind": "empty", "id": "l"}})).unwrap();
        assert!(matches!(paste.kind, ViewKind::Button { style: ButtonStyle::PasteButton, .. }));
    }


    #[test]
    fn section_m_round_trip() {
        let j = json!({
            "kind": "grid", "id": "g", "alignment": "leading",
            "horizontalSpacing": 8.0, "verticalSpacing": 4.0,
            "rows": [{"alignment": "top", "cells": [
                {"kind": "text", "id": "a", "text": "A", "modifiers": [
                    {"id": "cell-a"}, {"gridCellColumns": 2}, {"pin": "header"},
                    {"contentMargins": {"top": 1.0, "leading": 2.0, "bottom": 3.0, "trailing": 4.0}},
                    {"scrollIndicators": {"vertical": "hidden"}},
                    {"scrollTargetBehavior": "viewAligned"},
                    {"scrollTargetLayout": true},
                    {"scrollPosition": null}
                ]}
            ]}]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        let ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows } = &n.kind else { panic!() };
        assert_eq!((*alignment, *horizontal_spacing, *vertical_spacing, rows[0].alignment), (Alignment::Leading, Some(8.0), Some(4.0), Some(Alignment::Top)));
        assert_eq!(rows[0].cells[0].modifiers[0], Modifier::Identity("cell-a".into()));
        assert_eq!(rows[0].cells[0].modifiers[1], Modifier::GridCellColumns(2));
        assert_eq!(rows[0].cells[0].modifiers[2], Modifier::Pin("header".into()));
        assert_eq!(rows[0].cells[0].modifiers[5], Modifier::ScrollTargetBehavior(ScrollSnap::ViewAligned));
        assert_eq!(rows[0].cells[0].modifiers[6], Modifier::ScrollTargetLayout(true));
        assert_eq!(rows[0].cells[0].modifiers[7], Modifier::ScrollPosition);
        let lazy: ViewNode = serde_json::from_value(json!({
            "kind": "lazyGrid", "id": "lg", "axis": "horizontal", "spacing": 8.0,
            "tracks": [{"size": "flexible", "min": 10.0, "max": "inf"}, {"size": "fixed", "value": 40.0}, {"size": "adaptive", "min": 20.0}],
            "children": [{"kind": "empty", "id": "c"}]
        })).unwrap();
        let ViewKind::LazyGrid { axis, tracks, .. } = &lazy.kind else { panic!() };
        assert_eq!(*axis, Axis::Horizontal);
        assert_eq!(tracks[0].max, Some(f64::INFINITY));
        assert_eq!(tracks[1].size, TrackSize::Fixed);
        assert_eq!(serde_json::to_value(&lazy).unwrap()["tracks"][0]["max"], "inf");
        let geo: ViewNode = serde_json::from_value(json!({
            "kind": "geometry", "id": "geo", "proposal": [100.0, 80.0],
            "child": {"kind": "empty", "id": "geo/0"}
        })).unwrap();
        assert!(matches!(geo.kind, ViewKind::Geometry { proposal, .. } if proposal == [100.0, 80.0]));
        let fits: ViewNode = serde_json::from_value(json!({
            "kind": "viewThatFits", "id": "f", "axes": "horizontal",
            "children": [{"kind": "empty", "id": "f/0"}]
        })).unwrap();
        assert!(matches!(fits.kind, ViewKind::ViewThatFits { axes: FitAxes::Horizontal, .. }));
        let stack: ViewNode = serde_json::from_value(json!({
            "kind": "vstack", "id": "v", "pinned": "headers", "children": []
        })).unwrap();
        assert!(matches!(stack.kind, ViewKind::VStack { pinned: Some(PinMode::Headers), .. }));
        let scroll: ViewNode = serde_json::from_value(json!({
            "kind": "scroll", "id": "s", "axis": "horizontal", "indicators": "hidden",
            "child": {"kind": "empty", "id": "s/0"}
        })).unwrap();
        assert!(matches!(scroll.kind, ViewKind::Scroll { indicators: Some(ref ind), .. } if ind == "hidden"));
        for node in [&n, &lazy, &geo, &fits, &stack, &scroll] {
            let again: ViewNode = serde_json::from_value(serde_json::to_value(node).unwrap()).unwrap();
            assert_eq!(node, &again);
        }
        // An ordinary stack still parses with no pinned field.
        let plain: ViewNode = serde_json::from_str(r#"{"kind":"hstack","id":"h","children":[]}"#).unwrap();
        assert!(matches!(plain.kind, ViewKind::HStack { pinned: None, .. }));
    }

    #[test]
    fn section_o_text_and_image_parse() {
        let j = json!({
            "kind": "text", "id": "t", "text": "Ab",
            "runs": [{"text": "A", "kerning": 1.5, "tracking": 0.5, "baselineOffset": 2.0, "fontDesign": "serif",
                      "fontWidth": "condensed", "monospaced": true, "monospacedDigit": true, "textScale": "secondary",
                      "underline": true, "underlinePattern": "dashDot", "underlineColor": {"semantic": "accent"}}],
            "modifiers": [
                {"kerning": 1.0}, {"tracking": 2.0}, {"baselineOffset": -1.0},
                {"fontDesign": "rounded"}, {"fontWidth": null}, {"monospaced": false}, {"monospacedDigit": true},
                {"textScale": "secondary"}, {"truncationMode": "middle"},
                {"lineLimit": {"min": 2, "max": 3, "reserves": true}},
                {"underline": {"active": true, "pattern": "dot", "color": {"semantic": "primary"}}},
                {"italic": true}, {"textCase": "uppercase"}, {"allowsTightening": true}, {"minimumScaleFactor": 0.7},
                {"symbolRenderingMode": "hierarchical"}, {"symbolVariableValueMode": "draw"},
                {"symbolColorRenderingMode": "gradient"}, {"allowedDynamicRange": "high"},
                {"font": {"size": 18, "weight": "bold", "design": "monospaced"}},
                {"strikethroughStyle": {"active": true, "pattern": "dash"}}
            ]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        let ViewKind::Text { runs, .. } = &n.kind else { panic!() };
        assert_eq!(runs[0].kerning, Some(1.5));
        assert_eq!(runs[0].font_design, Some(FontDesign::Serif));
        assert_eq!(runs[0].underline_pattern, Some(LinePattern::DashDot));
        assert_eq!(n.modifiers[8], Modifier::TruncationMode(Truncation::Middle));
        assert_eq!(n.modifiers[9], Modifier::LineLimit(Some(LineLimit { min: Some(2), max: Some(3), reserves: true })));
        assert_eq!(n.modifiers[19], Modifier::Font(Font::System { size: 18.0, weight: Weight::Bold, design: Some(FontDesign::Monospaced) }));
        assert_eq!(n.modifiers[20], Modifier::StrikethroughStyle(Box::new(LineStyle { active: true, pattern: LinePattern::Dash, color: None })));
        let img: ViewNode = serde_json::from_value(json!({
            "kind": "image", "id": "i", "bitmap": {"path": "/tmp/m.png", "width": 16.0, "height": 16.0, "scale": 2.0},
            "interpolation": "none", "antialiased": false, "renderingMode": "original", "symbolRenderingMode": "multicolor"
        })).unwrap();
        let ViewKind::Image { system_name, bitmap, interpolation, antialiased, rendering_mode, symbol_rendering_mode, .. } = &img.kind else { panic!() };
        assert!(system_name.is_empty());
        assert_eq!(bitmap.as_ref().unwrap().scale, 2.0);
        assert_eq!(*interpolation, Some(Interpolation::None));
        assert_eq!(*antialiased, Some(false));
        assert_eq!(*rendering_mode, Some(TemplateMode::Original));
        assert_eq!(*symbol_rendering_mode, Some(SymbolMode::Multicolor));
        let unlimited: ViewNode = serde_json::from_value(json!({"kind":"empty","id":"e","modifiers":[{"lineLimit": null}]})).unwrap();
        assert_eq!(unlimited.modifiers[0], Modifier::LineLimit(None));
        for node in [&n, &img, &unlimited] {
            let again: ViewNode = serde_json::from_value(serde_json::to_value(node).unwrap()).unwrap();
            assert_eq!(node, &again);
        }
    }

    #[test]
    fn section_n_shapes_and_paths_parse() {
        let j = json!({
            "kind": "vstack", "id": "0",
            "modifiers": [{"containerShape": {"shape": "roundedRectangle", "cornerRadius": 24.0, "cornerStyle": "continuous"}}],
            "children": [
                {"kind": "shape", "id": "0/0", "shape": "ellipse", "fill": {"semantic": "accent"}},
                {"kind": "shape", "id": "0/1", "shape": "unevenRoundedRectangle", "cornerRadii": [20.0, 4.0, 16.0, 8.0], "cornerStyle": "circular",
                 "stroke": {"color": {"semantic": "primary"}, "lineWidth": 3.0, "lineCap": "round", "lineJoin": "bevel", "miterLimit": 5.0, "dash": [8.0, 4.0], "dashPhase": 2.0, "antialiased": false, "inset": 1.5}},
                {"kind": "shape", "id": "0/2", "shape": "containerRelative"},
                {"kind": "shape", "id": "0/3", "shape": "concentricRectangle",
                 "concentric": {"corners": [{"concentric": null}, {"concentric": 6.0}, {"fixed": 12.0}, {"concentric": null}], "uniform": "top"}},
                {"kind": "shape", "id": "0/4", "shape": "path",
                 "path": [{"m": [0.0, 0.0]}, {"l": [40.0, 0.0]}, {"q": [50.0, 20.0, 40.0, 40.0]}, {"c": [30.0, 50.0, 10.0, 50.0, 0.0, 40.0]}, "z"],
                 "pathSize": [40.0, 40.0], "custom": true, "sizeHint": {"fixed": [40.0, 40.0]}, "mirrorRtl": true,
                 "fill": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0},
                 "fillStyle": {"eoFill": true, "antialiased": false}}
            ]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert!(matches!(&n.modifiers[0], Modifier::ContainerShape(cs) if matches!(cs.as_ref(), ContainerShapeSpec::RoundedRectangle { corner_radius: Some(24.0), .. })));
        let ViewKind::VStack { children, .. } = &n.kind else { panic!() };
        assert!(matches!(children[0].kind, ViewKind::Shape { shape: ShapeKind::Ellipse, .. }));
        let ViewKind::Shape { extra, .. } = &children[1].kind else { panic!() };
        let (Some(st), Some(radii)) = (&extra.stroke, &extra.corner_radii) else { panic!() };
        assert_eq!(*radii, [20.0, 4.0, 16.0, 8.0]);
        assert_eq!(st.line_cap, LineCap::Round);
        assert_eq!(st.line_join, LineJoin::Bevel);
        assert_eq!(st.dash, vec![8.0, 4.0]);
        assert_eq!(st.inset, 1.5);
        assert!(!st.antialiased);
        let ViewKind::Shape { extra: ex4, .. } = &children[4].kind else { panic!() };
        let (Some(ops), Some(fs), Some(ShapeSizeHint::Fixed(sz))) = (&ex4.path, &ex4.fill_style, &ex4.size_hint) else { panic!() };
        assert!(ex4.custom && ex4.mirror_rtl);
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[4], PathOp::Z);
        assert!(fs.eo_fill && !fs.antialiased);
        assert_eq!(*sz, [40.0, 40.0]);
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn section_o_round_trip() {
        let j = json!({
            "kind": "text", "id": "t", "text": "42",
            "modifiers": [
                {"animation": {"curve": {"spring": {"mass": 1.0, "stiffness": 157.9, "damping": 25.1, "initialVelocity": 0.0, "settling": 0.5}}, "duration": 0.5, "delay": 0.1, "speed": 1.5, "repeat": "forever", "autoreverses": false, "token": 7}},
                {"animation": null},
                {"transition": {"insertion": {"opacity": 0.0, "edge": "leading"}, "removal": {"scale": 0.5, "anchor": [0.5, 0.5], "offset": [10.0, -5.0]}}},
                {"matchedGeometry": {"id": "pill", "namespace": "ns1", "properties": "position", "anchor": [0.0, 0.5], "isSource": false}},
                {"contentTransition": "numericTextUp"}
            ]
        });
        let n: ViewNode = serde_json::from_value(j).unwrap();
        assert_eq!(n.modifiers.len(), 5);
        assert!(matches!(&n.modifiers[0], Modifier::Animation(Some(spec)) if spec.token == 7 && spec.repeat == Some(RepeatCount::Forever(ForeverTag::Forever)) && !spec.autoreverses));
        assert_eq!(n.modifiers[1], Modifier::Animation(None));
        assert!(matches!(&n.modifiers[2], Modifier::Transition(t) if t.insertion.edge == Some(TransitionEdge::Leading) && t.removal.scale == Some(0.5)));
        assert!(matches!(&n.modifiers[3], Modifier::MatchedGeometry(m) if m.id == "pill" && m.namespace == "ns1" && m.properties == MatchedProps::Position && !m.is_source));
        assert_eq!(n.modifiers[4], Modifier::ContentTransition(ContentTransitionKind::NumericTextUp));
        let again: ViewNode = serde_json::from_value(serde_json::to_value(&n).unwrap()).unwrap();
        assert_eq!(n, again);
    }
}
