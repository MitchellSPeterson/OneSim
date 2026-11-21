// Per-pixel Liquid Glass / scroll-edge composite (mirrors onesim_render::glass::draw / draw_edge and the
// halo / own shadows of raster::paint_backdrop). Works on premultiplied 0..255 values like the CPU pixmap.

struct C {
    i0: vec4<i32>,   // dispatch region x0, y0, w, h (page px)
    i1: vec4<i32>,   // flags (1 halo, 2 glass, 4 own shadow, 8 edge), clip count, snapshot w, h
    i2: vec4<i32>,   // snapshot x0, y0, edge rows, edge f
    i3: vec4<i32>,   // halo grid gx0, gy0, gw, gh
    i4: vec4<i32>,   // own shadow grid gx0, gy0, gw, gh
    i5: vec4<i32>,   // halo f, own f, edge bw, edge bh
    f0: vec4<f32>,   // shape x, y, w, h
    f1: vec4<f32>,   // shape r, lens_d, lens_w, rim_w (px)
    f2: vec4<f32>,   // sat, mul, add (0..255), rim_a
    f3: vec4<f32>,   // rim_dir, opacity, tint alpha (0 = none), vertical rim flag
    f4: vec4<f32>,   // tint rgb (0..255), outline alpha
    f5: vec4<f32>,   // halo rgb (0..255), alpha
    f6: vec4<f32>,   // own shadow rgb (0..255), alpha
    f7: vec4<f32>,   // edge colour rgb (0..255), glass outline width (px)
};

@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var acc: texture_2d<f32>;
@group(0) @binding(2) var blurred: texture_2d<f32>;
@group(0) @binding(3) var halo: texture_2d<f32>;
@group(0) @binding(4) var own: texture_2d<f32>;
// Two vec4 per ancestor clip (bounds, radius), then one vec4 per scroll-edge row.
@group(0) @binding(5) var<storage, read> params: array<vec4<f32>>;
@group(0) @binding(6) var out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var linear_sampler: sampler;

struct Sd { d: f32, n: vec2<f32> };

fn rr_sdf(p: vec2<f32>, x: f32, y: f32, w: f32, h: f32, r0: f32) -> Sd {
    let r = max(min(r0, min(w / 2.0, h / 2.0)), 0.0);
    let d = p - vec2<f32>(x + w / 2.0, y + h / 2.0);
    let q = abs(d) - vec2<f32>(w / 2.0 - r, h / 2.0 - r);
    let s = vec2<f32>(select(-1.0, 1.0, d.x >= 0.0), select(-1.0, 1.0, d.y >= 0.0));
    let m = max(q, vec2<f32>(0.0));
    let ml = length(m);
    var n: vec2<f32>;
    if (ml > 1e-4) {
        n = s * m / ml;
    } else if (q.x > q.y) {
        n = vec2<f32>(s.x, 0.0);
    } else {
        n = vec2<f32>(0.0, s.y);
    }
    return Sd(ml + min(max(q.x, q.y), 0.0) - r, n);
}

fn bilinear(t: texture_2d<f32>, w: i32, h: i32, px: f32, py: f32) -> vec3<f32> {
    let x = clamp(px, 0.0, f32(w) - 1.0);
    let y = clamp(py, 0.0, f32(h) - 1.0);
    let uv = (vec2<f32>(x, y) + 0.5) / vec2<f32>(textureDimensions(t));
    return textureSampleLevel(t, linear_sampler, uv, 0.0).rgb;
}

