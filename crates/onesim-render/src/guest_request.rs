//! Work a guest hands to its host because it needs the host's image decoders or fonts (protocol section H,
//! `request` / `response`): expo-image's `generateBlurhashAsync` / `generateThumbhashAsync` and expo-font's
//! `renderToImageAsync`. The native host and the browser host both answer through [`handle`].

use crate::scene::{Color, Node, Rect, Scene, TextRun};
use crate::text::{FontSpec, Fonts, TextAlign};
use serde_json::{json, Value};
use tiny_skia::Pixmap;

/// Answer `method` with `params`; `Err` is the message the guest rejects with.
pub fn handle(fonts: &mut Fonts, method: &str, params: &Value) -> Result<Value, String> {
    let path = || params.get("path").and_then(Value::as_str).ok_or("no image path");
    match method {
        "image.blurhash" => {
            let n = |k: &str| params.get(k).and_then(Value::as_u64).unwrap_or(4) as usize;
            let (x, y) = (n("x"), n("y"));
            if !(1..=9).contains(&x) || !(1..=9).contains(&y) {
                return Err(format!("blurhash components must be 1...9, got {x}x{y}"));
            }
            let pm = crate::image::load(path()?).ok_or("cannot decode the image")?;
            Ok(json!(blurhash(&pm, x, y)))
        }
        "image.thumbhash" => {
            let pm = crate::image::load(path()?).ok_or("cannot decode the image")?;
            Ok(json!(base64(&thumbhash(&pm))))
        }
        "text.render" => render_text(fonts, params),
        "image.decode_hash" => {
            let uri = params.get("uri").and_then(Value::as_str).ok_or("no hash uri")?;
            let pm = decode_hash(uri, params).ok_or_else(|| format!("Unable to generate an image from the given hash {uri:?}"))?;
            Ok(json!({ "png": base64(&pm.encode_png().map_err(|e| e.to_string())?), "width": pm.width(), "height": pm.height() }))
        }
        _ => Err(format!("unknown request {method:?}")),
    }
}

/// Swift's `Float.pi`, which (unlike `f32::consts::PI`) is rounded toward zero: the encoders below match expo's Swift
/// ones bit for bit only with it.
const SWIFT_PI: f32 = f32::from_bits(0x4049_0fda);

// ---- blurhash (expo-image ios/Utils/Blurhash.swift: the full-size premultiplied sRGB bitmap) ----

fn srgb_to_linear(v: u8) -> f32 {
    let v = f32::from(v) / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(v: f32) -> i64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 { (v * 12.92 * 255.0 + 0.5) as i64 } else { ((1.055 * v.powf(1.0 / 2.4) - 0.055) * 255.0 + 0.5) as i64 }
}

fn sign_pow(v: f32, e: f32) -> f32 {
    v.abs().powf(e).copysign(v)
}

const B83: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

fn encode83(v: i64, len: u32, out: &mut String) {
    for i in 1..=len {
        out.push(B83[((v / 83i64.pow(len - i)) % 83) as usize] as char);
    }
}

