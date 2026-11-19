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
    assert!(frames > 0, "expected at least one measured frame");
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
        let (_, _, (cold_hits, cold_misses)) = gpu.encode_profile();
        let c: Vec<f64> = (0..frames).map(|_| time(&mut cpu, &mut fonts)).collect();
        let mut g = Vec::with_capacity(frames);
        let mut stages: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(frames));
        let (mut hits, mut misses) = (0, 0);
        for _ in 0..frames {
            g.push(time(&mut gpu, &mut fonts));
            let (build, vello, (h, m)) = gpu.encode_profile();
            let ft = gpu.diagnostics().last_frame;
            for (samples, sample) in stages.iter_mut().zip([build, vello, ft.gpu, ft.readback]) {
                samples.push(sample.as_secs_f64() * 1e3);
            }
            hits += h;
            misses += m;
        }
        let (c50, c95, c99) = stats(c);
        let (g50, g95, g99) = stats(g);
        println!("{name:<22} {c50:>8.2} {c95:>8.2} {c99:>8.2} | {g50:>8.2} {g95:>8.2} {g99:>8.2} {gpu_cold:>8.2} | {:>5.1}x", c50 / g50);
        let [build, vello, wait, readback] = stages.map(|samples| stats(samples).0);
        println!("  gpu stage p50 (ms): list {build:.2}, Vello {vello:.2}, wait {wait:.2}, readback {readback:.2}; sprite hits/misses: first {cold_hits}/{cold_misses}, next {frames} {hits}/{misses}");
    }
    println!("ms per capture; gpu cold = first frame incl. sprite generation and atlas allocation");
}
