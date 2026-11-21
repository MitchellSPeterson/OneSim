use crate::scene::Color;
use serde_json::Value;
use taffy::prelude::*;

pub(super) struct EdgeKeys {
    pub all: &'static str,
    pub horizontal: &'static str,
    pub vertical: &'static str,
    pub top: &'static str,
    pub right: &'static str,
    pub end: &'static str,
    pub bottom: &'static str,
    pub left: &'static str,
    pub start: &'static str,
}

pub(super) const MARGIN_KEYS: EdgeKeys = EdgeKeys {
    all: "margin",
    horizontal: "marginHorizontal",
    vertical: "marginVertical",
    top: "marginTop",
    right: "marginRight",
    end: "marginEnd",
    bottom: "marginBottom",
    left: "marginLeft",
    start: "marginStart",
};

pub(super) const PADDING_KEYS: EdgeKeys = EdgeKeys {
    all: "padding",
    horizontal: "paddingHorizontal",
    vertical: "paddingVertical",
    top: "paddingTop",
    right: "paddingRight",
    end: "paddingEnd",
    bottom: "paddingBottom",
    left: "paddingLeft",
    start: "paddingStart",
};

pub(super) const BORDER_WIDTH_KEYS: EdgeKeys = EdgeKeys {
    all: "borderWidth",
    horizontal: "borderHorizontalWidth",
    vertical: "borderVerticalWidth",
    top: "borderTopWidth",
    right: "borderRightWidth",
    end: "borderEndWidth",
    bottom: "borderBottomWidth",
    left: "borderLeftWidth",
    start: "borderStartWidth",
};

pub(super) const BORDER_COLOR_KEYS: [&str; 4] = [
    "borderTopColor",
    "borderRightColor",
    "borderBottomColor",
    "borderLeftColor",
];

const CORNER_RADIUS_KEYS: [&str; 4] = [
    "borderTopLeftRadius",
    "borderTopRightRadius",
    "borderBottomLeftRadius",
    "borderBottomRightRadius",
];

pub(super) fn css_color(s: &str) -> Option<Color> {
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

pub(super) fn num(p: &Value, k: &str) -> Option<f64> {
    p.get(k).and_then(Value::as_f64)
}

/// Yoga edge precedence: edge > start/end > horizontal/vertical > all. Order: top, right, bottom, left.
fn edges<'a>(p: &'a Value, k: &EdgeKeys) -> [Option<&'a Value>; 4] {
    let g = |key: &str| p.get(key).filter(|v| !v.is_null());
    let all = g(k.all);
    let (h, v) = (g(k.horizontal).or(all), g(k.vertical).or(all));
    [g(k.top).or(v), g(k.right).or(g(k.end)).or(h), g(k.bottom).or(v), g(k.left).or(g(k.start)).or(h)]
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

pub(super) fn style(p: &Value) -> Style {
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
        margin: rect(edges(p, &MARGIN_KEYS), V::lpa),
        padding: rect(edges(p, &PADDING_KEYS), V::lp),
        border: rect(edges(p, &BORDER_WIDTH_KEYS), V::lp),
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

pub(super) fn weight(p: &Value) -> u16 {
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

pub(super) fn corner_radius(p: &Value) -> f64 {
    num(p, "borderRadius").unwrap_or_else(|| {
        // ponytail: per-corner radii collapse to the largest, per-corner paths need a custom clip/fill.
        CORNER_RADIUS_KEYS
            .iter()
            .filter_map(|k| num(p, k))
            .fold(0.0, f64::max)
    })
}
