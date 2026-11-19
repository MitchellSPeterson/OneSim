//! Headless scene preparation and capture workloads for the v0.3 benchmark report.
//! cargo run --release -p simcore-render-gpu --example frame_bench -- cpu|gpu [frames] [device] [scenario]

#[path = "../tests/common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use simcore_protocol::{Appearance, ViewNode};
use simcore_render::glass::{GlassSpec, GlassVariant};
use simcore_render::{build_scene, device, Color, CpuRenderer, Device, Fonts, Gradient, ImageRun, Node, Rect, Renderer, Scene, SceneParams};
use simcore_render_gpu::GpuRenderer;

const DEVICES: [&str; 2] = ["iphone-se-3", "iphone-17-pro"];
const SCENARIOS: [&str; 7] = ["long-scroll", "nav-push-pop", "form-keyboard", "glass-stack", "image-grid", "animation-60", "animation-120"];

fn layout(root: &ViewNode, dev: &Device, fonts: &mut Fonts, scroll: &HashMap<String, (f64, f64)>, focus: Option<&str>) -> Scene {
    build_scene(
        root,
        &SceneParams {
            viewport: (dev.viewport.width, dev.viewport.height),
            device: dev,
            scale: dev.scale,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            scroll,
            sheets: &Default::default(),
            focus,
            overscroll: false,
            status_bar: true,
            chrome: simcore_render::ChromeParams { keyboard: focus.is_some(), shift: false },
        },
        fonts,
    )
}

fn list_tree(form: bool, text: &str) -> ViewNode {
    let count = if form { 20 } else { 250 };
    let rows: Vec<_> = (0..count)
        .map(|i| {
            if form {
                json!({"kind":"text_field", "id":format!("field{i}"), "field":i+1, "placeholder":format!("Field {}", i+1), "text":if i == 2 {text} else {"Example value"}})
            } else {
                json!({"kind":"text", "id":format!("row{i}"), "text":format!("Setting {:03}   •   Details and preferences", i+1)})
            }
        })
        .collect();
    serde_json::from_value(json!({
        "kind":"navigation", "id":"nav", "title":if form {"Account"} else {"Settings"}, "displayMode":"inline",
        "content":{"kind":"list", "id":"list", "style":"insetGrouped", "sections":[{"rows":rows}]}
    }))
    .unwrap()
}

fn translate_x(nodes: &mut [Node], by: f64, parent_offset: usize) {
    for node in nodes {
        node.parent = node.parent.map(|i| i + parent_offset);
        node.bounds.x += by;
        if let Some(image) = &mut node.image {
            image.dest.x += by;
        }
    }
}

fn rect(id: impl Into<String>, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(id, "rect");
    n.bounds = Rect { x, y, w, h };
    n
}

fn gradient(dev: &Device) -> Scene {
    let (w, h) = (dev.viewport.width, dev.viewport.height);
    let mut bg = rect("background", 0.0, 0.0, w, h);
    bg.gradient = Some(Gradient { angle: 30.0, stops: vec![Color::rgba(0.94, 0.87, 0.46, 1.0), Color::rgba(0.15, 0.6, 0.8, 1.0)], points: None, locations: None });
    Scene { size: (w, h), appearance: Appearance::Light, nodes: vec![bg], unsupported_symbols: vec![], notes: vec![] }
}

fn glass_stack(dev: &Device, progress: f64) -> Scene {
    let mut scene = gradient(dev);
    let (w, h) = scene.size;
    for i in 0..4 {
        let offset = f64::from(i);
        let mut panel = rect(format!("glass{i}"), w * (0.05 + offset * 0.09) + (progress * std::f64::consts::TAU + offset).sin() * 10.0, h * (0.10 + offset * 0.12), w * 0.62, h * 0.32);
        panel.radius = 26.0;
        panel.opacity = 0.82;
        panel.glass = Some(GlassSpec {
            variant: GlassVariant::Regular,
            tint: None,
            interactive: false,
            blur: Some(9.0),
            mix: None,
            shadow: None,
            flat: None,
        });
        scene.nodes.push(panel);
    }
    scene
}

