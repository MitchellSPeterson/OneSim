use std::collections::HashMap;
use std::env;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

const CMD_MOVE: u8 = 0;
const CMD_LINE: u8 = 1;
const CMD_QUAD: u8 = 2;
const CMD_CUBIC: u8 = 3;
const CMD_CLOSE: u8 = 4;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/symbols_map.rs");
    println!("cargo:rerun-if-changed=../../assets/symbols/phosphor");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_path = Path::new(&manifest_dir);
    let map_src = fs::read_to_string(manifest_path.join("src/symbols_map.rs"))
        .expect("failed to read src/symbols_map.rs");
    let phosphor_dir = manifest_path.join("../../assets/symbols/phosphor");

    let rows = parse_symbols_map(&map_src);
    let mut bin: Vec<u8> = Vec::with_capacity(256 * 1024);
    let mut cache: HashMap<String, [(u32, u32); 3]> = HashMap::new();
    let mut table_rows: Vec<(String, [(u32, u32); 3])> = Vec::with_capacity(rows.len());

    for (sf_name, phosphor_name) in rows {
        let ranges = if let Some(&cached) = cache.get(&phosphor_name) {
            cached
        } else {
            let paths = [
                phosphor_dir.join(format!("regular/{phosphor_name}.svg")),
                phosphor_dir.join(format!("bold/{phosphor_name}-bold.svg")),
                phosphor_dir.join(format!("fill/{phosphor_name}-fill.svg")),
            ];
            let mut r = [(0u32, 0u32); 3];
            for (i, p) in paths.iter().enumerate() {
                let svg = fs::read_to_string(p)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()));
                let start = bin.len() as u32;
                let mut found_any = false;
                for d in extract_path_ds(&svg) {
                    found_any = true;
                    parse_svg_path(&d, &mut bin);
                }
                assert!(found_any, "no <path d=...> found in {}", p.display());
                let len = (bin.len() as u32) - start;
                r[i] = (start, len);
            }
            cache.insert(phosphor_name, r);
            r
        };
        table_rows.push((sf_name, ranges));
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    fs::write(out_path.join("symbols.bin"), &bin).expect("failed to write symbols.bin");

    let mut table = String::with_capacity(table_rows.len() * 64 + 256);
    table.push_str(
        "pub static SYMBOLS_BIN: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/symbols.bin\"));\n",
    );
    table.push_str("pub static MAP: &[(&str, [(u32, u32); 3])] = &[\n");
    for (sf_name, r) in &table_rows {
        table.push_str(&format!(
            "    ({sf_name:?}, [({}, {}), ({}, {}), ({}, {})]),\n",
            r[0].0, r[0].1, r[1].0, r[1].1, r[2].0, r[2].1
        ));
    }
    table.push_str("];\n");
    fs::write(out_path.join("symbols_table.rs"), table).expect("failed to write symbols_table.rs");
}

fn parse_symbols_map(src: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("row!(") else {
            continue;
        };
        let mut parts = Vec::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            if c == '"' {
                let mut s = String::new();
                for ch in chars.by_ref() {
                    if ch == '"' {
                        break;
                    }
                    s.push(ch);
                }
                parts.push(s);
            }
        }
        if parts.len() == 2 {
            rows.push((parts.remove(0), parts.remove(0)));
        }
    }
    assert!(!rows.is_empty(), "no row!(...) entries found in src/symbols_map.rs");
    rows
}

fn extract_path_ds(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(idx) = rest.find("<path") {
        rest = &rest[idx + 5..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if let Some(d_idx) = tag.find("d=\"") {
            let after = &tag[d_idx + 3..];
            if let Some(q_end) = after.find('"') {
                out.push(after[..q_end].to_string());
            }
        } else if let Some(d_idx) = tag.find("d='") {
            let after = &tag[d_idx + 3..];
            if let Some(q_end) = after.find('\'') {
                out.push(after[..q_end].to_string());
            }
        }
    }
    out
}

fn push_f32(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&(v as f32).to_le_bytes());
}

fn push_move(out: &mut Vec<u8>, x: f64, y: f64) {
    out.push(CMD_MOVE);
    push_f32(out, x);
    push_f32(out, y);
}

fn push_line(out: &mut Vec<u8>, x: f64, y: f64) {
    out.push(CMD_LINE);
    push_f32(out, x);
    push_f32(out, y);
}

fn push_quad(out: &mut Vec<u8>, x1: f64, y1: f64, x: f64, y: f64) {
    out.push(CMD_QUAD);
    push_f32(out, x1);
    push_f32(out, y1);
    push_f32(out, x);
    push_f32(out, y);
}

