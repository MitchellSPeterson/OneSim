//! CPU vs GPU capture latency on real showcase scenes at 402x874 @3x.
//!
//!   cargo run --release -p simcore-render-gpu --example bench [-- FRAMES]
//!
//! Steady state: the same scene is captured repeatedly (sprite/shadow caches warm); the cold first frame is shown separately.
#[path = "../tests/common/mod.rs"]
mod common;

use simcore_render::{CpuRenderer, Fonts, Renderer};
use simcore_render_gpu::GpuRenderer;
use std::time::Instant;

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| v[((v.len() as f64 * q).ceil() as usize).clamp(1, v.len()) - 1];
    (at(0.5), at(0.95), at(0.99))
}

fn main() {
    let frames: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(100);
    let mut gpu = match GpuRenderer::new_headless() {
        Ok(g) => g,
        Err(e) => {
            println!("no GPU adapter: {e}");
            return;
        }
    };
    let mut cpu = CpuRenderer::new();
    let mut fonts = Fonts::new();
    println!("adapter: {}   frames: {frames}   scale 3 (1206x2622)", gpu.diagnostics().adapter);
    println!("{:<22} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8} {:>8} | {:>6}", "scene", "cpu p50", "p95", "p99", "gpu p50", "p95", "p99", "cold", "speedup");
    let wanted = ["home", "typography", "todos+tabbar", "controls+tabbar", "shapes+tabbar"];
    for (name, tree) in common::trees().into_iter().filter(|(n, _)| wanted.contains(&n.as_str())) {
        let scene = common::build(&tree, false, &mut fonts);
        let time = |r: &mut dyn Renderer, fonts: &mut Fonts| {
            let t = Instant::now();
            r.capture(&scene, fonts, 3.0).expect("capture");
            t.elapsed().as_secs_f64() * 1000.0
        };
        time(&mut cpu, &mut fonts);
        let gpu_cold = time(&mut gpu, &mut fonts);
        let c: Vec<f64> = (0..frames).map(|_| time(&mut cpu, &mut fonts)).collect();
        let g: Vec<f64> = (0..frames).map(|_| time(&mut gpu, &mut fonts)).collect();
        let (c50, c95, c99) = stats(c);
        let (g50, g95, g99) = stats(g);
        let ft = gpu.diagnostics().last_frame;
        println!(
            "{name:<22} {c50:>8.1} {c95:>8.1} {c99:>8.1} | {g50:>8.1} {g95:>8.1} {g99:>8.1} {gpu_cold:>8.1} | {:>5.1}x   (last gpu frame: encode {:.1} gpu {:.1} readback {:.1} ms)",
            c50 / g50,
            ft.encode.as_secs_f64() * 1e3,
            ft.gpu.as_secs_f64() * 1e3,
            ft.readback.as_secs_f64() * 1e3
        );
    }
    println!("ms per capture; gpu cold = first frame incl. sprite generation and atlas allocation");
}
