//! Image decoding for React Native `Image` and expo-image (PNG via tiny-skia, JPEG via zune-jpeg, GIF via `gif`,
//! WebP via `image-webp`), cached by path. Animated GIF / WebP keep every frame composited to the full canvas
//! ([`Animation`]); [`load`] is the first frame.

use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tiny_skia::{Pixmap, PremultipliedColorU8};

const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECODED_ENTRIES: usize = 1024;
/// Frames of one animation kept decoded; longer animations decode the rest again as they play.
const MAX_ANIMATION_BYTES: usize = 256 * 1024 * 1024;
/// iOS (RCTAnimatedImage, ImageIO) shows a frame whose delay is under 20 ms for 100 ms.
const MIN_DELAY_MS: f64 = 20.0;
const DEFAULT_DELAY_MS: f64 = 100.0;

#[derive(Default)]
struct Images {
    // Browser registrations supply the file itself, so retain its encoded bytes for future loads.
    registered: HashMap<String, Arc<[u8]>>,
    decoded: HashMap<Arc<str>, Decoded>,
    oldest: VecDeque<Arc<str>>,
    decoded_bytes: usize,
}

#[derive(Clone)]
struct Decoded {
    first: Arc<Pixmap>,
    animation: Option<Arc<Animation>>,
}

impl Images {
    fn insert(&mut self, path: &str, d: Decoded) {
        if let Some(previous) = self.decoded.remove(path) {
            self.decoded_bytes -= previous.first.data().len();
            self.oldest.retain(|p| p.as_ref() != path);
        }
        let bytes = d.first.data().len();
        if bytes > MAX_DECODED_BYTES {
            return;
        }
        while self.decoded_bytes + bytes > MAX_DECODED_BYTES || self.decoded.len() >= MAX_DECODED_ENTRIES {
            let oldest = self.oldest.pop_front().expect("decoded cache order");
            self.decoded_bytes -= self.decoded.remove(&oldest).expect("decoded cache entry").first.data().len();
        }
        let path: Arc<str> = Arc::from(path);
        self.oldest.push_back(path.clone());
        self.decoded.insert(path, d);
        self.decoded_bytes += bytes;
    }
}

fn cache() -> MutexGuard<'static, Images> {
    static CACHE: OnceLock<Mutex<Images>> = OnceLock::new();
    CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// The frames of an animated GIF or WebP, played the way iOS plays them.
pub struct Animation {
    bytes: Arc<[u8]>,
    format: Format,
    size: (u32, u32),
    /// How long each frame shows (ms).
    pub delays: Vec<f64>,
    /// Plays in all (from the GIF's NETSCAPE block or the WebP loop count); `None` loops forever. Afterwards the last
    /// frame stays.
    pub loops: Option<u32>,
    frames: Mutex<Frames>,
}

impl std::fmt::Debug for Animation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Animation").field("size", &self.size).field("delays", &self.delays).field("loops", &self.loops).finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Format {
    Gif,
    WebP,
}

/// Decoded frames (those that fit the budget) and a sequential decoder for the rest.
struct Frames {
    kept: Vec<Option<Arc<Pixmap>>>,
    stream: Option<Stream>,
}

impl Animation {
    pub fn len(&self) -> usize {
        self.delays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.delays.is_empty()
    }

    /// One play (ms).
    pub fn duration(&self) -> f64 {
        self.delays.iter().sum()
    }

    /// Frame shown `ms` after the image appeared, and whether the animation is still running then.
    pub fn frame_at(&self, ms: f64) -> (usize, bool) {
        let one = self.duration();
        if one <= 0.0 || self.len() < 2 {
            return (0, false);
        }
        let ms = ms.max(0.0);
        if let Some(n) = self.loops {
            if ms >= one * f64::from(n) {
                return (self.len() - 1, false);
            }
        }
        let mut t = ms % one;
        for (i, d) in self.delays.iter().enumerate() {
            if t < *d {
                return (i, true);
            }
            t -= d;
        }
        (self.len() - 1, true)
    }

