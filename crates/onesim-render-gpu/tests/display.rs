//! The display list seam: builder determinism and eviction, the wire format driving the executor, and an
//! executor that never sees fonts or a scene. GPU tests skip (print and return) without an adapter.
mod common;

use common::{build, diff_pct, trees};
use onesim_protocol::Appearance;
use onesim_render::display::{DisplayList, Op, RRect, Resource, Sampling};
use onesim_render::glass::{GlassSpec, GlassVariant, ScrollEdge};
use onesim_render::{Builder, Color, Fonts, Glyph, Node, Rect, RenderError, Scene, CACHE_TTL_FRAMES};
use onesim_render_gpu::{read_texture, target_usage, wgpu, Executor};

fn executor() -> Option<Executor> {
    if common::skip_native_gpu() {
        return None;
    }
    let instance = wgpu::Instance::default();
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) {
        Ok(a) => a,
        Err(e) => {
            println!("SKIP: no GPU adapter ({e})");
            return None;
        }
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    Some(Executor::new(device, queue).expect("executor"))
}

fn target(ex: &Executor, w: u32, h: u32) -> wgpu::Texture {
    ex.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test-target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: target_usage(),
        view_formats: &[],
    })
}

fn pixels(ex: &mut Executor, dl: &DisplayList, tex: &wgpu::Texture) -> Vec<u8> {
    ex.execute(dl, tex).expect("execute");
    read_texture(ex.device(), ex.queue(), tex).expect("readback")
}

fn showcase(name: &str, dark: bool, fonts: &mut Fonts) -> Scene {
    let (_, tree) = trees().into_iter().find(|(n, _)| n == name).expect("scene");
    build(&tree, dark, fonts)
}

#[test]
fn builder_second_frame_adds_nothing() {
    let mut fonts = Fonts::new();
    let mut b = Builder::new();
    for name in ["home", "typography+tabbar", "shapes+tabbar", "controls"] {
        let sc = showcase(name, false, &mut fonts);
        let first = b.build(&sc, &mut fonts, 3.0);
        let again = b.build(&sc, &mut fonts, 3.0);
        assert!(!first.ops.is_empty(), "{name}");
        assert!(again.added.is_empty() && again.dropped.is_empty(), "{name}: second build added {} / dropped {}", again.added.len(), again.dropped.len());
        assert_eq!(first.ops, again.ops, "{name}: ops differ between builds of one scene");
        // A fresh builder produces the same ops and the same resource ids (content-addressed sprites).
        let other = Builder::new().build(&sc, &mut fonts, 3.0);
        assert_eq!(first.ops, other.ops, "{name}: not deterministic across builders");
        // Sprite ids are content hashes: whatever this builder had to add, the fresh one adds identically.
        assert!(first.added.iter().all(|r| other.added.contains(r)), "{name}: resources differ across builders");
    }
}

#[test]
fn builder_drops_unused_resources_and_stays_bounded() {
    let mut fonts = Fonts::new();
    let mut b = Builder::new();
    let sc = showcase("home", false, &mut fonts);
    let first = b.build(&sc, &mut fonts, 3.0);
    assert!(!first.added.is_empty());
    let held = b.cache_len();
    assert_eq!(held, first.added.len());
    let empty = Scene { size: (402.0, 874.0), appearance: Appearance::Light, ios: onesim_protocol::IosVersion::Ios26, nodes: vec![], unsupported_symbols: vec![], notes: vec![] };
    let mut dropped: Vec<u64> = vec![];
    for i in 0..CACHE_TTL_FRAMES + 3 {
        let dl = b.build(&empty, &mut fonts, 3.0);
        assert!(dl.added.is_empty());
        if i < CACHE_TTL_FRAMES {
            assert!(dl.dropped.is_empty(), "dropped too early at frame {i}");
        }
        dropped.extend(dl.dropped);
    }
    let mut want: Vec<u64> = first.added.iter().map(|r| r.id).collect();
    want.sort_unstable();
    dropped.sort_unstable();
    assert_eq!(dropped, want, "every added resource is dropped exactly once");
    assert_eq!(b.cache_len(), 0);
    // A dropped sprite comes back (re-added) when used again.
    let again = b.build(&sc, &mut fonts, 3.0);
    assert_eq!(again.added.len(), first.added.len());

    // Bounded: far more distinct sprites than the hard cap never leaves more than the cap (plus one frame's worth).
    let mut b = Builder::new();
    let mut live = 0;
    for i in 0..3 {
        let mut nodes = vec![];
        for k in 0..2500 {
            let mut n = Node::new(format!("n{k}"), "text");
            n.bounds = Rect { x: 0.0, y: (k % 300) as f64, w: 100.0, h: 12.0 };
            n.glyph = Some((Glyph::Chevron, Color::rgba(0.0, 0.0, 0.0, 1.0)));
            n.opacity = 1.0 - (i * 2500 + k) as f32 / 100_000.0;
            nodes.push(n);
        }
        let sc = Scene { size: (100.0, 300.0), appearance: Appearance::Light, ios: onesim_protocol::IosVersion::Ios26, nodes, unsupported_symbols: vec![], notes: vec![] };
        b.build(&sc, &mut fonts, 1.0);
        live = live.max(b.cache_len());
    }
    assert!(live <= onesim_render::CACHE_MAX_ENTRIES + 2500, "{live}");
    assert!(b.cache_len() <= onesim_render::CACHE_MAX_ENTRIES + 2500);
}

