//! Renders hand-written showcase trees (tests/data/*.json) to PNGs.
//!
//!   cargo run -p simcore-render --example showcase [-- NAME... --out target/showcase-render --focus ID]
//! Writes `<name>.png` and `<name>-dark.png` at 402x874 @3x (same as tests/fixtures/showcase-ios).

use simcore_protocol::{Appearance, ViewNode};
use simcore_render::{build_scene, render_png, Fonts, SceneParams};
use std::collections::HashMap;

const TREES: &[(&str, &str)] = &[
    ("home", include_str!("../tests/data/home.json")),
    ("typography", include_str!("../tests/data/typography.json")),
    ("shapes", include_str!("../tests/data/shapes.json")),
    ("todos", include_str!("../tests/data/todos.json")),
    ("todos-toggled", include_str!("../tests/data/todos-toggled.json")),
    ("controls", include_str!("../tests/data/controls.json")),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let out = value("--out").unwrap_or_else(|| "target/showcase-render".into());
    let focus = value("--focus");
    let names: Vec<&String> = args.iter().filter(|a| !a.starts_with("--") && Some(*a) != value("--out").as_ref() && Some(*a) != focus.as_ref()).collect();
    std::fs::create_dir_all(&out).expect("out dir");
    let mut fonts = Fonts::new();
    println!("SF Pro found: {}", fonts.has_sf());
    let scroll = HashMap::new();
    for (name, json) in TREES {
        if !names.is_empty() && !names.iter().any(|n| n.as_str() == *name) {
            continue;
        }
        let root: ViewNode = serde_json::from_str(json).expect("tree");
        for dark in [false, true] {
            let a = if dark { Appearance::Dark } else { Appearance::Light };
            let p = SceneParams {
                viewport: (402.0, 874.0),
                safe_top: 62.0,
                safe_bottom: 34.0,
                scale: 3.0,
                appearance: a,
                scroll: &scroll,
                focus: focus.as_deref(),
                status_bar: true,
                chrome: Default::default(),
            };
            let scene = build_scene(&root, &p, &mut fonts);
            let png = render_png(&scene, &mut fonts, 3.0).expect("png");
            let path = format!("{out}/{name}{}.png", if dark { "-dark" } else { "" });
            std::fs::write(&path, png).expect("write");
            println!("wrote {path}");
        }
    }
}
