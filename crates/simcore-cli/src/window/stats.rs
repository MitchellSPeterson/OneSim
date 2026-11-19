//! `SIMCORE_FRAME_STATS=1`: one line per second with frame counts and per-stage percentiles.

use std::time::Duration;

/// Stage durations of one drawn frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Host input handling and animation ticks since the previous frame (guest round trips).
    pub guest: Duration,
    /// Tree to laid-out scene.
    pub scene: Duration,
    /// Scene to display list (GPU backend; zero on CPU).
    pub list: Duration,
    /// Display list to GPU work, or the CPU raster.
    pub exec: Duration,
    /// Swap-chain acquire (vsync wait) and present, or the softbuffer copy.
    pub present: Duration,
}

impl Frame {
    pub fn total(&self) -> Duration {
        self.guest + self.scene + self.list + self.exec + self.present
    }
}

#[derive(Default)]
pub struct Stats {
    frames: Vec<Frame>,
    /// Frames drawn although nothing asked for one (an OS-initiated redraw): must stay zero while idle.
    idle_frames: u64,
    /// Event-loop wakeups of any kind this second.
    wakeups: u64,
    /// Draw attempts that showed nothing (window hidden, acquire timeout).
    skipped: u64,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// p-th percentile (0..=1) of `v`, nearest rank.
pub fn percentile(v: &mut [Duration], p: f64) -> Duration {
    if v.is_empty() {
        return Duration::ZERO;
    }
    v.sort_unstable();
    v[((v.len() as f64 * p).ceil() as usize).clamp(1, v.len()) - 1]
}

impl Stats {
    pub fn wake(&mut self) {
        self.wakeups += 1;
    }

    pub fn skip(&mut self) {
        self.skipped += 1;
    }

    pub fn frame(&mut self, f: Frame, idle: bool) {
        self.frames.push(f);
        self.idle_frames += u64::from(idle);
    }

    /// The report line for the elapsed second; resets the counters.
    pub fn report(&mut self, backend: &str) -> String {
        let frames = std::mem::take(&mut self.frames);
        let col = |get: fn(&Frame) -> Duration| {
            let mut v: Vec<Duration> = frames.iter().map(get).collect();
            format!("{:.1}/{:.1}", ms(percentile(&mut v.clone(), 0.5)), ms(percentile(&mut v, 0.95)))
        };
        let line = format!(
            "frames={} idle_frames={} skipped={} wakeups={} backend={backend} ms p50/p95: total {} guest {} scene {} list {} exec {} present {}",
            frames.len(),
            self.idle_frames,
            self.skipped,
            self.wakeups,
            col(Frame::total),
            col(|f| f.guest),
            col(|f| f.scene),
            col(|f| f.list),
            col(|f| f.exec),
            col(|f| f.present),
        );
        self.idle_frames = 0;
        self.wakeups = 0;
        self.skipped = 0;
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut v: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&mut v, 0.5), Duration::from_millis(50));
        assert_eq!(percentile(&mut v, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&mut [], 0.5), Duration::ZERO);
    }

    #[test]
    fn report_counts_and_resets() {
        let mut s = Stats::default();
        s.wake();
        s.frame(Frame { exec: Duration::from_millis(4), ..Frame::default() }, false);
        s.frame(Frame::default(), true);
        let r = s.report("gpu");
        assert!(r.starts_with("frames=2 idle_frames=1 skipped=0 wakeups=1 backend=gpu"), "{r}");
        assert!(s.report("gpu").starts_with("frames=0 idle_frames=0 skipped=0 wakeups=0"));
    }
}
