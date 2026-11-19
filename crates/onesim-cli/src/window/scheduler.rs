//! Host-owned frame scheduler: pure bookkeeping, no window or clock of its own (callers pass `now`).
//!
//! Rendering is on demand. Anything that changes what is on screen calls [`Scheduler::invalidate`]; the event
//! loop asks [`Scheduler::poll`] when it is about to sleep. Invalidations coalesce into one draw, draws are at most
//! one per refresh interval, and with nothing dirty and nothing animating the answer is [`Poll::Idle`]: the loop
//! sleeps until the next OS event.

use std::time::{Duration, Instant};

/// Why the picture may be stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Input,
    Resize,
    Appearance,
    Renderer,
    /// The guest committed a tree with this version. Ignored when a frame at least that new was already drawn.
    Tree(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    /// Draw now.
    Draw,
    /// Dirty or animating, but the last draw was too recent: wake at this time.
    WaitUntil(Instant),
    /// Nothing to do: sleep until an event.
    Idle,
}

#[derive(Debug)]
pub struct Scheduler {
    dirty: bool,
    animating: bool,
    last_draw: Option<Instant>,
    drawn_tree: u64,
    interval: Duration,
}

impl Scheduler {
    /// `interval`: the display's refresh period (the most frequent draw rate). The first frame is due at once.
    pub fn new(interval: Duration) -> Self {
        Self { dirty: true, animating: false, last_draw: None, drawn_tree: 0, interval }
    }

    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// Mark the picture stale. Returns whether it was news (a stale guest tree is not).
    pub fn invalidate(&mut self, why: Reason) -> bool {
        if matches!(why, Reason::Tree(v) if v <= self.drawn_tree) {
            return false;
        }
        self.dirty = true;
        true
    }

    /// Momentum or a bounce is in flight: keep drawing every interval until told otherwise.
    pub fn set_animating(&mut self, on: bool) {
        self.animating = on;
    }

    pub fn animating(&self) -> bool {
        self.animating
    }

    /// Whether a draw is wanted at all (ignoring the rate limit).
    pub fn wants_frame(&self) -> bool {
        self.dirty || self.animating
    }

    pub fn poll(&self, now: Instant) -> Poll {
        if !self.wants_frame() {
            return Poll::Idle;
        }
        match self.last_draw {
            Some(t) if now.saturating_duration_since(t) < self.interval => Poll::WaitUntil(t + self.interval),
            _ => Poll::Draw,
        }
    }

    /// Record a finished draw of a tree at `version`. Clears the dirty flag, so everything invalidated before this
    /// call is served by this frame.
    pub fn drawn(&mut self, now: Instant, version: u64) {
        self.dirty = false;
        self.last_draw = Some(now);
        self.drawn_tree = self.drawn_tree.max(version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ60: Duration = Duration::from_micros(16_667);

    #[test]
    fn first_frame_is_due_then_idle() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        assert_eq!(s.poll(t0), Poll::Draw);
        s.drawn(t0, 1);
        assert_eq!(s.poll(t0), Poll::Idle);
        // However long we wait, nothing is asked for.
        assert_eq!(s.poll(t0 + Duration::from_secs(3)), Poll::Idle);
    }

    #[test]
    fn invalidations_coalesce_into_one_draw() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        s.drawn(t0, 1);
        for r in [Reason::Input, Reason::Input, Reason::Resize, Reason::Appearance, Reason::Renderer, Reason::Tree(2)] {
            assert!(s.invalidate(r));
        }
        let t1 = t0 + Duration::from_millis(40);
        assert_eq!(s.poll(t1), Poll::Draw);
        s.drawn(t1, 2);
        assert_eq!(s.poll(t1), Poll::Idle);
    }

    #[test]
    fn at_most_one_draw_per_interval() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        s.drawn(t0, 1);
        s.invalidate(Reason::Input);
        let soon = t0 + Duration::from_millis(5);
        assert_eq!(s.poll(soon), Poll::WaitUntil(t0 + HZ60));
        assert_eq!(s.poll(t0 + HZ60), Poll::Draw);
    }

    #[test]
    fn stale_trees_do_not_redraw() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        s.drawn(t0, 5);
        assert!(!s.invalidate(Reason::Tree(5)));
        assert!(!s.invalidate(Reason::Tree(3)));
        assert_eq!(s.poll(t0 + Duration::from_secs(1)), Poll::Idle);
        assert!(s.invalidate(Reason::Tree(6)));
        assert_eq!(s.poll(t0 + Duration::from_secs(1)), Poll::Draw);
    }

    #[test]
    fn animation_draws_every_interval_until_it_stops() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        s.drawn(t0, 1);
        s.set_animating(true);
        let mut now = t0;
        for _ in 0..5 {
            assert_eq!(s.poll(now), Poll::WaitUntil(now + HZ60));
            now += HZ60;
            assert_eq!(s.poll(now), Poll::Draw);
            s.drawn(now, 1);
        }
        s.set_animating(false);
        // The final at-rest frame is drawn by the caller in the same step that learns the animation ended.
        assert_eq!(s.poll(now + HZ60), Poll::Idle);
    }

    #[test]
    fn invalidation_during_a_draw_survives_it() {
        let t0 = Instant::now();
        let mut s = Scheduler::new(HZ60);
        s.drawn(t0, 1);
        s.invalidate(Reason::Input);
        s.drawn(t0 + HZ60, 1);
        // An input that lands after the frame was drawn is not lost.
        s.invalidate(Reason::Input);
        assert_eq!(s.poll(t0 + 2 * HZ60), Poll::Draw);
    }
}
