//! GPU renderer against the CPU reference. Every test skips (prints and returns) without a GPU adapter.
mod common;

use common::{build, diff_pct, trees};
use simcore_protocol::{Appearance, ImageScale};
use simcore_render::glass::{GlassSpec, GlassVariant, ScrollEdge};
use simcore_render::{
    Color, FontSpec, Fonts, Glyph, Gradient, ImageRun, Node, Rect, Renderer, Scene, Shadow, SymbolRun, SymbolSpec, TextAlign, TextRun,
};
use simcore_render_gpu::GpuRenderer;
use std::sync::Arc;

fn gpu() -> Option<GpuRenderer> {
    match GpuRenderer::new_headless() {
        Ok(g) => Some(g),
        Err(e) => {
            println!("SKIP: no GPU adapter ({e})");
            None
        }
    }
}

fn node(id: &str, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(id, "rect");
    n.bounds = Rect { x, y, w, h };
    n
}

fn scene(dark: bool, nodes: Vec<Node>) -> Scene {
    let mut bg = node("bg", 0.0, 0.0, 120.0, 90.0);
    bg.fill = Some(if dark { Color::rgba(0.1, 0.1, 0.12, 1.0) } else { Color::rgba(1.0, 1.0, 1.0, 1.0) });
    let mut all = vec![bg];
    all.extend(nodes);
    Scene { size: (120.0, 90.0), appearance: if dark { Appearance::Dark } else { Appearance::Light }, nodes: all, unsupported_symbols: vec![], notes: vec![] }
}

/// Stripes so the glass has a backdrop to bend and blur.
fn stripes() -> Vec<Node> {
    (0..8)
        .map(|i| {
            let mut n = node(&format!("s{i}"), i as f64 * 15.0, 0.0, 15.0, 90.0);
            n.fill = Some(if i % 2 == 0 { Color::rgba(0.9, 0.2, 0.2, 1.0) } else { Color::rgba(0.1, 0.4, 0.9, 1.0) });
            n
        })
        .collect()
}

fn compare(g: &mut GpuRenderer, name: &str, sc: &Scene, max_pct: f64) -> f64 {
    let mut fonts = Fonts::new();
    let cpu = simcore_render::render(sc, &mut fonts, 3.0);
    let f = g.capture(sc, &mut fonts, 3.0).expect("gpu capture");
    assert_eq!((f.width, f.height), (cpu.width(), cpu.height()));
    let d = diff_pct(&f.rgba, cpu.data(), 8);
    if let Ok(dir) = std::env::var("SIMCORE_GPU_DUMP") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}/{name}-cpu.png"), cpu.encode_png().unwrap()).unwrap();
        std::fs::write(format!("{dir}/{name}-gpu.png"), f.encode_png().unwrap()).unwrap();
    }
    println!("{name:<14} {d:6.3}% pixels differ by >8 (max allowed {max_pct}%)  backdrops gpu/cpu {:?}", g.backdrop_counts());
    assert!(d <= max_pct, "{name}: {d}% > {max_pct}%");
    d
}

fn text(s: &str, strike: bool) -> TextRun {
    TextRun { text: s.into(), font: FontSpec::new(17.0, 20.33, 400, TextAlign::Leading), color: Color::rgba(0.0, 0.0, 0.0, 1.0), strike, offset_x: 0.0, spans: vec![] }
}

