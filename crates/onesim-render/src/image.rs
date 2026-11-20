//! Image decoding for React Native `Image` (PNG via tiny-skia, JPEG via zune-jpeg), cached by path.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tiny_skia::Pixmap;

const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECODED_ENTRIES: usize = 1024;

#[derive(Default)]
struct Images {
    // Browser registrations supply the file itself, so retain its encoded bytes for future loads.
    registered: HashMap<String, Arc<[u8]>>,
    decoded: HashMap<Arc<str>, Arc<Pixmap>>,
    oldest: VecDeque<Arc<str>>,
    decoded_bytes: usize,
}

impl Images {
    fn insert(&mut self, path: &str, pixmap: Arc<Pixmap>) {
        if let Some(previous) = self.decoded.remove(path) {
            self.decoded_bytes -= previous.data().len();
            self.oldest.retain(|p| p.as_ref() != path);
        }
        let bytes = pixmap.data().len();
        if bytes > MAX_DECODED_BYTES {
            return;
        }
        while self.decoded_bytes + bytes > MAX_DECODED_BYTES || self.decoded.len() >= MAX_DECODED_ENTRIES {
            let oldest = self.oldest.pop_front().expect("decoded cache order");
            self.decoded_bytes -= self.decoded.remove(&oldest).expect("decoded cache entry").data().len();
        }
        let path: Arc<str> = Arc::from(path);
        self.oldest.push_back(path.clone());
        self.decoded.insert(path, pixmap);
        self.decoded_bytes += bytes;
    }
}

fn cache() -> MutexGuard<'static, Images> {
    static CACHE: OnceLock<Mutex<Images>> = OnceLock::new();
    CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
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
    loop {
        let source = {
            let images = cache();
            if let Some(p) = images.decoded.get(path) {
                return Some(p.clone());
            }
            images.registered.get(path).cloned()
        };
        let pixmap = match &source {
            Some(bytes) => decode(bytes),
            None => std::fs::read(path).ok().and_then(|bytes| decode(&bytes)),
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
        let pixmap = Arc::new(pixmap?);
        images.insert(path, pixmap.clone());
        return Some(pixmap);
    }
}

/// Provide the image at `path` from memory (the browser has no files: docs/web.md). False if it does not decode.
pub fn put(path: &str, bytes: &[u8]) -> bool {
    let Some(pm) = decode(bytes) else { return false };
    let mut images = cache();
    images.registered.insert(path.to_string(), Arc::from(bytes));
    images.insert(path, Arc::new(pm));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reloads_registered_image_after_eviction() {
        const PATH: &str = "registered-image-eviction-test.jpg";
        let jpeg = include_bytes!("../tests/fixtures/red8.jpg");
        assert!(put(PATH, jpeg));
        let first = load(PATH).unwrap();
        let mut images = cache();
        for i in 0..MAX_DECODED_ENTRIES {
            images.insert(&format!("image-eviction-test-{i}"), first.clone());
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