/// The blurhash of `pm` with `cx` x `cy` components, as expo-image computes it on iOS.
pub fn blurhash(pm: &Pixmap, cx: usize, cy: usize) -> String {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let px = pm.data();
    let lin: Vec<[f32; 3]> = px.as_chunks::<4>().0.iter().map(|p| [srgb_to_linear(p[0]), srgb_to_linear(p[1]), srgb_to_linear(p[2])]).collect();
    let mut factors = Vec::with_capacity(cx * cy);
    for j in 0..cy {
        for i in 0..cx {
            let norm = if i == 0 && j == 0 { 1.0 } else { 2.0 };
            let mut acc = [0f32; 3];
            for x in 0..w {
                for y in 0..h {
                    let basis = norm * (SWIFT_PI * i as f32 * x as f32 / w as f32).cos() * (SWIFT_PI * j as f32 * y as f32 / h as f32).cos();
                    let c = lin[y * w + x];
                    for k in 0..3 {
                        acc[k] += basis * c[k];
                    }
                }
            }
            let s = 1.0 / (w * h) as f32;
            factors.push([acc[0] * s, acc[1] * s, acc[2] * s]);
        }
    }
    let mut hash = String::new();
    encode83(((cx - 1) + (cy - 1) * 9) as i64, 1, &mut hash);
    let ac = &factors[1..];
    let max = if ac.is_empty() {
        encode83(0, 1, &mut hash);
        1.0
    } else {
        let actual = ac.iter().flat_map(|f| f.iter().map(|v| v.abs())).fold(0f32, f32::max);
        let q = (actual * 166.0 - 0.5).floor().clamp(0.0, 82.0) as i64;
        encode83(q, 1, &mut hash);
        (q + 1) as f32 / 166.0
    };
    let dc = factors[0];
    encode83((linear_to_srgb(dc[0]) << 16) + (linear_to_srgb(dc[1]) << 8) + linear_to_srgb(dc[2]), 4, &mut hash);
    for f in ac {
        let q = |v: f32| (sign_pow(v / max, 0.5) * 9.0 + 9.5).floor().clamp(0.0, 18.0) as i64;
        encode83(q(f[0]) * 19 * 19 + q(f[1]) * 19 + q(f[2]), 2, &mut hash);
    }
    hash
}

// ---- thumbhash (expo-image ios/Utils/Thumbhash.swift: drawn into at most 100 x 100, unpremultiplied) ----

/// `pm` scaled to fit 100 x 100 (bilinear; iOS draws with Core Graphics), as unpremultiplied RGBA.
fn thumb_pixels(pm: &Pixmap) -> (usize, usize, Vec<u8>) {
    let (sw, sh) = (pm.width() as f64, pm.height() as f64);
    let m = sw.max(sh);
    let w = ((100.0 * sw / m).round() as u32).max(1);
    let h = ((100.0 * sh / m).round() as u32).max(1);
    let mut out = Pixmap::new(w, h).expect("thumb size");
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    out.draw_pixmap(0, 0, pm.as_ref(), &paint, tiny_skia::Transform::from_scale(w as f32 / sw as f32, h as f32 / sh as f32), None);
    let mut rgba = out.data().to_vec();
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = (u16::from(*c) * 255 / a).min(255) as u8;
            }
        }
    }
    (w as usize, h as usize, rgba)
}

/// The ThumbHash of `pm`.
pub fn thumbhash(pm: &Pixmap) -> Vec<u8> {
    let (w, h, rgba) = thumb_pixels(pm);
    rgba_to_thumbhash(w, h, &rgba)
}