fn push_cubic(out: &mut Vec<u8>, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
    out.push(CMD_CUBIC);
    push_f32(out, x1);
    push_f32(out, y1);
    push_f32(out, x2);
    push_f32(out, y2);
    push_f32(out, x);
    push_f32(out, y);
}

fn push_close(out: &mut Vec<u8>) {
    out.push(CMD_CLOSE);
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    fn skip_ws_comma(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' | b',' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn has_number(&mut self) -> bool {
        self.skip_ws_comma();
        matches!(self.peek(), Some(b'0'..=b'9' | b'+' | b'-' | b'.'))
    }

    fn read_number(&mut self) -> f64 {
        self.skip_ws_comma();
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let mut has_digits = false;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            has_digits = true;
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                has_digits = true;
                self.pos += 1;
            }
        }
        assert!(has_digits, "expected number at pos {start}");
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let exp_start = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let mut exp_digits = false;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                exp_digits = true;
                self.pos += 1;
            }
            if !exp_digits {
                self.pos = exp_start;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        s.parse::<f64>().unwrap_or_else(|e| panic!("invalid number {s:?}: {e}"))
    }

    fn read_flag(&mut self) -> bool {
        self.skip_ws_comma();
        let b = self.peek().expect("expected arc flag");
        self.pos += 1;
        match b {
            b'0' => false,
            b'1' => true,
            _ => panic!("invalid arc flag {:?}", b as char),
        }
    }
}