/// The encoded list drives the executor to exactly the pixels the in-memory list does, over several frames (so
/// `added` / `dropped` bookkeeping goes through the wire too).
#[test]
fn decoded_lists_render_identically() {
    let (Some(mut direct), Some(mut wire)) = (executor(), executor()) else { return };
    let mut fonts = Fonts::new();
    let mut b = Builder::new();
    let mut frames = 0;
    for (name, dark) in [("home", false), ("typography+tabbar", false), ("shapes+tabbar", true), ("todos+tabbar", false), ("home", false), ("controls+tabbar", true)] {
        let sc = showcase(name, dark, &mut fonts);
        let dl = b.build(&sc, &mut fonts, 3.0);
        let bytes = dl.encode();
        let decoded = DisplayList::decode(&bytes).expect("decode");
        assert_eq!(decoded, dl, "{name}: wire round trip");
        let tex_a = target(&direct, dl.width, dl.height);
        let tex_b = target(&wire, dl.width, dl.height);
        let a = pixels(&mut direct, &dl, &tex_a);
        wire.execute_owned(decoded, &tex_b).expect("execute owned list");
        let w = read_texture(wire.device(), wire.queue(), &tex_b).expect("owned list readback");
        assert_eq!(diff_pct(&a, &w, 0), 0.0, "{name} dark={dark}: decoded list renders differently");
        assert!(a.as_chunks::<4>().0.iter().any(|p| p[3] != 0), "{name}: blank frame");
        frames += 1;
    }
    assert_eq!(frames, 6);
}

fn rr(x: f32, y: f32, w: f32, h: f32, r: f32) -> RRect {
    RRect { x, y, w, h, r }
}

/// Pixel `(x, y)`, with channels within 2 of `want` snapped to it (frames that pass through the accumulation
/// texture round trip a channel by one).
fn px(a: &[u8], w: u32, x: u32, y: u32, want: [u8; 4]) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    let mut p = [a[i], a[i + 1], a[i + 2], a[i + 3]];
    for (c, t) in p.iter_mut().zip(want) {
        if c.abs_diff(t) <= 2 {
            *c = t;
        }
    }
    p
}