fn rgba_to_thumbhash(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let (mut ar, mut ag, mut ab, mut aa) = (0f32, 0f32, 0f32, 0f32);
    for p in rgba.as_chunks::<4>().0 {
        let a = f32::from(p[3]) / 255.0;
        ar += a / 255.0 * f32::from(p[0]);
        ag += a / 255.0 * f32::from(p[1]);
        ab += a / 255.0 * f32::from(p[2]);
        aa += a;
    }
    if aa > 0.0 {
        (ar, ag, ab) = (ar / aa, ag / aa, ab / aa);
    }
    let has_alpha = aa < (w * h) as f32;
    let l_limit = if has_alpha { 5 } else { 7 };
    let max_wh = w.max(h) as f32;
    let lx = ((l_limit * w) as f32 / max_wh).round().max(1.0) as usize;
    let ly = ((l_limit * h) as f32 / max_wh).round().max(1.0) as usize;
    let mut lpqa = vec![0f32; w * h * 4];
    for (o, p) in lpqa.as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_chunks::<4>().0) {
        let a = f32::from(p[3]) / 255.0;
        let r = ar * (1.0 - a) + a / 255.0 * f32::from(p[0]);
        let g = ag * (1.0 - a) + a / 255.0 * f32::from(p[1]);
        let b = ab * (1.0 - a) + a / 255.0 * f32::from(p[2]);
        o.copy_from_slice(&[(r + g + b) / 3.0, (r + g) / 2.0 - b, r - g, a]);
    }
    let channel = |c: usize, nx: usize, ny: usize| -> (f32, Vec<f32>, f32) {
        let (mut dc, mut ac, mut scale) = (0f32, vec![], 0f32);
        for cy in 0..ny {
            let mut cx = 0;
            while cx * ny < nx * (ny - cy) {
                let mut f = 0f32;
                for y in 0..h {
                    let fy = (SWIFT_PI / h as f32 * cy as f32 * (y as f32 + 0.5)).cos();
                    for x in 0..w {
                        let fx = (SWIFT_PI / w as f32 * cx as f32 * (x as f32 + 0.5)).cos();
                        f += lpqa[(y * w + x) * 4 + c] * fx * fy;
                    }
                }
                f /= (w * h) as f32;
                if cx > 0 || cy > 0 {
                    ac.push(f);
                    scale = scale.max(f.abs());
                } else {
                    dc = f;
                }
                cx += 1;
            }
        }
        if scale > 0.0 {
            for v in &mut ac {
                *v = 0.5 + 0.5 / scale * *v;
            }
        }
        (dc, ac, scale)
    };
    let (l_dc, l_ac, l_scale) = channel(0, lx.max(3), ly.max(3));
    let (p_dc, p_ac, p_scale) = channel(1, 3, 3);
    let (q_dc, q_ac, q_scale) = channel(2, 3, 3);
    let (a_dc, a_ac, a_scale) = if has_alpha { channel(3, 5, 5) } else { (1.0, vec![], 1.0) };
    let landscape = w > h;
    let header24 = ((63.0 * l_dc).round() as u32)
        | (((31.5 + 31.5 * p_dc).round() as u32) << 6)
        | (((31.5 + 31.5 * q_dc).round() as u32) << 12)
        | (((31.0 * l_scale).round() as u32) << 18)
        | (u32::from(has_alpha) << 23);
    let header16 = (if landscape { ly } else { lx }) as u16
        | (((63.0 * p_scale).round() as u16) << 3)
        | (((63.0 * q_scale).round() as u16) << 9)
        | (u16::from(landscape) << 15);
    let mut hash = vec![(header24 & 255) as u8, ((header24 >> 8) & 255) as u8, (header24 >> 16) as u8, (header16 & 255) as u8, (header16 >> 8) as u8];
    if has_alpha {
        hash.push(((15.0 * a_dc).round() as u8) | (((15.0 * a_scale).round() as u8) << 4));
    }
    let mut odd = false;
    for f in l_ac.iter().chain(&p_ac).chain(&q_ac).chain(if has_alpha { a_ac.iter() } else { [].iter() }) {
        let v = (15.0 * f).round() as u8;
        if odd {
            *hash.last_mut().expect("byte") |= v << 4;
        } else {
            hash.push(v);
        }
        odd = !odd;
    }
    hash
}

// ---- placeholders as sources (expo-image ios/Loaders/BlurhashLoader.swift, ThumbhashLoader.swift) ----

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `blurhash:/<hash>` at `width` x `height` (default 16), or `thumbhash:/<base64, '/' as '\\'>` at its own size.
fn decode_hash(uri: &str, p: &Value) -> Option<Pixmap> {
    let (kind, hash) = uri.split_once(":/")?;
    let hash = percent_decode(hash.trim_start_matches('/'));
    match kind {
        "blurhash" => {
            let dim = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| *v >= 1.0).unwrap_or(16.0) as u32;
            decode_blurhash(&hash, dim("width"), dim("height"))
        }
        "thumbhash" => {
            let mut h = hash.replace('\\', "/");
            while !h.len().is_multiple_of(4) {
                h.push('=');
            }
            decode_thumbhash(&unbase64(&h)?)
        }
        _ => None,
    }
}

fn decode83(s: &str) -> Option<i64> {
    s.bytes().try_fold(0i64, |v, c| Some(v * 83 + B83.iter().position(|&x| x == c)? as i64))
}

