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
/// One refresh of the 120 Hz display.
pub const REFRESH: Duration = Duration::from_nanos(8_333_333);
/// Two refreshes: one frame dropped.
pub const ONE_DROPPED: Duration = Duration::from_micros(16_700);
/// The report keeps at most this many frame samples (about 2 hours at 120 fps).
const MAX_SAMPLES: usize = 1_000_000;

#[derive(Default)]
pub struct PerfStats {
    last_frame: Option<Instant>,
    /// (when, frame time, refreshes it took) of every frame since the overlay was turned on.
    frames: Vec<(Instant, Duration, u32)>,
    /// Start time not yet given to a refresh, so a late start and the short frame after it count
    /// as one refresh each, not as a slow frame (gate 2 review, 2026-10-22).
    carry: f64,
    /// When the first frame came, for the report's length.
    first_frame: Option<Instant>,
    /// Bytes each terminal wrote since the overlay came on.
    output_total: HashMap<String, usize>,
    /// Keys sent to each terminal, not echoed yet.
    keys: HashMap<String, Vec<Instant>>,
    /// Keys whose echo came, waiting for the frame that draws it to finish.
    echoed: Vec<Instant>,
    /// Keys in the frame now being drawn.
    drawing: Vec<Instant>,
    key_to_glyph: VecDeque<Duration>,
    /// From the key to its echo reaching the app: the shell, the daemon and the network, without
    /// parsing and frames. Splits key to glyph for gate 2.
    key_to_echo: VecDeque<Duration>,
    /// Bytes written to each terminal's screen, with when.
    output: HashMap<String, VecDeque<(Instant, usize)>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub frames: usize,
    pub p50: Duration,
    pub p99: Duration,
    pub p999: Duration,
    /// Frames that took two refreshes or more, and three or more.
    pub over_one_refresh: usize,
    pub over_two_refreshes: usize,
    /// Refreshes with no new frame.
    pub missed_refreshes: u64,
    pub over_16ms: usize,
    pub key_to_glyph_p95: Option<Duration>,
    pub key_to_echo_p95: Option<Duration>,
}

impl PerfStats {
    /// A frame starts. The time since the last start is the last frame's time (its draw, and the
    /// wait for the display when it kept up).
    pub fn frame(&mut self, now: Instant) {
        if let Some(last) = self.last_frame {
            let took = now.saturating_duration_since(last);
            if took >= IDLE_GAP {
                self.carry = 0.0;
            } else if self.frames.len() < MAX_SAMPLES {
                let refreshes = self.refreshes(took);
                self.frames.push((now, took, refreshes));
            }
        }
        self.last_frame = Some(now);
        self.first_frame.get_or_insert(now);
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

    /// How many refreshes a frame took. Its start may be late or early by up to half a refresh:
    /// the display link and the main thread add jitter, which the next frame makes up for.
    fn refreshes(&mut self, took: Duration) -> u32 {
        let refresh = REFRESH.as_secs_f64();
        let total = took.as_secs_f64() + self.carry;
        let count = (total / refresh).round().max(1.0);
        self.carry = (total - count * refresh).clamp(-refresh / 2.0, refresh / 2.0);
        count as u32
    }

    /// The overlay was off: the next frame does not count the time in between.
    pub fn pause(&mut self) {
        self.last_frame = None;
        self.carry = 0.0;
    }

    /// How long the samples cover, and the bytes each terminal wrote in that time.
    pub fn output_totals(&self, now: Instant) -> (Duration, Vec<(String, usize)>) {
        let span = self
            .first_frame
            .map_or(Duration::ZERO, |first| now.saturating_duration_since(first));
        let mut totals: Vec<(String, usize)> = self
            .output_total
            .iter()
            .map(|(terminal, bytes)| (terminal.clone(), *bytes))
            .collect();
        totals.sort();
        (span, totals)
    }

    /// A key pressed at `at` went to `terminal` as input.
    pub fn key(&mut self, terminal: &str, at: Instant) {
        self.keys.entry(terminal.to_string()).or_default().push(at);
    }

    /// A terminal got output: the keys typed into it before are echoed by it.
    pub fn output(&mut self, terminal: &str, bytes: usize, now: Instant) {
        if let Some(keys) = self.keys.get_mut(terminal) {
            for key in keys.iter() {
                self.key_to_echo
                    .push_back(now.saturating_duration_since(*key));
            }
            while self.key_to_echo.len() > KEY_SAMPLES {
                self.key_to_echo.pop_front();
            }
            self.echoed.append(keys);
        }
        *self.output_total.entry(terminal.to_string()).or_default() += bytes;
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
        let frames: Vec<(Duration, u32)> = self
            .frames
            .iter()
            .filter(|(when, _, _)| all || now.saturating_duration_since(*when) <= WINDOW)
            .map(|(_, took, refreshes)| (*took, *refreshes))
            .collect();
        let mut times: Vec<Duration> = frames.iter().map(|(took, _)| *took).collect();
        times.sort();
        let sorted = |samples: &VecDeque<Duration>| {
            let mut values: Vec<Duration> = samples.iter().copied().collect();
            values.sort();
            values
        };
        let keys = sorted(&self.key_to_glyph);
        let echoes = sorted(&self.key_to_echo);
        Summary {
            frames: times.len(),
            p50: percentile(&times, 0.5),
            p99: percentile(&times, 0.99),
            p999: percentile(&times, 0.999),
            over_one_refresh: frames.iter().filter(|(_, count)| *count >= 2).count(),
            over_two_refreshes: frames.iter().filter(|(_, count)| *count >= 3).count(),
            missed_refreshes: frames.iter().map(|(_, count)| u64::from(count - 1)).sum(),
            over_16ms: times.iter().filter(|took| **took > ONE_DROPPED).count(),
            key_to_glyph_p95: (!keys.is_empty()).then(|| percentile(&keys, 0.95)),
            key_to_echo_p95: (!echoes.is_empty()).then(|| percentile(&echoes, 0.95)),
        }
    }

    /// (frame time, refreshes) of every frame.
    pub fn frame_samples(&self) -> impl Iterator<Item = (Duration, u32)> + '_ {
        self.frames
            .iter()
            .map(|(_, took, refreshes)| (*took, *refreshes))
    }

