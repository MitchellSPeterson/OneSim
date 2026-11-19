//! React Native layout: Fabric's flat props -> Yoga-style flexbox (via taffy) -> `Scene`.
//!
//! Node ids in the scene are the reactTags; every RN box has `tag = Some(tag)` so the scene's
//! hit test returns the deepest tag under a point (`action` stays for real native actions: nav back, tabs). Content starts at y=0 (RN handles safe areas).

use super::swiftui::{add_chrome, rn_nav_bar, rn_nav_height, rn_tab_bar, switch_style, NavBack, SceneParams, TabBar, TabBarItem, KNOB, SWITCH, TAB_BAR_INSET};
use crate::scene::{Color, Glyph, ImageRun, Node, Rect, Role, Scene, ScrollInfo, TextRun};
use crate::text::{FontSpec, Fonts, Span, TextAlign};
use serde_json::Value;
use simcore_protocol::{Appearance, ViewKind, ViewNode};
use std::collections::BTreeSet;
use taffy::prelude::*;
use taffy::compute_leaf_layout;

/// Text leaf: what the measure function and the painter need.
#[derive(Clone)]
struct Text {
    text: String,
    spec: FontSpec,
    color: Color,
    strike: bool,
    /// Text input: one line unless multiline, never shorter than a line.
    field: bool,
    single: bool,
    /// Intrinsic size of a control (`RCTSwitch`).
    fixed: Option<(f32, f32)>,
    spans: Vec<Span>,
    explicit_lh: bool,
}

struct Item<'a> {
    node: &'a ViewNode,
    props: &'a Value,
    id: NodeId,
    kids: Vec<Item<'a>>,
    /// `RNSScreen` with a visible header.
    header: Option<Header>,
    /// `RNSTabsHostIOS`.
    tabs: Option<Tabs>,
}

impl<'a> Item<'a> {
    fn new(node: &'a ViewNode, props: &'a Value, id: NodeId, kids: Vec<Item<'a>>) -> Self {
        Item { node, props, id, kids, header: None, tabs: None }
    }
}

/// Native stack header of a screen (`RNSScreenStackHeaderConfig`).
struct Header {
    stack: String,
    title: Option<String>,
    large: bool,
    back: Option<String>,
}

/// `RNSTabsHostIOS`: the shared tab bar, unless hidden.
struct Tabs {
    bar: TabBar,
    hidden: bool,
}

fn tabs_info(cx: &mut Cx, p: &Value, children: &[ViewNode]) -> Tabs {
    let screens = children.iter().filter(|c| rn_component(c) == "RNSTabsScreenIOS");
    let items: Vec<TabBarItem> = screens
        .filter_map(|c| match &c.kind {
            ViewKind::Rn { props, .. } => {
                let img = |k: &str| props.get(k).and_then(|i| i.get("path")).and_then(Value::as_str).map(str::to_owned);
                Some(TabBarItem {
                    key: prop_str(props, "screenKey").unwrap_or_default(),
                    title: prop_str(props, "title").unwrap_or_default(),
                    icon: img("__simcoreIconImage"),
                    selected_icon: img("__simcoreSelectedIconImage"),
                    symbol: prop_str(props, "iconResourceName").filter(|_| props.get("iconType").and_then(Value::as_str) == Some("sfSymbol")),
                    badge: None,
                    action: None,
                })
            }
            _ => None,
        })
        .collect();
    let want = p.get("navStateRequest").and_then(|n| n.get("selectedScreenKey")).and_then(Value::as_str);
    let selected = items.iter().position(|t| Some(t.key.as_str()) == want).unwrap_or(0);
    Tabs { bar: TabBar { items, selected, tint: cx.color(p.get("tabBarTintColor")) }, hidden: p.get("tabBarHidden").and_then(Value::as_bool) == Some(true) }
}

#[derive(Clone, Copy, PartialEq, Default)]
struct SpanStyle {
    color: Option<[u8; 3]>,
    weight: Option<u16>,
    size: Option<f32>,
    mono: bool,
}

/// Cut `text` at every styled-range edge: one run per piece, styled by the (last) range covering it, else the paragraph's.
fn runs(text: &str, ranges: &[(usize, usize, SpanStyle)], spec: &FontSpec, base: Color) -> Vec<Span> {
    if ranges.is_empty() {
        return vec![];
    }
    let mut cuts: Vec<usize> = vec![0, text.len()];
    cuts.extend(ranges.iter().flat_map(|r| [r.0, r.1]).filter(|c| *c <= text.len() && text.is_char_boundary(*c)));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| {
            let st = ranges.iter().rfind(|r| r.0 <= w[0] && w[1] <= r.1).map(|r| r.2).unwrap_or_default();
            let color = st.color.map_or(base, |c| Color::rgba(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, base.a));
            Span {
                text: text[w[0]..w[1]].to_string(),
                pt: st.size.unwrap_or(spec.pt),
                weight: st.weight.unwrap_or(spec.weight),
                color,
                italic: false,
                underline: false,
                strike: false,
                mono: st.mono,
            }
        })
        .collect()
}

fn rn_component(n: &ViewNode) -> &str {
    match &n.kind {
        ViewKind::Rn { component, .. } => component,
        _ => "",
    }
}

/// Props of a screen's `RNSScreenStackHeaderConfig` child.
fn screen_header(screen: &ViewNode) -> Option<&Value> {
    let ViewKind::Rn { children, .. } = &screen.kind else { return None };
    children.iter().find(|c| rn_component(c) == "RNSScreenStackHeaderConfig").and_then(|c| match &c.kind {
        ViewKind::Rn { props, .. } => Some(props),
        _ => None,
    })
}

fn prop_str(p: &Value, k: &str) -> Option<String> {
    p.get(k).and_then(Value::as_str).map(str::to_owned)
}

/// Safe-area insets (top, bottom) the guest also derives from the viewport height.
fn insets(h: f64) -> (f64, f64) {
    match h as u32 {
        874 => (62.0, 34.0),
        852 => (59.0, 34.0),
        844 => (47.0, 34.0),
        812 => (44.0, 34.0),
        _ => (20.0, 0.0),
    }
}

/// CSS color strings (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, a few names).
fn css_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let c = |r: f32, g: f32, b: f32, a: f32| Some(Color::rgba(r / 255.0, g / 255.0, b / 255.0, a));
    if let Some(h) = s.strip_prefix('#') {
        let v = u32::from_str_radix(h, 16).ok()?;
        return match h.len() {
            3 => c(((v >> 8) & 0xf) as f32 * 17.0, ((v >> 4) & 0xf) as f32 * 17.0, (v & 0xf) as f32 * 17.0, 1.0),
            6 => c((v >> 16 & 0xff) as f32, (v >> 8 & 0xff) as f32, (v & 0xff) as f32, 1.0),
            8 => c((v >> 24 & 0xff) as f32, (v >> 16 & 0xff) as f32, (v >> 8 & 0xff) as f32, (v & 0xff) as f32 / 255.0),
            _ => None,
        };
    }
    if let Some(args) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let v: Vec<f32> = args.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        return c(*v.first()?, *v.get(1)?, *v.get(2)?, v.get(3).copied().unwrap_or(1.0));
    }
    match s {
        "black" => c(0.0, 0.0, 0.0, 1.0),
        "white" => c(255.0, 255.0, 255.0, 1.0),
        "red" => c(255.0, 59.0, 48.0, 1.0),
        "blue" => c(0.0, 122.0, 255.0, 1.0),
        "gray" | "grey" => c(142.0, 142.0, 147.0, 1.0),
        "transparent" => c(0.0, 0.0, 0.0, 0.0),
        _ => None,
    }
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

struct Cx<'a, 'f> {
    tree: TaffyTree<Text>,
    notes: BTreeSet<String>,
    scale: f64,
    dark: bool,
    fonts: &'f mut Fonts,
    /// Visible `RCTModalHostView`s: laid out against the viewport and painted last.
    modals: Vec<Item<'a>>,
    /// Safe-area insets (top, bottom) from the viewport height.
    insets: (f64, f64),
    /// Top inset applied to auto-adjusting scroll views under a stack header.
    inset_top: f64,
    /// Accumulated `rotate`/`rotateZ` of the ancestors, degrees (applied to images only).
    rot: f32,
    /// Hand-over from a stack to its top screen: stack tag and the previous screen's title.
    stack: Option<(String, Option<String>)>,
    /// The container being built lays its children out in a row.
    parent_row: bool,
    /// Tallest `lineHeight` of a nested span of the paragraph being collected.
    span_lh: f32,
    /// Core `SafeAreaView`s: padded by the part of the safe-area insets they overlap (after a first layout pass).
    safe: Vec<NodeId>,
}