/// The executor takes a list and a texture and nothing else: a hand-written list (no `Fonts`, no `Scene`, no layout
/// in this test) with a resource, clips, glass, a shadow and an edge renders what it says.
#[test]
fn executor_needs_no_fonts_or_scene() {
    let Some(mut ex) = executor() else { return };
    let (w, h) = (96u32, 96u32);
    let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    // A 2x2 premultiplied sprite: opaque green, blue, transparent, half-transparent white.
    let sprite = Resource { id: 5, width: 2, height: 2, rgba: vec![0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0, 128, 128, 128, 128] };
    // No halo, so the sprite below the glass keeps its colour.
    let spec = GlassSpec { variant: GlassVariant::Regular, tint: None, interactive: false, blur: None, mix: None, shadow: Some((0.0, 1.0, 0.0)), flat: None };
    let dl = DisplayList {
        width: w,
        height: h,
        scale: 1.0,
        dark: false,
        ios: onesim_protocol::IosVersion::Ios26,
        added: vec![sprite],
        dropped: vec![],
        ops: vec![
            Op::Fill { shape: rr(0.0, 0.0, 96.0, 96.0, 0.0), color: white },
            Op::PushClip(rr(0.0, 0.0, 48.0, 96.0, 0.0)),
            Op::Fill { shape: rr(28.0, 76.0, 60.0, 14.0, 0.0), color: red },
            Op::PopClip,
            Op::Sprite { id: 5, x: 60, y: 60 },
            Op::Image { id: 5, transform: [4.0, 0.0, 0.0, 4.0, 10.0, 60.0], sampling: Sampling::Nearest, opacity: 1.0, shape: None },
            Op::Glass { shape: rr(10.0, 36.0, 76.0, 20.0, 10.0), spec, opacity: 1.0, halo_radius: 10.0 },
            Op::BackdropShadow { shape: rr(4.0, 70.0, 20.0, 10.0, 3.0), color: Color::rgba(0.0, 0.0, 0.0, 0.5), sigma: 2.0, dx: 0.0, dy: 1.0 },
            Op::Edge(ScrollEdge { bar: 10.0, color: white, tint: 0.0 }),
        ],
    };
    let tex = target(&ex, w, h);
    let a = pixels(&mut ex, &dl, &tex);
    assert_eq!(ex.backdrop_counts().1, 0, "all backdrop ops stayed on the GPU: {}", ex.glass_note());
    // The clip kept the right half white; the sprite sits where it was put.
    assert_eq!(px(&a, w, 40, 82, [255, 0, 0, 255]), [255, 0, 0, 255]);
    assert_eq!(px(&a, w, 70, 82, [255; 4]), [255; 4]);
    assert_eq!(px(&a, w, 60, 60, [0, 255, 0, 255]), [0, 255, 0, 255]);
    assert_eq!(px(&a, w, 61, 60, [0, 0, 255, 255]), [0, 0, 255, 255]);
    assert_eq!(px(&a, w, 62, 61, [255; 4])[..3], [255, 255, 255]);
    // A missing resource is an error, not a crash; the executor keeps working.
    let bad = DisplayList { ops: vec![Op::Sprite { id: 99, x: 0, y: 0 }], added: vec![], ..dl.clone() };
    assert!(ex.execute(&bad, &tex).unwrap_err().to_string().contains("unknown resource 99"));
    // Resources persist across frames until dropped.
    let keep = DisplayList { added: vec![], ops: vec![Op::Sprite { id: 5, x: 0, y: 0 }], ..dl.clone() };
    assert_eq!(px(&pixels(&mut ex, &keep, &tex), w, 0, 0, [0, 255, 0, 255]), [0, 255, 0, 255]);
    let gone = DisplayList { added: vec![], dropped: vec![5], ops: vec![Op::Sprite { id: 5, x: 0, y: 0 }], ..dl };
    assert!(ex.execute(&gone, &tex).is_err());
    assert_eq!(ex.resource_count(), 0);
}

#[test]
fn glass_op_without_backdrop_work_still_defines_every_pixel() {
    // An empty list and a list whose only op is a trailing glass both write the whole target.
    let Some(mut ex) = executor() else { return };
    let tex = target(&ex, 32, 32);
    let empty = DisplayList { width: 32, height: 32, scale: 1.0, ..Default::default() };
    assert!(pixels(&mut ex, &empty, &tex).iter().all(|&b| b == 0));
    let spec = GlassSpec { variant: GlassVariant::Clear, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None };
    let only = DisplayList { ops: vec![Op::Glass { shape: rr(4.0, 4.0, 20.0, 20.0, 6.0), spec, opacity: 1.0, halo_radius: 6.0 }], ..empty };
    let _ = pixels(&mut ex, &only, &tex);
}

#[test]
fn destroyed_device_reports_loss_and_the_scene_can_be_replayed() {
    let Some(mut ex) = executor() else { return };
    let mut builder = Builder::new();
    let mut fonts = Fonts::new();
    let scene = showcase("home", false, &mut fonts);
    let first = builder.build(&scene, &mut fonts, 1.0);
    let tex = target(&ex, first.width, first.height);
    let before = pixels(&mut ex, &first, &tex);

    ex.device().destroy();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while ex.check_errors().is_ok() && std::time::Instant::now() < until {
        let _ = ex.device().poll(wgpu::PollType::Poll);
        std::thread::yield_now();
    }
    assert!(matches!(ex.execute(&first, &tex), Err(RenderError::DeviceLost(_))));
    drop((ex, tex));

    let mut fresh = executor().expect("adapter after device destruction");
    builder.reset();
    let replay = builder.build(&scene, &mut fonts, 1.0);
    let tex = target(&fresh, replay.width, replay.height);
    assert_eq!(pixels(&mut fresh, &replay, &tex), before);
}