    /// Frame `i`, composited onto the canvas the way the frames before it left it.
    pub fn frame(&self, i: usize) -> Option<Arc<Pixmap>> {
        let mut f = self.frames.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(Some(p)) = f.kept.get(i) {
            return Some(p.clone());
        }
        if f.stream.as_ref().is_none_or(|s| s.next > i) {
            f.stream = Some(Stream::new(&self.bytes, self.format)?);
        }
        let s = f.stream.as_mut()?;
        loop {
            let (p, _) = s.next_frame()?;
            if s.next - 1 == i {
                return Some(Arc::new(p));
            }
        }
    }
}

/// A GIF or WebP decoder that composites frame after frame.
struct Stream {
    next: usize,
    inner: StreamKind,
}

enum StreamKind {
    Gif { d: Box<gif::Decoder<Cursor<Arc<[u8]>>>>, canvas: Vec<u8>, size: (u32, u32) },
    WebP { d: Box<image_webp::WebPDecoder<Cursor<Arc<[u8]>>>>, buf: Vec<u8>, size: (u32, u32) },
}

impl Stream {
    fn new(bytes: &Arc<[u8]>, format: Format) -> Option<Self> {
        let inner = match format {
            Format::Gif => {
                let mut o = gif::DecodeOptions::new();
                o.set_color_output(gif::ColorOutput::RGBA);
                let d = o.read_info(Cursor::new(bytes.clone())).ok()?;
                let size = (u32::from(d.width()), u32::from(d.height()));
                StreamKind::Gif { canvas: vec![0; (size.0 * size.1 * 4) as usize], d: Box::new(d), size }
            }
            Format::WebP => {
                let d = image_webp::WebPDecoder::new(Cursor::new(bytes.clone())).ok()?;
                let size = d.dimensions();
                StreamKind::WebP { buf: vec![0; d.output_buffer_size()?], d: Box::new(d), size }
            }
        };
        Some(Self { next: 0, inner })
    }

    /// The next composited frame and its delay (ms, before the minimum-delay rule).
    fn next_frame(&mut self) -> Option<(Pixmap, f64)> {
        let out = match &mut self.inner {
            StreamKind::Gif { d, canvas, size } => {
                let f = d.read_next_frame().ok()??;
                let (w, h) = *size;
                let (fx, fy, fw, fh) = (u32::from(f.left), u32::from(f.top), u32::from(f.width), u32::from(f.height));
                let saved = (f.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
                for row in 0..fh {
                    let y = fy + row;
                    if y >= h {
                        break;
                    }
                    for col in 0..fw {
                        let x = fx + col;
                        let s = ((row * fw + col) * 4) as usize;
                        let Some(px) = f.buffer.get(s..s + 4) else { break };
                        // A transparent pixel of a frame leaves what is under it.
                        if x < w && px[3] != 0 {
                            let o = ((y * w + x) * 4) as usize;
                            canvas[o..o + 4].copy_from_slice(px);
                        }
                    }
                }
                let delay = f64::from(f.delay) * 10.0;
                let dispose = f.dispose;
                let p = pixmap_rgba(w, h, canvas)?;
                match dispose {
                    gif::DisposalMethod::Background => {
                        for row in fy..(fy + fh).min(h) {
                            for col in fx..(fx + fw).min(w) {
                                let o = ((row * w + col) * 4) as usize;
                                canvas[o..o + 4].fill(0);
                            }
                        }
                    }
                    gif::DisposalMethod::Previous => *canvas = saved.expect("saved canvas"),
                    _ => {}
                }
                (p, delay)
            }
            StreamKind::WebP { d, buf, size } => {
                let delay = f64::from(d.read_frame(buf).ok()?);
                let (w, h) = *size;
                let p = if d.has_alpha() { pixmap_rgba(w, h, buf)? } else { pixmap_rgb(w, h, buf)? };
                (p, delay)
            }
        };
        self.next += 1;
        Some(out)
    }
}

fn pixmap_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w, h)?;
    for (px, c) in pm.pixels_mut().iter_mut().zip(rgba.as_chunks::<4>().0) {
        *px = tiny_skia::ColorU8::from_rgba(c[0], c[1], c[2], c[3]).premultiply();
    }
    Some(pm)
}

