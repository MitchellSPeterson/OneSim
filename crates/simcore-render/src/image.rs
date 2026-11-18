//! Image decoding for React Native `Image` (PNG via tiny-skia, JPEG via zune-jpeg), cached by path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::Pixmap;

fn cache() -> &'static Mutex<HashMap<String, Arc<Pixmap>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Pixmap>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Decode PNG or JPEG bytes (sniffed by magic number) into a premultiplied pixmap.
pub fn decode(bytes: &[u8]) -> Option<Pixmap> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut d = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
        let rgb = d.decode().ok()?;
        let (w, h) = d.dimensions()?;
        let mut pm = Pixmap::new(w as u32, h as u32)?;
        for (px, c) in pm.pixels_mut().iter_mut().zip(rgb.as_chunks::<3>().0) {
            *px = tiny_skia::PremultipliedColorU8::from_rgba(c[0], c[1], c[2], 255)?;
        }
        Some(pm)
    } else {
        Pixmap::decode_png(bytes).ok()
    }
}

/// Decoded image at `path` (cached; failures are retried).
pub fn load(path: &str) -> Option<Arc<Pixmap>> {
    if let Some(p) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(path) {
        return Some(p.clone());
    }
    let pm = Arc::new(decode(&std::fs::read(path).ok()?)?);
    cache().lock().unwrap_or_else(|e| e.into_inner()).insert(path.to_string(), pm.clone());
    Some(pm)
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_jpeg_and_png() {
        let jpg = super::decode(include_bytes!("../tests/fixtures/red8.jpg")).expect("jpeg");
        assert_eq!((jpg.width(), jpg.height()), (8, 8));
        let p = jpg.pixel(4, 4).unwrap();
        assert!(p.red() > 230 && p.green() < 25 && p.blue() < 25, "{p:?}");
        let png = jpg.encode_png().unwrap();
        assert_eq!(super::decode(&png).map(|p| p.width()), Some(8));
        assert!(super::decode(b"nope").is_none());
    }
}