fn image_grid(dev: &Device, progress: f64, images: &[Arc<tiny_skia::Pixmap>]) -> Scene {
    let mut scene = gradient(dev);
    let cell = (scene.size.0 - 48.0) / 3.0;
    for i in 0..18 {
        let mut n = rect(format!("image{i}"), 12.0 + (i % 3) as f64 * (cell + 12.0), 18.0 + (i / 3) as f64 * (cell + 12.0), cell, cell);
        n.radius = 10.0;
        n.image = Some(ImageRun { pixmap: images[i % images.len()].clone(), dest: n.bounds, rotation: ((progress * std::f64::consts::TAU + i as f64).sin() * 35.0) as f32 });
        scene.nodes.push(n);
    }
    scene
}

fn animation(dev: &Device, seconds: f64) -> Scene {
    let mut scene = gradient(dev);
    let (w, h) = scene.size;
    for i in 0..72 {
        let t = seconds + f64::from(i) * 0.24;
        let size = 18.0 + f64::from(i % 9) * 4.0;
        let mut n = rect(format!("dot{i}"), (w - size) * (0.5 + 0.5 * t.sin()), (h - size) * (0.5 + 0.5 * (t * 1.3).cos()), size, size);
        n.radius = size / 2.0;
        n.fill = Some(Color::rgba(0.3, 0.4, 0.8, 0.7));
        scene.nodes.push(n);
    }
    scene
}

struct Workloads {
    home: ViewNode,
    detail: ViewNode,
    list: ViewNode,
    form_states: Vec<ViewNode>,
    images: Vec<Arc<tiny_skia::Pixmap>>,
}

impl Workloads {
    fn new() -> Self {
        let mut trees: HashMap<String, _> = common::trees().into_iter().collect();
        let images = (0..6)
            .map(|i| {
                let mut pixmap = tiny_skia::Pixmap::new(160, 160).unwrap();
                for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
                    let (x, y) = (index % 160, index / 160);
                    *pixel = tiny_skia::ColorU8::from_rgba((x + i * 41) as u8, (y + i * 29) as u8, (x + y) as u8, 255).premultiply();
                }
                Arc::new(pixmap)
            })
            .collect();
        Self {
            home: serde_json::from_value(trees.remove("home").unwrap()).unwrap(),
            detail: serde_json::from_value(trees.remove("typography").unwrap()).unwrap(),
            list: list_tree(false, ""),
            form_states: (0..="Sample text".len()).map(|i| list_tree(true, &"Sample text"[..i])).collect(),
            images,
        }
    }

    fn frame(&self, case: &str, dev: &Device, fonts: &mut Fonts, frame: usize) -> Scene {
        let scroll = HashMap::new();
        match case {
            "long-scroll" => layout(&self.list, dev, fonts, &HashMap::from([("list".into(), (0.0, (frame % 110) as f64 * 85.0))]), None),
            "nav-push-pop" => {
                let mut home = layout(&self.home, dev, fonts, &scroll, None);
                let mut detail = layout(&self.detail, dev, fonts, &scroll, None);
                let progress = 0.5 - 0.5 * ((frame % 48) as f64 * std::f64::consts::TAU / 48.0).cos();
                translate_x(&mut home.nodes, -progress * home.size.0 * 0.25, 0);
                translate_x(&mut detail.nodes, (1.0 - progress) * home.size.0, home.nodes.len());
                home.nodes.extend(detail.nodes);
                home
            }
            "form-keyboard" => layout(&self.form_states[frame % self.form_states.len()], dev, fonts, &scroll, Some("field2")),
            "glass-stack" => glass_stack(dev, (frame % 60) as f64 / 60.0),
            "image-grid" => image_grid(dev, (frame % 90) as f64 / 90.0, &self.images),
            "animation-60" => animation(dev, frame as f64 / 60.0),
            "animation-120" => animation(dev, frame as f64 / 120.0),
            _ => unreachable!(),
        }
    }
}

fn quantile(times: &mut [Duration], q: f64) -> f64 {
    times.sort_unstable();
    let i = ((times.len() as f64 * q).ceil() as usize).clamp(1, times.len()) - 1;
    times[i].as_secs_f64() * 1000.0
}

