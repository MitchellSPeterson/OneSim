//! Display list: a [`Scene`](crate::Scene) lowered to paint ops a GPU executor can run without fonts,
//! symbol tables or layout.
//!
//! Text, vector icons and symbols arrive as pre-rasterized **sprites** (premultiplied RGBA resources)
//! so every backend draws exactly the CPU reference's glyphs. Resources are sent once and referenced by
//! id; a builder keeps them alive across frames and lists the ids it dropped.
//!
//! The encoding ([`DisplayList::encode`] / [`DisplayList::decode`]) is a compact little-endian binary
//! format with no dependencies, so it can cross from the `wasm32-wasip1` runtime module (which owns the
//! guest, layout and fonts) to a `wasm32-unknown-unknown` WebGPU module in the browser. All geometry is
//! in **device pixels**; colors are straight-alpha sRGB 0...1 with node opacity already applied.

use crate::glass::{GlassSpec, GlassVariant, ScrollEdge};
use crate::Color;

/// Bumped on any incompatible change to the encoding.
pub const VERSION: u16 = 1;
const MAGIC: &[u8; 4] = b"SCDL";

/// A rounded rectangle in device pixels (`r` = corner radius; 0 = sharp).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
}

/// Bitmap sampling for [`Op::Image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sampling {
    /// Nearest neighbour (sprites, pixel-aligned bitmaps).
    #[default]
    Nearest,
    Bilinear,
    Bicubic,
}

/// A bitmap the list references by id: premultiplied RGBA8, rows top to bottom, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One paint operation, executed in order onto the frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Intersect the clip with an antialiased rounded rect until the matching [`Op::PopClip`].
    PushClip(RRect),
    PopClip,
    Fill { shape: RRect, color: Color },
    /// Stroke centred on `shape`'s outline, miter joins.
    Stroke { shape: RRect, width: f32, color: Color },
    /// Linear gradient from `p0` to `p1` over `shape`; `stops` are `(offset, color)`, interpolated in
    /// straight alpha.
    Gradient { shape: RRect, p0: (f32, f32), p1: (f32, f32), stops: Vec<(f32, Color)> },
    /// Gaussian shadow of `shape` (`sigma` in device px) offset by `(dx, dy)`.
    Shadow { shape: RRect, color: Color, sigma: f32, dx: f32, dy: f32 },
    /// Bitmap `id` mapped by the affine `[a, b, c, d, e, f]` (x' = a x + c y + e, y' = b x + d y + f),
    /// multiplied by `opacity`.
    Image { id: u64, transform: [f32; 6], sampling: Sampling, opacity: f32 },
    /// Bitmap `id` drawn 1:1 with its top-left at whole device pixel `(x, y)`.
    Sprite { id: u64, x: i32, y: i32 },
    /// Liquid Glass over what is painted so far (see `glass.rs`): backdrop sample, its outer shadow, then
    /// the glass, all inside the current clip.
    Glass { shape: RRect, spec: GlassSpec, opacity: f32 },
    /// Scroll-edge progressive blur + veil under a bar (see `glass::draw_edge`).
    Edge(ScrollEdge),
}

impl Op {
    /// Reads previously painted pixels (forces a pass boundary on GPU backends).
    pub fn is_backdrop(&self) -> bool {
        matches!(self, Op::Glass { .. } | Op::Edge(_))
    }
}

/// One frame of paint work plus the resource changes it needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayList {
    /// Frame size in device pixels.
    pub width: u32,
    pub height: u32,
    /// Device pixels per point (backdrop parameters are in points).
    pub scale: f32,
    pub dark: bool,
    /// Resources first used by this frame (the executor keeps them until dropped).
    pub added: Vec<Resource>,
    /// Resource ids no longer referenced; free them before executing `ops`.
    pub dropped: Vec<u64>,
    pub ops: Vec<Op>,
}