    pub fn key_samples(&self) -> impl Iterator<Item = &Duration> {
        self.key_to_glyph.iter()
    }

    pub fn echo_samples(&self) -> impl Iterator<Item = &Duration> {
        self.key_to_echo.iter()
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
        assert_eq!(summary.over_one_refresh, 1);
        assert_eq!(summary.missed_refreshes, 1);
        assert_eq!(summary.over_16ms, 1);
        assert_eq!(summary.p50, ms(8));
    }

    // Gate 2 runs of 2026-10-22: a frame that starts late and the short one after it are two
    // refreshes with two frames, not a slow frame.
    #[test]
    fn a_late_start_made_up_by_the_next_frame_is_on_time() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        let mut at = start;
        stats.frame(at);
        for micros in [8_333, 8_338, 12_000, 4_666, 11_500, 5_166, 8_333] {
            at += Duration::from_micros(micros);
            stats.frame(at);
        }
        let summary = stats.summary(at, true);
        assert_eq!(summary.frames, 7);
        assert_eq!(summary.over_one_refresh, 0);
        assert_eq!(summary.missed_refreshes, 0);
    }

    #[test]
    fn a_skipped_refresh_counts_once_even_when_split_over_two_frames() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        let mut at = start;
        stats.frame(at);
        // 16.7 ms is one refresh missed; 14 + 11 ms is three refreshes in two frames.
        for micros in [8_333, 16_667, 8_333, 14_000, 11_000, 8_333, 25_000] {
            at += Duration::from_micros(micros);
            stats.frame(at);
        }
        let summary = stats.summary(at, true);
        assert_eq!(summary.missed_refreshes, 4);
        assert_eq!(summary.over_one_refresh, 3);
        assert_eq!(summary.over_two_refreshes, 1);
    }

    #[test]
    fn counts_each_terminals_bytes_since_the_overlay_came_on() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.frame(start);
        stats.output("b", 10, start + ms(1));
        stats.output("a", 5, start + ms(2));
        stats.output("a", 5, start + Duration::from_secs(5));
        let (span, totals) = stats.output_totals(start + Duration::from_secs(6));
        assert_eq!(span, Duration::from_secs(6));
        assert_eq!(totals, [("a".to_string(), 10), ("b".to_string(), 10)]);
    }

    #[test]
    fn key_to_echo_ends_when_the_echo_reaches_the_screen() {
        let start = Instant::now();
        let mut stats = PerfStats::default();
        stats.key("T-1/1", start);
        stats.output("T-1/1", 1, start + ms(5));
        stats.frame(start + ms(8));
        stats.frame(start + ms(16));
        let summary = stats.summary(start + ms(16), false);
        assert_eq!(summary.key_to_echo_p95, Some(ms(5)));
        assert_eq!(summary.key_to_glyph_p95, Some(ms(16)));
        assert_eq!(stats.echo_samples().count(), 1);
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
