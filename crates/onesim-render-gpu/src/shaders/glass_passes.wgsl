// Backdrop helper passes for Liquid Glass and the scroll edge (see ../glass.rs).
// Colours are sRGB-encoded values in 0..1, blurred as encoded values exactly like the CPU reference.
// Every entry point reads `src` (a sampled texture) and writes `dst` (storage); `p` is a dynamic-offset uniform.

struct P {
    i: array<vec4<i32>, 4>,
    f: array<vec4<f32>, 4>,
};

@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba16float, write>;

fn fold(v: i32, lo: i32, hi: i32) -> i32 {
    let n = hi - lo;
    let n2 = 2 * n;
    let m = ((v - lo) % n2 + n2) % n2;
    return lo + select(n2 - 1 - m, m, m < n);
}

// i[0] = (x0, y0, w, h) page rect copied; i[1] = (lox, hix, loy, hiy) mirror fold bounds; i[2].x = mirror.
// Copies the page region (padded, optionally mirrored into the shape) as straight colour.
@compute @workgroup_size(8, 8)
fn snapshot(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = p.i[0].z;
    let h = p.i[0].w;
    if (i32(g.x) >= w || i32(g.y) >= h) { return; }
    var px = p.i[0].x + i32(g.x);
    var py = p.i[0].y + i32(g.y);
    if (p.i[2].x != 0) {
        px = fold(px, p.i[1].x, p.i[1].y);
        py = fold(py, p.i[1].z, p.i[1].w);
    }
    let dims = vec2<i32>(textureDimensions(src));
    var c = vec3<f32>(0.0);
    if (px >= 0 && py >= 0 && px < dims.x && py < dims.y) {
        c = textureLoad(src, vec2<i32>(px, py), 0).rgb;
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(c, 1.0));
}

// i[0] = (bw, bh, f, rows), i[1] = (pw, ph, 0, 0): mean of the f x f blocks of the top `rows` rows, premultiplied.
@compute @workgroup_size(8, 8)
fn edge_down(@builtin(global_invocation_id) g: vec3<u32>) {
    let bw = p.i[0].x;
    let bh = p.i[0].y;
    if (i32(g.x) >= bw || i32(g.y) >= bh) { return; }
    let f = p.i[0].z;
    let rows = p.i[0].w;
    let pw = p.i[1].x;
    let bx = i32(g.x);
    let by = i32(g.y);
    var acc = vec3<f32>(0.0);
    var n = 0.0;
    for (var yy = by * f; yy < min((by + 1) * f, rows); yy++) {
        for (var xx = bx * f; xx < min((bx + 1) * f, pw); xx++) {
            let c = textureLoad(src, vec2<i32>(xx, yy), 0);
            acc += c.rgb * c.a;
            n += 1.0;
        }
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc / max(n, 1.0), 1.0));
}

// i[0] = (w, h, r, 0): mean over [i - r, i + r], edge samples repeated.
@compute @workgroup_size(8, 8)
fn blur_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = p.i[0].x;
    let h = p.i[0].y;
    if (i32(g.x) >= w || i32(g.y) >= h) { return; }
    let r = p.i[0].z;
    var acc = vec3<f32>(0.0);
    for (var k = -r; k <= r; k++) {
        acc += textureLoad(src, vec2<i32>(clamp(i32(g.x) + k, 0, w - 1), i32(g.y)), 0).rgb;
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc / f32(2 * r + 1), 1.0));
}

@compute @workgroup_size(8, 8)
fn blur_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = p.i[0].x;
    let h = p.i[0].y;
    if (i32(g.x) >= w || i32(g.y) >= h) { return; }
    let r = p.i[0].z;
    var acc = vec3<f32>(0.0);
    for (var k = -r; k <= r; k++) {
        acc += textureLoad(src, vec2<i32>(i32(g.x), clamp(i32(g.y) + k, 0, h - 1)), 0).rgb;
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc / f32(2 * r + 1), 1.0));
}