fn parse_svg_path(d: &str, out: &mut Vec<u8>) {
    let mut p = Parser::new(d);
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut sx = 0.0_f64;
    let mut sy = 0.0_f64;
    let mut last_cubic_ctrl: Option<(f64, f64)> = None;
    let mut last_quad_ctrl: Option<(f64, f64)> = None;
    let mut cmd = 0u8;

    loop {
        p.skip_ws_comma();
        let Some(b) = p.peek() else { break };
        if b.is_ascii_alphabetic() {
            cmd = b;
            p.pos += 1;
        } else {
            assert!(cmd != 0, "path data started without command");
        }

        match cmd {
            b'M' | b'm' => {
                let rel = cmd == b'm';
                let mut first = true;
                while first || p.has_number() {
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    if first {
                        push_move(out, x, y);
                        sx = x;
                        sy = y;
                        first = false;
                        // Subsequent coordinate pairs are implicit L / l.
                        cmd = if rel { b'l' } else { b'L' };
                    } else {
                        push_line(out, x, y);
                    }
                    cx = x;
                    cy = y;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                }
            }
            b'L' | b'l' => {
                let rel = cmd == b'l';
                loop {
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    push_line(out, x, y);
                    cx = x;
                    cy = y;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'H' | b'h' => {
                let rel = cmd == b'h';
                loop {
                    let mut x = p.read_number();
                    if rel {
                        x += cx;
                    }
                    push_line(out, x, cy);
                    cx = x;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'V' | b'v' => {
                let rel = cmd == b'v';
                loop {
                    let mut y = p.read_number();
                    if rel {
                        y += cy;
                    }
                    push_line(out, cx, y);
                    cy = y;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'C' | b'c' => {
                let rel = cmd == b'c';
                loop {
                    let mut x1 = p.read_number();
                    let mut y1 = p.read_number();
                    let mut x2 = p.read_number();
                    let mut y2 = p.read_number();
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x1 += cx;
                        y1 += cy;
                        x2 += cx;
                        y2 += cy;
                        x += cx;
                        y += cy;
                    }
                    push_cubic(out, x1, y1, x2, y2, x, y);
                    last_cubic_ctrl = Some((x2, y2));
                    last_quad_ctrl = None;
                    cx = x;
                    cy = y;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'S' | b's' => {
                let rel = cmd == b's';
                loop {
                    let (x1, y1) = match last_cubic_ctrl {
                        Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                        None => (cx, cy),
                    };
                    let mut x2 = p.read_number();
                    let mut y2 = p.read_number();
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x2 += cx;
                        y2 += cy;
                        x += cx;
                        y += cy;
                    }
                    push_cubic(out, x1, y1, x2, y2, x, y);
                    last_cubic_ctrl = Some((x2, y2));
                    last_quad_ctrl = None;
                    cx = x;
                    cy = y;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'Q' | b'q' => {
                let rel = cmd == b'q';
                loop {
                    let mut x1 = p.read_number();
                    let mut y1 = p.read_number();
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x1 += cx;
                        y1 += cy;
                        x += cx;
                        y += cy;
                    }
                    push_quad(out, x1, y1, x, y);
                    last_quad_ctrl = Some((x1, y1));
                    last_cubic_ctrl = None;
                    cx = x;
                    cy = y;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'T' | b't' => {
                let rel = cmd == b't';
                loop {
                    let (x1, y1) = match last_quad_ctrl {
                        Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                        None => (cx, cy),
                    };
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    push_quad(out, x1, y1, x, y);
                    last_quad_ctrl = Some((x1, y1));
                    last_cubic_ctrl = None;
                    cx = x;
                    cy = y;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'A' | b'a' => {
                let rel = cmd == b'a';
                loop {
                    let rx = p.read_number();
                    let ry = p.read_number();
                    let x_rot = p.read_number();
                    let large_arc = p.read_flag();
                    let sweep = p.read_flag();
                    let mut x = p.read_number();
                    let mut y = p.read_number();
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    arc_to_cubics(out, cx, cy, rx, ry, x_rot, large_arc, sweep, x, y);
                    cx = x;
                    cy = y;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    if !p.has_number() {
                        break;
                    }
                }
            }
            b'Z' | b'z' => {
                push_close(out);
                cx = sx;
                cy = sy;
                last_cubic_ctrl = None;
                last_quad_ctrl = None;
            }
            other => panic!("unsupported SVG path command {:?}", other as char),
        }
    }
}

/// W3C SVG 1.1 Appendix F.6.5 / F.6.6 elliptical arc to cubic Bezier conversion.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(
    out: &mut Vec<u8>,
    x1: f64,
    y1: f64,
    rx_in: f64,
    ry_in: f64,
    x_axis_rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) {
    if (x1 - x2).abs() < 1e-12 && (y1 - y2).abs() < 1e-12 {
        return;
    }
    let mut rx = rx_in.abs();
    let mut ry = ry_in.abs();
    if rx < 1e-12 || ry < 1e-12 {
        push_line(out, x2, y2);
        return;
    }

    let phi = x_axis_rotation_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    let dx2 = (x1 - x2) / 2.0;
    let dy2 = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    // F.6.6: Ensure radii are large enough.
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let mut rx2 = rx * rx;
    let mut ry2 = ry * ry;
    let lambda = x1p2 / rx2 + y1p2 / ry2;
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
        rx2 = rx * rx;
        ry2 = ry * ry;
    }

    // F.6.5.2: Compute (cx', cy').
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = rx2 * y1p2 + ry2 * x1p2;
    let coef = if den > 1e-18 { (num / den).sqrt() } else { 0.0 }
        * if large_arc == sweep { -1.0 } else { 1.0 };
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);

    // F.6.5.3: Compute (cx, cy).
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;

    // F.6.5.5 / F.6.5.6: Compute theta1 and dtheta.
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = vec_angle(1.0, 0.0, ux, uy);
    let mut dtheta = vec_angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }

    let segs = ((dtheta.abs() / (PI / 2.0)).ceil() as usize).max(1);
    let delta = dtheta / segs as f64;
    let alpha = (4.0 / 3.0) * (delta / 4.0).tan();

    let mut t1 = theta1;
    let (mut sin_t1, mut cos_t1) = t1.sin_cos();
    for i in 0..segs {
        let t2 = if i + 1 == segs { theta1 + dtheta } else { t1 + delta };
        let (sin_t2, cos_t2) = t2.sin_cos();

        let p1x = cos_t1 - alpha * sin_t1;
        let p1y = sin_t1 + alpha * cos_t1;
        let p2x = cos_t2 + alpha * sin_t2;
        let p2y = sin_t2 - alpha * cos_t2;

        let map = |px: f64, py: f64| -> (f64, f64) {
            let ex = rx * px;
            let ey = ry * py;
            (cos_phi * ex - sin_phi * ey + cx, sin_phi * ex + cos_phi * ey + cy)
        };
        let (c1x, c1y) = map(p1x, p1y);
        let (c2x, c2y) = map(p2x, p2y);
        let (ex, ey) = if i + 1 == segs { (x2, y2) } else { map(cos_t2, sin_t2) };
        push_cubic(out, c1x, c1y, c2x, c2y, ex, ey);

        t1 = t2;
        sin_t1 = sin_t2;
        cos_t1 = cos_t2;
    }
}

fn vec_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let dot = ux * vx + uy * vy;
    let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    let cos = if len > 1e-18 { (dot / len).clamp(-1.0, 1.0) } else { 1.0 };
    let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
    sign * cos.acos()
}