#[derive(Clone, Copy)]
enum V {
    Auto,
    Pt(f32),
    Pct(f32),
}

fn val(v: Option<&Value>) -> V {
    match v {
        Some(Value::Number(n)) => V::Pt(n.as_f64().unwrap_or(0.0) as f32),
        Some(Value::String(s)) => match s.strip_suffix('%').and_then(|p| p.trim().parse::<f32>().ok()) {
            Some(p) => V::Pct(p / 100.0),
            None => s.parse().map_or(V::Auto, V::Pt),
        },
        _ => V::Auto,
    }
}

impl V {
    fn dim(self) -> Dimension {
        match self {
            V::Auto => auto(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
    fn lpa(self) -> LengthPercentageAuto {
        match self {
            V::Auto => auto(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
    fn lp(self) -> LengthPercentage {
        match self {
            V::Auto => zero(),
            V::Pt(x) => length(x),
            V::Pct(x) => percent(x),
        }
    }
}

fn num(p: &Value, k: &str) -> Option<f64> {
    p.get(k).and_then(Value::as_f64)
}

/// Yoga edge precedence: edge > start/end > horizontal/vertical > all. Order: top, right, bottom, left.
fn edges<'a>(p: &'a Value, b: &str, sfx: &str) -> [Option<&'a Value>; 4] {
    let g = |side: &str| p.get(format!("{b}{side}{sfx}")).filter(|v| !v.is_null());
    let (h, v) = (g("Horizontal").or(g("")), g("Vertical").or(g("")));
    [g("Top").or(v), g("Right").or(g("End")).or(h), g("Bottom").or(v), g("Left").or(g("Start")).or(h)]
}

fn rect<T>(e: [Option<&Value>; 4], f: impl Fn(V) -> T) -> taffy::Rect<T> {
    // Unset margin/padding/border is 0, only an explicit "auto" is auto.
    let [top, right, bottom, left] = e.map(|v| f(v.map_or(V::Pt(0.0), |v| val(Some(v)))));
    taffy::Rect { top, right, bottom, left }
}

fn align(s: Option<&str>, default: AlignItems) -> AlignItems {
    match s {
        Some("flex-start") => AlignItems::FLEX_START,
        Some("flex-end") => AlignItems::FLEX_END,
        Some("center") => AlignItems::CENTER,
        Some("baseline") => AlignItems::BASELINE,
        Some("stretch") => AlignItems::STRETCH,
        _ => default,
    }
}

fn justify(s: Option<&str>, default: AlignContent) -> AlignContent {
    match s {
        Some("flex-start") => AlignContent::FLEX_START,
        Some("flex-end") => AlignContent::FLEX_END,
        Some("center") => AlignContent::CENTER,
        Some("stretch") => AlignContent::STRETCH,
        Some("space-between") => AlignContent::SPACE_BETWEEN,
        Some("space-around") => AlignContent::SPACE_AROUND,
        Some("space-evenly") => AlignContent::SPACE_EVENLY,
        _ => default,
    }
}

fn style(p: &Value) -> Style {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let wrap = matches!(s("flexWrap"), Some("wrap" | "wrap-reverse"));
    let horizontal = p.get("horizontal").and_then(Value::as_bool) == Some(true);
    let flex = num(p, "flex");
    // Yoga: flexGrow/flexShrink/flexBasis override `flex`; positive flex = grow n, basis 0; negative = shrink.
    let grow = num(p, "flexGrow").or(flex.filter(|f| *f > 0.0)).unwrap_or(0.0);
    let shrink = num(p, "flexShrink").or(flex.filter(|f| *f < 0.0).map(|f| -f)).unwrap_or(0.0);
    let basis = match p.get("flexBasis") {
        Some(b) if !b.is_null() => val(Some(b)).dim(),
        _ if flex.is_some_and(|f| f > 0.0) => length(0.0),
        _ => auto(),
    };
    let gap = |k: &str| val(p.get(k).filter(|v| !v.is_null()).or(p.get("gap"))).lp();
    let zero_min = |k: &str| match val(p.get(k)) {
        V::Auto => length(0.0),
        v => v.lpa(),
    };
    Style {
        display: if s("display") == Some("none") { Display::None } else { Display::Flex },
        position: if s("position") == Some("absolute") { Position::Absolute } else { Position::Relative },
        flex_direction: match s("flexDirection") {
            Some("row") => FlexDirection::Row,
            Some("row-reverse") => FlexDirection::RowReverse,
            Some("column-reverse") => FlexDirection::ColumnReverse,
            None if horizontal => FlexDirection::Row,
            _ => FlexDirection::Column,
        },
        flex_wrap: match s("flexWrap") {
            Some("wrap") => FlexWrap::Wrap,
            Some("wrap-reverse") => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        },
        align_items: align(s("alignItems"), AlignItems::STRETCH),
        align_self: s("alignSelf").filter(|a| *a != "auto").map(|a| align(Some(a), AlignItems::STRETCH)),
        // Yoga applies alignContent to wrapped containers only (RN default flex-start); taffy would shrink the single line.
        align_content: if wrap { justify(s("alignContent"), AlignContent::FLEX_START) } else { AlignContent::STRETCH },
        justify_content: justify(s("justifyContent"), AlignContent::FLEX_START),
        gap: taffy::Size { width: gap("columnGap"), height: gap("rowGap") },
        margin: rect(edges(p, "margin", ""), V::lpa),
        padding: rect(edges(p, "padding", ""), V::lp),
        border: rect(edges(p, "border", "Width"), V::lp),
        inset: taffy::Rect {
            top: val(p.get("top")).lpa(),
            right: val(p.get("right")).lpa(),
            bottom: val(p.get("bottom")).lpa(),
            left: val(p.get("left")).lpa(),
        },
        size: taffy::Size { width: val(p.get("width")).dim(), height: val(p.get("height")).dim() },
        // Yoga has no CSS "automatic minimum size": unset min is 0.
        min_size: taffy::Size { width: zero_min("minWidth"), height: zero_min("minHeight") },
        max_size: taffy::Size { width: val(p.get("maxWidth")).lpa(), height: val(p.get("maxHeight")).lpa() },
        flex_grow: grow as f32,
        flex_shrink: shrink as f32,
        flex_basis: basis,
        aspect_ratio: num(p, "aspectRatio").map(|a| a as f32),
        box_sizing: BoxSizing::BorderBox,
        ..Style::default()
    }
}

fn weight(p: &Value) -> u16 {
    match p.get("fontWeight") {
        Some(Value::String(s)) => match s.as_str() {
            "bold" => 700,
            "normal" => 400,
            n => n.parse().unwrap_or(400),
        },
        Some(Value::Number(n)) => n.as_u64().unwrap_or(400) as u16,
        _ => 400,
    }
}

impl<'a> Cx<'a, '_> {
    /// `RNCSafeAreaView` / `RNSSafeAreaView` `edges` set to `additive` (or `true`) add the inset to the padding.
    fn safe_padding(&self, p: &Value, st: &mut Style) {
        let on = |side: &str| p.get("edges").and_then(|e| e.get(side)).is_some_and(|v| v.as_bool() == Some(true) || v.as_str() == Some("additive"));
        let pad = |side: &str, extra: f64| {
            let base = num(p, &format!("padding{side}")).or(num(p, if matches!(side, "Top" | "Bottom") { "paddingVertical" } else { "paddingHorizontal" })).or(num(p, "padding")).unwrap_or(0.0);
            length((base + extra) as f32)
        };
        if on("top") {
            st.padding.top = pad("Top", self.insets.0);
        }
        if on("bottom") {
            st.padding.bottom = pad("Bottom", self.insets.1);
        }
    }

    fn note(&mut self, m: impl Into<String>) {
        self.notes.insert(m.into());
    }

    /// RN colors are 0xAARRGGBB numbers (`processColor`); anything else is ignored and reported once.
    fn color(&mut self, v: Option<&Value>) -> Option<Color> {
        let v = v.filter(|v| !v.is_null())?;
        let Some(n) = v.as_u64().or(v.as_i64().map(|i| i as u32 as u64)).filter(|n| *n <= u32::MAX as u64) else {
            self.note("unsupported_color: non-numeric color (PlatformColor/DynamicColorIOS?) ignored");
            return None;
        };
        let c = |shift: u32| ((n >> shift) & 0xff) as f32 / 255.0;
        Some(Color::rgba(c(16), c(8), c(0), c(24)))
    }

    fn line_h(&self, size: f32) -> f32 {
        let s = self.scale as f32;
        (size * 1.1934 * s).ceil() / s
    }

    /// Text of a paragraph subtree; `RCTVirtualText` spans that restyle color, weight, size or family become spans
    /// (inherited by nested spans).
    // ponytail: per-span fontStyle, letterSpacing and underline are ignored; `numberOfLines` always ellipsizes at the tail.
    fn collect(&mut self, n: &ViewNode, out: &mut String, spans: &mut Vec<(usize, usize, SpanStyle)>, inherit: SpanStyle, transform: Option<&str>) {
        let ViewKind::Rn { component, props, children } = &n.kind else { return };
        let mut inh = inherit;
        let transform = props.get("textTransform").and_then(Value::as_str).or(transform);
        if component == "RCTVirtualText" {
            if let Some(c) = self.color(props.get("color")) {
                inh.color = Some([(c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8]);
            }
            if props.get("fontWeight").is_some_and(|w| !w.is_null()) {
                inh.weight = Some(weight(props));
            }
            if let Some(sz) = num(props, "fontSize") {
                inh.size = Some(sz as f32);
            }
            self.span_lh = self.span_lh.max(num(props, "lineHeight").unwrap_or(0.0) as f32);
            if props.get("fontFamily").and_then(Value::as_str).is_some_and(|f| f.contains("mono")) {
                inh.mono = true;
            }
        }
        if component == "RCTRawText" {
            let t = props.get("text").and_then(Value::as_str).unwrap_or("");
            let t = match transform {
                Some("uppercase") => t.to_uppercase(),
                Some("lowercase") => t.to_lowercase(),
                _ => t.to_string(),
            };
            if inh != SpanStyle::default() {
                spans.push((out.len(), out.len() + t.len(), inh));
            }
            out.push_str(&t);
        }
        for c in children {
            self.collect(c, out, spans, inh, transform);
        }
    }


    fn text(&mut self, node: &ViewNode, p: &Value) -> Text {
        let mut text = String::new();
        let mut spans = vec![];
        self.span_lh = 0.0;
        self.collect(node, &mut text, &mut spans, SpanStyle::default(), None);
        let lines = num(p, "numberOfLines").filter(|n| *n >= 1.0).map(|n| n as usize);
        let size = num(p, "fontSize").unwrap_or(14.0) as f32;
        let align = match p.get("textAlign").and_then(Value::as_str) {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Trailing,
            _ => TextAlign::Leading,
        };
        let mut spec = FontSpec::new(size, self.line_h(size), weight(p), align);
        spec.mono = p.get("fontFamily").and_then(Value::as_str).is_some_and(|f| f.contains("mono"));
        spec.pitch = spec.line_height; // RN lines are one font line height apart (SwiftUI's 22/17 leading does not apply)
        // A nested span's lineHeight replaces the paragraph's for the lines it sits on; applied to the whole paragraph (ponytail).
        if let Some(lh) = Some(self.span_lh as f64).filter(|l| *l > 0.0).or(num(p, "lineHeight").filter(|l| *l > 0.0)) {
            (spec.line_height, spec.pitch) = (lh as f32, lh as f32);
        }
        spec.lines = lines;
        spec.fixed_pitch = self.span_lh > 0.0 || num(p, "lineHeight").is_some_and(|l| l > 0.0);
        let color = self.color(p.get("color")).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let spans = runs(&text, &spans, &spec, color);
        // Yoga measures these paragraphs in points. Keep Inter's tracking fit constant when rasterizing
        // so that word wrapping, line caps and the width of styled runs agree with layout.
        spec.inter_point_tracking = spec.lines.is_some() || spec.fixed_pitch || spans.len() > 1;
        let strike = p.get("textDecorationLine").and_then(Value::as_str).is_some_and(|d| d.contains("line-through"));
        Text { text, spec, color, strike, field: false, single: false, fixed: None, spans, explicit_lh: num(p, "lineHeight").is_some() }
    }

    fn build(&mut self, node: &'a ViewNode) -> Option<Item<'a>> {
        let ViewKind::Rn { component, props, children } = &node.kind else { return None };
        let input = matches!(component.as_str(), "RCTSinglelineTextInputView" | "RCTMultilineTextInputView");
        match component.as_str() {
            "RCTRawText" | "RCTVirtualText" => return None,
            "RootView" | "RCTView" | "RCTScrollView" | "RCTScrollContentView" | "RCTImageView" | "RCTText" | "RCTSwitch"
            | "RCTModalHostView" | "RNSScreenStack" | "RNSScreen" | "RNSScreenContentWrapper" | "RNSTabsHostIOS"
            | "RNSTabsScreenIOS" | "RNCSafeAreaProvider" | "RNCSafeAreaView" | "RNSSafeAreaView"
            | "ViewManagerAdapter_ExpoImage" | "ViewManagerAdapter_SymbolModule" | "RNGestureHandlerButton" | "RNGestureHandlerRootView"
            | "ActivityIndicatorView" | "PullToRefreshView" | "SafeAreaView" | "ViewManagerAdapter_ExpoLinearGradient" | "ViewManagerAdapter_ExpoBlur_ExpoBlurView" => {}
            // Drawn by the host from the screen's header props.
            "RNSScreenStackHeaderConfig" | "RNSScreenStackHeaderSubview" => return None,
            c if c.starts_with("ViewManagerAdapter_ExpoGlassEffect_") => {}
            _ if input => {}
            other => self.note(format!("unsupported_component: {other}")),
        }
        let mut st = style(props);
        // Yoga measures a row item's flex basis at most as wide as the container, taffy at max-content (which would
        // overflow wrapping text): a growing item without a size starts from 0 instead.
        if self.parent_row && st.flex_grow > 0.0 && props.get("width").is_none() && props.get("flexBasis").is_none() && props.get("flex").is_none() {
            st.flex_basis = length(0.0);
        }
        let row = matches!(st.flex_direction, FlexDirection::Row | FlexDirection::RowReverse);
        if component == "RCTScrollContentView" && props.get("width").is_none() {
            // Yoga sizes a vertical scroll view's content to the scroll view's width.
            st.size.width = percent(1.0);
        }
        if matches!(component.as_str(), "RNCSafeAreaView" | "RNSSafeAreaView") {
            self.safe_padding(props, &mut st);
        }
        // iOS puts the refresh control above the content (drawn by the scroll view), outside the content flow.
        if component == "PullToRefreshView" {
            st.position = Position::Absolute;
            st.size = taffy::Size { width: percent(1.0), height: length(0.0) };
        }
        if component == "RCTText" {
            let t = self.text(node, props);
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item::new(node, props, id, vec![]));
        }
        if input {
            let mut t = self.text(node, props);
            let value = props.get("__simcoreText").and_then(Value::as_str).unwrap_or("");
            t.text = if value.is_empty() {
                props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
            } else if props.get("secureTextEntry").and_then(Value::as_bool) == Some(true) {
                "\u{2022}".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            (t.field, t.single) = (true, component == "RCTSinglelineTextInputView");
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item::new(node, props, id, vec![]));
        }
        if component == "RCTSwitch" {
            let t = Text {
                text: String::new(),
                spec: FontSpec::new(14.0, 17.0, 400, TextAlign::Leading),
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
                strike: false,
                field: false,
                single: false,
                fixed: Some((SWITCH.0 as f32, SWITCH.1 as f32)),
                spans: vec![],
                explicit_lh: false,
            };
            let id = self.tree.new_leaf_with_context(st, t).ok()?;
            return Some(Item::new(node, props, id, vec![]));
        }
        // Native containers draw one child: a stack its top screen, a tab host the selected tab.
        let mut tabs = None;
        let mut picked: Vec<&'a ViewNode> = children.iter().collect();
        if component == "RNSScreenStack" {
            let screens: Vec<&ViewNode> = children.iter().filter(|c| rn_component(c) == "RNSScreen").collect();
            let prev = screens.len().checked_sub(2).and_then(|i| screens.get(i)).and_then(|s| screen_header(s)).and_then(|h| prop_str(h, "title"));
            self.stack = Some((node.id.clone(), prev.filter(|_| screens.len() > 1)));
            picked = screens.last().copied().into_iter().collect();
        } else if component == "RNSTabsHostIOS" {
            let t = tabs_info(self, props, children);
            picked = children.iter().filter(|c| rn_component(c) == "RNSTabsScreenIOS").skip(t.bar.selected).take(1).collect();
            tabs = Some(t);
        }
        let mut header = None;
        if component == "RNSScreen" {
            let (stack, prev) = self.stack.take().unwrap_or_default();
            if let Some(hp) = screen_header(node) {
                let flag = |k: &str| hp.get(k).and_then(Value::as_bool) == Some(true);
                if !flag("hidden") {
                    let back = if flag("hideBackButton") { None } else { prop_str(hp, "backTitle").filter(|t| !t.is_empty()).or(prev) };
                    header = Some(Header { stack, title: prop_str(hp, "title"), large: flag("largeTitle"), back });
                }
            }
        }
        // The tab bar (83 pt with its safe area) is the bottom safe-area inset of the tab's content.
        let saved_insets = self.insets;
        if tabs.as_ref().is_some_and(|t| !t.hidden) {
            self.insets.1 = TAB_BAR_INSET;
        }
        let saved_row = std::mem::replace(&mut self.parent_row, row);
        let kids: Vec<Item> = picked.into_iter().filter_map(|c| self.build(c)).collect();
        (self.insets, self.parent_row) = (saved_insets, saved_row);
        let ids: Vec<NodeId> = kids.iter().map(|k| k.id).collect();
        let id = self.tree.new_with_children(st, &ids).ok()?;
        if component == "SafeAreaView" {
            self.safe.push(id);
        }
        let mut item = Item::new(node, props, id, kids);
        (item.header, item.tabs) = (header, tabs);
        if component == "RCTModalHostView" {
            if props.get("visible").and_then(Value::as_bool) != Some(false) {
                self.modals.push(item);
            }
            return None;
        }
        Some(item)
    }

    /// Lay out the subtree at `id` as a root of `w` x `h` points. Core `SafeAreaView`s then get the part of the
    /// insets they overlap as padding (replacing their own, like RN's SafeAreaView shadow node) and it is laid out again.
    fn compute(&mut self, id: NodeId, w: f64, h: f64) {
        self.compute_pass(id, w, h);
        let mut again = false;
        for s in self.safe.clone() {
            let (mut y, mut n) = (0.0, s);
            loop {
                y += self.tree.layout(n).map_or(0.0, |l| l.location.y as f64);
                match self.tree.parent(n) {
                    Some(p) => n = p,
                    None => break,
                }
            }
            if n != id {
                continue;
            }
            // ponytail: ancestors' scroll offsets are ignored (a SafeAreaView inside a scrolled ScrollView pads as if unscrolled).
            let sh = self.tree.layout(s).map_or(0.0, |l| l.size.height as f64);
            let (top, bottom) = ((self.insets.0 - y).clamp(0.0, self.insets.0), (y + sh - (h - self.insets.1)).clamp(0.0, self.insets.1));
            let mut st = self.tree.style(s).cloned().unwrap_or_default();
            st.padding = taffy::Rect { left: length(0.0), right: length(0.0), top: length(top as f32), bottom: length(bottom as f32) };
            let _ = self.tree.set_style(s, st);
            again = true;
        }
        if again {
            self.compute_pass(id, w, h);
        }
    }

    fn compute_pass(&mut self, id: NodeId, w: f64, h: f64) {
        let mut st = self.tree.style(id).cloned().unwrap_or_default();
        st.size = taffy::Size { width: length(w as f32), height: length(h as f32) };
        st.position = Position::Relative;
        let _ = self.tree.set_style(id, st);
        let scale = self.scale as f32;
        let fonts = &mut *self.fonts;
        let _ = self.tree.compute_layout_with_measure(
            id,
            taffy::Size { width: AvailableSpace::Definite(w as f32), height: AvailableSpace::Definite(h as f32) },
            |inp, _, t, style| {
                compute_leaf_layout(inp, style, |_, _| 0.0, |known, avail| {
                    let Some(t) = t else { return taffy::Size::ZERO };
                    if let Some((fw, fh)) = t.fixed {
                        return taffy::Size { width: known.width.unwrap_or(fw), height: known.height.unwrap_or(fh) };
                    }
                    let wrap = known.width.or(match avail.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => Some(0.0),
                        // Yoga measures flex bases at most as wide as the container; the screen bounds any container.
                        AvailableSpace::MaxContent => Some(w as f32),
                    });
                    let (mw, mh) = fonts.measure(&t.text, &t.spec, if t.single { None } else { wrap }, &t.spans);
                    let mh = if t.field { mh.max(t.spec.line_height as f64) } else { mh };
                    // Yoga rounds text sizes up to the pixel grid.
                    let up = |x: f64| (x as f32 * scale).ceil() / scale;
                    // iOS paragraphs with an explicit lineHeight come out one device pixel taller (measured on iOS 26.5).
                    let extra = if t.explicit_lh { 1.0 / scale } else { 0.0 };
                    taffy::Size { width: known.width.unwrap_or(up(mw)), height: known.height.unwrap_or(up(mh) + extra) }
                })
            },
        );
    }
}

pub fn layout(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    let (layers, root): (&[simcore_protocol::Layer], &ViewNode) = match &root.kind {
        ViewKind::Presentation { content, layers } => (layers, content),
        _ => (&[], root),
    };
    let (w, h) = params.viewport;
    let dark = params.appearance == Appearance::Dark;
    let mut cx = Cx { tree: TaffyTree::new(), notes: BTreeSet::new(), scale: params.scale.max(1.0), dark, fonts, modals: vec![], insets: insets(h), inset_top: 0.0, rot: 0.0, stack: None, parent_row: false, span_lh: 0.0, safe: vec![] };
    cx.tree.disable_rounding();
    let bg = if dark { 0.0 } else { 1.0 };
    let mut screen = Node::new("root", "screen");
    screen.bounds = Rect { x: 0.0, y: 0.0, w, h };
    screen.fill = Some(Color::rgba(bg, bg, bg, 1.0));
    let mut nodes = vec![screen];
    if let Some(item) = cx.build(root) {
        // The surface root fills the viewport.
        cx.compute(item.id, w, h);
        paint(&mut cx, &item, (0.0, 0.0), 0, 1.0, params, &mut nodes);
        for m in std::mem::take(&mut cx.modals) {
            cx.compute(m.id, w, h);
            let first = nodes.len();
            paint(&mut cx, &m, (0.0, 0.0), 0, 1.0, params, &mut nodes);
            if m.props.get("transparent").and_then(Value::as_bool) != Some(true) {
                nodes[first].fill = Some(Color::rgba(bg, bg, bg, 1.0));
            }
        }
    }
    super::swiftui::rn_alerts(params, cx.fonts, layers, &mut nodes);
    // RN does not avoid the keyboard: it just covers the bottom of the screen.
    if params.chrome.keyboard && nodes.iter().any(|n| n.field.is_some() && Some(n.id.as_str()) == params.focus) {
        let kb_h = params.device.kb_base_h + params.device.safe_bottom;
        crate::chrome::keyboard(&mut nodes, w, h, kb_h, dark, params.chrome.shift, params.device);
    } else if params.device.safe_bottom > 0.0 && !dark && !nodes.iter().any(|n| n.kind == "tabbar") {
        // Home indicator (measured on iOS 26.5: 144 x 4.7 pt, 13 pt above the bottom edge). ponytail: light only,
        // the dark references show none; it also dims/hides on iOS, which is not modelled.
        let mut bar = Node::new("home_indicator", "home_indicator");
        bar.bounds = Rect { x: (w - 144.0) / 2.0, y: h - 13.0, w: 144.0, h: 4.7 };
        bar.parent = Some(0);
        bar.radius = 3.0;
        bar.fill = Some(Color::rgba(0.0, 0.0, 0.0, 0.61));
        bar.inspect = false;
        nodes.push(bar);
    }
    add_chrome(&mut nodes, params, cx.fonts);
    Scene {
        size: params.viewport,
        appearance: params.appearance,
        nodes,
        unsupported_symbols: vec![],
        notes: cx.notes.into_iter().collect(),
    }
}

fn corner_radius(p: &Value) -> f64 {
    num(p, "borderRadius").unwrap_or_else(|| {
        // ponytail: per-corner radii collapse to the largest, per-corner paths need a custom clip/fill.
        ["TopLeft", "TopRight", "BottomLeft", "BottomRight"]
            .iter()
            .filter_map(|c| num(p, &format!("border{c}Radius")))
            .fold(0.0, f64::max)
    })
}

/// `origin`: page position of the parent's content origin (scroll offset already subtracted).
fn paint(cx: &mut Cx, it: &Item, origin: (f64, f64), parent: usize, parent_opacity: f32, params: &SceneParams, out: &mut Vec<Node>) {
    let Ok(&l) = cx.tree.layout(it.id) else { return };
    let p = it.props;
    if cx.tree.style(it.id).is_ok_and(|s| s.display == Display::None) {
        return;
    }
    // ponytail: transform supports translateX/translateY only; scale/rotate/skew are ignored (Scene has no transforms).
    let (mut tx, mut ty) = (0.0, 0.0);
    for t in p.get("transform").and_then(Value::as_array).into_iter().flatten() {
        tx += num(t, "translateX").unwrap_or(0.0);
        ty += num(t, "translateY").unwrap_or(0.0);
    }
    let mut spin = 0.0f32;
    for t in p.get("transform").and_then(Value::as_array).into_iter().flatten() {
        let r = t.get("rotateZ").or(t.get("rotate")).and_then(Value::as_str).and_then(|d| d.strip_suffix("deg").and_then(|v| v.parse::<f32>().ok()));
        spin += r.unwrap_or(0.0);
    }
    let (x, y) = (origin.0 + l.location.x as f64 + tx, origin.1 + l.location.y as f64 + ty);
    let (w, h) = (l.size.width as f64, l.size.height as f64);
    let ViewKind::Rn { component, .. } = &it.node.kind else { return };
    let mut n = Node::new(&it.node.id, "view");
    n.bounds = Rect { x, y, w, h };
    n.parent = Some(parent);
    n.tag = it.node.id.parse().ok();
    n.opacity = (num(p, "opacity").unwrap_or(1.0) as f32).clamp(0.0, 1.0) * parent_opacity;
    n.fill = cx.color(p.get("backgroundColor"));
    let black = Color::rgba(0.0, 0.0, 0.0, 1.0);
    let all = cx.color(p.get("borderColor")).unwrap_or(black);
    let side = ["Top", "Right", "Bottom", "Left"].map(|e| cx.color(p.get(format!("border{e}Color"))).unwrap_or(all));
    n.radius = corner_radius(p);
    if matches!(p.get("overflow").and_then(Value::as_str), Some("hidden" | "scroll")) || component == "RCTScrollView" {
        n.clip_radius = Some(n.radius);
    }
    let (mut ox, mut oy) = (x, y);
    if component == "RCTScrollView" {
        n.kind = "scroll";
        let vertical = p.get("horizontal").and_then(Value::as_bool) != Some(true);
        let content = it.kids.iter().find(|k| rn_component(k.node) != "PullToRefreshView").and_then(|k| cx.tree.layout(k.id).ok()).map_or((0.0, 0.0), |c| (c.size.width as f64, c.size.height as f64));
        // A refreshing `RefreshControl` keeps its band open above the content (UIRefreshControl's extra top inset).
        let refreshing = it.kids.iter().any(|k| rn_component(k.node) == "PullToRefreshView" && k.props.get("refreshing").and_then(Value::as_bool) == Some(true));
        // Content inset: an explicit `contentInset`, or the header/status area for `automatic` adjustment under a stack header.
        let ci = |k: &str| p.get("contentInset").and_then(|i| num(i, k));
        let auto = p.get("contentInsetAdjustmentBehavior").and_then(Value::as_str) == Some("automatic") && p.get("contentInset").is_none();
        let (it_top, it_bottom) = (ci("top").unwrap_or(if auto { cx.inset_top } else { 0.0 }) + if refreshing { REFRESH_H } else { 0.0 }, ci("bottom").unwrap_or(0.0));
        let max = if vertical { (0.0, (content.1 + it_top + it_bottom - h).max(0.0)) } else { ((content.0 - w).max(0.0), 0.0) };
        // `contentOffset` is only the initial offset: any host scroll state wins.
        let initial = p.get("contentOffset").map_or((0.0, 0.0), |o| (num(o, "x").unwrap_or(0.0), num(o, "y").unwrap_or(0.0)));
        let off = params.scroll.get(&it.node.id).copied().unwrap_or(initial);
        let off = (off.0.clamp(0.0, max.0), off.1.clamp(0.0, max.1));
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None, content: Some(content) });
        (ox, oy) = (x - off.0, y - off.1 + it_top);
    }
    if let Some(g) = p.get("experimental_backgroundImage").and_then(Value::as_array).and_then(|a| a.first()) {
        if g.get("type").and_then(Value::as_str) == Some("linear-gradient") {
            let stops: Vec<Color> = g.get("colorStops").and_then(Value::as_array).into_iter().flatten().filter_map(|c| cx.color(c.get("color"))).collect();
            let angle = g.get("direction").and_then(|d| num(d, "value")).unwrap_or(180.0) as f32;
            if stops.len() >= 2 {
                n.gradient = Some(crate::scene::Gradient { angle, stops, points: None, locations: None });
            }
        }
    }
    let me = out.len();
    let mut extras: Vec<Node> = vec![];
    let ctx = cx.tree.get_node_context(it.id).cloned();
    let a11y = p.get("accessibilityLabel").and_then(Value::as_str).map(str::to_owned);
    if component == "ViewManagerAdapter_SymbolModule" {
        n.kind = "image";
        n.role = Role::Image;
        let mut name = prop_str(p, "name").unwrap_or_default();
        // ponytail: only quarter turns of chevrons are supported (the symbol renderer has no rotation).
        let turn = p.get("transform").and_then(Value::as_array).into_iter().flatten().find_map(|t| t.get("rotate").and_then(Value::as_str)).and_then(|d| d.strip_suffix("deg").and_then(|v| v.parse::<f32>().ok())).unwrap_or(0.0);
        if let (Some(dir), true) = (name.strip_prefix("chevron."), turn != 0.0) {
            let dirs = ["up", "right", "down", "left"];
            if let Some(i) = dirs.iter().position(|d| *d == dir) {
                name = format!("chevron.{}", dirs[(i as i32 + (turn / 90.0).round() as i32).rem_euclid(4) as usize]);
            }
        }
        let weight = match p.get("weight").and_then(Value::as_str) {
            Some("ultraLight") => 100,
            Some("thin") => 200,
            Some("light") => 300,
            Some("medium") => 500,
            Some("semibold") => 600,
            Some("bold") => 700,
            Some("heavy") => 800,
            Some("black") => 900,
            _ => 400,
        };
        let color = p.get("tintColor").and_then(Value::as_str).and_then(css_color).unwrap_or(Color::rgba(0.0, 0.0, 0.0, 1.0));
        let scale = match p.get("scale").and_then(Value::as_str) {
            Some("small") => simcore_protocol::ImageScale::Small,
            Some("large") => simcore_protocol::ImageScale::Large,
            _ => simcore_protocol::ImageScale::Medium,
        };
        // Point size such that the symbol's natural box fits the view.
        let unit = crate::SymbolSpec { name: name.clone(), point_size: 100.0, weight, scale };
        if let Some(m) = crate::symbols::metrics(&unit) {
            let k = (w / m.w).min(h / m.h) * 100.0 / 100.0;
            let spec = crate::SymbolSpec { point_size: k * 100.0, ..unit };
            n.symbol = Some(crate::SymbolRun { spec, color, size: (m.w * k, m.h * k) });
        } else {
            cx.note(format!("unsupported_symbol: {name}"));
        }
        n.label = Some(name);
    }
    if component.starts_with("ViewManagerAdapter_ExpoGlassEffect_GlassView") {
        let variant = if p.get("glassEffectStyle").and_then(Value::as_str) == Some("clear") { simcore_protocol::GlassVariant::Clear } else { simcore_protocol::GlassVariant::Regular };
        let tint = p.get("tintColor").and_then(|t| t.as_str().and_then(css_color).or_else(|| cx.color(Some(t))));
        n.glass = Some(crate::glass::GlassSpec { variant, tint, interactive: p.get("isInteractive").and_then(Value::as_bool) == Some(true), blur: None, mix: None, shadow: None, flat: None });
        if n.radius == 0.0 {
            n.radius = h.min(w) / 2.0;
        }
    }
    if component == "ActivityIndicatorView" {
        // `hidesWhenStopped` (default on): a stopped indicator draws nothing. ponytail: a static frame (spokes at rest).
        let stopped = p.get("animating").and_then(Value::as_bool) == Some(false);
        if !(stopped && p.get("hidesWhenStopped").and_then(Value::as_bool) != Some(false)) {
            let c = cx.color(p.get("color")).unwrap_or(Color::rgba(0.6, 0.6, 0.6, 1.0));
            n.glyph = Some((Glyph::Spinner, c));
            if prop_str(p, "size").as_deref() == Some("large") {
                // UIKit's large indicator is 33 pt, pinned to the top-left of RN's 36 pt frame (measured on iOS 26.5).
                n.glyph = Some((Glyph::SpinnerLarge, c));
                n.bounds = Rect { x, y, w: 33.0, h: 33.0 };
            }
        }
        n.kind = "spinner";
    }
    if component == "PullToRefreshView" {
        // The spinner sits centred in the open band above the content (`origin` is the content's, shifted by the band).
        if p.get("refreshing").and_then(Value::as_bool) == Some(true) {
            let c = cx.color(p.get("tintColor")).unwrap_or(if cx.dark { REFRESH_TINT.1 } else { REFRESH_TINT.0 });
            n.bounds = Rect { x: x + (w - REFRESH_SPINNER) / 2.0, y: y - (REFRESH_H + REFRESH_SPINNER) / 2.0, w: REFRESH_SPINNER, h: REFRESH_SPINNER };
            n.glyph = Some((Glyph::SpinnerLarge, c));
        }
        n.kind = "refresh";
    }
    if component == "ViewManagerAdapter_ExpoLinearGradient" {
        let stops: Vec<Color> = p.get("colors").and_then(Value::as_array).into_iter().flatten().filter_map(|c| cx.color(Some(c))).collect();
        let pt = |k: &str, d: [f32; 2]| p.get(k).and_then(Value::as_array).filter(|a| a.len() == 2).map_or(d, |a| [a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32]);
        let (a, b) = (pt("startPoint", [0.5, 0.0]), pt("endPoint", [0.5, 1.0]));
        let locations = p.get("locations").and_then(Value::as_array).map(|l| l.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>()).filter(|l| l.len() == stops.len());
        if !stops.is_empty() {
            n.gradient = Some(crate::scene::Gradient { angle: 0.0, stops, points: Some([a[0], a[1], b[0], b[1]]), locations });
        }
    }
    if component == "ViewManagerAdapter_ExpoBlur_ExpoBlurView" {
        n.glass = Some(blur_spec(prop_str(p, "tint").as_deref().unwrap_or("default"), num(p, "intensity").unwrap_or(50.0), cx.dark));
    }
    if matches!(component.as_str(), "RCTImageView" | "ViewManagerAdapter_ExpoImage") {
        n.kind = "image";
        n.role = Role::Image;
        n.label = a11y.clone();
        if let Some(img) = p.get("__simcoreImage") {
            let path = img.get("path").and_then(Value::as_str).unwrap_or("");
            match crate::image::load(path) {
                Some(pixmap) => {
                    let dest = image_dest(p, (w, h), (pixmap.width() as f64, pixmap.height() as f64), params.scale);
                    // Template images (`tintColor`) draw their alpha in the tint color.
                    let tint = p.get("tintColor").and_then(|t| t.as_str().and_then(css_color).or_else(|| cx.color(Some(t))));
                    let pixmap = tint.and_then(|c| tinted(&pixmap, c)).unwrap_or(pixmap);
                    n.image = Some(ImageRun { pixmap, dest: Rect { x: x + dest.x, y: y + dest.y, ..dest }, rotation: cx.rot + spin });
                }
                None => cx.note(format!("image_decode_failed: {path}")),
            }
        }
    }
    if component == "RCTSwitch" {
        let on = p.get("value").and_then(Value::as_bool) == Some(true);
        let disabled = p.get("disabled").and_then(Value::as_bool) == Some(true);
        // RN's off track is `tintColor`; unset it is a darker gray than SwiftUI's (measured on iOS 26.5).
        let fill = if on {
            cx.color(p.get("onTintColor"))
        } else {
            cx.color(p.get("tintColor")).or(Some(Color::rgba(120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0, if cx.dark { 0.36 } else { 0.43 })))
        };
        let thumb = cx.color(p.get("thumbTintColor"));
        let base = n.opacity;
        let mut knob = switch_style(&mut n, &it.node.id, cx.dark, on, disabled, fill, thumb);
        n.kind = "switch";
        n.label = a11y;
        n.bounds = Rect { x, y, w: SWITCH.0, h: SWITCH.1 };
        n.opacity = base * if disabled { 0.5 } else { 1.0 };
        knob.bounds = Rect { x: x + if on { SWITCH.0 - 2.0 - KNOB.0 } else { 2.0 }, y: y + (SWITCH.1 - KNOB.1) / 2.0, w: KNOB.0, h: KNOB.1 };
        knob.parent = Some(me);
        knob.opacity = n.opacity;
        extras.push(knob);
    }
    if matches!(component.as_str(), "RCTSinglelineTextInputView" | "RCTMultilineTextInputView") {
        extras = text_field(cx, &mut n, it, me, &l, params);
    }
    if let Some(t) = ctx.filter(|_| component == "RCTText") {
        n.kind = "text";
        n.role = Role::Text;
        n.label = Some(t.text.clone());
        let run = TextRun { text: t.text, font: t.spec, color: t.color, strike: t.strike, offset_x: 0.0, spans: t.spans };
        let (b, pd) = (l.border, l.padding);
        // A lineHeight below the font's own line height puts the glyphs higher in the line box than centering
        // (UIKit's maximumLineHeight drops the surplus ascent; fitted to the iOS 26.5 "Welcome to Expo" title: 0.55 x surplus).
        let nat = cx.line_h(t.spec.size) as f64;
        let lift = if num(p, "lineHeight").is_some_and(|l| l > 0.0 && l < nat) { -(nat - t.spec.line_height as f64) * 0.55 } else { 0.0 };
        let (ix, iy) = ((b.left + pd.left) as f64, (b.top + pd.top) as f64 + lift);
        let (iw, ih) = (w - ix - (b.right + pd.right) as f64, h - iy - (b.bottom + pd.bottom) as f64);
        if (ix, iy, iw, ih) == (0.0, 0.0, w, h) {
            n.text = Some(run);
            out.push(n);
        } else {
            out.push(n);
            let mut c = Node::new(format!("{}#text", it.node.id), "text");
            c.bounds = Rect { x: x + ix, y: y + iy, w: iw, h: ih };
            c.parent = Some(me);
            c.opacity = out[me].opacity;
            c.inspect = false;
            c.text = Some(run);
            out.push(c);
        }
    } else {
        out.push(n);
    }
    out.extend(extras);
    // Borders: uniform = one stroked outline, otherwise one filled strip per side (square corners).
    let b = [l.border.top, l.border.right, l.border.bottom, l.border.left].map(f64::from);
    let eff = out[me].opacity;
    let child = |id: &str, r: Rect| {
        let mut c = Node::new(format!("{}#{id}", it.node.id), "border");
        c.bounds = r;
        c.parent = Some(me);
        c.opacity = eff;
        c.inspect = false;
        c
    };
    if b[0] > 0.0 && b.iter().all(|v| *v == b[0]) && side.iter().all(|c| *c == side[0]) {
        let bw = b[0];
        let mut c = child("border", Rect { x: x + bw / 2.0, y: y + bw / 2.0, w: w - bw, h: h - bw });
        c.radius = (out[me].radius - bw / 2.0).max(0.0);
        c.stroke = Some((side[0], bw));
        out.push(c);
    } else {
        let strips = [
            Rect { x, y, w, h: b[0] },
            Rect { x: x + w - b[1], y, w: b[1], h },
            Rect { x, y: y + h - b[2], w, h: b[2] },
            Rect { x, y, w: b[3], h },
        ];
        for (i, r) in strips.into_iter().enumerate().filter(|(i, _)| b[*i] > 0.0) {
            let mut c = child(&format!("edge{i}"), r);
            c.fill = Some(side[i]);
            out.push(c);
        }
    }
    let (saved_inset, saved_rot) = (cx.inset_top, cx.rot);
    cx.rot += spin;
    let bar_h = it.header.as_ref().map(|hd| rn_nav_height(params, cx.fonts, params.viewport.0, hd.title.as_deref(), hd.large));
    if let Some(bh) = bar_h {
        cx.inset_top = cx.insets.0 + bh;
    }
    for k in &it.kids {
        paint(cx, k, (ox, oy), me, eff, params, out);
    }
    (cx.inset_top, cx.rot) = (saved_inset, saved_rot);
    if let Some(hd) = &it.header {
        let back = hd.back.clone().map(|label| NavBack { action: hd.stack.parse().ok(), label, pill: true });
        rn_nav_bar(params, cx.fonts, &hd.stack, cx.insets.0, hd.title.as_deref(), hd.large, back, me, out);
    }
    if let Some(t) = it.tabs.as_ref().filter(|t| !t.hidden) {
        rn_tab_bar(params, cx.fonts, &t.bar, &it.node.id, me, eff, out);
    }
    // Touchables (Pressable/Touchable*/Button set these flags) are buttons for inspection and `--click LABEL`:
    // label = accessibilityLabel, else their text, like iOS VoiceOver.
    let touchable = ["onClick", "onStartShouldSetResponder"].iter().any(|k| p.get(*k) == Some(&Value::Bool(true)))
        || p.get("accessibilityRole").and_then(Value::as_str) == Some("button");
    if touchable && !matches!(out[me].role, Role::Text | Role::TextField | Role::Toggle) {
        out[me].role = Role::Button;
        out[me].label = p.get("accessibilityLabel").and_then(Value::as_str).map(str::to_owned).or_else(|| {
            let t: Vec<_> = out[me + 1..].iter().filter(|c| c.role == Role::Text).filter_map(|c| c.label.clone()).collect();
            (!t.is_empty()).then(|| t.join(" "))
        });
    }
}

/// Height of a refreshing `RefreshControl`'s band above the content, its spinner box and default tint (iOS 26.5).
const REFRESH_H: f64 = 181.0 / 3.0;
const REFRESH_SPINNER: f64 = 28.0;
/// (light, dark): the leading spoke's colour.
const REFRESH_TINT: (Color, Color) = (Color { r: 174.0 / 255.0, g: 174.0 / 255.0, b: 175.0 / 255.0, a: 1.0 }, Color { r: 73.0 / 255.0, g: 73.0 / 255.0, b: 79.0 / 255.0, a: 1.0 });

/// expo-blur `BlurView`: a `UIBlurEffect` style (by `tint`) faded in by `intensity` (0...100, the effect animator's
/// fraction, taken as linear), drawn as a flat glass: backdrop blur sigma (pt), saturation, then `c * mul + add`.
/// Fitted on iOS 26.5 per style at full intensity (`default` from its 50% capture).
// ponytail: all `system*Material` thicknesses share systemMaterial's fit, `extraLight` is `light`, `prominent` is `regular`.
fn blur_spec(tint: &str, intensity: f64, dark: bool) -> crate::glass::GlassSpec {
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
fn image_dest(p: &Value, (bw, bh): (f64, f64), (iw, ih): (f64, f64), scale: f64) -> Rect {
    let fit = |s: f64| Rect { x: (bw - iw * s) / 2.0, y: (bh - ih * s) / 2.0, w: iw * s, h: ih * s };
    match p.get("resizeMode").or(p.get("contentFit")).and_then(Value::as_str) {
        Some("stretch" | "fill") => Rect { x: 0.0, y: 0.0, w: bw, h: bh },
        Some("contain") => fit((bw / iw).min(bh / ih)),
        // Natural size (pixels / device scale), scaled down to fit.
        Some("center") => fit((1.0 / scale).min(bw / iw).min(bh / ih)),
        Some("none") => fit(1.0 / scale),
        Some("scale-down") => fit((1.0 / scale).min((bw / iw).min(bh / ih))),
        // ponytail: `repeat` (tiling) draws as cover.
        _ => fit((bw / iw).max(bh / ih)),
    }
}

/// A text input: node fields, the text run (placeholder when empty) and the caret. Returns the child nodes.
fn text_field(cx: &mut Cx, n: &mut Node, it: &Item, me: usize, l: &taffy::Layout, params: &SceneParams) -> Vec<Node> {
    let p = it.props;
    let Some(t) = cx.tree.get_node_context(it.id).cloned() else { return vec![] };
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
                TextAlign::Trailing => (cw - tw).max(0.0),
                TextAlign::Leading => 0.0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_scene, device, DEFAULT_DEVICE};
    use serde_json::json;
    use std::collections::HashMap;

    fn raw(id: &str, text: &str) -> Value {
        json!({"kind":"rn","id":id,"component":"RCTRawText","props":{"text":text},"children":[]})
    }

    #[test]
    fn centered_text_and_padded_pressable() {
        let tree: ViewNode = serde_json::from_value(json!({
            "kind":"rn","id":"1","component":"RootView","props":{},"children":[
              {"kind":"rn","id":"2","component":"RCTView",
               "props":{"flex":1,"alignItems":"center","justifyContent":"center","backgroundColor":0xFFFFFFFFu64},
               "children":[
                 {"kind":"rn","id":"3","component":"RCTText","props":{"fontSize":20},"children":[raw("4","Count: 0")]},
                 {"kind":"rn","id":"5","component":"RCTView",
                  "props":{"padding":12,"borderRadius":8,"backgroundColor":0xFF0080FFu64,"marginTop":10},
                  "children":[{"kind":"rn","id":"6","component":"RCTText","props":{"color":0xFFFFFFFFu64},"children":[raw("7","Press")]}]}]}]
        }))
        .unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0),
            device: dev,
            scale: 3.0,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            scroll: &scroll,
            focus: None,
            overscroll: false,
            status_bar: true,
            chrome: Default::default(),
            sheets: &HashMap::new(),
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let f = |id: &str| scene.nodes.iter().find(|n| n.id == id).unwrap().bounds;
        let (root, col, count, btn, label) = (f("1"), f("2"), f("3"), f("5"), f("6"));
        assert_eq!((root.x, root.y, root.w, root.h), (0.0, 0.0, 402.0, 874.0));
        assert_eq!((col.w, col.h), (402.0, 874.0));
        // Text is content sized and centered on both axes together with the button below it.
        let (tw, th) = (count.w, count.h);
        assert!(tw > 50.0 && th > 15.0 && th < 30.0, "{tw}x{th}");
        assert!((count.x + tw / 2.0 - 201.0).abs() < 0.01, "text centered horizontally");
        assert!((btn.x + btn.w / 2.0 - 201.0).abs() < 0.01, "button centered horizontally");
        let block_h = th + 10.0 + btn.h;
        assert!((count.y - (874.0 - block_h) / 2.0).abs() < 0.01, "block centered vertically");
        assert!((btn.y - (count.y + th + 10.0)).abs() < 0.01, "marginTop respected");
        // Padding 12 around the label; the label hugs its content.
        assert!((label.x - (btn.x + 12.0)).abs() < 0.01 && (label.y - (btn.y + 12.0)).abs() < 0.01);
        assert!((btn.w - (label.w + 24.0)).abs() < 0.01 && (btn.h - (label.h + 24.0)).abs() < 0.01);
        let b = scene.nodes.iter().find(|n| n.id == "5").unwrap();
        assert_eq!((b.radius, b.fill.map(|c| (c.r, c.g, c.b, c.a))), (8.0, Some((0.0, 0.5019608, 1.0, 1.0))));
        // Hit testing returns the deepest tag.
        let (cx, cy) = label.center();
        assert_eq!(scene.hit_test(cx, cy).map(|i| scene.nodes[i].id.as_str()), Some("6"));
        assert_eq!(scene.hit_test(1.0, 400.0).map(|i| scene.nodes[i].id.as_str()), Some("2"));
    }

    #[test]
    fn text_input_switch_image_and_modal() {
        let png = std::env::temp_dir().join(format!("simcore-rn-img-{}.png", std::process::id()));
        let mut pm = tiny_skia::Pixmap::new(4, 2).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pm.save_png(&png).unwrap();
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        let tree: ViewNode = serde_json::from_value(v("1", "RootView", json!({}), vec![
            v("2", "RCTSinglelineTextInputView", json!({"fontSize":17,"placeholder":"Name","__simcoreText":""}), vec![]),
            v("3", "RCTSwitch", json!({"value":true,"onTintColor":0xFFFF0000u64,"accessibilityLabel":"Notify"}), vec![]),
            v("4", "RCTImageView", json!({"width":100,"height":50,"resizeMode":"contain","borderRadius":10,
                "__simcoreImage":{"path":png.to_str().unwrap(),"width":4,"height":2}}), vec![]),
            v("5", "RCTModalHostView", json!({"visible":true}), vec![v("6", "RCTView", json!({"flex":1,"backgroundColor":0xFF00FF00u64}), vec![])]),
        ])).unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0), device: dev, scale: 3.0, appearance: Appearance::Light, dynamic_type: Default::default(),
            scroll: &scroll, focus: Some("2"), overscroll: false, status_bar: true, chrome: crate::ChromeParams { keyboard: true, shift: false }, sheets: &HashMap::new(),
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let n = |id: &str| scene.nodes.iter().position(|n| n.id == id).unwrap();
        let field = &scene.nodes[n("2")];
        assert_eq!((field.role, field.field, field.label.as_deref(), field.focused), (Role::TextField, Some(2), Some("Name"), true));
        assert_eq!(field.bounds.w, 402.0, "stretched, no intrinsic padding");
        assert!(field.bounds.h > 17.0 && field.bounds.h < 24.0, "line height, got {}", field.bounds.h);
        assert!(scene.nodes.iter().any(|c| c.id == "2#caret"), "focused field draws a caret");
        assert!(scene.nodes.iter().any(|c| c.kind == "keyboard"), "focus shows the software keyboard");
        let sw = &scene.nodes[n("3")];
        assert_eq!((sw.bounds.w, sw.bounds.h, sw.role, sw.label.as_deref(), sw.value.as_deref()), (63.0, 28.0, Role::Toggle, Some("Notify"), Some("on")));
        assert_eq!(sw.fill.map(|c| (c.r, c.g, c.b)), Some((1.0, 0.0, 0.0)));
        let img = &scene.nodes[n("4")];
        let dest = img.image.as_ref().expect("decoded").dest;
        assert_eq!((img.bounds.w, img.bounds.h, dest.w, dest.h), (100.0, 50.0, 100.0, 50.0));
        // The modal covers the viewport above the page and takes the hit.
        assert_eq!(scene.nodes[n("5")].bounds.w, 402.0);
        assert!(n("5") > n("4"));
        assert_eq!(scene.hit_test(200.0, 300.0).map(|i| scene.nodes[i].id.as_str()), Some("6"));
        let _ = std::fs::remove_file(png);
    }

