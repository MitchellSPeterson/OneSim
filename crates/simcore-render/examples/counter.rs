//! Milestone 1: a hand-written counter ViewNode rendered by simcore-render.
//!
//!   cargo run -p simcore-render --example counter [-- --out counter.png --scale 2 --dark]
//!   cargo run -p simcore-render --example counter --features window -- --window

use simcore_protocol::{Appearance, ViewNode};
use simcore_render::{build_scene, render_png, Fonts, Scene, SceneParams};

const VIEWPORT: (f64, f64) = (402.0, 874.0);
const INCREMENT: u64 = 1;

fn tree(count: u32) -> ViewNode {
    serde_json::from_value(serde_json::json!({
        "kind": "vstack", "id": "0", "spacing": 16.0, "alignment": "center",
        "children": [
            {"kind": "text", "id": "0.0", "text": format!("Count: {count}")},
            {"kind": "button", "id": "0.1", "action": INCREMENT, "label": {
                "kind": "text", "id": "0.1.label", "text": "Increment",
                "modifiers": [
                    {"foregroundColor": {"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0}},
                    {"padding": {"top": 12.0, "leading": 24.0, "bottom": 12.0, "trailing": 24.0}},
                    {"background": {"kind": "color", "id": "0.1.bg", "color": {"semantic": "accent"}}},
                    {"cornerRadius": 12.0}
                ]}}
        ]
    }))
    .expect("valid tree")
}

fn build(count: u32, vp: (f64, f64), a: Appearance, scale: f64, fonts: &mut Fonts) -> Scene {
    let scroll = std::collections::HashMap::new();
    let p = SceneParams {
        viewport: vp,
        safe_top: 62.0,
        safe_bottom: 34.0,
        scale,
        appearance: a,
        dynamic_type: Default::default(),
        scroll: &scroll,
        focus: None,
        overscroll: false,
        status_bar: true,
        chrome: Default::default(),
    };
    build_scene(&tree(count), &p, fonts)
}

fn scene(count: u32, vp: (f64, f64), dark: bool, scale: f64, fonts: &mut Fonts) -> Scene {
    build(count, vp, if dark { Appearance::Dark } else { Appearance::Light }, scale, fonts)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned();
    let dark = flag("--dark");
    let mut fonts = Fonts::new();

    #[cfg(feature = "window")]
    if flag("--window") {
        return window::run(dark);
    }
    #[cfg(not(feature = "window"))]
    if flag("--window") {
        eprintln!("rebuild with --features window");
        std::process::exit(2);
    }

    let out = value("--out").unwrap_or_else(|| "counter.png".into());
    let scale: f64 = value("--scale").and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let png = render_png(&scene(0, VIEWPORT, dark, scale, &mut fonts), &mut fonts, scale).expect("png");
    std::fs::write(&out, png).expect("write png");
    println!("wrote {out}");
}

#[cfg(feature = "window")]
mod window {
    use super::*;
    use std::{num::NonZeroU32, rc::Rc};
    use winit::{
        application::ApplicationHandler,
        dpi::LogicalSize,
        event::{ElementState, MouseButton, WindowEvent},
        event_loop::{ActiveEventLoop, EventLoop},
        window::{Window, WindowId},
    };

    struct App {
        window: Option<Rc<Window>>,
        surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
        fonts: Fonts,
        count: u32,
        dark: bool,
        cursor: (f64, f64),
    }

    impl App {
        fn redraw(&mut self) {
            let (Some(win), Some(surface)) = (&self.window, &mut self.surface) else { return };
            let size = win.inner_size();
            let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
            let sf = win.scale_factor();
            let vp = (size.width as f64 / sf, size.height as f64 / sf);
            let a = if self.dark { Appearance::Dark } else { Appearance::Light };
            let sc = build(self.count, vp, a, sf, &mut self.fonts);
            let pm = simcore_render::render(&sc, &mut self.fonts, sf);
            surface.resize(w, h).expect("resize");
            let mut buf = surface.buffer_mut().expect("buffer");
            for (dst, px) in buf.iter_mut().zip(pm.pixels()) {
                *dst = (px.red() as u32) << 16 | (px.green() as u32) << 8 | px.blue() as u32;
            }
            buf.present().expect("present");
        }
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title("SimCore counter")
                .with_inner_size(LogicalSize::new(VIEWPORT.0, VIEWPORT.1));
            let win = Rc::new(el.create_window(attrs).expect("window"));
            let ctx = softbuffer::Context::new(win.clone()).expect("context");
            self.surface = Some(softbuffer::Surface::new(&ctx, win.clone()).expect("surface"));
            self.window = Some(win);
        }

        fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
            match ev {
                WindowEvent::CloseRequested => el.exit(),
                WindowEvent::RedrawRequested => self.redraw(),
                WindowEvent::Resized(_) => self.window.iter().for_each(|w| w.request_redraw()),
                WindowEvent::CursorMoved { position, .. } => self.cursor = (position.x, position.y),
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    let Some(win) = &self.window else { return };
                    let sf = win.scale_factor();
                    let size = win.inner_size();
                    let a = if self.dark { Appearance::Dark } else { Appearance::Light };
                    let sc = build(self.count, (size.width as f64 / sf, size.height as f64 / sf), a, sf, &mut self.fonts);
                    if let Some(i) = sc.hit_test(self.cursor.0 / sf, self.cursor.1 / sf) {
                        if sc.nodes[i].action == Some(INCREMENT) {
                            self.count += 1;
                            win.request_redraw();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn run(dark: bool) {
        let mut app = App { window: None, surface: None, fonts: Fonts::new(), count: 0, dark, cursor: (0.0, 0.0) };
        EventLoop::new().expect("event loop").run_app(&mut app).expect("run");
    }
}