/// The blurhash's colours over `w` x `h` pixels (opaque).
fn decode_blurhash(hash: &str, w: u32, h: u32) -> Option<Pixmap> {
    if hash.len() < 6 || !hash.is_ascii() {
        return None;
    }
    let size = decode83(&hash[0..1])?;
    let (nx, ny) = ((size % 9 + 1) as usize, (size / 9 + 1) as usize);
    let max = (decode83(&hash[1..2])? + 1) as f32 / 166.0;
    if hash.len() != 4 + 2 * nx * ny {
        return None;
    }
    let mut colors = Vec::with_capacity(nx * ny);
    for i in 0..nx * ny {
        if i == 0 {
            let v = decode83(&hash[2..6])?;
            colors.push([srgb_to_linear((v >> 16) as u8), srgb_to_linear(((v >> 8) & 255) as u8), srgb_to_linear((v & 255) as u8)]);
        } else {
            let v = decode83(&hash[4 + i * 2..6 + i * 2])?;
            let q = |n: i64| sign_pow((n as f32 - 9.0) / 9.0, 2.0) * max;
            colors.push([q(v / (19 * 19)), q((v / 19) % 19), q(v % 19)]);
        }
    }
    let mut pm = Pixmap::new(w, h)?;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut c = [0f32; 3];
            for j in 0..ny {
                for i in 0..nx {
                    let basis = (SWIFT_PI * x as f32 * i as f32 / w as f32).cos() * (SWIFT_PI * y as f32 * j as f32 / h as f32).cos();
                    for k in 0..3 {
                        c[k] += colors[i + j * nx][k] * basis;
                    }
                }
            }
            pm.pixels_mut()[y * w as usize + x] = tiny_skia::PremultipliedColorU8::from_rgba(linear_to_srgb(c[0]) as u8, linear_to_srgb(c[1]) as u8, linear_to_srgb(c[2]) as u8, 255)?;
        }
    }
    Some(pm)
}