// Blurred coverage of a shadow grid at pixel centre (x, y).
fn grid_at(t: texture_2d<f32>, g: vec4<i32>, f: i32, x: i32, y: i32) -> f32 {
    let u = (f32(x) + 0.5 - f32(g.x)) / f32(f) - 0.5;
    let v = (f32(y) + 0.5 - f32(g.y)) / f32(f) - 0.5;
    return bilinear(t, g.z, g.w, u, v).r;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (i32(gid.x) >= c.i0.z || i32(gid.y) >= c.i0.w) { return; }
    let x = c.i0.x + i32(gid.x);
    let y = c.i0.y + i32(gid.y);
    let flags = c.i1.x;
    let px = vec2<f32>(f32(x) + 0.5, f32(y) + 0.5);
    let old = textureLoad(acc, vec2<i32>(x, y), 0);
    var a = old.a;
    var col = old.rgb * 255.0 * a;

    // Clip coverage of the ancestors.
    var m = 1.0;
    for (var k = 0; k < c.i1.y; k++) {
        let cl = params[2 * k];
        let radius = params[2 * k + 1].x;
        m *= clamp(0.5 - rr_sdf(px, cl.x, cl.y, cl.z, cl.w, radius).d, 0.0, 1.0);
    }

    // Glass halo (soft outer shadow), source-over.
    if ((flags & 1) != 0) {
        let gx = x - c.i3.x;
        let gy = y - c.i3.y;
        let f = c.i5.x;
        if (gx >= 0 && gy >= 0 && gx < c.i3.z * f && gy < c.i3.w * f) {
            let sa = grid_at(halo, c.i3, f, x, y) * c.f5.w;
            col = c.f5.rgb * sa * m + col * (1.0 - sa * m);
            a = sa * m + a * (1.0 - sa * m);
        }
    }

    // Glass.
    if ((flags & 2) != 0) {
        let sh = rr_sdf(px, c.f0.x, c.f0.y, c.f0.z, c.f0.w, c.f1.x);
        let outline = c.f7.w + 0.5;
        if (c.f4.w > 0.0 && sh.d > -0.5 && sh.d < outline) {
            let k = clamp(sh.d + 0.5, 0.0, 1.0) * clamp(outline - sh.d, 0.0, 1.0)
                * c.f4.w * (0.3 + 0.7 * abs(sh.n.x)) * c.f3.y * m;
            col = round(col * (1.0 - k));
        }
        let cov = clamp(0.5 - sh.d, 0.0, 1.0) * c.f3.y * m;
        if (cov > 0.0) {
            let d = max(-sh.d, 0.0);
            let lens_w = c.f1.z;
            let t = max(1.0 - d / lens_w, 0.0);
            let disp = c.f1.y * t * t;
            let l = px - sh.n * disp - vec2<f32>(f32(c.i2.x), f32(c.i2.y)) - 0.5;
            var v = bilinear(blurred, c.i1.z, c.i1.w, l.x, l.y) * 255.0;
            let lum = 0.2126 * v.r + 0.7152 * v.g + 0.0722 * v.b;
            v = clamp((vec3<f32>(lum) + (v - vec3<f32>(lum)) * c.f2.x) * c.f2.y + vec3<f32>(c.f2.z), vec3<f32>(0.0), vec3<f32>(255.0));
            if (c.f3.z > 0.0) {
                v = mix(v, c.f4.rgb, c.f3.z);
            }
            let rim_w = c.f1.w;
            if (d < rim_w + 1.0) {
                let dir = select((sh.n.x + sh.n.y) * -0.70710678, sh.n.y, c.f3.w != 0.0);
                let k = clamp(1.0 - d / rim_w, 0.0, 1.0) * c.f2.w * (1.0 - c.f3.x + c.f3.x * abs(dir));
                v = mix(v, vec3<f32>(255.0), k);
            }
            col = mix(col, v, cov);
            a = 1.0;
        }
    }

    // The node's own shadow, source-over.
    if ((flags & 4) != 0) {
        let gx = x - c.i4.x;
        let gy = y - c.i4.y;
        let f = c.i5.y;
        if (gx >= 0 && gy >= 0 && gx < c.i4.z * f && gy < c.i4.w * f) {
            let sa = grid_at(own, c.i4, f, x, y) * c.f6.w;
            col = c.f6.rgb * sa * m + col * (1.0 - sa * m);
            a = sa * m + a * (1.0 - sa * m);
        }
    }

    // Scroll edge: veil toward the background colour with a progressive blur.
    if ((flags & 8) != 0 && y < c.i2.z) {
        let row = params[2 * c.i1.y + y];
        let f = c.i2.w;
        var d = col;
        if (row.y > 0.0) {
            let u = (f32(x) + 0.5) / f32(f) - 0.5;
            let v = (f32(y) + 0.5) / f32(f) - 0.5;
            d = mix(d, bilinear(blurred, c.i5.z, c.i5.w, u, v) * 255.0, row.y);
        }
        let v = clamp(mix(d, c.f7.rgb, row.x) - vec3<f32>(row.z * 255.0), vec3<f32>(0.0), vec3<f32>(255.0));
        col = mix(d, v, m);
        col = mix(col, vec3<f32>(select(0.0, 255.0, row.w > 0.0)), abs(row.w) * m);
    }

    var rgb = vec3<f32>(0.0);
    if (a > 0.0) { rgb = col / a; }
    textureStore(out, vec2<i32>(x, y), vec4<f32>(rgb / 255.0, a));
}