// ---------------------------------------------------------------------------------------------- encoding

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u32(u32::try_from(n).expect("display list section over 4G entries"));
    }
    fn color(&mut self, c: Color) {
        [c.r, c.g, c.b, c.a].into_iter().for_each(|v| self.f32(v));
    }
    fn rrect(&mut self, r: RRect) {
        [r.x, r.y, r.w, r.h, r.r].into_iter().for_each(|v| self.f32(v));
    }
    fn opt_f32s(&mut self, v: Option<&[f32]>) {
        match v {
            None => self.u8(0),
            Some(v) => {
                self.u8(1);
                v.iter().for_each(|&x| self.f32(x));
            }
        }
    }
}

struct R<'a>(&'a [u8]);

type Res<T> = Result<T, String>;

impl R<'_> {
    fn take(&mut self, n: usize) -> Res<&[u8]> {
        if self.0.len() < n {
            return Err("display list truncated".into());
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn arr<const N: usize>(&mut self) -> Res<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn i32(&mut self) -> Res<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Res<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn f32(&mut self) -> Res<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    fn bool(&mut self) -> Res<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(format!("bad bool {b}")),
        }
    }
    /// A count, bounded by the bytes left (each entry is at least `min` bytes) so corrupt input cannot
    /// make us allocate unboundedly.
    fn len(&mut self, min: usize) -> Res<usize> {
        let n = self.u32()? as usize;
        if n.saturating_mul(min) > self.0.len() {
            return Err("display list count exceeds its data".into());
        }
        Ok(n)
    }
    fn color(&mut self) -> Res<Color> {
        Ok(Color::rgba(self.f32()?, self.f32()?, self.f32()?, self.f32()?))
    }
    fn rrect(&mut self) -> Res<RRect> {
        Ok(RRect { x: self.f32()?, y: self.f32()?, w: self.f32()?, h: self.f32()?, r: self.f32()? })
    }
    fn opt_f32s<const N: usize>(&mut self) -> Res<Option<[f32; N]>> {
        if !self.bool()? {
            return Ok(None);
        }
        let mut v = [0f32; N];
        for x in &mut v {
            *x = self.f32()?;
        }
        Ok(Some(v))
    }
}

fn encode_spec(w: &mut W, s: &GlassSpec) {
    w.u8(match s.variant {
        GlassVariant::Regular => 0,
        GlassVariant::Clear => 1,
        GlassVariant::Identity => 2,
    });
    let tint = s.tint.map(|c| [c.r, c.g, c.b, c.a]);
    w.opt_f32s(tint.as_ref().map(|v| &v[..]));
    w.u8(s.interactive as u8);
    w.opt_f32s(s.blur.as_ref().map(std::slice::from_ref));
    w.opt_f32s(s.mix.map(|(a, b)| [a, b]).as_ref().map(|v| &v[..]));
    w.opt_f32s(s.shadow.map(|(a, b, c)| [a, b, c]).as_ref().map(|v| &v[..]));
    w.opt_f32s(s.flat.as_ref().map(std::slice::from_ref));
}

fn decode_spec(r: &mut R) -> Res<GlassSpec> {
    let variant = match r.u8()? {
        0 => GlassVariant::Regular,
        1 => GlassVariant::Clear,
        2 => GlassVariant::Identity,
        v => return Err(format!("bad glass variant {v}")),
    };
    Ok(GlassSpec {
        variant,
        tint: r.opt_f32s::<4>()?.map(|[a, b, c, d]| Color::rgba(a, b, c, d)),
        interactive: r.bool()?,
        blur: r.opt_f32s::<1>()?.map(|[v]| v),
        mix: r.opt_f32s::<2>()?.map(|[a, b]| (a, b)),
        shadow: r.opt_f32s::<3>()?.map(|[a, b, c]| (a, b, c)),
        flat: r.opt_f32s::<1>()?.map(|[v]| v),
    })
}

