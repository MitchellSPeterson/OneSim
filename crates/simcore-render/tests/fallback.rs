//! Forces the open-licensed fallback (SIMCORE_FALLBACK=1) and checks Inter + Phosphor are used.
//! One test per process so the env var is set before any host code reads it.
use simcore_protocol::{Appearance, ViewNode};
use simcore_render::{build_scene, device, Device, DEFAULT_DEVICE, fonts_capability, render, symbols_capability, FontSpec, Fonts, SceneParams, TextAlign};
use std::collections::HashMap;

#[test]
fn forced_fallback_uses_inter_and_phosphor() {
    std::env::set_var("SIMCORE_FALLBACK", "1");
    assert_eq!((fonts_capability(), symbols_capability()), ("bundled", "bundled"));

    // Inter is used and the wght axis applies: regular < semibold < bold widths.
    let mut f = Fonts::new();
    assert!(f.has_inter() && !f.has_sf());
    let w = |f: &mut Fonts, wt| f.measure("Hamburgefonstiv", &FontSpec::new(17.0, 20.33, wt, TextAlign::Leading), None, &[]).0;
    let (r, sb, b) = (w(&mut f, 400), w(&mut f, 600), w(&mut f, 700));
    assert!(r < sb && sb < b, "{r} {sb} {b}");

    // A mapped symbol renders non-empty; an unmapped one is reported and drawn as a placeholder.
    let v: ViewNode = serde_json::from_str(
        r#"{"kind":"vstack","id":"v","children":[
            {"kind":"image","id":"ok","systemName":"star.fill","resizable":false},
            {"kind":"image","id":"bold","systemName":"checkmark.circle","resizable":false,"modifiers":[{"fontWeight":"bold"}]},
            {"kind":"image","id":"bad","systemName":"no.such.symbol.xyz","resizable":false}]}"#,
    )
    .unwrap();
    let scroll = HashMap::new();
    let dev = Device { island: None, safe_top: 0.0, safe_bottom: 0.0, ..*device(DEFAULT_DEVICE).unwrap() };
    let p = SceneParams {
        viewport: (100.0, 200.0),
        device: &dev,
        scale: 1.0,
        appearance: Appearance::Light,
        dynamic_type: Default::default(),
        ios: Default::default(),
        scroll: &scroll,
        sheets: &Default::default(),
        focus: None,
        overscroll: false,
        status_bar: false,
        chrome: Default::default(),
    };
    let scene = build_scene(&v, &p, &mut f);
    assert_eq!(scene.unsupported_symbols, ["no.such.symbol.xyz"]);
    let node = |id: &str| scene.nodes.iter().find(|n| n.id == id).unwrap();
    assert!(node("ok").symbol.is_some() && node("bold").symbol.is_some() && node("bad").symbol.is_none());
    let pm = render(&scene, &mut f, 1.0);
    let b = node("ok").bounds;
    let dark = (b.y as u32..(b.y + b.h) as u32).flat_map(|y| (b.x as u32..(b.x + b.w) as u32).map(move |x| (x, y))).filter(|&(x, y)| pm.pixel(x, y).is_some_and(|c| c.alpha() > 0 && c.red() < 200)).count();
    assert!(dark > 100, "star.fill drew {dark} dark pixels");
}