#[test]
fn unit_scenes_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let blue = Color::rgba(0.0, 0.48, 1.0, 1.0);
    let mut cases: Vec<(&str, Scene, f64)> = vec![];

    let mut n = node("r", 10.3, 10.7, 50.4, 30.2);
    n.fill = Some(blue);
    cases.push(("rect", scene(false, vec![n]), 0.5));

    let mut n = node("rr", 10.3, 10.7, 70.0, 40.0);
    n.fill = Some(blue);
    n.radius = 12.0;
    cases.push(("rrect", scene(false, vec![n]), 1.0));

    let mut n = node("st", 10.3, 10.7, 70.0, 40.0);
    n.stroke = Some((Color::rgba(1.0, 0.2, 0.2, 1.0), 3.0));
    n.radius = 10.0;
    cases.push(("stroke", scene(false, vec![n]), 2.0));

    let mut n = node("gr", 10.0, 10.0, 90.0, 50.0);
    n.radius = 12.0;
    n.gradient = Some(Gradient { angle: 45.0, stops: vec![Color::rgba(1.0, 0.8, 0.0, 1.0), Color::rgba(1.0, 0.0, 0.5, 1.0)], points: None, locations: None });
    let mut m = node("gr2", 10.0, 62.0, 90.0, 20.0);
    m.gradient = Some(Gradient { angle: 0.0, stops: vec![Color::rgba(0.0, 0.0, 0.0, 1.0), Color::rgba(0.0, 1.0, 0.0, 0.5), Color::rgba(1.0, 1.0, 1.0, 1.0)], points: Some([0.0, 0.0, 1.0, 0.0]), locations: Some(vec![0.0, 0.3, 1.0]) });
    cases.push(("gradient", scene(false, vec![n, m]), 1.5));

    let mut p = node("clip", 10.0, 10.0, 60.0, 60.0);
    p.fill = Some(Color::rgba(0.9, 0.9, 0.9, 1.0));
    p.clip_radius = Some(20.0);
    let mut c = node("child", 0.0, 30.0, 100.0, 80.0);
    c.parent = Some(1);
    c.fill = Some(Color::rgba(0.2, 0.8, 0.3, 1.0));
    let mut c2 = node("child2", 20.0, 12.0, 40.0, 20.0);
    c2.parent = Some(1);
    c2.fill = Some(Color::rgba(0.9, 0.3, 0.1, 1.0));
    c2.radius = 6.0;
    cases.push(("clip", scene(false, vec![p, c, c2]), 1.0));

    let mut n = node("op", 10.0, 10.0, 60.0, 40.0);
    n.fill = Some(Color::rgba(0.0, 0.0, 0.0, 1.0));
    n.radius = 8.0;
    n.opacity = 0.5;
    n.stroke = Some((Color::rgba(1.0, 0.0, 0.0, 1.0), 2.0));
    cases.push(("opacity", scene(false, vec![n]), 1.0));

    let mut px = tiny_skia::Pixmap::new(16, 16).unwrap();
    for (i, p) in px.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((i % 16) as u8, (i / 16) as u8);
        *p = tiny_skia::ColorU8::from_rgba(x * 16, y * 16, 128, if (x + y) % 5 == 0 { 100 } else { 255 }).premultiply();
    }
    let mut n = node("img", 10.0, 10.0, 70.0, 60.0);
    n.radius = 10.0;
    n.opacity = 0.9;
    n.image = Some(ImageRun { pixmap: Arc::new(px), dest: Rect { x: 5.0, y: 15.0, w: 80.0, h: 50.0 }, rotation: 0.0 });
    let mut rotated = n.clone();
    rotated.image.as_mut().unwrap().rotation = 35.0;
    cases.push(("image", scene(false, vec![n]), 1.0));
    cases.push(("rotated-image", scene(false, vec![rotated]), 1.0));

    let mut n = node("sh", 25.0, 25.0, 60.0, 35.0);
    n.fill = Some(Color::rgba(1.0, 1.0, 1.0, 1.0));
    n.radius = 12.0;
    n.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.4), radius: 6.0, dx: 2.0, dy: 4.0 });
    cases.push(("shadow", scene(false, vec![n]), 2.0));

    let mut n = node("tx", 10.4, 10.3, 100.0, 24.0);
    n.text = Some(text("Hello, GPU 123", false));
    let mut m = node("tx2", 10.0, 40.0, 100.0, 24.0);
    m.text = Some(text("Struck text", true));
    m.opacity = 0.7;
    cases.push(("text", scene(false, vec![n, m]), 0.05));

    let mut n = node("sym", 10.0, 10.0, 30.0, 30.0);
    n.symbol = Some(SymbolRun { spec: SymbolSpec { name: "star.fill".into(), point_size: 20.0, weight: 400, scale: ImageScale::Medium }, color: Color::rgba(1.0, 0.6, 0.0, 1.0), size: (20.0, 20.0) });
    let mut m = node("gl", 50.0, 10.0, 8.0, 14.0);
    m.glyph = Some((Glyph::Chevron, Color::rgba(0.3, 0.3, 0.3, 1.0)));
    let mut k = node("bat", 70.0, 10.0, 27.0, 13.0);
    k.glyph = Some((Glyph::Battery, Color::rgba(0.0, 0.0, 0.0, 1.0)));
    cases.push(("symbol+glyph", scene(false, vec![n, m, k]), 0.05));

    for dark in [false, true] {
        let mut nodes = stripes();
        let mut n = node("glass", 15.0, 25.0, 90.0, 36.0);
        n.radius = 18.0;
        n.glass = Some(GlassSpec { variant: GlassVariant::Regular, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None });
        n.fill = Some(Color::rgba(1.0, 1.0, 1.0, 0.05));
        let mut t = node("gt", 30.0, 33.0, 60.0, 20.0);
        t.parent = Some(9);
        t.text = Some(text("Glass", false));
        nodes.push(n);
        nodes.push(t);
        cases.push((if dark { "glass-dark" } else { "glass" }, scene(dark, nodes), 1.0));
    }

    for (name, sc, max) in &cases {
        compare(&mut g, name, sc, *max);
    }
}