    #[test]
    fn spinner_refresh_safe_area_gradient_blur() {
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        let tree: ViewNode = serde_json::from_value(v("1", "RootView", json!({}), vec![
            v("2", "SafeAreaView", json!({"flex":1}), vec![
                v("3", "ActivityIndicatorView", json!({"width":20,"height":20}), vec![]),
                v("4", "ActivityIndicatorView", json!({"width":36,"height":36,"animating":false}), vec![]),
                v("5", "ViewManagerAdapter_ExpoLinearGradient", json!({"height":50,"colors":[0xFFFF0000u64, 0xFF0000FFu64],"locations":[0.0, 0.3],"startPoint":[0.0, 0.0],"endPoint":[1.0, 1.0]}), vec![]),
                v("6", "ViewManagerAdapter_ExpoBlur_ExpoBlurView", json!({"height":50,"tint":"dark","intensity":100}), vec![]),
                v("7", "RCTScrollView", json!({"flex":1}), vec![
                    v("8", "PullToRefreshView", json!({"refreshing":true}), vec![]),
                    v("9", "RCTScrollContentView", json!({}), vec![v("10", "RCTView", json!({"height":44}), vec![])])])])])).unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0), device: dev, scale: 3.0, appearance: Appearance::Light, dynamic_type: Default::default(),
            scroll: &scroll, focus: None, overscroll: false, status_bar: true, chrome: crate::ChromeParams { keyboard: false, shift: false }, sheets: &HashMap::new(),
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let n = |id: &str| &scene.nodes[scene.nodes.iter().position(|n| n.id == id).unwrap()];
        // SafeAreaView at the screen top pads by the full insets.
        assert_eq!(n("3").bounds.y, 62.0);
        assert!(n("3").glyph.is_some() && n("4").glyph.is_none(), "stopped indicator hides");
        let g = n("5").gradient.as_ref().unwrap();
        assert_eq!((g.points, g.locations.clone()), (Some([0.0, 0.0, 1.0, 1.0]), Some(vec![0.0, 0.3])));
        assert!(n("6").glass.is_some_and(|g| g.flat.is_some()));
        // Refreshing: the content opens a band; the spinner is centred in it.
        assert!((n("10").bounds.y - n("7").bounds.y - REFRESH_H).abs() < 1e-3);
        assert!(n("8").glyph.is_some());
        assert!((n("8").bounds.y + REFRESH_SPINNER / 2.0 - n("7").bounds.y - REFRESH_H / 2.0).abs() < 1e-3);
        assert_eq!(n("7").bounds.y + n("7").bounds.h, 874.0 - 34.0, "bottom inset");
    }

    #[test]
    fn native_stack_and_tabs() {
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        let screen = |id: &str, title: &str, body: Value| {
            v(id, "RNSScreen", json!({}), vec![
                v(&format!("{id}c"), "RNSScreenContentWrapper", json!({"flex":1}), vec![body]),
                v(&format!("{id}h"), "RNSScreenStackHeaderConfig", json!({"title":title,"largeTitle":false}), vec![])])
        };
        let tab = |id: &str, key: &str, title: &str, body: Value| v(id, "RNSTabsScreenIOS", json!({"screenKey":key,"title":title}), vec![body]);
        let tree: ViewNode = serde_json::from_value(v("1", "RootView", json!({}), vec![
            v("2", "RNSScreenStack", json!({"flex":1}), vec![
                screen("10", "Fruits", v("11", "RCTView", json!({"flex":1}), vec![])),
                screen("20", "Apple", v("21", "RCTView", json!({"flex":1}), vec![]))]),
            v("3", "RNSTabsHostIOS", json!({"navStateRequest":{"selectedScreenKey":"b"}}), vec![
                tab("30", "a", "Home", v("31", "RCTView", json!({"flex":1}), vec![])),
                tab("40", "b", "Explore", v("41", "RCTView", json!({"flex":1}), vec![]))]),
        ])).unwrap();
        let dev = device(DEFAULT_DEVICE).unwrap();
        let scroll = HashMap::new();
        let params = SceneParams {
            viewport: (402.0, 874.0), device: dev, scale: 3.0, appearance: Appearance::Light, dynamic_type: Default::default(),
            scroll: &scroll, focus: None, overscroll: false, status_bar: true, chrome: Default::default(), sheets: &HashMap::new(),
        };
        let scene = build_scene(&tree, &params, &mut Fonts::new());
        assert!(scene.notes.is_empty(), "{:?}", scene.notes);
        let has = |id: &str| scene.nodes.iter().any(|n| n.id == id);
        // Only the top screen and the selected tab are drawn.
        assert!(has("21") && !has("11") && has("41") && !has("31"));
        let back = scene.nodes.iter().find(|n| n.id == "2#back").expect("back pill");
        assert_eq!((back.role, back.label.as_deref()), (Role::Button, Some("Fruits")));
        let tabs: Vec<_> = scene.nodes.iter().filter(|n| n.id.starts_with("3#tab-") && n.role == Role::Button).collect();
        assert_eq!(tabs.iter().map(|n| n.label.as_deref().unwrap()).collect::<Vec<_>>(), ["Home", "Explore"]);
        assert!(tabs[1].fill.is_some() && tabs[0].fill.is_none(), "selected tab has the capsule");
        assert!(tabs.iter().all(|n| n.action == Some(3)), "items are hit-testable (the host maps them to SelectTab)");
    }
}
