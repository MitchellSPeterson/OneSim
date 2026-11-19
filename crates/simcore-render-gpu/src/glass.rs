//! Liquid Glass and scroll-edge backdrops on the GPU.
//!
//! Operates on the frame accumulated so far (`acc`, an `Rgba8Unorm` texture, straight alpha). Per backdrop node
//! one command buffer runs compute passes that mirror `simcore_render::glass` (same parameters from
//! `spec_params`, same sRGB-encoded arithmetic):
//!
//! 1. `snapshot`: copy the padded region around the shape (mirror-folded for flat materials) into scratch `A`;
//!    for the scroll edge, `edge_down` averages the band into `A` instead.
//! 2. `blur_h`/`blur_v` x3: Kovesi box blur with the CPU's radii (`glass::box_radii`), edges replicated.
//! 3. Shadows (glass halo, node shadow): coverage of the rounded rect on a coarse grid (about sigma/4 pixels per
//!    cell), exact gaussian on the grid (`gauss_h`/`gauss_v`), bilinear lookup when compositing.
//! 4. `glass_composite.wgsl`: per pixel, in the CPU's order: ancestor clip coverage, halo, glass (lens, colour
//!    transform, tint, rim), node shadow, scroll edge. Reads `acc`, writes the frame target.
//!
//! Finally the touched region is copied from the target back into `acc`.

use simcore_render::glass::{self as g, GlassVariant};
use simcore_render::{Node, Scene};
use std::num::NonZeroU64;
use vello::wgpu;

const PASSES: &str = include_str!("shaders/glass_passes.wgsl");
const COMPOSITE: &str = include_str!("shaders/glass_composite.wgsl");
/// Dynamic-offset uniform slots per node (snapshot 1 + blur 6, halo 3, own shadow 3 = 13).
const SLOTS: u64 = 16;
const MAX_CLIPS: usize = 4;
/// `P` in `glass_passes.wgsl`: four vec4<i32> + four vec4<f32>.
const P_SIZE: u64 = 128;
/// `C` in `glass_composite.wgsl`: 6 + 8 vec4 + 4 clips + radii.
const C_SIZE: u64 = 19 * 16;
const SCRATCH: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

type Ints = [[i32; 4]; 4];
type Floats = [[f32; 4]; 4];

struct Pipes {
    snapshot: wgpu::ComputePipeline,
    edge_down: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    shadow_cov: wgpu::ComputePipeline,
    gauss_h: wgpu::ComputePipeline,
    gauss_v: wgpu::ComputePipeline,
    composite: wgpu::ComputePipeline,
}

struct Img {
    w: u32,
    h: u32,
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Scratch textures, row table and the bind groups over them (rebuilt when any of them or the frame textures change).
struct Scratch {
    gen: u64,
    a: Img,
    _b: Img,
    h0: Img,
    _h1: Img,
    o0: Img,
    _o1: Img,
    rows: wgpu::Buffer,
    rows_cap: u64,
    acc_a: wgpu::BindGroup,
    a_b: wgpu::BindGroup,
    b_a: wgpu::BindGroup,
    a_h0: wgpu::BindGroup,
    h0_h1: wgpu::BindGroup,
    h1_h0: wgpu::BindGroup,
    a_o0: wgpu::BindGroup,
    o0_o1: wgpu::BindGroup,
    o1_o0: wgpu::BindGroup,
    comp: wgpu::BindGroup,
}

/// The frame textures a node is composited on.
pub(crate) struct Surfaces<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub acc: &'a wgpu::Texture,
    pub acc_view: &'a wgpu::TextureView,
    pub target: &'a wgpu::Texture,
    pub target_view: &'a wgpu::TextureView,
    /// Changes whenever `acc` / `target` are re-created.
    pub gen: u64,
    pub size: (u32, u32),
}

pub(crate) struct GlassGpu {
    pipes: Pipes,
    bgl_pass: wgpu::BindGroupLayout,
    bgl_comp: wgpu::BindGroupLayout,
    stride: u64,
    uniform: wgpu::Buffer,
    comp: wgpu::Buffer,
    slots: Vec<u8>,
    rows_cpu: Vec<u8>,
    scratch: Option<Scratch>,
}

struct ShadowPlan {
    /// Grid origin (page px), size (cells) and cell size (px).
    gx0: i32,
    gy0: i32,
    gw: u32,
    gh: u32,
    f: i32,
    sigma_cells: f32,
    shape: [f32; 4],
    r: f32,
    rgb: [f32; 3],
    alpha: f32,
}

