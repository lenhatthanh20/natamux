//! The numbers of phase 1's gate 2: frame times, key to glyph, and output per terminal.
//! Pure: callers pass the time, so tests need no clock.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// The overlay shows this much recent history.
pub const WINDOW: Duration = Duration::from_secs(10);
/// A gap longer than this between frames is an idle window, not a slow frame.
const IDLE_GAP: Duration = Duration::from_millis(250);
/// Key to glyph is over the last this many keys.
const KEY_SAMPLES: usize = 100;
/// The report keeps at most this many frame samples (about 2 hours at 120 fps).
const MAX_SAMPLES: usize = 1_000_000;

#[derive(Default)]
pub struct PerfStats {
    last_frame: Option<Instant>,
    /// (when, frame time) of every frame since the overlay was turned on.
    frames: Vec<(Instant, Duration)>,
    /// Keys sent to each terminal, not echoed yet.
    keys: HashMap<String, Vec<Instant>>,
    /// Keys whose echo came, waiting for the frame that draws it to finish.
    echoed: Vec<Instant>,
    /// Keys in the frame now being drawn.
    drawing: Vec<Instant>,
    key_to_glyph: VecDeque<Duration>,
    /// Bytes written to each terminal's screen, with when.
    output: HashMap<String, VecDeque<(Instant, usize)>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub frames: usize,
    pub p50: Duration,
    pub p99: Duration,
    pub p999: Duration,
    pub over_8ms: usize,
    pub over_16ms: usize,
    pub key_to_glyph_p95: Option<Duration>,
}

impl PerfStats {
    /// A frame starts. The time since the last start is the last frame's time (its draw, and the
    /// wait for the display when it kept up).
    pub fn frame(&mut self, now: Instant) {
        if let Some(last) = self.last_frame {
            let took = now.saturating_duration_since(last);
            if took < IDLE_GAP && self.frames.len() < MAX_SAMPLES {
                self.frames.push((now, took));
            }
        }
        self.last_frame = Some(now);
        // The frame before this one drew the echo of these keys: it is on screen now.
        for key in self.drawing.drain(..) {
            self.key_to_glyph
                .push_back(now.saturating_duration_since(key));
        }
        while self.key_to_glyph.len() > KEY_SAMPLES {
            self.key_to_glyph.pop_front();
        }
        self.drawing = std::mem::take(&mut self.echoed);
    }

    /// The overlay was off: the next frame does not count the time in between.
    pub fn pause(&mut self) {
        self.last_frame = None;
    }

    /// A key pressed at `at` went to `terminal` as input.
    pub fn key(&mut self, terminal: &str, at: Instant) {
        self.keys.entry(terminal.to_string()).or_default().push(at);
    }

    /// A terminal got output: the keys typed into it before are echoed by it.
    pub fn output(&mut self, terminal: &str, bytes: usize, now: Instant) {
        if let Some(keys) = self.keys.get_mut(terminal) {
            self.echoed.append(keys);
        }
        let samples = self.output.entry(terminal.to_string()).or_default();
        samples.push_back((now, bytes));
        while samples
            .front()
            .is_some_and(|(when, _)| now.saturating_duration_since(*when) > Duration::from_secs(1))
        {
            samples.pop_front();
        }
    }

    /// Bytes per second of each terminal over the last second, busiest first.
    pub fn output_rates(&self, now: Instant) -> Vec<(String, f64)> {
        let mut rates: Vec<(String, f64)> = self
            .output
            .iter()
            .map(|(terminal, samples)| {
                let bytes: usize = samples
                    .iter()
                    .filter(|(when, _)| {
                        now.saturating_duration_since(*when) <= Duration::from_secs(1)
                    })
                    .map(|(_, bytes)| bytes)
                    .sum();
                (terminal.clone(), bytes as f64)
            })
            .filter(|(_, rate)| *rate > 0.0)
            .collect();
        rates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        rates
    }

