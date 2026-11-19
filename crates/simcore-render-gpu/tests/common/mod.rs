//! Shared helpers: real showcase scenes and the pixel-diff metric.
#![allow(dead_code)]

use simcore_protocol::{Appearance, ViewNode};
use simcore_render::{build_scene, device, Device, Fonts, Scene, SceneParams, DEFAULT_DEVICE};
use std::collections::HashMap;

const TREES: &[(&str, &str)] = &[
    ("home", include_str!("../../../simcore-render/tests/data/home.json")),
    ("typography", include_str!("../../../simcore-render/tests/data/typography.json")),
    ("shapes", include_str!("../../../simcore-render/tests/data/shapes.json")),
    ("todos", include_str!("../../../simcore-render/tests/data/todos.json")),
    ("todos-toggled", include_str!("../../../simcore-render/tests/data/todos-toggled.json")),
    ("controls", include_str!("../../../simcore-render/tests/data/controls.json")),
];

/// The showcase trees, plus each wrapped in a `TabView` (Liquid Glass tab bar over the content).
pub fn trees() -> Vec<(String, serde_json::Value)> {
    let mut out = vec![];
    for (name, json) in TREES {
        let v: serde_json::Value = serde_json::from_str(json).expect("tree");
        out.push((name.to_string(), v.clone()));
        let tabs = serde_json::json!([
            {"title": "Home", "systemImage": "house", "action": 1},
            {"title": "Counter", "action": 2},
            {"title": "Settings", "systemImage": "gearshape", "badge": "3", "action": 3}
        ]);
        out.push((format!("{name}+tabbar"), serde_json::json!({"kind": "tabview", "id": "tabs", "selected": 0, "tabs": tabs, "content": v})));
    }
    out
}

/// 402x874 scene (the showcase device) at the given appearance.
pub fn build(tree: &serde_json::Value, dark: bool, fonts: &mut Fonts) -> Scene {
    let root: ViewNode = serde_json::from_value(tree.clone()).expect("tree parses");
    let dev = Device { island: None, safe_top: 62.0, safe_bottom: 34.0, ..*device(DEFAULT_DEVICE).unwrap() };
    let scroll = HashMap::new();
    let p = SceneParams {
        viewport: (402.0, 874.0),
        device: &dev,
        scale: 3.0,
        appearance: if dark { Appearance::Dark } else { Appearance::Light },
        dynamic_type: Default::default(),
        scroll: &scroll,
        sheets: &Default::default(),
        focus: None,
        overscroll: false,
        status_bar: true,
        chrome: Default::default(),
    };
    build_scene(&root, &p, fonts)
}

/// Percentage of pixels where any channel differs by more than `tol`.
pub fn diff_pct(a: &[u8], b: &[u8], tol: u8) -> f64 {
    assert_eq!(a.len(), b.len(), "frame sizes differ");
    let (pa, pb) = (a.as_chunks::<4>().0, b.as_chunks::<4>().0);
    let n = pa.iter().zip(pb).filter(|(x, y)| x.iter().zip(*y).any(|(p, q)| p.abs_diff(*q) > tol)).count();
    100.0 * n as f64 / (a.len() / 4) as f64
}