/// A ThumbHash's picture (at most 32 x 32, `thumbHashToRGBA`).
fn decode_thumbhash(hash: &[u8]) -> Option<Pixmap> {
    if hash.len() < 5 {
        return None;
    }
    let header24 = u32::from(hash[0]) | (u32::from(hash[1]) << 8) | (u32::from(hash[2]) << 16);
    let header16 = u16::from(hash[3]) | (u16::from(hash[4]) << 8);
    let l_dc = (header24 & 63) as f32 / 63.0;
    let p_dc = ((header24 >> 6) & 63) as f32 / 31.5 - 1.0;
    let q_dc = ((header24 >> 12) & 63) as f32 / 31.5 - 1.0;
    let l_scale = ((header24 >> 18) & 31) as f32 / 31.0;
    let has_alpha = (header24 >> 23) != 0;
    let p_scale = ((header16 >> 3) & 63) as f32 / 63.0;
    let q_scale = ((header16 >> 9) & 63) as f32 / 63.0;
    let landscape = (header16 >> 15) != 0;
    let lx = (if landscape { if has_alpha { 5 } else { 7 } } else { header16 & 7 }).max(3) as usize;
    let ly = (if landscape { header16 & 7 } else if has_alpha { 5 } else { 7 }).max(3) as usize;
    let (mut a_dc, mut a_scale) = (1f32, 1f32);
    if has_alpha {
        let b = *hash.get(5)?;
        a_dc = f32::from(b & 15) / 15.0;
        a_scale = f32::from(b >> 4) / 15.0;
    }
    let start = if has_alpha { 6 } else { 5 };
    let mut index = 0usize;
    let mut channel = |nx: usize, ny: usize, scale: f32| -> Option<Vec<f32>> {
        let mut ac = vec![];
        for cy in 0..ny {
            let mut cx = if cy > 0 { 0 } else { 1 };
            while cx * ny < nx * (ny - cy) {
                let q = (hash.get(start + (index >> 1))? >> ((index & 1) << 2)) & 15;
                ac.push((f32::from(q) / 7.5 - 1.0) * scale);
                index += 1;
                cx += 1;
            }
        }
        Some(ac)
    };
    let l_ac = channel(lx, ly, l_scale)?;
    let p_ac = channel(3, 3, p_scale * 1.25)?;
    let q_ac = channel(3, 3, q_scale * 1.25)?;
    let a_ac = if has_alpha { channel(5, 5, a_scale)? } else { vec![] };
    // thumbHashToApproximateAspectRatio reads the header's own lx / ly (not clamped to 3).
    let (rx, ry) = if landscape { (if has_alpha { 5 } else { 7 }, header16 & 7) } else { (header16 & 7, if has_alpha { 5 } else { 7 }) };
    let ratio = f32::from(rx) / f32::from(ry.max(1));
    let w = (if ratio > 1.0 { 32.0 } else { 32.0 * ratio }).round().max(1.0) as usize;
    let h = (if ratio > 1.0 { 32.0 / ratio } else { 32.0 }).round().max(1.0) as usize;
    let (cx_stop, cy_stop) = (lx.max(if has_alpha { 5 } else { 3 }), ly.max(if has_alpha { 5 } else { 3 }));
    let mut pm = Pixmap::new(w as u32, h as u32)?;
    for y in 0..h {
        for x in 0..w {
            let fx: Vec<f32> = (0..cx_stop).map(|cx| (SWIFT_PI / w as f32 * (x as f32 + 0.5) * cx as f32).cos()).collect();
            let fy: Vec<f32> = (0..cy_stop).map(|cy| (SWIFT_PI / h as f32 * (y as f32 + 0.5) * cy as f32).cos()).collect();
            let (mut l, mut p, mut q, mut a) = (l_dc, p_dc, q_dc, a_dc);
            let mut j = 0;
            for (cy, fyc) in fy.iter().enumerate().take(ly) {
                let fy2 = fyc * 2.0;
                let mut cx = if cy > 0 { 0 } else { 1 };
                while cx * ly < lx * (ly - cy) {
                    l += l_ac[j] * fx[cx] * fy2;
                    j += 1;
                    cx += 1;
                }
            }
            j = 0;
            for (cy, fyc) in fy.iter().enumerate().take(3) {
                let fy2 = fyc * 2.0;
                for fxc in &fx[(if cy > 0 { 0 } else { 1 })..3 - cy] {
                    let f = fxc * fy2;
                    p += p_ac[j] * f;
                    q += q_ac[j] * f;
                    j += 1;
                }
            }
            if has_alpha {
                j = 0;
                for (cy, fyc) in fy.iter().enumerate().take(5) {
                    let fy2 = fyc * 2.0;
                    for fxc in &fx[(if cy > 0 { 0 } else { 1 })..5 - cy] {
                        a += a_ac[j] * fxc * fy2;
                        j += 1;
                    }
                }
            }
            let b = l - 2.0 / 3.0 * p;
            let r = (3.0 * l - b + q) / 2.0;
            let g = r - q;
            let c = |v: f32| (255.0 * v.min(1.0)).max(0.0) as u8;
            pm.pixels_mut()[y * w + x] = tiny_skia::ColorU8::from_rgba(c(r), c(g), c(b), c(a)).premultiply();
        }
    }
    Some(pm)
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = vec![];
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= if c == b'=' { 0 } else { val(c)? } << (18 - 6 * i);
        }
        let bs = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bs[..3 - pad.min(2)]);
    }
    Some(out)
}