    /// Frames of the last WINDOW before `now`, or all of them with `all`.
    pub fn summary(&self, now: Instant, all: bool) -> Summary {
        let mut times: Vec<Duration> = self
            .frames
            .iter()
            .filter(|(when, _)| all || now.saturating_duration_since(*when) <= WINDOW)
            .map(|(_, took)| *took)
            .collect();
        times.sort();
        let mut keys: Vec<Duration> = self.key_to_glyph.iter().copied().collect();
        keys.sort();
        Summary {
            frames: times.len(),
            p50: percentile(&times, 0.5),
            p99: percentile(&times, 0.99),
            p999: percentile(&times, 0.999),
            over_8ms: times
                .iter()
                .filter(|took| took.as_secs_f64() > 0.0083)
                .count(),
            over_16ms: times
                .iter()
                .filter(|took| took.as_secs_f64() > 0.0167)
                .count(),
            key_to_glyph_p95: (!keys.is_empty()).then(|| percentile(&keys, 0.95)),
        }
    }

    pub fn frame_samples(&self) -> impl Iterator<Item = &(Instant, Duration)> {
        self.frames.iter()
    }

    pub fn key_samples(&self) -> impl Iterator<Item = &Duration> {
        self.key_to_glyph.iter()
    }
}

/// Nearest-rank percentile of sorted values; zero when there are none.
pub fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nearest_rank_percentiles() {
        let values: Vec<Duration> = (1..=100).map(ms).collect();
        assert_eq!(percentile(&values, 0.5), ms(50));
        assert_eq!(percentile(&values, 0.99), ms(99));
        assert_eq!(percentile(&values, 0.999), ms(100));
        assert_eq!(percentile(&[], 0.5), Duration::ZERO);
    }

    #[test]
    fn frame_times_skip_idle_gaps_and_count_slow_frames() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        let mut at = start;
        for step in [8, 8, 9, 20, 8, 1000, 8] {
            at += ms(step);
            stats.frame(at);
        }
        let summary = stats.summary(at, true);
        // The first frame has no start before it; the 1 s gap is idle.
        assert_eq!(summary.frames, 5);
        assert_eq!(summary.over_8ms, 2);
        assert_eq!(summary.over_16ms, 1);
        assert_eq!(summary.p50, ms(8));
    }

    #[test]
    fn the_overlay_window_is_the_last_ten_seconds() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.frame(start);
        stats.frame(start + ms(30));
        stats.frame(start + ms(38));
        let later = start + Duration::from_secs(11);
        stats.frame(later);
        stats.frame(later + ms(8));
        assert_eq!(stats.summary(later + ms(8), false).frames, 1);
        assert_eq!(stats.summary(later + ms(8), true).frames, 3);
    }

    #[test]
    fn key_to_glyph_ends_when_the_frame_with_the_echo_is_done() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.key("T-1/1", start);
        stats.frame(start + ms(4));
        stats.output("T-1/1", 1, start + ms(6));
        // This frame draws the echo; the next one starts once it is on screen.
        stats.frame(start + ms(8));
        assert_eq!(stats.summary(start + ms(8), false).key_to_glyph_p95, None);
        stats.frame(start + ms(16));
        assert_eq!(
            stats.summary(start + ms(16), false).key_to_glyph_p95,
            Some(ms(16))
        );
    }

    #[test]
    fn only_output_of_the_terminal_that_got_the_key_echoes_it() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.key("T-1/1", start);
        // A bench terminal floods meanwhile: that is not the echo.
        stats.output("T-1/901", 65_536, start + ms(1));
        stats.frame(start + ms(8));
        stats.frame(start + ms(16));
        assert_eq!(stats.summary(start + ms(16), false).key_to_glyph_p95, None);
        stats.output("T-1/1", 1, start + ms(20));
        stats.frame(start + ms(24));
        stats.frame(start + ms(32));
        assert_eq!(
            stats.summary(start + ms(32), false).key_to_glyph_p95,
            Some(ms(32))
        );
    }

    #[test]
    fn output_rate_is_bytes_in_the_last_second() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.output("a", 500_000, start);
        stats.output("a", 500_000, start + ms(500));
        stats.output("b", 10, start + ms(900));
        let rates = stats.output_rates(start + ms(900));
        assert_eq!(
            rates,
            [("a".to_string(), 1_000_000.0), ("b".to_string(), 10.0)]
        );
        assert_eq!(
            stats.output_rates(start + Duration::from_secs(3)),
            Vec::<(String, f64)>::new()
        );
    }
}