fn pixmap_rgb(w: u32, h: u32, rgb: &[u8]) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w, h)?;
    for (px, c) in pm.pixels_mut().iter_mut().zip(rgb.as_chunks::<3>().0) {
        *px = PremultipliedColorU8::from_rgba(c[0], c[1], c[2], 255)?;
    }
    Some(pm)
}

fn ios_delay(ms: f64) -> f64 {
    if ms < MIN_DELAY_MS { DEFAULT_DELAY_MS } else { ms }
}

/// Every frame of an animated image (first frame, animation); a single-frame GIF/WebP is static.
fn decode_animated(bytes: &Arc<[u8]>, format: Format) -> Option<Decoded> {
    let mut s = Stream::new(bytes, format)?;
    let (size, loops) = match &s.inner {
        // As ImageIO reports it (and RCTAnimatedImage plays it on iOS 14+): a NETSCAPE loop count of 0 loops forever,
        // N plays N + 1 times, and a GIF without the block plays once (`gif` reads those as Infinite / Finite(N) /
        // Finite(0)).
        StreamKind::Gif { size, d, .. } => (*size, match d.repeat() {
            gif::Repeat::Infinite => None,
            gif::Repeat::Finite(n) => Some(u32::from(n) + 1),
        }),
        StreamKind::WebP { size, d, .. } => {
            if !d.is_animated() {
                let mut d = image_webp::WebPDecoder::new(Cursor::new(bytes.clone())).ok()?;
                let mut buf = vec![0; d.output_buffer_size()?];
                d.read_image(&mut buf).ok()?;
                let (w, h) = d.dimensions();
                let p = if d.has_alpha() { pixmap_rgba(w, h, &buf)? } else { pixmap_rgb(w, h, &buf)? };
                return Some(Decoded { first: Arc::new(p), animation: None });
            }
            (*size, match d.loop_count() {
                image_webp::LoopCount::Forever => None,
                image_webp::LoopCount::Times(n) => Some(u32::from(n.get())),
            })
        }
    };
    let (mut kept, mut delays, mut bytes_kept) = (vec![], vec![], 0usize);
    while let Some((p, delay)) = s.next_frame() {
        delays.push(ios_delay(delay));
        bytes_kept += p.data().len();
        kept.push((bytes_kept <= MAX_ANIMATION_BYTES).then(|| Arc::new(p)));
    }
    let first = kept.first()?.clone()?;
    if kept.len() < 2 {
        return Some(Decoded { first, animation: None });
    }
    let animation = Animation { bytes: bytes.clone(), format, size, delays, loops, frames: Mutex::new(Frames { kept, stream: None }) };
    Some(Decoded { first, animation: Some(Arc::new(animation)) })
}

fn decode_entry(bytes: &Arc<[u8]>) -> Option<Decoded> {
    if bytes.starts_with(b"GIF8") {
        return decode_animated(bytes, Format::Gif);
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return decode_animated(bytes, Format::WebP);
    }
    decode(bytes).map(|p| Decoded { first: Arc::new(p), animation: None })
}

/// Decode PNG, JPEG, GIF or WebP bytes (sniffed by magic number) into a premultiplied pixmap (the first frame).
pub fn decode(bytes: &[u8]) -> Option<Pixmap> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut d = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
        let rgb = d.decode().ok()?;
        let (w, h) = d.dimensions()?;
        pixmap_rgb(w as u32, h as u32, &rgb)
    } else if bytes.starts_with(b"GIF8") || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP") {
        let d = decode_entry(&Arc::from(bytes))?;
        Some(Arc::unwrap_or_clone(d.first))
    } else {
        Pixmap::decode_png(bytes).ok()
    }
}

fn load_entry(path: &str) -> Option<Decoded> {
    loop {
        let source = {
            let images = cache();
            if let Some(p) = images.decoded.get(path) {
                return Some(p.clone());
            }
            images.registered.get(path).cloned()
        };
        let decoded = match &source {
            Some(bytes) => decode_entry(bytes),
            None => std::fs::read(path).ok().and_then(|bytes| decode_entry(&Arc::from(bytes))),
        };
        let mut images = cache();
        if let Some(p) = images.decoded.get(path) {
            return Some(p.clone());
        }
        // A concurrent put may replace the source while decoding outside the lock.
        match (source.as_ref(), images.registered.get(path)) {
            (None, None) => (),
            (Some(old), Some(current)) if Arc::ptr_eq(old, current) => (),
            _ => continue,
        }
        let decoded = decoded?;
        images.insert(path, decoded.clone());
        return Some(decoded);
    }
}