impl DisplayList {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W(Vec::with_capacity(64 + self.ops.len() * 32 + self.added.iter().map(|r| r.rgba.len() + 20).sum::<usize>()));
        w.0.extend_from_slice(MAGIC);
        w.u16(VERSION);
        w.u32(self.width);
        w.u32(self.height);
        w.f32(self.scale);
        w.u8(self.dark as u8);
        w.len(self.added.len());
        for r in &self.added {
            w.u64(r.id);
            w.u32(r.width);
            w.u32(r.height);
            w.len(r.rgba.len());
            w.0.extend_from_slice(&r.rgba);
        }
        w.len(self.dropped.len());
        self.dropped.iter().for_each(|&id| w.u64(id));
        w.len(self.ops.len());
        for op in &self.ops {
            match op {
                Op::PushClip(r) => {
                    w.u8(0);
                    w.rrect(*r);
                }
                Op::PopClip => w.u8(1),
                Op::Fill { shape, color } => {
                    w.u8(2);
                    w.rrect(*shape);
                    w.color(*color);
                }
                Op::Stroke { shape, width, color } => {
                    w.u8(3);
                    w.rrect(*shape);
                    w.f32(*width);
                    w.color(*color);
                }
                Op::Gradient { shape, p0, p1, stops } => {
                    w.u8(4);
                    w.rrect(*shape);
                    [p0.0, p0.1, p1.0, p1.1].into_iter().for_each(|v| w.f32(v));
                    w.len(stops.len());
                    for (o, c) in stops {
                        w.f32(*o);
                        w.color(*c);
                    }
                }
                Op::Shadow { shape, color, sigma, dx, dy } => {
                    w.u8(5);
                    w.rrect(*shape);
                    w.color(*color);
                    [*sigma, *dx, *dy].into_iter().for_each(|v| w.f32(v));
                }
                Op::Image { id, transform, sampling, opacity } => {
                    w.u8(6);
                    w.u64(*id);
                    transform.iter().for_each(|&v| w.f32(v));
                    w.u8(*sampling as u8);
                    w.f32(*opacity);
                }
                Op::Sprite { id, x, y } => {
                    w.u8(7);
                    w.u64(*id);
                    w.i32(*x);
                    w.i32(*y);
                }
                Op::Glass { shape, spec, opacity } => {
                    w.u8(8);
                    w.rrect(*shape);
                    encode_spec(&mut w, spec);
                    w.f32(*opacity);
                }
                Op::Edge(e) => {
                    w.u8(9);
                    w.f32(e.bar);
                    w.color(e.color);
                    w.f32(e.tint);
                }
            }
        }
        w.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut r = R(bytes);
        if r.take(4)? != MAGIC {
            return Err("not a SimCore display list".into());
        }
        let v = r.u16()?;
        if v != VERSION {
            return Err(format!("display list version {v}, expected {VERSION}"));
        }
        let (width, height, scale, dark) = (r.u32()?, r.u32()?, r.f32()?, r.bool()?);
        let n = r.len(20)?;
        let mut added = Vec::with_capacity(n);
        for _ in 0..n {
            let (id, w, h) = (r.u64()?, r.u32()?, r.u32()?);
            let n = r.len(1)?;
            if n as u64 != u64::from(w) * u64::from(h) * 4 {
                return Err(format!("resource {id}: {n} bytes for {w}x{h}"));
            }
            added.push(Resource { id, width: w, height: h, rgba: r.take(n)?.to_vec() });
        }
        let n = r.len(8)?;
        let dropped = (0..n).map(|_| r.u64()).collect::<Res<_>>()?;
        let n = r.len(1)?;
        let mut ops = Vec::with_capacity(n);
        for _ in 0..n {
            ops.push(match r.u8()? {
                0 => Op::PushClip(r.rrect()?),
                1 => Op::PopClip,
                2 => Op::Fill { shape: r.rrect()?, color: r.color()? },
                3 => Op::Stroke { shape: r.rrect()?, width: r.f32()?, color: r.color()? },
                4 => {
                    let shape = r.rrect()?;
                    let (p0, p1) = ((r.f32()?, r.f32()?), (r.f32()?, r.f32()?));
                    let k = r.len(20)?;
                    let stops = (0..k).map(|_| Ok((r.f32()?, r.color()?))).collect::<Res<_>>()?;
                    Op::Gradient { shape, p0, p1, stops }
                }
                5 => Op::Shadow { shape: r.rrect()?, color: r.color()?, sigma: r.f32()?, dx: r.f32()?, dy: r.f32()? },
                6 => {
                    let id = r.u64()?;
                    let mut transform = [0f32; 6];
                    for t in &mut transform {
                        *t = r.f32()?;
                    }
                    let sampling = match r.u8()? {
                        0 => Sampling::Nearest,
                        1 => Sampling::Bilinear,
                        2 => Sampling::Bicubic,
                        s => return Err(format!("bad sampling {s}")),
                    };
                    Op::Image { id, transform, sampling, opacity: r.f32()? }
                }
                7 => Op::Sprite { id: r.u64()?, x: r.i32()?, y: r.i32()? },
                8 => Op::Glass { shape: r.rrect()?, spec: decode_spec(&mut r)?, opacity: r.f32()? },
                9 => Op::Edge(ScrollEdge { bar: r.f32()?, color: r.color()?, tint: r.f32()? }),
                t => return Err(format!("bad op tag {t}")),
            });
        }
        if !r.0.is_empty() {
            return Err(format!("{} trailing bytes after display list", r.0.len()));
        }
        Ok(Self { width, height, scale, dark, added, dropped, ops })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DisplayList {
        let c = Color::rgba(0.1, 0.2, 0.3, 0.4);
        let rr = RRect { x: 1.0, y: 2.0, w: 30.0, h: 40.0, r: 5.0 };
        let spec = GlassSpec { variant: GlassVariant::Clear, tint: Some(c), interactive: true, blur: Some(4.0), mix: None, shadow: Some((0.2, 8.0, 2.0)), flat: None };
        DisplayList {
            width: 120,
            height: 80,
            scale: 3.0,
            dark: true,
            added: vec![Resource { id: 7, width: 2, height: 1, rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] }],
            dropped: vec![3, 4],
            ops: vec![
                Op::PushClip(rr),
                Op::Fill { shape: rr, color: c },
                Op::Stroke { shape: rr, width: 1.5, color: c },
                Op::Gradient { shape: rr, p0: (0.0, 0.0), p1: (0.0, 40.0), stops: vec![(0.0, c), (1.0, c)] },
                Op::Shadow { shape: rr, color: c, sigma: 6.0, dx: 0.0, dy: 2.0 },
                Op::Image { id: 7, transform: [1.0, 0.0, 0.0, 1.0, 5.0, 6.0], sampling: Sampling::Bicubic, opacity: 0.5 },
                Op::Sprite { id: 7, x: -3, y: 9 },
                Op::Glass { shape: rr, spec, opacity: 1.0 },
                Op::Edge(ScrollEdge { bar: 100.0, color: c, tint: 0.5 }),
                Op::PopClip,
            ],
        }
    }

    #[test]
    fn round_trips() {
        let dl = sample();
        assert_eq!(DisplayList::decode(&dl.encode()).unwrap(), dl);
        assert_eq!(dl.ops.iter().filter(|o| o.is_backdrop()).count(), 2);
    }

    #[test]
    fn rejects_corrupt_input() {
        let bytes = sample().encode();
        for cut in [0, 3, 6, bytes.len() / 2, bytes.len() - 1] {
            assert!(DisplayList::decode(&bytes[..cut]).is_err(), "truncated at {cut}");
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(DisplayList::decode(&extra).unwrap_err().contains("trailing"));
        let mut bad = bytes;
        bad[4] = 99;
        assert!(DisplayList::decode(&bad).unwrap_err().contains("version"));
        // A huge resource count must not allocate.
        let mut huge = sample();
        huge.added.clear();
        let mut b = huge.encode();
        b[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(DisplayList::decode(&b).is_err());
    }
}