impl ShadowPlan {
    fn new(b: (f32, f32, f32, f32), radius_pt: f32, sigma_pt: f32, dx: f32, dy: f32, rgba: [u8; 4], s: f32) -> Self {
        let sigma = (sigma_pt * s).max(0.5);
        let pad = (sigma * 3.0).ceil() + 2.0;
        let (px, py) = (b.0 * s + dx * s, b.1 * s + dy * s);
        let (w, h) = ((b.2 * s + 2.0 * pad).ceil() as u32 + 1, (b.3 * s + 2.0 * pad).ceil() as u32 + 1);
        let f = ((sigma / 4.0).floor() as i32).max(1);
        ShadowPlan {
            gx0: (px.floor() - pad) as i32,
            gy0: (py.floor() - pad) as i32,
            gw: w.div_ceil(f as u32),
            gh: h.div_ceil(f as u32),
            f,
            sigma_cells: sigma / f as f32,
            shape: [px, py, b.2 * s, b.3 * s],
            r: radius_pt * s,
            rgb: [rgba[0] as f32, rgba[1] as f32, rgba[2] as f32],
            alpha: rgba[3] as f32 / 255.0,
        }
    }

    /// Page rectangle `(x0, y0, x1, y1)` the shadow covers.
    fn rect(&self) -> (i32, i32, i32, i32) {
        (self.gx0, self.gy0, self.gx0 + (self.gw as i32) * self.f, self.gy0 + (self.gh as i32) * self.f)
    }
}

fn u8c(v: f32) -> u8 {
    (v * 255.0).round() as u8
}

fn put_i(out: &mut Vec<u8>, v: &[[i32; 4]]) {
    v.iter().flatten().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
}

fn put_f(out: &mut Vec<u8>, v: &[[f32; 4]]) {
    v.iter().flatten().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
}

fn groups(n: u32) -> u32 {
    n.div_ceil(8)
}

fn pass_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None }
}

fn tex_ty() -> wgpu::BindingType {
    wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }
}

fn storage_ty(format: wgpu::TextureFormat) -> wgpu::BindingType {
    wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format, view_dimension: wgpu::TextureViewDimension::D2 }
}