#[test]
fn showcase_scenes_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    let mut worst = 0.0f64;
    for (name, tree) in trees() {
        for dark in [false, true] {
            let sc = build(&tree, dark, &mut fonts);
            let glass = sc.nodes.iter().filter(|n| simcore_render::needs_backdrop(n)).count();
            let cpu = simcore_render::render(&sc, &mut fonts, 3.0);
            let f = g.capture(&sc, &mut fonts, 3.0).expect("capture");
            assert_eq!((f.width, f.height), (cpu.width(), cpu.height()));
            let d = diff_pct(&f.rgba, cpu.data(), 8);
            println!("{name:<24}{} backdrop nodes {glass:2}  {d:6.3}% differ >8, gpu/cpu {:?}", if dark { " dark" } else { "     " }, g.backdrop_counts());
            worst = worst.max(d);
            assert!(d < 0.5, "{name} dark={dark}: {d}%");
        }
    }
    println!("worst: {worst:.3}%");
}

#[test]
fn resize_between_frames() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    let mut a = node("a", 5.0, 5.0, 40.0, 20.0);
    a.fill = Some(Color::rgba(0.0, 0.5, 1.0, 1.0));
    let mut s1 = scene(false, vec![a.clone()]);
    s1.size = (80.0, 60.0);
    let mut s2 = scene(false, vec![a]);
    s2.size = (101.0, 77.0);
    for (sc, scale, dims) in [(&s1, 2.0, (160, 120)), (&s2, 3.0, (303, 231)), (&s1, 1.0, (80, 60)), (&s2, 3.0, (303, 231))] {
        let f = g.capture(sc, &mut fonts, scale).expect("capture");
        assert_eq!((f.width, f.height), dims);
        assert_eq!(f.rgba.len(), (dims.0 * dims.1 * 4) as usize);
        let cpu = simcore_render::render(sc, &mut fonts, scale);
        let d = diff_pct(&f.rgba, cpu.data(), 8);
        assert!(d < 0.5, "{dims:?}: {d}%");
    }
    // Degenerate sizes follow the convention: at least 1 px.
    let mut tiny = scene(false, vec![]);
    tiny.size = (0.1, 0.1);
    let f = g.capture(&tiny, &mut fonts, 1.0).expect("capture");
    assert_eq!((f.width, f.height), (1, 1));
}

#[test]
fn caches_stay_bounded_over_200_frames() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    let mut peak = 0;
    for i in 0..200 {
        // Every frame has text no earlier frame had: without eviction the cache would hold 200 sprites.
        let mut t = node("t", 5.0 + (i as f64) * 0.37, 10.0, 100.0, 24.0);
        t.text = Some(text(&format!("frame {i}"), false));
        let sc = scene(false, vec![t]);
        g.capture(&sc, &mut fonts, 2.0).expect("capture");
        peak = peak.max(g.cache_len());
    }
    println!("peak cache entries over 200 frames: {peak}");
    assert!(peak <= 125, "cache grew to {peak}");
    assert_eq!(g.diagnostics().frames, 200);
}

#[test]
fn diagnostics_report_adapter() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    g.capture(&scene(false, vec![]), &mut fonts, 1.0).unwrap();
    let d = g.diagnostics();
    println!("{d:#?}");
    assert_eq!(d.backend, "gpu");
    assert!(d.adapter.contains('(') && !d.limitations.is_empty() && d.frames == 1);
}

