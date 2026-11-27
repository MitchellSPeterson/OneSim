//! SwiftUI-semantics layout: parent proposes, child chooses, parent places.
//!
//! Stacks probe each child at 0 and infinity before the final proposal; `Ctx::memo`
//! caches results per (node, proposal) so nesting stays polynomial instead of 3^depth.
//!
//! Visual constants (row height, card radius, nav bar heights, control sizes) are measured from
//! an iOS 26 / iPhone 17 Pro simulator, see `tests/fixtures/showcase-ios/README.md`.

use onesim_protocol::{BarVisibility, NavigationChrome, NavigationSearch, ShapePaint, StyleReference, TextInputBorderShape, TextInputOptions, TextInputStyle, ToolbarRegion, VisualOp};
use std::sync::Arc;

use crate::device::Device;
use crate::scene::{Color, CustomShapeEval, Glyph, Node, Pager, Rect, Role, Scene, ScrollInfo, Shadow, SymbolRun, TextRun, VectorFill, VectorGeom, VectorShape, VectorStroke};
use crate::symbols::{self, SymbolSpec};
use crate::text::{FontSpec, Fonts, Span, TextAlign};
use onesim_protocol::{
    Alignment, AnimationSpec, Appearance, Axis, ButtonStyle, ConcentricCorners, ContainerShapeSpec, ContentMode, ContentTransitionKind, CornerStyleSpec, Detent, DisplayMode, DynamicTypeSize, Edges, FitAxes, Font, GaugeStyle, GlassShapeKind, GlassVariant, GridTrack, ImageScale, IosVersion, Layer, LineCap, LineJoin, ListStyle, MatchedGeometryMod, MatchedProps, Modifier, PageIndexBackground, PageIndexDisplay, PickerOption, PickerStyle, RoundedCornerStyle, ScrollSnap, Section, Semantic, ShapeKind, ShapeSizeHint, TabPage, TrackSize, TransitionSpec, UniformCorners, ViewKind, ViewNode,
};

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use unicode_segmentation::UnicodeSegmentation;

mod accessibility;
mod containers;
mod controls;
mod metrics;
mod modifiers;
mod navigation;
mod paint;
mod presentation;
mod share;
use presentation::{lifted_copy, SheetStyle};
mod shapes;

pub use crate::layout::common::SceneParams;
use crate::layout::common::{
    control_fill_for, rgba, switch_style, tinted, Embedded, ModalLayers, NavBack, Placement, TabBar,
    TabBarItem, KNOB, PILL, SWITCH, TAB_BAR_INSET,
};
use metrics::*;
use navigation::NavBar;
use paint::ResolvedPaint;
use shapes::{container_shape_layer, resolve_container_shapes};

/// Prepared layout for one static SwiftUI list. Clear it before replacing or mutating the view tree
/// or replacing its font system. Layout inputs are checked separately on each pass.
#[derive(Default)]
pub struct SwiftUiLayoutCache {
    list: Option<(ListLayoutKey, Arc<ListContent>)>,
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
    under_inline_bar: bool,
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
        // Header and footer views may hold host state (a nested scroll offset) the retained layout would freeze.
        if section.header_view.is_some() || section.footer_view.is_some() {
            return false;
        }
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


struct Ctx<'a> {
    paints: Vec<ResolvedPaint>,
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
    memo: HashMap<(usize, u64, u64), Arc<Lay>>,
    list_cache: Option<&'a mut SwiftUiLayoutCache>,
    /// Symbols that fell back to a placeholder.
    unsupported: BTreeSet<String>,
    /// Elevated grouped colors (content of a sheet in dark mode).
    elevated: bool,
    /// Inside a floating (non-large) sheet: lists are transparent over its glass.
    floating: bool,
    /// Laying out a list right under an inline navigation bar: a header-less first section starts 35pt down. Under a
    /// large title (its band holds that gap), in a sheet or on its own it starts flush.
    under_inline_bar: bool,
    /// An alert is up: prominent tints in the presenter turn gray.
    desat: bool,
    /// A sheet is up over this screen: its back button loses the glass circle (alerts keep it).
    sheet_up: bool,
    /// Animation specs referenced by `Env::anim` (keeps `Env` `Copy` and small).
    anims: Vec<AnimationSpec>,
    has_container_shapes: bool,
}


fn ctx<'a>(params: &'a SceneParams, fonts: &'a mut Fonts) -> Ctx<'a> {
    Ctx {
        paints: Vec::new(),
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
        under_inline_bar: false,
        desat: false,
        sheet_up: false,
        anims: Vec::new(),
        has_container_shapes: false,
    }
}