fn make_img(device: &wgpu::Device, w: u32, h: u32) -> Img {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("simcore-glass-scratch"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCRATCH,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Img { w, h, _tex: tex, view }
}

impl GlassGpu {
    /// Compile the pipelines. `Err` carries the validation message (the caller falls back to the CPU backdrop path).
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let uniform_ty = |dynamic: bool, size: u64| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: NonZeroU64::new(size),
        };
        let bgl_pass = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("simcore-glass-pass"),
            entries: &[pass_entry(0, uniform_ty(true, P_SIZE)), pass_entry(1, tex_ty()), pass_entry(2, storage_ty(SCRATCH))],
        });
        let bgl_comp = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("simcore-glass-composite"),
            entries: &[
                pass_entry(0, uniform_ty(false, C_SIZE)),
                pass_entry(1, tex_ty()),
                pass_entry(2, tex_ty()),
                pass_entry(3, tex_ty()),
                pass_entry(4, tex_ty()),
                pass_entry(5, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                pass_entry(6, storage_ty(wgpu::TextureFormat::Rgba8Unorm)),
            ],
        });
        let layout = |bgl: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(bgl)], immediate_size: 0 })
        };
        let (pl_pass, pl_comp) = (layout(&bgl_pass), layout(&bgl_comp));
        let m_pass = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("glass-passes"), source: wgpu::ShaderSource::Wgsl(PASSES.into()) });
        let m_comp = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("glass-composite"), source: wgpu::ShaderSource::Wgsl(COMPOSITE.into()) });
        let pipe = |l: &wgpu::PipelineLayout, m: &wgpu::ShaderModule, entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(l),
                module: m,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let pipes = Pipes {
            snapshot: pipe(&pl_pass, &m_pass, "snapshot"),
            edge_down: pipe(&pl_pass, &m_pass, "edge_down"),
            blur_h: pipe(&pl_pass, &m_pass, "blur_h"),
            blur_v: pipe(&pl_pass, &m_pass, "blur_v"),
            shadow_cov: pipe(&pl_pass, &m_pass, "shadow_cov"),
            gauss_h: pipe(&pl_pass, &m_pass, "gauss_h"),
            gauss_v: pipe(&pl_pass, &m_pass, "gauss_v"),
            composite: pipe(&pl_comp, &m_comp, "main"),
        };
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("glass shaders: {e}"));
        }
        let stride = (device.limits().min_uniform_buffer_offset_alignment as u64).max(P_SIZE).next_power_of_two();
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("simcore-glass-slots"),
            size: stride * SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let comp = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("simcore-glass-composite"),
            size: C_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self { pipes, bgl_pass, bgl_comp, stride, uniform, comp, slots: Vec::new(), rows_cpu: Vec::new(), scratch: None })
    }

    /// Whether node `idx` can be composited here (otherwise the caller uses the CPU backdrop path).
    pub fn supports(scene: &Scene, idx: usize, scale: f64) -> bool {
        let n = &scene.nodes[idx];
        let glass = n.glass.as_ref().is_some_and(|g| g.variant != GlassVariant::Identity);
        // The edge blurs what the pixmap holds after the glass and shadows: keep that combination on the CPU.
        if n.edge.is_some() && (glass || n.shadow.is_some()) {
            return false;
        }
        simcore_render::clip_chain(scene, idx, scale).len() <= MAX_CLIPS
    }

    fn pass_bg(&self, device: &wgpu::Device, src: &wgpu::TextureView, dst: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("simcore-glass-pass"),
            layout: &self.bgl_pass,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.uniform, offset: 0, size: NonZeroU64::new(P_SIZE) }),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(dst) },
            ],
        })
    }

    /// Make the scratch textures at least `need` = [(w, h) of A/B, of H0/H1, of O0/O1] and bind them to this frame.
    fn ensure(&mut self, f: &Surfaces, need: [(u32, u32); 3]) -> Result<(), String> {
        let max = f.device.limits().max_texture_dimension_2d;
        let up = |v: u32| if v == 0 { 0 } else { v.div_ceil(64) * 64 };
        let cur = self.scratch.as_ref().filter(|s| s.gen == f.gen);
        let have = [cur.map_or((0, 0), |s| (s.a.w, s.a.h)), cur.map_or((0, 0), |s| (s.h0.w, s.h0.h)), cur.map_or((0, 0), |s| (s.o0.w, s.o0.h))];
        let dims: Vec<(u32, u32)> = (0..3).map(|i| (have[i].0.max(up(need[i].0)), have[i].1.max(up(need[i].1)))).collect();
        if dims.iter().any(|&(w, h)| w > max || h > max) {
            return Err(format!("glass scratch {dims:?} exceeds the texture limit {max}"));
        }
        let rows_need = (f.size.1 as u64 * 16).max(16);
        if cur.is_some_and(|s| s.rows_cap >= rows_need) && (0..3).all(|i| have[i] == dims[i]) {
            return Ok(());
        }
        // Unused pairs stay 1x1.
        let d = |i: usize| (dims[i].0.max(1), dims[i].1.max(1));
        let (a, b) = (make_img(f.device, d(0).0, d(0).1), make_img(f.device, d(0).0, d(0).1));
        let (h0, h1) = (make_img(f.device, d(1).0, d(1).1), make_img(f.device, d(1).0, d(1).1));
        let (o0, o1) = (make_img(f.device, d(2).0, d(2).1), make_img(f.device, d(2).0, d(2).1));
        let rows = f.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("simcore-edge-rows"),
            size: rows_need,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let comp = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("simcore-glass-composite"),
            layout: &self.bgl_comp,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.comp, offset: 0, size: NonZeroU64::new(C_SIZE) }),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(f.acc_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&a.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&h0.view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&o0.view) },
                wgpu::BindGroupEntry { binding: 5, resource: rows.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(f.target_view) },
            ],
        });
        let bg = |s: &wgpu::TextureView, d: &wgpu::TextureView| self.pass_bg(f.device, s, d);
        let s = Scratch {
            gen: f.gen,
            acc_a: bg(f.acc_view, &a.view),
            a_b: bg(&a.view, &b.view),
            b_a: bg(&b.view, &a.view),
            a_h0: bg(&a.view, &h0.view),
            h0_h1: bg(&h0.view, &h1.view),
            h1_h0: bg(&h1.view, &h0.view),
            a_o0: bg(&a.view, &o0.view),
            o0_o1: bg(&o0.view, &o1.view),
            o1_o0: bg(&o1.view, &o0.view),
            comp,
            rows_cap: rows_need,
            rows,
            a,
            _b: b,
            h0,
            _h1: h1,
            o0,
            _o1: o1,
        };
        self.scratch = Some(s);
        Ok(())
    }

    fn slot(&mut self, i: Ints, f: Floats) -> u32 {
        let off = self.slots.len() as u64;
        assert!(off / self.stride < SLOTS, "glass slot overflow");
        put_i(&mut self.slots, &i);
        put_f(&mut self.slots, &f);
        self.slots.resize((off + self.stride) as usize, 0);
        off as u32
    }

    /// Composite backdrop node `idx` onto the frame: reads `acc`, writes `target` (and copies the result back to `acc`).
    pub fn encode(&mut self, fr: &Surfaces, scene: &Scene, idx: usize, scale: f64) -> Result<(), String> {
        let n: &Node = &scene.nodes[idx];
        let (s, (pw, ph)) = (scale as f32, fr.size);
        let dark = scene.appearance == simcore_protocol::Appearance::Dark;
        let b = n.bounds;
        let bb = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);

        // Glass: parameters, shape and the snapshot rectangle.
        let glass = n.glass.as_ref().filter(|gs| gs.variant != GlassVariant::Identity).map(|gs| {
            let p = g::spec_params(gs, dark);
            let r = (n.radius as f32).min(bb.2 / 2.0).min(bb.3 / 2.0) * s;
            let shape = g::Shape { x: bb.0 * s, y: bb.1 * s, w: bb.2 * s, h: bb.3 * s, r };
            let snap = g::snapshot_rect(shape, s, &p, pw, ph);
            (gs, p, shape, snap)
        });
        let halo = glass.as_ref().filter(|(_, p, ..)| p.sh_a > 0.0).map(|(_, p, ..)| {
            let rgba = [0, 0, 0, u8c(p.sh_a * n.opacity)];
            ShadowPlan::new(bb, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32, p.sh_sigma, 0.0, p.sh_dy, rgba, s)
        });
        let own = n.shadow.as_ref().map(|sd| {
            let c = sd.color;
            let rgba = [u8c(c.r), u8c(c.g), u8c(c.b), u8c(c.a * n.opacity)];
            ShadowPlan::new(bb, n.radius.max(n.clip_radius.unwrap_or(0.0)) as f32, sd.radius as f32, sd.dx as f32, sd.dy as f32, rgba, s)
        });
        let edge = n.edge.as_ref();
        let edge_f = (s.round() as u32).max(1);
        let edge_rows = edge.map_or(0, |e| g::edge_rows(e, s, ph as usize) as u32);
        let (ebw, ebh) = (pw.div_ceil(edge_f), edge_rows.div_ceil(edge_f));

        let a_need = match (&glass, edge) {
            (Some((_, _, _, snap)), _) => (snap.2 as u32, snap.3 as u32),
            (None, Some(_)) => (ebw, ebh),
            _ => (0, 0),
        };
        self.ensure(fr, [a_need, halo.as_ref().map_or((0, 0), |h| (h.gw, h.gh)), own.as_ref().map_or((0, 0), |h| (h.gw, h.gh))])?;

        // Dispatch region (page px), clipped to the frame.
        let mut rect = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut grow = |r: (i32, i32, i32, i32)| rect = (rect.0.min(r.0), rect.1.min(r.1), rect.2.max(r.2), rect.3.max(r.3));
        if let Some((_, _, sh, _)) = &glass {
            grow((sh.x.floor() as i32 - 1, sh.y.floor() as i32 - 1, (sh.x + sh.w).ceil() as i32 + 1, (sh.y + sh.h).ceil() as i32 + 1));
        }
        if let Some(h) = &halo {
            grow(h.rect());
        }
        if let Some(o) = &own {
            grow(o.rect());
        }
        if edge.is_some() {
            grow((0, 0, pw as i32, edge_rows as i32));
        }
        let (rx0, ry0, rx1, ry1) = (rect.0.max(0), rect.1.max(0), rect.2.min(pw as i32), rect.3.min(ph as i32));
        if rx1 <= rx0 || ry1 <= ry0 {
            return Ok(());
        }

        self.slots.clear();
        let mut jobs: Vec<(Job, u32, (u32, u32))> = vec![];
        let radii_job = |this: &mut Self, jobs: &mut Vec<(Job, u32, (u32, u32))>, radii: [usize; 3], w: u32, h: u32| {
            for r in radii {
                let i = [[w as i32, h as i32, r as i32, 0], [0; 4], [0; 4], [0; 4]];
                let o = this.slot(i, [[0.0; 4]; 4]);
                jobs.push((Job::BlurH, o, (w, h)));
                let o = this.slot(i, [[0.0; 4]; 4]);
                jobs.push((Job::BlurV, o, (w, h)));
            }
        };

        // Glass snapshot + blur, or the edge band.
        if let Some((gs, p, shape, snap)) = &glass {
            let _ = gs;
            let (x0, y0, w, h) = (snap.0, snap.1, snap.2 as u32, snap.3 as u32);
            let (lo_x, lo_y) = (shape.x.round() as i32, shape.y.round() as i32);
            let hi = |v: f32, lo: i32| (v.round() as i32).max(lo + 1);
            let i = [[x0, y0, w as i32, h as i32], [lo_x, hi(shape.x + shape.w, lo_x), lo_y, hi(shape.y + shape.h, lo_y)], [p.mirror as i32, 0, 0, 0], [0; 4]];
            let o = self.slot(i, [[0.0; 4]; 4]);
            jobs.push((Job::Snapshot, o, (w, h)));
            radii_job(self, &mut jobs, g::box_radii(p.sigma * s), w, h);
        } else if let Some(e) = edge {
            let i = [[ebw as i32, ebh as i32, edge_f as i32, edge_rows as i32], [pw as i32, ph as i32, 0, 0], [0; 4], [0; 4]];
            let o = self.slot(i, [[0.0; 4]; 4]);
            jobs.push((Job::EdgeDown, o, (ebw, ebh)));
            radii_job(self, &mut jobs, g::box_radii(g::EDGE_SIGMA * s / edge_f as f32), ebw, ebh);
            let _ = e;
        }
        for (plan, which) in [(&halo, Which::H), (&own, Which::O)] {
            let Some(sp) = plan else { continue };
            let k = if sp.f > 1 { 4 } else { 1 };
            let i = [[sp.gw as i32, sp.gh as i32, sp.f, k], [0; 4], [0; 4], [0; 4]];
            let f = [[sp.gx0 as f32, sp.gy0 as f32, sp.shape[0], sp.shape[1]], [sp.shape[2], sp.shape[3], sp.r, 0.0], [0.0; 4], [0.0; 4]];
            let o = self.slot(i, f);
            jobs.push((Job::ShadowCov(which), o, (sp.gw, sp.gh)));
            let r = (sp.sigma_cells * 3.0).ceil() as i32;
            let i = [[sp.gw as i32, sp.gh as i32, r, 0], [0; 4], [0; 4], [0; 4]];
            let f = [[sp.sigma_cells, 0.0, 0.0, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]];
            let o = self.slot(i, f);
            jobs.push((Job::GaussH(which), o, (sp.gw, sp.gh)));
            let o = self.slot(i, f);
            jobs.push((Job::GaussV(which), o, (sp.gw, sp.gh)));
        }

        // Composite uniform.
        let chain = simcore_render::clip_chain(scene, idx, scale);
        let flags = halo.is_some() as i32 | (glass.is_some() as i32) << 1 | (own.is_some() as i32) << 2 | (edge.is_some() as i32) << 3;
        let (snap, shape, p, gs) = match &glass {
            Some((gs, p, shape, snap)) => (*snap, [shape.x, shape.y, shape.w, shape.h, shape.r], Some(*p), Some(*gs)),
            None => ((0, 0, 1, 1), [0.0; 5], None, None),
        };
        let pr = p.unwrap_or_else(|| g::params(GlassVariant::Regular, dark));
        let (tint, tint_a) = match gs.and_then(|gs| gs.tint) {
            Some(t) => {
                let sh = pr.tint_shift * 255.0;
                ([(t.r * 255.0 + sh).clamp(0.0, 255.0), (t.g * 255.0 + sh).clamp(0.0, 255.0), (t.b * 255.0 + sh).clamp(0.0, 255.0)], t.a.max(1e-6))
            }
            None => ([0.0; 3], 0.0),
        };
        let hp = halo.as_ref();
        let op = own.as_ref();
        let (ep, ec) = (edge_f as i32, edge.map_or([0.0; 3], |e| [e.color.r * 255.0, e.color.g * 255.0, e.color.b * 255.0]));
        let mut c = Vec::with_capacity(C_SIZE as usize);
        put_i(
            &mut c,
            &[
                [rx0, ry0, rx1 - rx0, ry1 - ry0],
                [flags, chain.len() as i32, snap.2 as i32, snap.3 as i32],
                [snap.0, snap.1, edge_rows as i32, ep],
                hp.map_or([0; 4], |h| [h.gx0, h.gy0, h.gw as i32, h.gh as i32]),
                op.map_or([0; 4], |h| [h.gx0, h.gy0, h.gw as i32, h.gh as i32]),
                [hp.map_or(1, |h| h.f), op.map_or(1, |h| h.f), ebw as i32, ebh as i32],
            ],
        );
        let mut clips = [[0.0f32; 4]; 4];
        let mut radii = [0.0f32; 4];
        for (k, cl) in chain.iter().enumerate() {
            clips[k] = [cl[0], cl[1], cl[2], cl[3]];
            radii[k] = cl[4];
        }
        let sh4 = |h: Option<&ShadowPlan>| h.map_or([0.0; 4], |h| [h.rgb[0], h.rgb[1], h.rgb[2], h.alpha]);
        put_f(
            &mut c,
            &[
                [shape[0], shape[1], shape[2], shape[3]],
                [shape[4], pr.lens_d * s, (pr.lens_w * s).max(0.5), (pr.rim_w * s).max(1.0)],
                [pr.sat, pr.mul, pr.add * 255.0, pr.rim_a],
                [pr.rim_dir, n.opacity, tint_a, 0.0],
                [tint[0], tint[1], tint[2], 0.0],
                sh4(hp),
                sh4(op),
                [ec[0], ec[1], ec[2], 0.0],
            ],
        );
        put_f(&mut c, &clips);
        put_f(&mut c, &[radii]);
        debug_assert_eq!(c.len() as u64, C_SIZE);
        fr.queue.write_buffer(&self.comp, 0, &c);
        fr.queue.write_buffer(&self.uniform, 0, &self.slots);
        if let Some(e) = edge {
            self.rows_cpu.clear();
            for y in 0..edge_rows as usize {
                let [a, bw, t] = g::edge_row(e, y, s);
                [a, bw, t, 0.0].iter().for_each(|v| self.rows_cpu.extend_from_slice(&v.to_le_bytes()));
            }
            if let Some(sc) = &self.scratch {
                fr.queue.write_buffer(&sc.rows, 0, &self.rows_cpu);
            }
        }

        let Some(sc) = self.scratch.as_ref() else { return Err("glass scratch missing".into()) };
        let mut enc = fr.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("simcore-glass") });
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("simcore-glass"), timestamp_writes: None });
            for (job, off, (w, h)) in &jobs {
                let (pipe, bg) = match job {
                    Job::Snapshot => (&self.pipes.snapshot, &sc.acc_a),
                    Job::EdgeDown => (&self.pipes.edge_down, &sc.acc_a),
                    Job::BlurH => (&self.pipes.blur_h, &sc.a_b),
                    Job::BlurV => (&self.pipes.blur_v, &sc.b_a),
                    Job::ShadowCov(Which::H) => (&self.pipes.shadow_cov, &sc.a_h0),
                    Job::ShadowCov(_) => (&self.pipes.shadow_cov, &sc.a_o0),
                    Job::GaussH(Which::H) => (&self.pipes.gauss_h, &sc.h0_h1),
                    Job::GaussH(_) => (&self.pipes.gauss_h, &sc.o0_o1),
                    Job::GaussV(Which::H) => (&self.pipes.gauss_v, &sc.h1_h0),
                    Job::GaussV(_) => (&self.pipes.gauss_v, &sc.o1_o0),
                };
                cp.set_pipeline(pipe);
                cp.set_bind_group(0, bg, &[*off]);
                cp.dispatch_workgroups(groups(*w), groups(*h), 1);
            }
            cp.set_pipeline(&self.pipes.composite);
            cp.set_bind_group(0, &sc.comp, &[]);
            cp.dispatch_workgroups(groups((rx1 - rx0) as u32), groups((ry1 - ry0) as u32), 1);
        }
        let origin = wgpu::Origin3d { x: rx0 as u32, y: ry0 as u32, z: 0 };
        enc.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: fr.target, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: fr.acc, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: (rx1 - rx0) as u32, height: (ry1 - ry0) as u32, depth_or_array_layers: 1 },
        );
        fr.queue.submit([enc.finish()]);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Which {
    H,
    O,
}

#[derive(Clone, Copy)]
enum Job {
    Snapshot,
    EdgeDown,
    BlurH,
    BlurV,
    ShadowCov(Which),
    GaussH(Which),
    GaussV(Which),
}
