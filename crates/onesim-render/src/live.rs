//! Time-driven content of a laid-out scene that runs on its own, as UIKit runs it: a spinning
//! `UIActivityIndicatorView` / refresh control and animated GIF / WebP playback. Each starts when its view first
//! appears; [`Live::apply`] sets what the scene shows at the session clock.

use crate::scene::{Glyph, Scene};
use std::collections::HashMap;
use std::sync::Arc;

/// One spoke step of UIActivityIndicatorView: 8 steps a second, clockwise.
pub const SPINNER_STEP_MS: f64 = 125.0;

/// When each live view appeared (session clock, ms), so its animation runs from there.
#[derive(Debug, Default)]
pub struct Live {
    shown: HashMap<String, Run>,
}

/// One live view's clock: when it started, and where a paused one stopped.
#[derive(Debug, Clone)]
struct Run {
    start: f64,
    paused_at: Option<f64>,
    /// The animated image's file; another file in the same view plays from its start.
    source: Option<Arc<str>>,
}

impl Live {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance spinners and animated images in `scene` to `clock_ms`. Returns whether any of them is still moving
    /// (the caller keeps ticking); a view that went away starts over when it comes back.
    pub fn apply(&mut self, scene: &mut Scene, clock_ms: f64) -> bool {
        let mut moving = false;
        let mut seen = Vec::new();
        for n in &mut scene.nodes {
            let spins = matches!(n.glyph, Some((Glyph::Spinner { spins: true, .. } | Glyph::SpinnerLarge { spins: true, .. }, _)));
            let plays = n.image.as_ref().is_some_and(|i| i.frames.is_some());
            if !spins && !plays {
                continue;
            }
            let frames = n.image.as_ref().and_then(|i| i.frames.as_ref());
            let paused = frames.is_some_and(|f| f.paused);
            let source = frames.map(|f| f.source.clone());
            let fresh = Run { start: clock_ms, paused_at: None, source: source.clone() };
            let run = self.shown.entry(n.id.clone()).or_insert_with(|| fresh.clone());
            if run.source != source {
                *run = fresh;
            }
            // A pause freezes the view's time; resuming carries on from there.
            match (paused, run.paused_at) {
                (true, None) => run.paused_at = Some(clock_ms),
                (false, Some(at)) => {
                    run.start += clock_ms - at;
                    run.paused_at = None;
                }
                _ => {}
            }
            let t = (run.paused_at.unwrap_or(clock_ms) - run.start).max(0.0);
            seen.push(n.id.clone());
            if let Some((Glyph::Spinner { step, spins: true } | Glyph::SpinnerLarge { step, spins: true }, _)) = n.glyph.as_mut() {
                *step = ((t / SPINNER_STEP_MS).floor() as u64 % 8) as u8;
                moving = true;
            }
            if let Some(img) = n.image.as_mut() {
                if let Some(f) = &img.frames {
                    let (i, running) = f.animation.frame_at(t);
                    moving |= running && !f.paused;
                    if let Some(frame) = f.animation.frame(i) {
                        let frame = f.tint.and_then(|c| crate::layout::common::tinted(&frame, c)).unwrap_or(frame);
                        img.pixmap = f.tile.and_then(|(w, h)| crate::layout::common::tiled(&frame, w, h)).unwrap_or(frame);
                    }
                }
            }
        }
        if seen.len() != self.shown.len() {
            self.shown.retain(|id, _| seen.contains(id));
        }
        moving
    }

    /// Forget every start time (a reload or a new tree generation that replaced the views).
    pub fn reset(&mut self) {
        self.shown.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, ImageFrames, ImageRun, Node, Rect};

    fn spinner(id: &str, spins: bool) -> Node {
        let mut n = Node::new(id, "spinner");
        n.glyph = Some((Glyph::Spinner { step: 0, spins }, Color::rgba(0.5, 0.5, 0.5, 1.0)));
        n
    }

    fn step(s: &Scene, i: usize) -> u8 {
        match s.nodes[i].glyph {
            Some((Glyph::Spinner { step, .. } | Glyph::SpinnerLarge { step, .. }, _)) => step,
            _ => panic!("no spinner"),
        }
    }

    fn scene(nodes: Vec<Node>) -> Scene {
        Scene { size: (100.0, 100.0), appearance: onesim_protocol::Appearance::Light, ios: onesim_protocol::IosVersion::Ios27, nodes, unsupported_symbols: vec![], notes: vec![] }
    }

    #[test]
    fn spinner_steps_clockwise_eight_times_a_second_from_when_it_appears() {
        let mut live = Live::new();
        let mut s = scene(vec![spinner("a", true)]);
        assert!(live.apply(&mut s, 1000.0));
        assert_eq!(step(&s, 0), 0, "starts at its first frame");
        for (t, want) in [(1124.0, 0), (1125.0, 1), (1250.0, 2), (1875.0, 7), (2000.0, 0), (2130.0, 1)] {
            let mut s = scene(vec![spinner("a", true)]);
            live.apply(&mut s, t);
            assert_eq!(step(&s, 0), want, "at {t} ms");
        }
    }

