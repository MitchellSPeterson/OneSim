use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

/// System fonts + glyph cache, shared by layout (measuring) and raster (drawing).
pub struct Fonts {
    sys: FontSystem,
    cache: SwashCache,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    pub fn new() -> Self {
        Self { sys: FontSystem::new(), cache: SwashCache::new() }
    }

    fn buffer(&mut self, text: &str, size: f32, line_h: f32, width: Option<f32>) -> Buffer {
        let mut b = Buffer::new(&mut self.sys, Metrics::new(size, line_h));
        b.set_size(width, None);
        b.set_text(text, &Attrs::new().family(Family::SansSerif), Shaping::Advanced, None);
        b.shape_until_scroll(&mut self.sys, false);
        b
    }

    /// Size of `text`, wrapping at `width` when given.
    pub fn measure(&mut self, text: &str, size: f32, line_h: f32, width: Option<f32>) -> (f64, f64) {
        let b = self.buffer(text, size, line_h, width);
        let (mut w, mut lines) = (0f32, 0usize);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            lines += 1;
        }
        (w.ceil() as f64, (lines.max(1) as f32 * line_h) as f64)
    }

    /// Calls `f(x, y, w, h, rgba)` for every glyph pixel run, relative to the text origin.
    pub fn draw(
        &mut self,
        text: &str,
        size: f32,
        line_h: f32,
        width: f32,
        rgba: [u8; 4],
        mut f: impl FnMut(i32, i32, u32, u32, [u8; 4]),
    ) {
        let mut b = self.buffer(text, size, line_h, Some(width));
        let c = Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
        b.draw(&mut self.sys, &mut self.cache, c, |x, y, w, h, c| {
            f(x, y, w, h, [c.r(), c.g(), c.b(), c.a()])
        });
    }
}