// Signed distance (negative inside) to a rounded rect.
fn sdf(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r0: f32) -> f32 {
    let r = max(min(r0, min(w / 2.0, h / 2.0)), 0.0);
    let d = vec2<f32>(px - (x + w / 2.0), py - (y + h / 2.0));
    let q = abs(d) - vec2<f32>(w / 2.0 - r, h / 2.0 - r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// i[0] = (gw, gh, f, k); f[0] = (gx0, gy0, shape x, shape y); f[1] = (shape w, shape h, shape r, 0).
// Coverage of the (offset) shape averaged over f x f cells of a coarse grid whose origin is page pixel (gx0, gy0).
@compute @workgroup_size(8, 8)
fn shadow_cov(@builtin(global_invocation_id) g: vec3<u32>) {
    let gw = p.i[0].x;
    let gh = p.i[0].y;
    if (i32(g.x) >= gw || i32(g.y) >= gh) { return; }
    let f = f32(p.i[0].z);
    let k = p.i[0].w;
    let step = f / f32(k);
    var acc = 0.0;
    for (var a = 0; a < k; a++) {
        for (var b = 0; b < k; b++) {
            let px = p.f[0].x + f32(g.x) * f + (f32(a) + 0.5) * step;
            let py = p.f[0].y + f32(g.y) * f + (f32(b) + 0.5) * step;
            let sd = sdf(px, py, p.f[0].z, p.f[0].w, p.f[1].x, p.f[1].y, p.f[1].z);
            acc += clamp(0.5 - sd, 0.0, 1.0);
        }
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc / f32(k * k), 0.0, 0.0, 1.0));
}

// i[0] = (w, h, r, 0), f[0] = (sigma, a, a2, inv_sum): exact gaussian via recurrence, taps outside count as zero.
@compute @workgroup_size(8, 8)
fn gauss_h(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = p.i[0].x;
    let h = p.i[0].y;
    if (i32(g.x) >= w || i32(g.y) >= h) { return; }
    let r = p.i[0].z;
    let a = p.f[0].y;
    let a2 = p.f[0].z;
    let inv_sum = p.f[0].w;
    let gx = i32(g.x);
    let gy = i32(g.y);
    var acc = textureLoad(src, vec2<i32>(gx, gy), 0).r;
    var wt = 1.0;
    var step = a;
    for (var k = 1; k <= r; k++) {
        wt *= step;
        step *= a2;
        let xl = gx - k;
        let xr = gx + k;
        if (xl >= 0) { acc += wt * textureLoad(src, vec2<i32>(xl, gy), 0).r; }
        if (xr < w) { acc += wt * textureLoad(src, vec2<i32>(xr, gy), 0).r; }
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc * inv_sum, 0.0, 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn gauss_v(@builtin(global_invocation_id) g: vec3<u32>) {
    let w = p.i[0].x;
    let h = p.i[0].y;
    if (i32(g.x) >= w || i32(g.y) >= h) { return; }
    let r = p.i[0].z;
    let a = p.f[0].y;
    let a2 = p.f[0].z;
    let inv_sum = p.f[0].w;
    let gx = i32(g.x);
    let gy = i32(g.y);
    var acc = textureLoad(src, vec2<i32>(gx, gy), 0).r;
    var wt = 1.0;
    var step = a;
    for (var k = 1; k <= r; k++) {
        wt *= step;
        step *= a2;
        let yt = gy - k;
        let yb = gy + k;
        if (yt >= 0) { acc += wt * textureLoad(src, vec2<i32>(gx, yt), 0).r; }
        if (yb < h) { acc += wt * textureLoad(src, vec2<i32>(gx, yb), 0).r; }
    }
    textureStore(dst, vec2<i32>(g.xy), vec4<f32>(acc * inv_sum, 0.0, 0.0, 1.0));
}