    #[test]
    fn stopped_spinner_rests_and_does_not_keep_the_scene_live() {
        let mut live = Live::new();
        let mut s = scene(vec![spinner("a", false)]);
        assert!(!live.apply(&mut s, 5000.0));
        assert_eq!(step(&s, 0), 0);
    }

    #[test]
    fn a_view_that_comes_back_starts_over() {
        let mut live = Live::new();
        live.apply(&mut scene(vec![spinner("a", true)]), 0.0);
        live.apply(&mut scene(vec![Node::new("other", "view")]), 300.0);
        let mut s = scene(vec![spinner("a", true)]);
        live.apply(&mut s, 400.0);
        assert_eq!(step(&s, 0), 0);
    }

    fn gif(id: &str) -> Node {
        gif_at(id, "onesim-live-anim8.gif")
    }

    fn gif_at(id: &str, file: &str) -> Node {
        let path = std::env::temp_dir().join(file);
        std::fs::write(&path, include_bytes!("../tests/fixtures/anim8.gif")).unwrap();
        let path = path.to_str().unwrap();
        let mut n = Node::new(id, "image");
        n.image = Some(ImageRun {
            pixmap: crate::image::load(path).unwrap(),
            dest: Rect { x: 0.0, y: 0.0, w: 8.0, h: 8.0 },
            rotation: 0.0,
            filter: 2,
            frames: Some(ImageFrames { source: Arc::from(path), animation: crate::image::animation(path).unwrap(), tint: None, tile: None, paused: false }),
        });
        n
    }

    fn green_at(s: &Scene, x: u32) -> u8 {
        s.nodes[0].image.as_ref().unwrap().pixmap.pixel(x, 1).unwrap().green()
    }

    #[test]
    fn animated_image_shows_the_frame_due_then_rests_after_its_loops() {
        let mut live = Live::new();
        let mut s = scene(vec![gif("g")]);
        assert!(live.apply(&mut s, 0.0));
        assert_eq!(green_at(&s, 1), 0, "frame 0 is red");
        let mut s = scene(vec![gif("g")]);
        assert!(live.apply(&mut s, 150.0));
        assert_eq!(green_at(&s, 1), 255, "frame 1 is green on the left");
        let mut s = scene(vec![gif("g")]);
        assert!(!live.apply(&mut s, 1800.0), "played three times: done");
        assert_eq!(s.nodes[0].image.as_ref().unwrap().pixmap.pixel(1, 1).unwrap().blue(), 255, "rests on the last (blue) frame");
    }

    #[test]
    fn tinted_animation_tints_every_frame() {
        let mut live = Live::new();
        let mut n = gif("g");
        n.image.as_mut().unwrap().frames.as_mut().unwrap().tint = Some(Color::rgba(0.0, 0.0, 1.0, 1.0));
        let mut s = scene(vec![n]);
        live.apply(&mut s, 150.0);
        let p = s.nodes[0].image.as_ref().unwrap().pixmap.pixel(1, 1).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 255));
        assert!(Arc::strong_count(&s.nodes[0].image.as_ref().unwrap().pixmap) >= 1);
    }

    #[test]
    fn a_paused_animation_holds_its_frame_and_resumes_from_it() {
        let mut live = Live::new();
        let at = |live: &mut Live, t: f64, paused: bool| {
            let mut n = gif("g");
            n.image.as_mut().unwrap().frames.as_mut().unwrap().paused = paused;
            let mut s = scene(vec![n]);
            let moving = live.apply(&mut s, t);
            (green_at(&s, 1), moving)
        };
        assert_eq!(at(&mut live, 0.0, false), (0, true));
        assert_eq!(at(&mut live, 150.0, true), (255, false), "stopped on frame 1, nothing to tick");
        assert_eq!(at(&mut live, 900.0, true), (255, false), "still frame 1 later");
        assert_eq!(at(&mut live, 920.0, false).0, 255, "resumes where it stopped (150 ms in)");
        assert_eq!(at(&mut live, 1060.0, false).0, 255, "frame 1 runs to 300 ms of its own time");
        assert_eq!(at(&mut live, 1080.0, false).0, 0, "then frame 2 (blue)");
        let mut held = Live::new();
        assert_eq!(at(&mut held, 500.0, true), (0, false), "autoplay off: the first frame");
    }

    #[test]
    fn a_new_source_in_the_same_view_plays_from_its_first_frame() {
        let mut live = Live::new();
        live.apply(&mut scene(vec![gif("g")]), 0.0);
        let mut s = scene(vec![gif("g")]);
        assert!(!live.apply(&mut s, 1800.0), "the first animation finished");
        let mut s = scene(vec![gif_at("g", "onesim-live-anim8-b.gif")]);
        assert!(live.apply(&mut s, 1810.0), "the replacement runs");
        assert_eq!(green_at(&s, 1), 0, "from its first (red) frame");
        let mut s = scene(vec![gif_at("g", "onesim-live-anim8-b.gif")]);
        live.apply(&mut s, 1960.0);
        assert_eq!(green_at(&s, 1), 255, "and keeps its own clock across redraws");
    }

}