#[test]
fn repeated_frames_are_identical() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    let mut t = node("t", 5.0, 10.0, 100.0, 24.0);
    t.text = Some(text("cached text", false));
    let sc = scene(false, vec![t]);
    let first = g.capture(&sc, &mut fonts, 3.0).unwrap();
    for i in 0..3 {
        let again = g.capture(&sc, &mut fonts, 3.0).unwrap();
        assert_eq!(diff_pct(&first.rgba, &again.rgba, 0), 0.0, "frame {i} differs from the first (sprite cache hit)");
    }
}

fn glass_scene(dark: bool) -> Scene {
    let mut nodes = stripes();
    let mut n = node("glass", 15.0, 25.0, 90.0, 36.0);
    n.radius = 18.0;
    n.glass = Some(GlassSpec { variant: GlassVariant::Regular, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None });
    let mut t = node("gt", 30.0, 33.0, 60.0, 20.0);
    t.parent = Some(9);
    t.text = Some(text("Glass", false));
    nodes.push(n);
    nodes.push(t);
    scene(dark, nodes)
}

/// Cached sprites must survive frames (and passes) that draw no image at all, and frames split by glass.
#[test]
fn sprite_cache_survives_imageless_frames() {
    let Some(mut g) = gpu() else { return };
    let mut fonts = Fonts::new();
    let mut plain = node("p", 10.0, 10.0, 50.0, 30.0);
    plain.fill = Some(Color::rgba(0.2, 0.4, 0.9, 1.0));
    let imageless = scene(false, vec![plain]);
    let mut t = node("gt", 30.0, 33.0, 60.0, 20.0);
    t.parent = None;
    t.text = Some(text("Glass", false));
    let mut with_text = scene(false, vec![t]);
    with_text.nodes.truncate(2);
    for (i, sc) in [glass_scene(true), glass_scene(true), imageless.clone(), glass_scene(false), with_text.clone(), imageless, with_text, glass_scene(true)].iter().enumerate() {
        let cpu = simcore_render::render(sc, &mut fonts, 3.0);
        let f = g.capture(sc, &mut fonts, 3.0).unwrap();
        let d = diff_pct(&f.rgba, cpu.data(), 8);
        println!("seq {i}: {d:.3}%");
        assert!(d < 0.1, "frame {i}: {d}%");
    }
}

fn spec(variant: GlassVariant) -> GlassSpec {
    GlassSpec { variant, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None }
}

fn glass_node(id: &str, x: f64, y: f64, w: f64, h: f64, r: f64, gs: GlassSpec) -> Node {
    let mut n = node(id, x, y, w, h);
    n.radius = r;
    n.glass = Some(gs);
    n
}