/// Draw the iOS status bar over content from the React Native layout.
pub(crate) fn rn_status_bar(nodes: &mut Vec<Node>, params: &SceneParams, fonts: &mut Fonts, device: &Device, opacity: f32, y: f64, light: f32) {
    let primary = Color::rgba(light, light, light, 1.0);
    let mix = |dark: f32, white: f32| (dark + (white - dark) * light) / 255.0;
    let signal = Color::rgba(mix(60.0, 235.0), mix(60.0, 235.0), mix(67.0, 245.0), 0.3);
    let lay = ctx(params, fonts).status_bar(params.viewport.0, device, params.ios, Some((primary, signal)));
    flatten(lay, 0.0, y, Some(0), opacity, nodes);
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
    let bar = NavBar { id, w, h: params.viewport.1, top, bar_h, title, large: large && title.is_some(), large_title, title_h, band, offset: 0.0, back, inline_title_bounds: None };
    flatten(cx.nav_bar(bar), 0.0, top, Some(parent), 1.0, out);
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

/// Presentation layers over a React Native screen: `alert` (`Alert.alert`, the scrim and card of `.alert`), `dialog`
/// without an anchor (`ActionSheetIOS.showActionSheetWithOptions`) and `share` (`showShareActionSheetWithOptions`, the
/// share sheet of `ShareLink`).
pub(crate) fn rn_alerts(params: &SceneParams, fonts: &mut Fonts, layers: &[Layer], out: &mut Vec<Node>) {
    let (w, h) = params.viewport;
    let geo = Geo { w, h, safe_top: params.device.safe_top, safe_bottom: params.device.safe_bottom, dev_bottom: params.device.safe_bottom, body: 17.0 };
    for (i, layer) in layers.iter().enumerate() {
        let own = match layer {
            Layer::Dialog { appearance, .. } | Layer::Share { appearance, .. } => *appearance,
            _ => None,
        };
        // `userInterfaceStyle`: this layer alone in the other appearance.
        let p = SceneParams { appearance: own.unwrap_or(params.appearance), ..params.reborrow() };
        let mut cx = ctx(&p, fonts);
        let lay = match layer {
            Layer::Alert { title, message, actions } => cx.alert(title, message.as_deref(), actions, &geo),
            // iPhone draws the centred card whatever the `anchor` (an iPad would point a popover at it).
            Layer::Dialog { title, title_visible, message, actions, dismiss, .. } => {
                cx.action_sheet(title_visible.then_some(title.as_str()), message.as_deref(), actions, *dismiss, &geo)
            }
            Layer::Share { items, title, dismiss, copy, activities, image, icon, url, .. } => {
                let t = title.clone().unwrap_or_else(|| if items.len() == 1 { items[0].clone() } else { format!("{} Items", items.len()) });
                // A link iOS has no page preview for (the guest does not fetch one) shows its host by a compass tile.
                let t = if *url && title.is_none() && image.is_none() && icon.is_none() { url_host(&t).unwrap_or(t) } else { t };
                let sheet = share::ShareSheet { title: &t, titled: title.is_some(), url: *url, preview: icon.as_deref().or(image.as_deref()),
                    copy: *copy, activities, dismiss: *dismiss, link_glyph: *url };
                cx.share_sheet(&format!("layer{i}"), &sheet, &geo)
            }
            _ => continue,
        };
        flatten(lay, 0.0, 0.0, Some(0), 1.0, out);
    }
}

/// `https://host:port/path` -> `host`.
fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_string())
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
    let kb_h = if params.chrome.keypad {
        crate::look::keypad_height(params.device, params.ios)
    } else {
        crate::look::keyboard_height(params.device, params.ios)
    };
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
    // Prominent tints under a presented layer turn gray; a context menu leaves them alone.
    let tints_gray = |ls: &[Layer]| ls.iter().any(|l| !matches!(l, Layer::ContextMenu { .. }));
    cx.desat = tints_gray(layers);
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
        cx.desat = tints_gray(&layers[i + 1..]);
        cx.sheet_up = layers[i + 1..].iter().any(|l| matches!(l, Layer::Sheet { .. }));
        let lay = match layer {
            Layer::Sheet { content, detents, drag_indicator, dismiss, background, corner_radius, background_interaction, content_interaction } => {
                let depth = layers[..i].iter().filter(|l| matches!(l, Layer::Sheet { .. })).count();
                let large = (h - safe_top - SHEET_TOP_GAP - depth as f64 * SHEET_STACK_STEP).max(1.0);
                let style = SheetStyle {
                    background: background.as_deref(),
                    corner_radius: *corner_radius,
                    interactive_up_through: match background_interaction {
                        Some(onesim_protocol::BackgroundInteraction::Enabled { up_through }) => {
                            Some(up_through.map_or(f64::INFINITY, |d| Ctx::detent_h(&d, h, large)))
                        }
                        _ => None,
                    },
                    scrolls_first: *content_interaction == Some(onesim_protocol::ContentInteraction::Scrolls),
                };
                cx.sheet(&id, content, detents, *drag_indicator, *dismiss, &style, params.sheets.get(&id).copied().unwrap_or_default(), depth, &geo)
            }
            Layer::Cover { content } => cx.cover(content, &geo),
            Layer::Alert { title, message, actions } => cx.alert(title, message.as_deref(), actions, &geo),
            Layer::Popover { anchor, content, dismiss, arrow, attachment, arrow_edge, background } => {
                let rect = anchor.as_ref().and_then(|a| nodes.iter().find(|n| &n.id == a)).map(|n| n.bounds);
                if *arrow {
                    cx.bubble(&id, content, rect, attachment.as_ref(), arrow_edge.as_deref(), background.as_deref(), *dismiss, &geo)
                } else {
                    cx.content_popover(&id, content, rect, *dismiss, &geo)
                }
            }
            Layer::Menu { anchor, items, dismiss, centered } => {
                let rect = anchor.as_ref().and_then(|a| nodes.iter().find(|n| &n.id == a)).map(|n| n.bounds);
                cx.menu(items, rect, *dismiss, *centered, &geo)
            }
            Layer::ContextMenu { anchor, items, dismiss, preview } => {
                // The anchor's outermost node: modifiers wrap the view in `<id>#…` layers (flattened first).
                let at = anchor.as_ref().and_then(|a| nodes.iter().position(|n| &n.id == a || n.id.strip_prefix(a.as_str()).is_some_and(|r| r.starts_with('#'))));
                let r = at.map_or(Rect { x: geo.w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 }, |i| nodes[i].bounds);
                let (cxp, cyp) = r.center();
                let lifted = Rect { x: cxp - r.w * CONTEXT_LIFT / 2.0, y: cyp - r.h * CONTEXT_LIFT / 2.0, w: r.w * CONTEXT_LIFT, h: r.h * CONTEXT_LIFT };
                let (under, menu, _) = cx.context_menu(&id, items, lifted, preview.as_deref(), *dismiss, &geo);
                flatten(under, 0.0, 0.0, Some(0), 1.0, &mut nodes);
                if let (Some(i), None) = (at, preview) {
                    let copy = lifted_copy(&nodes, i, CONTEXT_LIFT, &format!("{id}#lift/"));
                    // The view leaves its place while lifted.
                    for n in &mut nodes[i..i + copy.len()] {
                        n.opacity = 0.0;
                    }
                    nodes.extend(copy);
                }
                menu
            }
            Layer::Share { items, title, dismiss, copy, activities, image, icon, url, .. } => {
                // The header names the item (or the preview's title, drawn darker); several items are counted.
                let t = title.clone().unwrap_or_else(|| if items.len() == 1 { items[0].clone() } else { format!("{} Items", items.len()) });
                let sheet = share::ShareSheet { title: &t, titled: title.is_some(), url: *url, preview: icon.as_deref().or(image.as_deref()),
                    copy: *copy, activities, dismiss: *dismiss, link_glyph: false };
                cx.share_sheet(&id, &sheet, &geo)
            }
            Layer::Dialog { title, title_visible, message, actions, anchor, dismiss, .. } => {
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
    if cx.has_container_shapes {
        resolve_container_shapes(&mut nodes);
    }
    if params.status_bar {
        let lay = cx.status_bar(w, params.device, params.ios, None);
        flatten(lay, 0.0, 0.0, Some(0), 1.0, &mut nodes);
    }
    let dark = params.appearance == Appearance::Dark;
    if kb {
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift, params.chrome.phone_symbols, params.device,
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
    accessibility::apply(&mut nodes);
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
        n.decoration |= parent.is_some_and(|i| out[i].decoration);
        if n.decoration {
            n.strip_interactive();
        }
        opacity *= n.opacity;
        n.opacity = opacity;
        if n.composite.is_some() {
            opacity = 1.0;
        }
        out.push(*n);
        parent = Some(out.len() - 1);
    }
    if let Some(embedded) = lay.embedded {
        embedded.place((x, y), parent, opacity, out, modals);
    }
    if let Some((y_off, shared)) = lay.shared_kids {
        for (dx, dy, k) in &shared.kids {
            flatten_tree_ref(k, x + dx, y + dy + y_off, parent, opacity, out, modals);
        }
    }
    for (dx, dy, k) in lay.kids {
        flatten_tree(k, x + dx, y + dy, parent, opacity, out, modals);
    }
}

fn flatten_tree_ref(lay: &Lay, x: f64, y: f64, mut parent: Option<usize>, mut opacity: f32, out: &mut Vec<Node>, modals: &mut ModalLayers) {
    if let Some(n) = &lay.node {
        let mut n = (**n).clone();
        let (l, t, r, b) = lay.infl;
        n.bounds = Rect { x: x - l, y: y - t, w: lay.size.w + l + r, h: lay.size.h + t + b };
        if let Some(im) = n.image.as_mut() {
            im.dest.x += x;
            im.dest.y += y;
        }
        n.parent = parent;
        n.decoration |= parent.is_some_and(|i| out[i].decoration);
        if n.decoration {
            n.strip_interactive();
        }
        opacity *= n.opacity;
        n.opacity = opacity;
        if n.composite.is_some() {
            opacity = 1.0;
        }
        out.push(n);
        parent = Some(out.len() - 1);
    }
    if let Some(embedded) = &lay.embedded {
        embedded.place((x, y), parent, opacity, out, modals);
    }
    if let Some((y_off, shared)) = &lay.shared_kids {
        for (dx, dy, k) in &shared.kids {
            flatten_tree_ref(k, x + dx, y + dy + y_off, parent, opacity, out, modals);
        }
    }
    for (dx, dy, k) in &lay.kids {
        flatten_tree_ref(k, x + dx, y + dy, parent, opacity, out, modals);
    }
}


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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const HOME: &str = include_str!("../../../tests/data/home.json");
    const CONTROLS: &str = include_str!("../../../tests/data/controls.json");
    const TODOS: &str = include_str!("../../../tests/data/todos.json");
    const TYPOGRAPHY: &str = include_str!("../../../tests/data/typography.json");

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

    fn scene_ios(json: &str, ios: IosVersion) -> Scene {
        let v: ViewNode = serde_json::from_str(json).unwrap();
        let dev = crate::device::device("iphone-18-pro").unwrap();
        let params = SceneParams { viewport: (402.0, 874.0), device: dev, scale: 3.0, appearance: Appearance::Light, dynamic_type: DynamicTypeSize::Large,
            ios, scroll: &HashMap::new(), focus: None, sheets: &HashMap::new(), overscroll: false, status_bar: true, chrome: Default::default() };
        layout(&v, &params, &mut Fonts::new())
    }

    fn phone(json: &str) -> Scene {
        scene_with(json, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), None)
    }

    fn by_id<'a>(s: &'a Scene, id: &str) -> &'a Node {
        s.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
    }

    fn ui_by_id<'a>(n: &'a onesim_protocol::UiNode, id: &str) -> &'a onesim_protocol::UiNode {
        fn walk<'a>(n: &'a onesim_protocol::UiNode, id: &str) -> Option<&'a onesim_protocol::UiNode> {
            if n.id == id {
                return Some(n);
            }
            n.children.iter().find_map(|c| walk(c, id))
        }
        walk(n, id).unwrap_or_else(|| panic!("no ui node {id}"))
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 0.2, "{a} != {b}");
    }

    #[test]
    fn gradient_stops_paint_through_an_even_odd_path() {
        let scene = scene(r#"{
            "kind":"shape","id":"paint","shape":"path",
            "path":[{"m":[0,0]},{"l":[80,0]},{"l":[80,40]},{"l":[0,40]},"z",
                    {"m":[30,10]},{"l":[50,10]},{"l":[50,30]},{"l":[30,30]},"z"],
            "fillStyle":{"eoFill":true},
            "fill":{"gradient":{"kind":"linear","start":[0,0],"end":[1,0],"stops":[
                {"location":0.5,"color":{"r":1,"g":0,"b":0,"a":1}},
                {"location":0.5,"color":{"r":0,"g":0,"b":1,"a":1}}
            ]}}
        }"#, (80.0, 40.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let rgb = |x, y| {
            let p = pixels.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        assert_eq!(rgb(5, 20), (255, 0, 0));
        assert_eq!(rgb(40, 20), (255, 255, 255));
        assert_eq!(rgb(75, 20), (0, 0, 255));
    }

    #[test]
    fn masks_and_effects_preserve_layout_and_apply_in_modifier_order() {
        let scene = scene(r#"{
            "kind":"color","id":"content","color":{"r":1,"g":0,"b":0,"a":1},
            "modifiers":[
                {"mask":{"view":{"kind":"color","id":"mask","color":{"r":0,"g":0,"b":0,"a":0.5},
                    "modifiers":[{"frame":{"width":20,"height":20}}]},"alignment":"topLeading"}},
                {"effect":{"kind":"colorInvert"}},
                {"effect":{"kind":"affine","matrix":[1,0,0,1,30,10],"anchor":[0,0]}}
            ]}"#, (80.0, 40.0));
        let n = by_id(&scene, "content");
        assert_eq!((n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h), (0.0, 0.0, 80.0, 40.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let p = pixels.pixel(35, 15).unwrap();
        assert!((126..=128).contains(&p.red()));
        assert_eq!((p.green(), p.blue()), (255, 255));
        assert_eq!(pixels.pixel(5, 15).unwrap().red(), 255);
    }

    #[test]
    fn canvas_and_visual_effect_request_their_own_layout_frames() {
        let scene = scene(r#"{"kind":"visualEffect","id":"effect","frame":[0,0,100,100],
            "child":{"kind":"canvas","id":"canvas","children":[],
                "modifiers":[{"frame":{"width":40,"height":0}}]}}"#, (80.0, 60.0));
        let frames = scene.pending_shape_frames();
        assert_eq!(frames["canvas"], frames["effect"]);
        assert_eq!(frames["canvas"][2..], [40.0, 0.0]);
    }

    #[test]
    fn invalid_mesh_uses_its_background_and_empty_gradient_is_clear() {
        let scene = scene(r#"{"kind":"vstack","id":"stack","spacing":0,"children":[
            {"kind":"shape","id":"mesh","shape":"rectangle","fill":{"gradient":{"kind":"mesh",
                "width":4294967295,"height":2,"points":[],"colors":[],
                "background":{"r":0,"g":1,"b":0,"a":1},"smoothsColors":true}},
                "modifiers":[{"frame":{"height":30}}]},
            {"kind":"shape","id":"empty","shape":"rectangle","fill":{"gradient":{"kind":"linear","start":[0,0],"end":[0,0],"stops":[]}},
                "modifiers":[{"frame":{"height":30}}]}]}"#, (60.0, 60.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let p = pixels.pixel(20, 15).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (0, 255, 0));
        let p = pixels.pixel(20, 45).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255));
    }

    #[test]
    fn long_effect_chain_renders_and_releases_the_previous_display_frame() {
        use crate::display::{DisplayList, Op};

        let json = serde_json::json!({
            "kind": "color", "id": "red", "color": {"r": 1, "g": 0, "b": 0, "a": 1},
            "modifiers": vec![serde_json::json!({"effect":{"kind":"colorInvert"}}); 301]
        });
        let scene = scene(&json.to_string(), (8.0, 8.0));
        let mut fonts = Fonts::with_fallback(true);
        let cpu = crate::render(&scene, &mut fonts, 1.0);
        let px = cpu.pixel(4, 4).unwrap();
        assert_eq!((px.red(), px.green(), px.blue(), px.alpha()), (0, 255, 255, 255));

        let mut builder = crate::Builder::new();
        let built = builder.build(&scene, &mut fonts, 1.0);
        let frame = DisplayList::decode(&built.encode()).unwrap();
        let [Op::Fill { .. }, Op::Sprite { id, x: 0, y: 0 }] = frame.ops.as_slice() else {
            panic!("expected background Fill + bounded Op::Sprite, got {:?}", frame.ops)
        };
        assert_eq!(frame.added.len(), 1);
        assert_eq!(frame.added[0].id, *id);
        assert_eq!((frame.added[0].width, frame.added[0].height), (8, 8));
        assert_eq!(frame.added[0].rgba, cpu.data());

        // Second build of the same scene is a cache hit.
        let cached = builder.build(&scene, &mut fonts, 1.0);
        assert!(cached.added.is_empty());
        assert!(cached.dropped.is_empty());

        // Building an empty scene past CACHE_TTL_FRAMES drops the cached sprite.
        let empty = crate::Scene {
            size: (8.0, 8.0),
            appearance: onesim_protocol::Appearance::Light,
            ios: Default::default(),
            nodes: vec![],
            unsupported_symbols: vec![],
            notes: vec![],
        };
        let mut dropped = Vec::new();
        for _ in 0..=crate::display_build::CACHE_TTL_FRAMES {
            dropped.extend(builder.build(&empty, &mut fonts, 1.0).dropped);
        }
        assert_eq!(dropped, vec![*id]);
    }

    #[test]
    fn canvas_copy_respects_a_clip_at_composite_time() {
        let scene = scene(r#"{"kind":"canvas","id":"canvas","proposal":[40,20],"children":[
            {"kind":"color","id":"under","color":{"r":1,"g":0,"b":0,"a":1}},
            {"kind":"color","id":"over","color":{"r":0,"g":0,"b":1,"a":1},"modifiers":[
                {"mask":{"view":{"kind":"color","id":"clip","color":{"r":1,"g":1,"b":1,"a":1},
                    "modifiers":[{"frame":{"width":20,"height":20}}]},"alignment":"leading"}},
                {"effect":{"kind":"blend","mode":"copy"}}]}
        ]}"#, (40.0, 20.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let left = pixels.pixel(10, 10).unwrap();
        let right = pixels.pixel(30, 10).unwrap();
        assert_eq!((left.red(), left.blue()), (0, 255));
        assert_eq!((right.red(), right.blue()), (255, 0));
    }

    #[test]
    fn canvas_copy_preserves_coverage_of_transformed_and_transparent_paths() {
        let scene = scene(r#"{"kind":"canvas","id":"canvas","proposal":[40,20],"children":[
            {"kind":"color","id":"under","color":{"r":1,"g":0,"b":0,"a":1}},
            {"kind":"shape","id":"blue","shape":"path","fill":{"r":0,"g":0,"b":1,"a":1},
             "path":[{"m":[0,0]},{"l":[10,0]},{"l":[10,20]},{"l":[0,20]},"z"],
             "modifiers":[{"effect":{"kind":"affine","matrix":[1,0,0,1,10,0],"anchor":[0,0]}},
                          {"effect":{"kind":"blend","mode":"copy"}}]},
            {"kind":"shape","id":"clear","shape":"path","fill":{"r":0,"g":0,"b":0,"a":0},
             "path":[{"m":[25,0]},{"l":[35,0]},{"l":[35,20]},{"l":[25,20]},"z"],
             "modifiers":[{"effect":{"kind":"blend","mode":"copy"}}]}
        ]}"#, (40.0, 20.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let rgb = |x| {
            let p = pixels.pixel(x, 10).unwrap();
            (p.red(), p.green(), p.blue())
        };
        assert_eq!(rgb(5), (255, 0, 0));
        assert_eq!(rgb(15), (0, 0, 255));
        assert_eq!(rgb(30), (255, 255, 255));
        assert_eq!(rgb(38), (255, 0, 0));
    }

    #[test]
    fn hierarchical_fills_apply_to_the_inherited_paint() {
        let scene = scene(r#"{"kind":"vstack","id":"stack","spacing":0,"children":[
            {"kind":"shape","id":"level","shape":"rectangle","fill":{"semantic":"secondary","hierarchical":true},
                "modifiers":[{"foregroundColor":{"r":1,"g":0,"b":0,"a":1}},{"frame":{"height":20}}]},
            {"kind":"shape","id":"quinary","shape":"rectangle","fill":{"style":"quinaryFill"},
                "modifiers":[{"frame":{"height":20}}]}
        ]}"#, (40.0, 40.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let p = pixels.pixel(20, 10).unwrap();
        assert_eq!(p.red(), 255);
        assert!((100..=104).contains(&p.green()));
        assert!(pixels.pixel(20, 30).unwrap().red() < 255);
    }

    #[test]
    fn a_smaller_background_uses_center_alignment_by_default() {
        let scene = scene(r#"{"kind":"rectangle","id":"view","modifiers":[
            {"background":{"kind":"rectangle","id":"bg","modifiers":[{"frame":{"width":10,"height":8}}]}}
        ]}"#, (40.0, 20.0));
        let bg = by_id(&scene, "bg");
        assert_eq!((bg.bounds.x, bg.bounds.y, bg.bounds.w, bg.bounds.h), (15.0, 6.0, 10.0, 8.0));
    }

    #[test]
    fn foreground_palette_selects_explicit_secondary_and_tertiary_paints() {
        let scene = scene(r#"{"kind":"vstack","id":"stack","spacing":0,"children":[
            {"kind":"shape","id":"blue","shape":"rectangle","fill":{"semantic":"secondary","hierarchical":true}},
            {"kind":"shape","id":"green","shape":"rectangle","fill":{"semantic":"tertiary","hierarchical":true}}],
            "modifiers":[{"foregroundPalette":{"primary":{"r":1,"g":0,"b":0,"a":1},
                "secondary":{"r":0,"g":0,"b":1,"a":1},"tertiary":{"r":0,"g":1,"b":0,"a":1}}}]}"#, (40.0, 40.0));
        let pixels = crate::render(&scene, &mut Fonts::with_fallback(true), 1.0);
        let upper = pixels.pixel(20, 10).unwrap();
        let lower = pixels.pixel(20, 30).unwrap();
        assert_eq!((upper.red(), upper.green(), upper.blue()), (0, 0, 255));
        assert_eq!((lower.red(), lower.green(), lower.blue()), (0, 255, 0));
    }

    fn list_json(style: &str, extra_sections: &str) -> String {
        format!(r#"{{"kind":"list","id":"l","style":"{style}","sections":[
            {{"header":"Fruit","footer":"Two fruits.","rows":[{{"kind":"text","id":"a","text":"Apple"}},{{"kind":"text","id":"b","text":"Banana"}}]}}
            {extra_sections}]}}"#)
    }

    #[test]
    fn grouped_list_sections_span_the_width_with_hairlines() {
        let s = scene(&list_json("grouped", ""), (402.0, 600.0));
        let sec = by_id(&s, "l#s0");
        assert_eq!((sec.bounds.x, sec.bounds.w, sec.radius), (0.0, 402.0, 0.0));
        assert!(sec.fill.is_some(), "white section on the grouped background");
        let top = by_id(&s, "l#s0#top");
        assert_eq!((top.bounds.x, top.bounds.w, top.bounds.y), (0.0, 402.0, sec.bounds.y));
        // The last row's bottom line runs to the screen edge; rows between are inset by the margin.
        let mid = by_id(&s, "l#s0#r0#sep");
        assert!(mid.bounds.x >= 16.0 && mid.bounds.x + mid.bounds.w <= 386.0 + 0.01, "{:?}", mid.bounds);
        assert_eq!(by_id(&s, "l#s0#r1#sep").bounds.x + by_id(&s, "l#s0#r1#sep").bounds.w, 402.0);
        assert!(s.nodes[0].fill.is_some());
    }

    #[test]
    fn plain_list_puts_footers_in_rows_and_gaps_between_sections() {
        let s = scene(&list_json("plain", r#",{"header":"Veg","rows":[{"kind":"text","id":"c","text":"Carrot"}]}"#), (402.0, 600.0));
        let sec0 = by_id(&s, "l#s0");
        assert!(sec0.fill.is_none() && sec0.radius == 0.0);
        // Header 22pt below the top (40.33pt header cell), rows full width with a separator under every row.
        near(by_id(&s, "l#s0#header").bounds.y - sec0.bounds.y, -(40.33 - (40.33 - by_id(&s, "l#s0#header").bounds.h) / 2.0));
        assert!(s.nodes.iter().any(|n| n.id == "l#s0#r1#sep"), "last row separated too");
        let footer = by_id(&s, "l#s0#footer");
        near(footer.text.as_ref().unwrap().font.size as f64, 17.0);
        let sep = by_id(&s, "l#s0#footer#sep");
        let next = by_id(&s, "l#s1#header");
        assert!(next.bounds.y - (sep.bounds.y + 1.0) > PLAIN_SECTION_GAP, "a 22pt gap before the next header");
    }

    #[test]
    fn inset_grouped_spacing_follows_empty_headers_and_footers() {
        // Footer cell 29.67pt, then a header-less section's empty 17.3pt header; 35pt above the first under an inline
        // bar.
        let s = scene(r#"{"kind":"navigation","id":"n","title":"T","displayMode":"inline","content":{"kind":"list","id":"l","sections":[
            {"rows":[{"kind":"text","id":"a","text":"A"}],"footer":"Note"},
            {"rows":[{"kind":"text","id":"b","text":"B"}]},
            {"rows":[{"kind":"text","id":"c","text":"C"}]}]}}"#, (402.0, 600.0));
        let (s0, s1, s2) = (by_id(&s, "l#s0").bounds, by_id(&s, "l#s1").bounds, by_id(&s, "l#s2").bounds);
        near(s0.y, NAV_INLINE_H + SECTION_GAP + HEADERLESS_EXTRA_GAP);
        let fh = by_id(&s, "l#s0#footer").bounds.h;
        near(s1.y - (s0.y + s0.h), FOOTER_TOP + fh + FOOTER_BOTTOM + HEADERLESS_EXTRA_GAP);
        near(s2.y - (s1.y + s1.h), SECTION_GAP + HEADERLESS_EXTRA_GAP);
    }

    #[test]
    fn collapsible_section_header_toggles_and_hides_rows() {
        let s = scene(r#"{"kind":"list","id":"l","style":"sidebar","sections":[
            {"header":"Fruit","expanded":true,"toggle":7,"rows":[{"kind":"text","id":"a","text":"Apple"}]},
            {"header":"Veg","expanded":false,"toggle":8,"rows":[]}]}"#, (402.0, 600.0));
        let open = by_id(&s, "l#s0#toggle");
        assert_eq!((open.action, open.value.as_deref(), open.label.as_deref()), (Some(7), Some("expanded"), Some("Fruit")));
        assert_eq!(by_id(&s, "l#s1#toggle").value.as_deref(), Some("collapsed"));
        assert!(by_id(&s, "l#s0#chevron").symbol.as_ref().is_some_and(|r| r.spec.name == "chevron.down"));
        assert!(by_id(&s, "l#s1#chevron").symbol.as_ref().is_some_and(|r| r.spec.name == "chevron.right"));
        assert_eq!(by_id(&s, "l#s1").bounds.h, 0.0);
        assert!(by_id(&s, "l#s0").radius > 0.0, "sidebar sections are cards");
    }

    #[test]
    fn swiped_row_moves_its_content_and_lays_out_its_actions() {
        let row = |swipe: &str| format!(r#"{{"kind":"list","id":"l","sections":[{{"rows":[
            {{"kind":"text","id":"a","text":"Lunch","modifiers":[{{"listRow":{{"swipe":{swipe},
                "gesture":{{"key":"a#swipe","pans":true,"axis":"horizontal"}}}}}}]}},
            {{"kind":"text","id":"b","text":"Invoice"}}]}}]}}"#);
        let acts = r#""trailing":[{"label":{"kind":"text","id":"d","text":"Delete"},"action":1,"role":"destructive"},
                                  {"label":{"kind":"text","id":"f","text":"Flag"},"action":2,"tint":{"r":1,"g":0.5,"b":0,"a":1}}],
                      "leading":[{"label":{"kind":"label","id":"r","title":{"kind":"text","id":"rt","text":"Read"},
                                  "icon":{"kind":"image","id":"ri","systemName":"envelope"}},"action":3}]"#;
        // Closed: no actions drawn, the row carries the swipe gesture.
        let closed = scene(&row(&format!("{{{acts}}}")), (402.0, 600.0));
        assert!(!closed.nodes.iter().any(|n| n.id.starts_with("l#s0#r0#t")));
        assert_eq!(by_id(&closed, "l#s0#r0").gesture.as_ref().map(|g| g.key.as_str()), Some("a#swipe"));
        // Open trailing: Delete outermost (rightmost), both pills one width, content moved left by their span.
        let open = scene(&row(&format!(r#"{{{acts},"open":"trailing"}}"#)), (402.0, 600.0));
        let (del, flag) = (by_id(&open, "l#s0#r0#t0"), by_id(&open, "l#s0#r0#t1"));
        assert_eq!((del.action, flag.action), (Some(1), Some(2)));
        near(del.bounds.w, flag.bounds.w);
        assert!(flag.bounds.x + flag.bounds.w < del.bounds.x);
        let card = by_id(&open, "l#s0").bounds;
        near(card.x + card.w - (del.bounds.x + del.bounds.w), SWIPE_EDGE);
        let cell = by_id(&open, "l#s0#r0#cell").bounds;
        near(cell.x + cell.w, flag.bounds.x - SWIPE_GAP);
        assert_eq!(by_id(&open, "l#s0#r0#cell").clip_radius, Some(0.0));
        assert!(by_id(&open, "a").bounds.x < card.x, "content moved under the clip");
        assert!(by_id(&open, "l#s0#r0#revealed").fill.is_some());
        // Open leading: a Label action shows its symbol before its title.
        let lead = scene(&row(&format!(r#"{{{acts},"open":"leading"}}"#)), (402.0, 600.0));
        let read = by_id(&lead, "l#s0#r0#l0");
        assert!(by_id(&lead, "l#s0#r0#l0#i").bounds.x < by_id(&lead, "l#s0#r0#l0#t").bounds.x);
        near(read.bounds.x, card.x + SWIPE_EDGE);
        // Dragging further adds the finger's offset; a full swipe stretches the outermost action.
        let full = scene(&row(&format!(r#"{{{acts},"open":"trailing","offset":-150,"full":true}}"#)), (402.0, 600.0));
        assert!(by_id(&full, "l#s0#r0#t0").bounds.w > 200.0);
        assert!(!full.nodes.iter().any(|n| n.id == "l#s0#r0#t1"), "only the full-swipe action shows");
    }

    #[test]
    fn reorder_strip_spans_the_row_and_a_lifted_row_draws_on_top() {
        let s = scene(r#"{"kind":"list","id":"l","sections":[{"rows":[
            {"kind":"text","id":"a","text":"Anna","modifiers":[{"listRow":{"reorder":{"key":"a#reorder","pans":true,"axis":"vertical"},"lift":30}}]},
            {"kind":"text","id":"b","text":"Ben"}]}]}"#, (402.0, 600.0));
        let strip = by_id(&s, "l#s0#r0#reorder");
        let row = by_id(&s, "l#s0#r0").bounds;
        assert_eq!(strip.gesture.as_ref().map(|g| g.key.as_str()), Some("a#reorder"));
        near(strip.bounds.h, row.h);
        near(strip.bounds.x + strip.bounds.w, row.x + row.w);
        let ben = by_id(&s, "l#s0#r1").bounds;
        near(row.y - (ben.y - row.h), 30.0);
        let order: Vec<_> = s.nodes.iter().map(|n| n.id.as_str()).filter(|id| *id == "l#s0#r0" || *id == "l#s0#r1").collect();
        assert_eq!(order, ["l#s0#r1", "l#s0#r0"], "lifted row drawn last");
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
    fn lists_with_section_views_are_not_retained() {
        // Header and footer views can hold host state (a nested scroll offset), so their layout is never reused.
        let mut fonts = Fonts::with_fallback(true);
        let params = SceneParams {
            viewport: (180.0, 100.0), device: crate::device::device("iphone-17-pro").unwrap(), scale: 2.0,
            appearance: Appearance::Light, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios27,
            scroll: &HashMap::new(), sheets: &HashMap::new(), focus: None, overscroll: false, status_bar: false, chrome: Default::default(),
        };
        for field in ["headerView", "footerView"] {
            let tree: ViewNode = serde_json::from_str(&format!(r#"{{"kind":"list","id":"list","sections":[{{"{field}":
                {{"kind":"scroll","id":"h","axis":"horizontal","child":{{"kind":"text","id":"ht","text":"Wide header"}}}},
                "rows":[{{"kind":"text","id":"a","text":"Row"}}]}}]}}"#)).unwrap();
            let mut cache = SwiftUiLayoutCache::default();
            layout_with_cache(&tree, &params, &mut fonts, &mut cache);
            assert!(cache.list.is_none(), "{field} list was retained");
        }
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
    fn rounded_text_field_renders_styled_prompt() {
        let s = phone(r#"{"kind":"text_field","id":"f","field":4,"placeholder":"Name",
            "input_style":"rounded_border","prompt":{"kind":"text","id":"f#prompt","text":"Full name",
            "modifiers":[{"foregroundColor":{"r":1,"g":0,"b":0,"a":1}}]}}"#);
        let f = by_id(&s, "f");
        assert!(f.radius > 0.0 && f.bounds.h > 22.0);
        assert_eq!(f.label.as_deref(), Some("Name"));
        let prompt = by_id(&s, "f#prompt").text.as_ref().unwrap();
        assert_eq!(prompt.text, "Full name");
        assert_eq!(prompt.color, rgba(255, 0, 0, 1.0));
    }

    #[test]
    fn capsule_input_border_shape_applies_only_on_ios_27() {
        let tree: ViewNode = serde_json::from_str(r#"{"kind":"text_field","id":"f","field":4,
            "input_style":"rounded_border","input_border_shape":"capsule"}"#).unwrap();
        let mut params = SceneParams {
            viewport: (120.0, 100.0), device: crate::device::device("iphone-17-pro").unwrap(), scale: 3.0,
            appearance: Appearance::Light, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios26,
            scroll: &HashMap::new(), sheets: &HashMap::new(), focus: None,
            overscroll: false, status_bar: false, chrome: Default::default(),
        };
        let mut fonts = Fonts::new();
        let previous = layout(&tree, &params, &mut fonts);
        params.ios = IosVersion::Ios27;
        let current = layout(&tree, &params, &mut fonts);
        let radius = |s: &Scene| by_id(s, "f").radius;
        assert!(radius(&previous) < by_id(&previous, "f").bounds.h / 2.0);
        assert!(radius(&current) >= by_id(&current, "f").bounds.h / 2.0);
    }

    #[test]
    fn vertical_text_field_grows_and_places_caret_on_last_line() {
        let json = r#"{"kind":"text_field","id":"f","field":4,"axis":"vertical","text":"First line\n"}"#;
        let s = scene_with(json, (120.0, 200.0), false, (0.0, 0.0), &HashMap::new(), Some("f"));
        let f = by_id(&s, "f");
        let caret = by_id(&s, "f#caret");
        assert!(f.bounds.h > 40.0);
        assert!(caret.bounds.y > f.bounds.y + 20.0);
        assert!(caret.bounds.x < f.bounds.x + 4.0);
    }

    #[test]
    fn vertical_text_field_scrolls_beyond_its_line_limit() {
        let json = r#"{"kind":"text_field","id":"f","field":4,"axis":"vertical",
            "text":"First line\nSecond line\nThird line","modifiers":[{"lineLimit":{"max":2}}]}"#;
        let s = scene_with(json, (120.0, 200.0), false, (0.0, 0.0), &HashMap::new(), Some("f"));
        let f = by_id(&s, "f");
        assert_eq!(f.value.as_deref(), Some("First line\nSecond line\nThird line"));
        let scroll = f.scroll.unwrap();
        assert!(scroll.max.1 > 0.0 && scroll.offset.1 == scroll.max.1);
        assert!(f.bounds.contains(f.bounds.x + 1.0, by_id(&s, "f#caret").bounds.y));
    }

    #[test]
    fn text_editor_uses_available_height_and_scrolls_overflow() {
        let json = r#"{"kind":"text_field","id":"f","field":4,"editor":true,
            "text":"First line\nSecond line\nThird line\nFourth line\nFifth line"}"#;
        let s = scene(json, (120.0, 80.0));
        let f = by_id(&s, "f");
        near(f.bounds.h, 80.0);
        assert!(f.scroll.is_some_and(|scroll| scroll.max.1 > 0.0));
        assert_eq!(f.field, Some(4));

        let s = scene(json, (120.0, 0.0));
        near(by_id(&s, "f").bounds.h, 0.0);
    }

    #[test]
    fn secure_text_field_masks_graphemes_and_hides_disabled_caret() {
        let json = r#"{"kind":"text_field","id":"f","field":4,"secure":true,"disabled":true,"text":"é👩‍💻"}"#;
        let s = scene_with(json, (120.0, 200.0), false, (0.0, 0.0), &HashMap::new(), Some("f"));
        let f = by_id(&s, "f");
        assert!(f.secure && f.disabled && !f.focused && f.field.is_none());
        assert_eq!(s.nodes.iter().filter(|n| n.kind == "bullet").count(), 2);
        assert!(s.nodes.iter().all(|n| n.id != "f#caret" && n.id != "f#text"));
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
        let s = phone(include_str!("../../../tests/data/shapes.json"));
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
    fn picker_popover_keeps_passive_content_inert() {
        let s = phone(r#"{"kind":"presentation","id":"p",
          "content":{"kind":"text","id":"anchor","text":"Picker"},
          "layers":[{"kind":"popover","anchor":"anchor","dismiss":7,
            "content":{"kind":"text","id":"calendar","text":"March 2025"}}]}"#);
        let (x, y) = by_id(&s, "calendar").bounds.center();
        assert!(s.hit_test(x, y).is_none());
        assert_eq!(s.hit_test(2.0, 2.0).and_then(|i| s.nodes[i].action), Some(7));
    }

    #[test]
    fn picker_wheel_ignores_a_selection_outside_its_options() {
        let invalid = (isize::MAX as usize) + 1;
        let s = phone(&format!(r#"{{"kind":"wheel","id":"wheel","columns":[
            {{"width":100,"selected":{invalid},"options":[{{"text":"A","action":5}}]}},
            {{"width":100,"options":[]}}
        ]}}"#));
        assert_eq!(by_id(&s, "wheel").value.as_deref(), Some(""));
        assert!(s.nodes.iter().all(|node| node.action.is_none()));
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
    fn styled_sheets_drop_the_dim_and_scroll_first() {
        use crate::{DragPhase, Effect, Interaction};
        use onesim_protocol::{PointerEvent, PointerKind};
        // Background interaction up through medium: no scrim at medium, so the screen below takes taps.
        let j = r#"{"kind":"presentation","id":"p","content":{"kind":"button","id":"u","action":3,"label":{"kind":"text","id":"ul","text":"under"}},
            "layers":[{"kind":"sheet","detents":[{"kind":"medium"},{"kind":"large"}],"dismiss":7,"cornerRadius":16,
              "backgroundInteraction":{"kind":"enabled","upThrough":{"kind":"medium"}},"contentInteraction":"scrolls",
              "background":{"kind":"shape","id":"bg","shape":"rectangle","fill":{"r":1,"g":0.8,"b":0,"a":1}},
              "content":{"kind":"scroll","id":"sc","axis":"vertical","child":{"kind":"vstack","id":"c","children":[
                {"kind":"spacer","id":"sp","minLength":2000}]}}}]}"#;
        let show = |inter: &Interaction| scene_sheets(j, (402.0, 874.0), false, (62.0, 34.0), &HashMap::new(), None, &inter.sheets);
        let mut inter = Interaction::default();
        let s = show(&inter);
        assert!(!s.nodes.iter().any(|n| n.kind == "scrim"), "no dim at medium");
        near(by_id(&s, "layer0").radius, 16.0);
        let bg = by_id(&s, "bg");
        near(bg.bounds.w, by_id(&s, "layer0").bounds.w);
        let u = by_id(&s, "u").bounds.center();
        assert!(matches!(inter.pointer(&s, PointerEvent::new(PointerKind::Click, u.0, u.1)), Effect::Action { id: 3, .. }), "the screen below stays live");
        // contentInteraction(.scrolls): an upward drag on scrollable content at medium scrolls it, the sheet stays.
        inter.drag(&s, DragPhase::Begin, 200.0, 700.0, 0.0);
        inter.drag(&s, DragPhase::Move, 200.0, 600.0, 16.0);
        inter.drag(&s, DragPhase::End, 200.0, 600.0, 1000.0);
        let s = show(&inter);
        assert!((by_id(&s, "layer0").bounds.y - 415.0).abs() < 1.0, "still medium: {}", by_id(&s, "layer0").bounds.y);
        // At large the interaction no longer reaches through: the dim and its scrim are back.
        let mut big = Interaction::default();
        big.sheets.insert("layer0".into(), crate::SheetPos { detent: 1, top: None });
        assert!(show(&big).nodes.iter().any(|n| n.kind == "scrim"));
    }

    #[test]
    fn popover_bubble_points_at_its_anchor() {
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"vstack","id":"v","children":[
                {"kind":"button","id":"b","action":1,"label":{"kind":"text","id":"bl","text":"Bubble"}}]},
              "layers":[{"kind":"popover","anchor":"b","arrow":true,"arrowEdge":"top","dismiss":7,
                "content":{"kind":"text","id":"t","text":"Bubble content"}}]}"#,
        );
        let (b, card, arrow) = (by_id(&s, "b").bounds, by_id(&s, "layer0#card").bounds, by_id(&s, "layer0#arrow"));
        near(card.center().0, b.center().0);
        near(card.y, b.y + b.h + BUBBLE_TIP_GAP + POPOVER_ARROW_H);
        near(card.h, by_id(&s, "t").bounds.h + 2.0 * BUBBLE_PAD_V);
        assert!(matches!(arrow.glyph, Some((Glyph::ArrowUp, _))));
        assert_eq!(by_id(&s, "layer0#scrim").action, Some(7), "a tap outside dismisses");
        // arrowEdge bottom puts it above; a unit-point attachment moves the tip.
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"vstack","id":"v","children":[
                {"kind":"button","id":"b","action":1,"label":{"kind":"text","id":"bl","text":"Bubble"}}]},
              "layers":[{"kind":"popover","anchor":"b","arrow":true,"arrowEdge":"bottom","attachment":{"point":[0,0]},
                "content":{"kind":"text","id":"t","text":"Up"}}]}"#,
        );
        let (b, card) = (by_id(&s, "b").bounds, by_id(&s, "layer0#card").bounds);
        near(card.y + card.h + POPOVER_ARROW_H + BUBBLE_TIP_GAP, b.y);
        assert!(matches!(by_id(&s, "layer0#arrow").glyph, Some((Glyph::ArrowDown, _))));
        assert!(card.center().0 < b.center().0, "centred on the leading edge, kept on screen");
    }

    #[test]
    fn context_menu_lifts_the_view_and_hangs_the_menu_under_it() {
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"vstack","id":"v","children":[
                {"kind":"text","id":"t","text":"Hold for menu","modifiers":[{"padding":{"top":16,"leading":16,"bottom":16,"trailing":16}}]}]},
              "layers":[{"kind":"contextMenu","anchor":"t","dismiss":7,"items":[
                {"text":"Copy","action":2,"systemImage":"doc.on.doc"},{"text":"Delete","action":3,"destructive":true,"divider":true}]}]}"#,
        );
        let orig = s.nodes.iter().position(|n| n.id == "t" || n.id.starts_with("t#")).unwrap();
        let o = s.nodes[orig].bounds;
        assert_eq!(s.nodes[orig].opacity, 0.0, "the view leaves its place");
        let lift = s.nodes.iter().find(|n| n.id.starts_with("layer0#lift/")).unwrap();
        near(lift.bounds.w, o.w * CONTEXT_LIFT);
        near(lift.bounds.center().1, o.center().1);
        assert!(lift.action.is_none() && !lift.inspect);
        let card = by_id(&s, "menu").bounds;
        near(card.y, lift.bounds.y + lift.bounds.h + CONTEXT_MENU_GAP);
        near(card.center().0, o.center().0);
        assert_eq!(by_id(&s, "menu#1").action, Some(3));
        let scrim = by_id(&s, "layer0#scrim");
        assert_eq!(scrim.action, Some(7));
        assert!(scrim.fill.is_some_and(|c| c.a > 0.1), "the screen dims");
        // A custom preview replaces the lifted copy: on a platter centred on the anchor.
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"vstack","id":"v","children":[{"kind":"text","id":"t","text":"Hold"}]},
              "layers":[{"kind":"contextMenu","anchor":"t","preview":{"kind":"text","id":"pv","text":"Preview"},"items":[{"text":"Open","action":2}]}]}"#,
        );
        assert!(!s.nodes.iter().any(|n| n.id.starts_with("layer0#lift/")));
        near(by_id(&s, "layer0#preview").bounds.center().1, by_id(&s, "t").bounds.center().1);
    }

    #[test]
    fn share_sheet_copies_and_dismisses() {
        use crate::{DragPhase, Effect, Interaction};
        let s = phone(
            r#"{"kind":"presentation","id":"p","content":{"kind":"text","id":"u","text":"under"},
              "layers":[{"kind":"share","items":["Hello"],"dismiss":7,"copy":8}]}"#,
        );
        let card = by_id(&s, "layer0");
        let m = crate::look::look(IosVersion::Ios26).share;
        near(card.bounds.y + card.bounds.h, 874.0 - m.inset.1);
        assert_eq!(card.sheet.as_ref().map(|i| i.dismiss), Some(Some(7)));
        let title = s.nodes.iter().find(|n| n.id == "layer0#title").unwrap();
        assert_eq!(title.text.as_ref().map(|t| t.text.as_str()), Some("Hello"));
        let cell = |label: &str| s.nodes.iter().find(|n| n.label.as_deref() == Some(label) && n.role == Role::Button).unwrap().action;
        assert_eq!((cell("Copy"), cell("Save to Files"), cell("Reminders"), cell("More")), (Some(8), Some(7), Some(7), Some(7)));
        assert_eq!(by_id(&s, "layer0#scrim").action, Some(7));
        // A swipe down dismisses it like a sheet.
        let mut inter = Interaction::default();
        inter.drag(&s, DragPhase::Begin, 200.0, 560.0, 0.0);
        inter.drag(&s, DragPhase::Move, 200.0, 800.0, 16.0);
        assert!(matches!(inter.drag(&s, DragPhase::End, 200.0, 860.0, 100.0), Effect::Action { id: 7, .. }));
        // Activities other than Copy run their own guest actions.
        let s2 = phone(r#"{"kind":"presentation","id":"p","content":{"kind":"text","id":"u","text":"under"},
            "layers":[{"kind":"share","items":["Hello"],"dismiss":7,"copy":8,"activities":{"Reminders":9,"More":10,"Save to Files":11}}]}"#);
        let cell2 = |label: &str| s2.nodes.iter().find(|n| n.label.as_deref() == Some(label) && n.role == Role::Button).unwrap().action;
        assert_eq!((cell2("Reminders"), cell2("More"), cell2("Save to Files")), (Some(9), Some(10), Some(11)));
        // Links get the release's link activities and a blank tile; a preview title is drawn in the primary color.
        let link = r#"{"kind":"presentation","id":"p","content":{"kind":"text","id":"u","text":"under"},
            "layers":[{"kind":"share","items":["https://example.com"],"url":true,"title":"Example","image":{"kind":"image","id":"g","systemName":"globe"}}]}"#;
        let labels = |s: &Scene| s.nodes.iter().filter(|n| n.id.starts_with("layer0#cell") && n.role == Role::Button).map(|n| n.label.clone().unwrap()).collect::<Vec<_>>();
        assert_eq!(labels(&phone(link)), ["Reminders", "More", "Copy", "Add to Reading List"], "iOS 26.5");
        let s27 = scene_ios(link, IosVersion::Ios27);
        assert_eq!(labels(&s27), ["Reminders", "Safari", "More", "Copy", "Open in Safari", "Add to Reading List"], "iOS 27.2");
        assert!(!s27.nodes.iter().any(|n| n.id.starts_with("layer0#tile#")), "a symbol preview leaves the tile blank");
        // Several items are counted in the header.
        let s = phone(r#"{"kind":"presentation","id":"p","content":{"kind":"text","id":"u","text":"under"},"layers":[{"kind":"share","items":["a","b"]}]}"#);
        assert_eq!(by_id(&s, "layer0#title").text.as_ref().map(|t| t.text.as_str()), Some("2 Items"));
    }

    #[test]
    fn paste_button_is_an_accent_rounded_rect() {
        let s = phone(r#"{"kind":"button","id":"pb","style":"pasteButton","action":4,"label":{"kind":"label","id":"l",
            "title":{"kind":"text","id":"lt","text":"Paste"},"icon":{"kind":"image","id":"li","systemName":"doc.on.clipboard"}}}"#);
        let b = by_id(&s, "pb");
        near(b.bounds.h, PASTE_PAD.1);
        near(b.radius, PASTE_RADIUS);
        assert_eq!(b.action, Some(4));
        assert!(b.fill.is_some());
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
        assert!(matches!(by_id(&s, "sp#spinner").glyph, Some((Glyph::Spinner { .. }, _))));

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
    fn semantic_opacity_multiplies_the_resolved_color_in_both_appearances() {
        let root = r#"{"kind":"hstack","id":"root","children":[
          {"kind":"color","id":"primary","color":{"semantic":"primary","a":0.25}},
          {"kind":"color","id":"secondary","color":{"semantic":"secondary","a":0.5}},
          {"kind":"color","id":"clear","color":{"semantic":"accent","a":-1}},
          {"kind":"color","id":"opaque","color":{"semantic":"secondary","a":5}}
        ]}"#;
        for dark in [false, true] {
            let scene = scene_with(root, (120.0, 40.0), dark, (0.0, 0.0), &HashMap::new(), None);
            let color = |id| scene.nodes.iter().find(|n| n.id == id).unwrap().fill.unwrap();
            assert_eq!(color("primary"), if dark { Color::rgba(1.0, 1.0, 1.0, 0.25) } else { Color::rgba(0.0, 0.0, 0.0, 0.25) });
            assert_eq!(color("secondary").a, 0.3);
            assert_eq!(color("clear").a, 0.0);
            assert_eq!(color("opaque").a, 1.0);
        }
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

    #[test]
    fn navigation_chrome_visibility_is_independent_per_bar() {
        let mut root = serde_json::json!({
            "kind": "navigation", "id": "nav", "title": "Library",
            "chrome": {
                "navigationBarVisibility": "hidden",
                "bottomBarBackgroundVisibility": "visible",
                "bottomBarBackground": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0},
                "items": [
                    {"region": "trailing", "content": {
                        "kind": "button", "id": "top", "action": 7,
                        "label": {"kind": "text", "id": "top-label", "text": "Top"}
                    }},
                    {"region": "bottom", "content": {
                        "kind": "button", "id": "bottom", "action": 8,
                        "label": {"kind": "text", "id": "bottom-label", "text": "Bottom"}
                    }}
                ]
            },
            "content": {"kind": "text", "id": "content", "text": "Books"}
        });
        let scene = phone(&root.to_string());
        assert!(scene.nodes.iter().all(|node| node.id != "top" && node.id != "nav#title"));
        assert_eq!(by_id(&scene, "nav#bottomBar").fill, Some(Color::rgba(1.0, 0.0, 0.0, 1.0)));
        let bottom = by_id(&scene, "bottom#toolbarSurface");
        assert_eq!(bottom.action, Some(8));
        near(bottom.bounds.x + bottom.bounds.w / 2.0, 201.0);
        near(bottom.bounds.y + bottom.bounds.h / 2.0, 822.0);

        root["chrome"]["bottomBarVisibility"] = "hidden".into();
        root["chrome"]["navigationBarVisibility"] = "automatic".into();
        let scene = phone(&root.to_string());
        assert!(scene.nodes.iter().all(|node| node.id != "bottom" && node.id != "nav#bottomBar"));
        assert_eq!(by_id(&scene, "top#toolbarSurface").action, Some(7));
        assert_eq!(by_id(&scene, "nav#title").text.as_ref().unwrap().text, "Library");
    }

    #[test]
    fn inline_title_fits_between_toolbar_items() {
        let scene = phone(r#"{
            "kind":"navigation","id":"nav","displayMode":"inline",
            "title":"A long title constrained by toolbar actions",
            "chrome":{"items":[
                {"region":"leading","content":{"kind":"button","id":"leading","action":1,
                    "label":{"kind":"text","id":"leading-label","text":"Long leading action"}}},
                {"region":"trailing","content":{"kind":"button","id":"trailing","action":2,
                    "label":{"kind":"text","id":"trailing-label","text":"Long trailing action"}}}
            ]},
            "content":{"kind":"empty","id":"content"}
        }"#);
        let left = by_id(&scene, "leading#toolbarSurface").bounds;
        let right = by_id(&scene, "trailing#toolbarSurface").bounds;
        let title = by_id(&scene, "nav#title").bounds;
        assert!(left.x + left.w <= title.x);
        assert!(title.x + title.w <= right.x);
        assert!(title.h < 24.0);
    }

    #[test]
    fn search_key_requires_a_query() {
        let mut field = serde_json::json!({
            "kind": "text_field", "id": "search", "field": 3, "search_field": true, "text": ""
        });
        let focused = |field: &serde_json::Value| {
            let tree = serde_json::from_value(field.clone()).unwrap();
            let params = SceneParams {
                viewport: (402.0, 874.0), device: crate::device::device("iphone-17-pro").unwrap(), scale: 3.0,
                appearance: Appearance::Light, dynamic_type: DynamicTypeSize::Large, ios: IosVersion::Ios26,
                scroll: &HashMap::new(), sheets: &HashMap::new(), focus: Some("search"),
                overscroll: false, status_bar: false,
                chrome: crate::chrome::ChromeParams { keyboard: true, ..Default::default() },
            };
            layout(&tree, &params, &mut Fonts::new())
        };
        let empty = focused(&field);
        let key = by_id(&empty, "keyboard#key-return");
        assert!(key.disabled);
        assert!(matches!(key.glyph, Some((Glyph::Search, _))));
        assert!(empty.nodes.iter().all(|node| !node.id.starts_with("keyboard#suggestion-")));

        field["text"] = "書".into();
        let with_query = focused(&field);
        let key = by_id(&with_query, "keyboard#key-return");
        assert!(!key.disabled);
        assert_eq!(key.role, Role::Key);
    }

    #[test]
    fn accessibility_semantics_merge_sort_and_resolve() {
        let s = scene(
            r#"{
            "kind":"vstack","id":"col","children":[
                {"kind":"text","id":"alpha","text":"Alpha","modifiers":[
                    {"accessibility":{"label":"First","hint":"Opens","value":"1","identifier":"alpha","addTraits":["isHeader"],"sortPriority":1,"heading":1,"inputLabels":["A"],"actions":[{"name":"default","action":7},{"name":"Delete","action":8}]}},
                    {"accessibility":{"label":"Outer","removeTraits":["isHeader"],"addTraits":["isButton"]}}
                ]},
                {"kind":"text","id":"beta","text":"Beta","modifiers":[{"accessibility":{"hidden":true}}]},
                {"kind":"vstack","id":"pair","children":[
                    {"kind":"text","id":"one","text":"One"},
                    {"kind":"text","id":"two","text":"Two"}
                ],"modifiers":[{"accessibility":{"children":"combine"}}]},
                {"kind":"text","id":"go","text":"Go","modifiers":[{"accessibility":{
                    "activation":{"x":0,"y":0,"unit":true},
                    "actions":[{"name":"default","action":9}],
                    "adjustable":{"increment":1,"decrement":2},
                    "scroll":{"top":3,"bottom":4,"leading":5,"trailing":6},
                    "zoom":{"in":11,"out":12},
                    "custom":[{"key":"Note","id":"note","value":"Hi","importance":"high"}],
                    "rotors":[{"name":"links","entries":[{"label":"Docs","id":"d"}]}],
                    "directTouch":true,"responds":false,"chart":true,"ignoresInvertColors":true,
                    "textContentType":"sourceCode","focused":true,"linked":"g1"
                }}]},
                {"kind":"text","id":"plain","text":"Plain"},
                {"kind":"text","id":"renamed","text":"Visible","modifiers":[{"accessibility":{"label":"Renamed"}}]},
                {"kind":"text","id":"name","text":"Name","modifiers":[{"accessibility":{"pair":{"role":"label","id":"p"}}}]},
                {"kind":"text","id":"val","text":"Value","modifiers":[{"accessibility":{"pair":{"role":"content","id":"p"}}}]},
                {"kind":"text","id":"late","text":"Late","modifiers":[{"accessibility":{"sortPriority":0}}]},
                {"kind":"text","id":"early","text":"Early","modifiers":[{"accessibility":{"sortPriority":5}}]},
                {"kind":"vstack","id":"hid","modifiers":[{"accessibility":{"hidden":true}}],"children":[
                    {"kind":"text","id":"secret","text":"Secret"}
                ]},
                {"kind":"vstack","id":"ign","modifiers":[{"accessibility":{"children":"ignore","label":"Ignored"}}],"children":[
                    {"kind":"text","id":"hideA","text":"HideA"},
                    {"kind":"text","id":"hideB","text":"HideB"}
                ]},
                {"kind":"vstack","id":"els","modifiers":[{"accessibility":{"elements":["Only"]}}],"children":[
                    {"kind":"text","id":"gone","text":"Gone"}
                ]},
                {"kind":"text","id":"uni","text":"X","modifiers":[{"accessibility":{"label":"書"}}]},
                {"kind":"text","id":"neg","text":"Neg","modifiers":[{"accessibility":{"sortPriority":-1,"hint":"n"}}]}
            ]
        }"#,
            (200.0, 600.0),
        );

        let alpha = by_id(&s, "alpha");
        assert_eq!(alpha.text.as_ref().unwrap().text, "Alpha");
        assert_eq!(alpha.label.as_deref(), Some("Outer"));
        assert!(alpha.value.is_none());
        assert!(alpha.action.is_none());
        let ax = alpha.a11y.as_ref().unwrap();
        assert_eq!(ax.value.as_deref(), Some("1"));
        assert_eq!(ax.hint.as_deref(), Some("Opens"));
        assert_eq!(ax.identifier.as_deref(), Some("alpha"));
        assert_eq!(ax.traits, ["isButton"]);
        assert_eq!(ax.heading, Some(1));
        assert_eq!(ax.input_labels, ["A"]);
        assert_eq!(ax.actions.iter().map(|a| a.action).collect::<Vec<_>>(), [7, 8]);

        let go = by_id(&s, "go");
        assert!(go.action.is_none());
        assert_eq!(go.label.as_deref(), Some("Go"));

        let tree = s.ui_tree();
        let col = tree.children.iter().find(|n| n.id == "col").expect("col");
        let ids: Vec<_> = col.children.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["early", "alpha", "beta", "pair", "go", "plain", "renamed", "name", "val", "late", "hid", "ign", "els", "uni", "neg"]);
        assert_eq!(by_id(&s, "secret").a11y.as_ref().unwrap().hidden, Some(true));
        assert!(by_id(&s, "hideA").a11y.as_ref().unwrap().suppressed);
        assert!(by_id(&s, "hideB").a11y.as_ref().unwrap().suppressed);
        assert!(!by_id(&s, "ign").a11y.as_ref().unwrap().suppressed);
        assert!(by_id(&s, "gone").a11y.as_ref().unwrap().suppressed);
        assert_eq!(by_id(&s, "els").a11y.as_ref().unwrap().elements, ["Only"]);

        let ua = col.children.iter().find(|n| n.id == "alpha").unwrap();
        assert_eq!(ua.label.as_deref(), Some("Outer"));
        assert_eq!(ua.text.as_deref(), Some("Alpha"));
        assert_eq!(ua.value.as_deref(), Some("1"));
        assert!(ua.action.is_none());
        let uax = ua.accessibility.as_ref().unwrap();
        assert_eq!(uax.hint.as_deref(), Some("Opens"));
        assert_eq!(uax.identifier.as_deref(), Some("alpha"));
        assert_eq!(uax.traits, ["isButton"]);
        assert_eq!(uax.heading, Some(1));
        assert_eq!(uax.actions.iter().map(|a| (a.name.as_str(), a.action)).collect::<Vec<_>>(), [("default", 7), ("Delete", 8)]);

        let ug = col.children.iter().find(|n| n.id == "go").unwrap();
        let gax = ug.accessibility.as_ref().unwrap();
        assert_eq!(gax.actions.iter().map(|a| a.action).collect::<Vec<_>>(), [9]);
        assert!(ug.action.is_none());
        assert_eq!(gax.adjustable.as_ref().map(|a| (a.increment, a.decrement)), Some((1, 2)));
        assert_eq!(gax.scroll.as_ref().map(|a| (a.top, a.bottom, a.leading, a.trailing)), Some((Some(3), Some(4), Some(5), Some(6))));
        assert_eq!(gax.zoom.as_ref().map(|z| (z.zoom_in, z.zoom_out)), Some((Some(11), Some(12))));
        assert_eq!(gax.custom[0].key, "Note");
        assert_eq!(gax.custom[0].value.as_deref(), Some("Hi"));
        assert_eq!(gax.rotors[0].name, "links");
        assert_eq!(gax.rotors[0].entries[0].label, "Docs");
        assert!(gax.direct_touch && gax.chart && gax.ignores_invert_colors && gax.focused);
        assert_eq!(gax.responds, Some(false));
        assert_eq!(gax.text_content_type.as_deref(), Some("sourceCode"));
        assert_eq!(gax.linked.as_deref(), Some("g1"));

        assert!(col.children.iter().find(|n| n.id == "plain").unwrap().accessibility.is_none());
        let renamed = col.children.iter().find(|n| n.id == "renamed").unwrap();
        assert_eq!(renamed.label.as_deref(), Some("Renamed"));
        assert_eq!(renamed.text.as_deref(), Some("Visible"));
        assert!(renamed.accessibility.is_none());

        let (x, y) = crate::resolve_label(&tree, "Outer").unwrap();
        let center = alpha.bounds.center();
        assert!((x - center.0).abs() < 0.01 && (y - center.1).abs() < 0.01);
        assert!(crate::resolve_label(&tree, "Alpha").is_err());
        assert!(crate::resolve_label(&tree, "Beta").is_err());
        assert!(crate::resolve_label(&tree, "alpha").is_ok());
        let (gx, gy) = crate::resolve_label(&tree, "Go").unwrap();
        assert!((gx - go.bounds.x).abs() < 0.01 && (gy - go.bounds.y).abs() < 0.01);
        assert!(crate::resolve_label(&tree, "One, Two").is_ok());
        assert!(crate::resolve_label(&tree, "One").is_err());
        assert!(crate::resolve_label(&tree, "Two").is_err());
        assert!(crate::resolve_label(&tree, "Name, Value").is_ok());
        assert!(crate::resolve_label(&tree, "Name").is_err());
        assert!(crate::resolve_label(&tree, "Renamed").is_ok());
        assert!(crate::resolve_label(&tree, "Visible").is_err());
        assert!(crate::resolve_label(&tree, "Secret").is_err());
        assert!(crate::resolve_label(&tree, "HideA").is_err());
        assert!(crate::resolve_label(&tree, "Gone").is_err());
        assert!(crate::resolve_label(&tree, "Ignored").is_ok());
        assert!(crate::resolve_label(&tree, "書").is_ok());
        assert!(crate::resolve_label(&tree, "X").is_err());
    }

    #[test]
    fn accessibility_value_stays_editable_and_wrappers_keep_semantics() {
        let s = scene(
            r#"{
            "kind":"vstack","id":"col","children":[
                {"kind":"text_field","id":"field","field":1,"text":"abc","modifiers":[
                    {"accessibility":{"value":"Description","hint":"About the field"}}
                ]},
                {"kind":"text_field","id":"mask","field":2,"secure":true,"text":"ab"},
                {"kind":"text","id":"fade","text":"Visible","modifiers":[
                    {"opacity":0.5},
                    {"accessibility":{"label":"Spoken","identifier":"fade-id"}}
                ]},
                {"kind":"text","id":"backed","text":"Backed","modifiers":[
                    {"opacity":0.5},
                    {"background":{"kind":"color","id":"bg","color":{"r":1,"g":0,"b":0,"a":1}}},
                    {"accessibility":{"label":"Behind"}}
                ]},
                {"kind":"text","id":"over","text":"Over","modifiers":[
                    {"overlay":{"view":{"kind":"color","id":"ov","color":{"r":0,"g":0,"b":1,"a":1}}}},
                    {"accessibility":{"label":"Front"}}
                ]},
                {"kind":"text","id":"masked","text":"Masked","modifiers":[
                    {"mask":{"view":{"kind":"color","id":"hole","color":{"r":0,"g":0,"b":0,"a":1}}}},
                    {"accessibility":{"label":"Cut"}}
                ]}
            ]
        }"#,
            (200.0, 400.0),
        );

        let field = by_id(&s, "field");
        assert_eq!(field.value.as_deref(), Some("abc"));
        assert_eq!(field.a11y.as_ref().unwrap().value.as_deref(), Some("Description"));
        assert!(by_id(&s, "field#text").a11y.is_none());

        let mask = by_id(&s, "mask");
        assert_eq!(mask.value.as_deref(), Some("ab"));
        assert!(mask.a11y.is_none());

        let fade = by_id(&s, "fade");
        assert_eq!(fade.text.as_ref().unwrap().text, "Visible");
        assert_eq!(fade.label.as_deref(), Some("Spoken"));
        assert_eq!(fade.a11y.as_ref().unwrap().identifier.as_deref(), Some("fade-id"));
        assert!(by_id(&s, "fade#opacity0").a11y.is_none());

        assert_eq!(by_id(&s, "backed").label.as_deref(), Some("Behind"));
        assert_eq!(by_id(&s, "backed").text.as_ref().unwrap().text, "Backed");
        assert!(by_id(&s, "backed#opacity0").a11y.is_none());
        assert!(by_id(&s, "bg").a11y.is_none());

        assert_eq!(by_id(&s, "over").label.as_deref(), Some("Front"));
        assert!(by_id(&s, "ov").a11y.is_none());

        assert_eq!(by_id(&s, "masked").label.as_deref(), Some("Cut"));
        assert!(by_id(&s, "masked#mask0").a11y.is_none());
        assert!(by_id(&s, "hole").a11y.is_none());

        let tree = s.ui_tree();
        let field_ui = ui_by_id(&tree, "field");
        assert_eq!(field_ui.value.as_deref(), Some("Description"));
        assert_eq!(field_ui.accessibility.as_ref().unwrap().hint.as_deref(), Some("About the field"));
        assert_eq!(ui_by_id(&tree, "mask").value.as_deref(), Some("\u{2022}\u{2022}"));
        assert_eq!(ui_by_id(&tree, "fade").label.as_deref(), Some("Spoken"));
        assert_eq!(ui_by_id(&tree, "fade").text.as_deref(), Some("Visible"));
        assert!(crate::resolve_label(&tree, "Spoken").is_ok());
        assert!(crate::resolve_label(&tree, "Visible").is_err());
        assert!(crate::resolve_label(&tree, "Behind").is_ok());
        assert!(crate::resolve_label(&tree, "Front").is_ok());
        assert!(crate::resolve_label(&tree, "Cut").is_ok());
        fn contains(n: &onesim_protocol::UiNode, id: &str) -> bool {
            n.id == id || n.children.iter().any(|c| contains(c, id))
        }
        assert!(!contains(&tree, "fade#opacity0"));
        assert!(!contains(&tree, "masked#mask0"));
    }
}