fn base64(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for (i, sh) in [18, 12, 6, 0].into_iter().enumerate() {
            s.push(if i <= c.len() { A[((n >> sh) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

// ---- renderToImageAsync (expo-font ios/FontUtilsModule.swift) ----

/// `{text, family?, fonts?: [path], size, color?: 0xAARRGGBB, lineHeight?, scale}` -> `{png (base64), width, height,
/// scale}`: the string at its own size (points), drawn at `scale`. `family` is a family name (as
/// `UIFont.fontNames(forFamilyName:)` takes it) among the loaded font files `fonts`; otherwise the system font.
fn render_text(fonts: &mut Fonts, p: &Value) -> Result<Value, String> {
    let text = p.get("text").and_then(Value::as_str).unwrap_or("");
    let size = p.get("size").and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0).unwrap_or(24.0) as f32;
    let scale = p.get("scale").and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0).unwrap_or(3.0);
    let argb = p.get("color").and_then(Value::as_u64).unwrap_or(0xff00_0000) as u32;
    let ch = |s: u32| ((argb >> s) & 255) as f32 / 255.0;
    let color = Color::rgba(ch(16), ch(8), ch(0), ch(24));
    let family = p.get("family").and_then(Value::as_str).and_then(|name| {
        let paths: Vec<String> = p.get("fonts").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
        paths.iter().find_map(|path| fonts.custom_faces(path).iter().find(|f| f.family == name).map(|f| f.family))
    });
    // UIFont.lineHeight of the system font (the ratio React Native text uses too).
    let natural = ((size * 1.1934 * scale as f32).ceil()) / scale as f32;
    let line = p.get("lineHeight").and_then(Value::as_f64).filter(|l| l.is_finite() && *l > 0.0).map(|l| l as f32);
    let mut spec = FontSpec::new(size, line.unwrap_or(natural), 400, TextAlign::Leading);
    spec.family = family;
    spec.pitch = spec.line_height;
    spec.fixed_pitch = line.is_some();
    let (w, h) = fonts.measure(text, &spec, None, &[]);
    let (w, h) = (w.ceil().max(1.0), h.ceil().max(1.0));
    let mut n = Node::new("text", "text");
    n.bounds = Rect { x: 0.0, y: 0.0, w, h };
    n.text = Some(TextRun { text: text.to_string(), font: spec, color, strike: false, offset_x: 0.0, spans: vec![] });
    let scene = Scene { size: (w, h), appearance: onesim_protocol::Appearance::Light, ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] };
    let png = crate::raster::render_png(&scene, fonts, scale)?;
    Ok(json!({ "png": base64(&png), "width": w, "height": h, "scale": scale }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]));
        pm
    }

    #[test]
    fn blurhash_matches_the_reference_encoder() {
        // A solid colour has only its DC term: size flag, zero max AC, the colour.
        assert_eq!(blurhash(&solid(8, 8, [255, 0, 0, 255]), 1, 1), "00TI:j");
        let h = blurhash(&solid(4, 3, [0, 128, 255, 255]), 4, 3);
        assert_eq!(h.len(), 4 + 2 * 4 * 3, "{h}");
        assert_eq!(&h[..1], "L", "size flag (4 - 1) + (3 - 1) * 9 = 21");
        // A left-to-right ramp puts energy in the first horizontal AC term only.
        let mut ramp = Pixmap::new(16, 4).unwrap();
        for (i, px) in ramp.pixels_mut().iter_mut().enumerate() {
            let v = ((i % 16) * 17) as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(v, v, v, 255).unwrap();
        }
        let r = blurhash(&ramp, 2, 1);
        assert_ne!(&r[1..2], "0", "a non-zero AC maximum: {r}");
    }

    #[test]
    fn thumbhash_encoder_matches_expos_swift_encoder() {
        // The same opaque pixels through expo-image's `rgbaToThumbHash` (ios/Utils/Thumbhash.swift, run with swift).
        let mut px = vec![0u8; 30 * 20 * 4];
        for y in 0..20 {
            for x in 0..30 {
                let i = (y * 30 + x) * 4;
                px[i..i + 4].copy_from_slice(&[((x * 8) % 256) as u8, ((y * 12) % 256) as u8, ((x * y) % 256) as u8, 255]);
            }
        }
        let hex: String = rgba_to_thumbhash(30, 20, &px).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "da080e1d986086777088778678687878f6860ff788");
        for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if i % 30 < 5 {
                p[3] = 128;
            }
        }
        let hex: String = rgba_to_thumbhash(30, 20, &px).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "dc188e1b961e8187809986e5971ff98710428778888878", "with alpha");
    }

    #[test]
    fn thumbhash_header_reflects_size_alpha_and_orientation() {
        let opaque = thumbhash(&solid(200, 100, [10, 200, 30, 255]));
        assert_eq!(opaque[2] >> 7, 0, "no alpha bit");
        assert_eq!(opaque[4] >> 7, 1, "landscape");
        let clear = thumbhash(&solid(50, 100, [0, 0, 0, 0]));
        assert_eq!(clear[2] >> 7, 1, "alpha bit");
        assert_eq!(clear[4] >> 7, 0, "portrait");
        assert!(base64(&opaque).len() >= 8);
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn hashes_decode_back_to_their_colours() {
        // Encode a two-colour picture, decode it, and find the colours again on each side.
        let mut src = Pixmap::new(40, 20).unwrap();
        for (i, px) in src.pixels_mut().iter_mut().enumerate() {
            *px = if i % 40 < 20 { tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 0, 255) } else { tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 255, 255) }.unwrap();
        }
        let bh = blurhash(&src, 4, 3);
        let mut f = Fonts::with_fallback(true);
        let r = handle(&mut f, "image.decode_hash", &json!({"uri": format!("blurhash:/{}", bh.replace('#', "%23").replace('?', "%3F")), "width": 32, "height": 16})).unwrap();
        assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(32), Some(16)));
        let pm = decode_blurhash(&bh, 32, 16).unwrap();
        let (l, rr) = (pm.pixel(2, 8).unwrap(), pm.pixel(29, 8).unwrap());
        assert!(l.red() > 150 && l.blue() < 120 && rr.blue() > 150 && rr.red() < 120, "{l:?} {rr:?}");
        assert!(decode_blurhash("short", 4, 4).is_none());
        let th = thumbhash(&src);
        let pm = decode_thumbhash(&th).unwrap();
        assert!(pm.width() == 32 && pm.height() < 32, "landscape {}x{}", pm.width(), pm.height());
        let (l, rr) = (pm.pixel(2, 4).unwrap(), pm.pixel(29, 4).unwrap());
        assert!(l.red() > 150 && rr.blue() > 150, "{l:?} {rr:?}");
        let uri = format!("thumbhash:/{}", base64(&th).replace('/', "\\").trim_end_matches('='));
        assert!(handle(&mut f, "image.decode_hash", &json!({"uri": uri})).is_ok(), "{uri}");
        assert_eq!(unbase64("TWFu").unwrap(), b"Man");
        assert_eq!(unbase64("TWE=").unwrap(), b"Ma");
    }

    #[test]
    fn requests_check_their_inputs() {
        let mut f = Fonts::with_fallback(true);
        assert!(handle(&mut f, "image.blurhash", &json!({"path": "/nope.png"})).is_err());
        assert!(handle(&mut f, "image.blurhash", &json!({"path": "/nope.png", "x": 10})).unwrap_err().contains("1...9"));
        assert!(handle(&mut f, "nope", &json!({})).is_err());
    }

    #[test]
    fn render_text_draws_the_string_at_its_size() {
        let mut f = Fonts::with_fallback(true);
        let r = handle(&mut f, "text.render", &json!({"text": "Hi", "size": 20, "color": 0xffff0000u32, "scale": 2})).unwrap();
        let (w, h) = (r["width"].as_f64().unwrap(), r["height"].as_f64().unwrap());
        assert!(w > 10.0 && w < 40.0 && (20.0..30.0).contains(&h), "{w}x{h}");
        let tall = handle(&mut f, "text.render", &json!({"text": "Hi", "size": 20, "lineHeight": 40, "scale": 2})).unwrap();
        assert_eq!(tall["height"], 40.0);
        let two = handle(&mut f, "text.render", &json!({"text": "Hi\nthere", "size": 20, "scale": 2})).unwrap();
        assert!(two["height"].as_f64().unwrap() > h * 1.8, "two lines");
        let png = r["png"].as_str().unwrap();
        assert!(png.starts_with("iVBORw0KGgo"), "a PNG");
    }
}