/// Backdrop variants against the CPU, with the counters confirming GPU execution.
#[test]
fn glass_variants_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let red = Color::rgba(0.9, 0.1, 0.2, 0.5);
    let mut cases: Vec<(String, Scene, f64)> = vec![];
    let with = |dark: bool, extra: Vec<Node>| {
        let mut nodes = stripes();
        nodes.extend(extra);
        scene(dark, nodes)
    };
    for dark in [false, true] {
        let d = if dark { "-dark" } else { "" };
        let flat = GlassSpec { flat: Some(1.8), blur: Some(6.0), mix: Some((0.9, 0.05)), ..spec(GlassVariant::Regular) };
        cases.push((format!("flat{d}"), with(dark, vec![glass_node("f", 20.0, 20.0, 80.0, 40.0, 14.0, flat)]), 1.0));
        cases.push((format!("clear{d}"), with(dark, vec![glass_node("c", 15.0, 30.0, 90.0, 30.0, 15.0, spec(GlassVariant::Clear))]), 1.0));
        let tinted = GlassSpec { tint: Some(red), interactive: true, ..spec(GlassVariant::Regular) };
        cases.push((format!("tinted-interactive{d}"), with(dark, vec![glass_node("t", 20.0, 25.0, 80.0, 36.0, 18.0, tinted)]), 1.0));
        let over = GlassSpec { blur: Some(8.0), mix: Some((0.8, 0.2)), shadow: Some((0.25, 10.0, 4.0)), ..spec(GlassVariant::Regular) };
        let mut n = glass_node("o", 18.0, 20.0, 84.0, 44.0, 22.0, over);
        n.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.3, 0.4), radius: 5.0, dx: 1.0, dy: 3.0 });
        n.opacity = 0.8;
        cases.push((format!("overrides+own-shadow{d}"), with(dark, vec![n]), 1.0));
        // Three overlapping glass nodes: each sees the previous ones.
        let stack = vec![
            glass_node("a", 10.0, 12.0, 70.0, 40.0, 16.0, spec(GlassVariant::Regular)),
            glass_node("b", 35.0, 30.0, 70.0, 40.0, 20.0, spec(GlassVariant::Clear)),
            glass_node("c", 20.0, 48.0, 80.0, 30.0, 15.0, GlassSpec { tint: Some(red), ..spec(GlassVariant::Regular) }),
        ];
        cases.push((format!("stacked{d}"), with(dark, stack), 1.0));
        // Glass inside a rounded scroll container, partly outside it (clip + opacity).
        let mut clip = node("clip", 12.0, 10.0, 96.0, 64.0);
        clip.clip_radius = Some(18.0);
        let mut gl = glass_node("cg", 0.0, 20.0, 110.0, 34.0, 17.0, spec(GlassVariant::Regular));
        gl.parent = Some(9);
        gl.opacity = 0.9;
        cases.push((format!("clipped{d}"), with(dark, vec![clip, gl]), 1.0));
        // Scroll edge, plain and inside a clip.
        let mut e = node("edge", 0.0, 0.0, 120.0, 90.0);
        e.edge = Some(ScrollEdge { bar: 30.0, color: if dark { Color::rgba(0.1, 0.1, 0.12, 1.0) } else { Color::rgba(1.0, 1.0, 1.0, 1.0) }, tint: if dark { 0.0 } else { 3.0 / 255.0 } });
        cases.push((format!("edge{d}"), with(dark, vec![e.clone()]), 1.0));
        let mut clip = node("clip", 0.0, 0.0, 100.0, 80.0);
        clip.clip_radius = Some(12.0);
        e.parent = Some(9);
        cases.push((format!("edge-clipped{d}"), with(dark, vec![clip, e.clone()]), 1.0));
        // Edge combined with glass on one node: separate passes in the CPU's order, all on the GPU.
        let mut both = glass_node("both", 20.0, 20.0, 80.0, 30.0, 15.0, spec(GlassVariant::Regular));
        both.edge = e.edge;
        cases.push((format!("glass+edge{d}"), with(dark, vec![both]), 1.0));
        // The last clip must cut into the material, so ignoring the part past four clips fails parity.
        let mut nested = vec![];
        for k in 0..5 {
            let mut c = node("clip", 4.0 + 5.0 * k as f64, 4.0 + k as f64, 110.0 - 10.0 * k as f64, 80.0 - 2.0 * k as f64);
            c.clip_radius = Some(20.0 - k as f64);
            c.parent = (k > 0).then_some(9 + k - 1);
            nested.push(c);
        }
        let mut gl = glass_node("deep", 10.0, 20.0, 90.0, 34.0, 17.0, spec(GlassVariant::Regular));
        gl.parent = Some(13);
        nested.push(gl);
        cases.push((format!("five-clips{d}"), with(dark, nested), 1.0));

        let material = GlassSpec { shadow: Some((0.8, 4.0, 9.0)), ..spec(GlassVariant::Regular) };
        let mut halo = glass_node("halo", 20.0, 14.0, 80.0, 54.0, 2.0, material);
        halo.clip_radius = Some(27.0);
        cases.push((format!("halo-clip-radius{d}"), with(dark, vec![halo]), 1.0));
    }
    for (name, sc, max) in &cases {
        let before = g.backdrop_counts();
        compare(&mut g, name, sc, *max);
        let after = g.backdrop_counts();
        let (gpu_nodes, cpu_nodes) = (after.0 - before.0, after.1 - before.1);
        assert!(gpu_nodes > 0 && cpu_nodes == 0, "{name}: expected the GPU path, got gpu {gpu_nodes} cpu {cpu_nodes}");
    }
}

/// The CPU fallback (forced) still composes frames split at backdrop nodes correctly.
#[test]
fn forced_cpu_backdrop_path_matches() {
    let Some(mut g) = gpu() else { return };
    g.disable_gpu_glass("test");
    let mut nodes = stripes();
    nodes.push(glass_node("g", 15.0, 25.0, 90.0, 36.0, 18.0, spec(GlassVariant::Regular)));
    let sc = scene(false, nodes);
    compare(&mut g, "forced-cpu", &sc, 1.0);
    assert_eq!(g.backdrop_counts(), (0, 1));
    assert!(g.diagnostics().limitations[0].contains("CPU composite"));
}