fn run(work: &Workloads, dev: &Device, case: &str, count: usize, mut gpu: Option<GpuRenderer>) {
    let mut cpu = CpuRenderer::new();
    let mut fonts = Fonts::new();
    let mut samples: [Vec<Duration>; 7] = std::array::from_fn(|_| Vec::with_capacity(count));
    let mut over = [0, 0];
    let mut peak_resources = 0;
    for i in 0..count + 12 {
        let start = Instant::now();
        let scene = work.frame(case, dev, &mut fonts, i);
        let prepare = start.elapsed();
        if i == 0 && case == "form-keyboard" {
            assert!(scene.nodes.iter().any(|n| n.focused), "benchmark form field must be focused");
            assert!(scene.nodes.iter().any(|n| n.kind == "key"), "benchmark form must show its keyboard");
        }
        let renderer: &mut dyn Renderer = match &mut gpu {
            Some(g) => g,
            None => &mut cpu,
        };
        std::hint::black_box(renderer.capture(&scene, &mut fonts, dev.scale).expect("capture"));
        let total = start.elapsed();
        if i < 12 {
            continue;
        }
        let ft = renderer.diagnostics().last_frame;
        let (list, vello) = gpu.as_ref().map_or((Duration::ZERO, Duration::ZERO), |g| {
            peak_resources = peak_resources.max(g.cache_len());
            let (list, vello, _) = g.encode_profile();
            (list, vello)
        });
        for (times, sample) in samples.iter_mut().zip([total, prepare, ft.encode, list, vello, ft.gpu, ft.readback]) {
            times.push(sample);
        }
        for (missed, hz) in over.iter_mut().zip([60.0, 120.0]) {
            *missed += usize::from(total.as_secs_f64() > 1.0 / hz);
        }
    }
    let p95 = quantile(&mut samples[0], 0.95);
    let p99 = quantile(&mut samples[0], 0.99);
    let [p50, prepare, encode, list, vello, wait, readback] = samples.map(|mut times| quantile(&mut times, 0.50));
    let gpu_backdrops_on_cpu = gpu.as_ref().map_or(0, |g| g.backdrop_counts().1);
    println!("{},{case},{p50:.2},{p95:.2},{p99:.2},{},{},{prepare:.2},{encode:.2},{list:.2},{vello:.2},{wait:.2},{readback:.2},{gpu_backdrops_on_cpu},{peak_resources}", dev.name, over[0], over[1]);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let backend = args.next().expect("usage: frame_bench cpu|gpu [frames] [device] [scenario]");
    let use_gpu = match backend.as_str() {
        "cpu" => false,
        "gpu" => true,
        _ => panic!("expected cpu or gpu"),
    };
    let frames = args.next().map_or(120, |a| a.parse().expect("frame count"));
    assert!(frames > 0);
    let device_filter = args.next();
    let scenario_filter = args.next();
    assert!(device_filter.as_deref().is_none_or(|name| DEVICES.contains(&name)), "unknown benchmark device");
    assert!(scenario_filter.as_deref().is_none_or(|case| SCENARIOS.contains(&case)), "unknown benchmark scenario");
    let work = Workloads::new();
    println!("# backend={backend} frames={frames}, 12 warmup frames per scenario");
    println!("device,scenario,p50_ms,p95_ms,p99_ms,over_60,over_120,prepare_p50,encode_p50,list_p50,vello_p50,wait_p50,readback_p50,backdrop_fallback_ops,peak_image_resources");
    for name in DEVICES.into_iter().filter(|name| device_filter.as_deref().is_none_or(|f| f == *name)) {
        let dev = device(name).unwrap();
        println!("# {name}: {}x{} points @{}x", dev.viewport.width, dev.viewport.height, dev.scale);
        for case in SCENARIOS.into_iter().filter(|case| scenario_filter.as_deref().is_none_or(|f| f == *case)) {
            let gpu = use_gpu.then(|| GpuRenderer::new_headless().expect("GPU adapter"));
            run(&work, dev, case, frames, gpu);
        }
    }
}