/// Decoded image at `path` (cached; failures are retried). For an animation, its first frame.
pub fn load(path: &str) -> Option<Arc<Pixmap>> {
    load_entry(path).map(|d| d.first)
}

/// The frames of the animated GIF / WebP at `path`; `None` for a still image (or one that does not decode).
pub fn animation(path: &str) -> Option<Arc<Animation>> {
    load_entry(path).and_then(|d| d.animation)
}

/// Provide the image at `path` from memory (the browser has no files: docs/web.md). False if it does not decode.
pub fn put(path: &str, bytes: &[u8]) -> bool {
    let bytes: Arc<[u8]> = Arc::from(bytes);
    let Some(d) = decode_entry(&bytes) else { return false };
    let mut images = cache();
    images.registered.insert(path.to_string(), bytes);
    images.insert(path, d);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = p.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    fn anim(bytes: &[u8]) -> Arc<Animation> {
        decode_entry(&Arc::from(bytes)).and_then(|d| d.animation).expect("animated")
    }

    #[test]
    fn decodes_jpeg_and_png() {
        let jpg = decode(include_bytes!("../tests/fixtures/red8.jpg")).expect("jpeg");
        assert_eq!((jpg.width(), jpg.height()), (8, 8));
        let p = jpg.pixel(4, 4).unwrap();
        assert!(p.red() > 230 && p.green() < 25 && p.blue() < 25, "{p:?}");
        let png = jpg.encode_png().unwrap();
        assert_eq!(decode(&png).map(|p| p.width()), Some(8));
        assert!(decode(b"nope").is_none());
    }

    #[test]
    fn decodes_still_gif_with_transparency() {
        let g = decode(include_bytes!("../tests/fixtures/half8.gif")).expect("gif");
        assert_eq!((g.width(), g.height()), (8, 8));
        assert_eq!(rgba(&g, 1, 1), [255, 0, 0, 255]);
        assert_eq!(rgba(&g, 6, 6)[3], 0, "transparent half");
        assert!(decode_entry(&Arc::from(&include_bytes!("../tests/fixtures/half8.gif")[..])).unwrap().animation.is_none());
    }

    #[test]
    fn decodes_still_webp_lossless_and_lossy() {
        let r = decode(include_bytes!("../tests/fixtures/red8.webp")).expect("lossless webp");
        assert_eq!((r.width(), r.height()), (8, 8));
        assert_eq!(rgba(&r, 3, 3), [255, 0, 0, 255]);
        let b = decode(include_bytes!("../tests/fixtures/blue8-lossy.webp")).expect("lossy webp");
        let [cr, cg, cb, ca] = rgba(&b, 4, 4);
        assert!(cr < 30 && cg < 30 && cb > 220 && ca == 255, "{:?}", [cr, cg, cb, ca]);
        assert!(decode(b"RIFF\0\0\0\0WEBPjunk").is_none());
    }

    #[test]
    fn animated_gif_composites_frames_and_keeps_delays_and_loops() {
        let a = anim(include_bytes!("../tests/fixtures/anim8.gif"));
        assert_eq!(a.len(), 3);
        assert_eq!(a.delays, vec![100.0, 200.0, 300.0]);
        assert_eq!(a.loops, Some(3), "NETSCAPE loop count 2: three plays, as ImageIO reports it");
        // Frame 2 changes only the left half: the right half still shows frame 1's red.
        let f1 = a.frame(1).unwrap();
        assert_eq!(rgba(&f1, 1, 1), [0, 255, 0, 255]);
        assert_eq!(rgba(&f1, 6, 6), [255, 0, 0, 255]);
        assert_eq!(rgba(&a.frame(2).unwrap(), 6, 6), [0, 0, 255, 255]);
        assert_eq!(rgba(&a.frame(0).unwrap(), 1, 1), [255, 0, 0, 255]);
    }

    #[test]
    fn animation_timeline_follows_delays_then_rests_on_the_last_frame() {
        let a = anim(include_bytes!("../tests/fixtures/anim8.gif"));
        assert_eq!(a.duration(), 600.0);
        assert_eq!(a.frame_at(0.0), (0, true));
        assert_eq!(a.frame_at(99.0), (0, true));
        assert_eq!(a.frame_at(100.0), (1, true));
        assert_eq!(a.frame_at(350.0), (2, true));
        assert_eq!(a.frame_at(650.0), (0, true), "second play");
        assert_eq!(a.frame_at(1300.0), (1, true), "third play");
        assert_eq!(a.frame_at(1800.0), (2, false), "three plays, then the last frame stays");
        assert_eq!(a.frame_at(1e9), (2, false));
        assert_eq!(a.frame_at(-5.0), (0, true));
    }

    #[test]
    fn animated_webp_frames_and_forever_loop() {
        let a = anim(include_bytes!("../tests/fixtures/anim8.webp"));
        assert_eq!(a.len(), 2);
        assert_eq!(a.delays, vec![50.0, 150.0]);
        assert_eq!(a.loops, None);
        assert_eq!(rgba(&a.frame(0).unwrap(), 2, 2), [255, 0, 0, 255]);
        let [r, g, _, al] = rgba(&a.frame(1).unwrap(), 2, 2);
        assert!(g > 200 && r < 40 && al == 128, "half-transparent green frame: {:?}", [r, g, al]);
        assert_eq!(a.frame_at(1e6 + 60.0), (1, true), "loops forever");
    }

    #[test]
    fn short_frame_delays_show_for_100ms_like_ios() {
        assert_eq!(ios_delay(0.0), 100.0);
        assert_eq!(ios_delay(10.0), 100.0);
        assert_eq!(ios_delay(20.0), 20.0);
        assert_eq!(ios_delay(70.0), 70.0);
    }

    #[test]
    fn frames_past_the_budget_decode_again_in_order() {
        let a = anim(include_bytes!("../tests/fixtures/anim8.gif"));
        a.frames.lock().unwrap().kept = vec![None; 3]; // as if nothing fit the budget
        assert_eq!(rgba(&a.frame(2).unwrap(), 6, 6), [0, 0, 255, 255]);
        assert_eq!(rgba(&a.frame(1).unwrap(), 6, 6), [255, 0, 0, 255], "restarts the decoder for an earlier frame");
        assert_eq!(rgba(&a.frame(1).unwrap(), 1, 1), [0, 255, 0, 255]);
        assert!(a.frame(3).is_none());
    }

    #[test]
    fn loads_animation_by_path_and_registered_bytes() {
        let path = std::env::temp_dir().join("onesim-anim-test.gif");
        std::fs::write(&path, include_bytes!("../tests/fixtures/anim8.gif")).unwrap();
        let p = path.to_str().unwrap();
        assert_eq!(load(p).map(|f| f.width()), Some(8));
        assert_eq!(animation(p).map(|a| a.len()), Some(3));
        assert!(put("registered-anim.webp", include_bytes!("../tests/fixtures/anim8.webp")));
        assert_eq!(animation("registered-anim.webp").map(|a| a.len()), Some(2));
        assert!(put("registered-still.webp", include_bytes!("../tests/fixtures/red8.webp")));
        assert!(animation("registered-still.webp").is_none());
        assert!(load("registered-still.webp").is_some());
    }

    #[test]
    fn reloads_registered_image_after_eviction() {
        const PATH: &str = "registered-image-eviction-test.jpg";
        let jpeg = include_bytes!("../tests/fixtures/red8.jpg");
        assert!(put(PATH, jpeg));
        let first = load(PATH).unwrap();
        let mut images = cache();
        for i in 0..MAX_DECODED_ENTRIES {
            images.insert(&format!("image-eviction-test-{i}"), Decoded { first: first.clone(), animation: None });
        }
        assert!(!images.decoded.contains_key(PATH));
        drop(images);

        let reloaded = load(PATH).unwrap();
        assert!(!Arc::ptr_eq(&first, &reloaded));
        assert_eq!(first.data(), reloaded.data());
        assert!(!put(PATH, b"invalid replacement"));
        assert!(Arc::ptr_eq(&reloaded, &load(PATH).unwrap()));
    }
}
