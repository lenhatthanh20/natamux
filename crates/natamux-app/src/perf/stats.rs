//! The numbers of phase 1's gate 2: frame times, key to glyph, and output per terminal.
//! Pure: callers pass the time, so tests need no clock.

use std::collections::{HashMap, HashSet, VecDeque};
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

/// What a `--gate2` bench had: the terminals and what they printed.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchInfo {
    pub terminals: usize,
    pub shown: usize,
    pub output: &'static str,
}

/// The hidden phase of `--gate2`: the task table on screen, every bench terminal hidden.
#[derive(Clone, Debug, PartialEq)]
pub struct HiddenPhase {
    pub seconds: f64,
    pub draws_per_second: f64,
    /// The app's CPU in the phase, as a percent of one core; None when it could not be read.
    pub cpu_percent: Option<f64>,
}

#[derive(Default)]
pub struct PerfStats {
    /// Set by `--gate2`; kept by `next_part`.
    pub bench: Option<BenchInfo>,
    pub hidden_phase: Option<HiddenPhase>,
    /// The dashboard link went to Reconnecting. Kept by `next_part`.
    pub link_lost: u32,
    /// Socket reconnects of each terminal. Kept by `next_part`.
    pub reconnects: HashMap<String, u32>,
    /// Panes that had drawn a screen and showed the skeleton: should stay 0. Kept by `next_part`.
    pub skeletons: HashMap<String, u32>,
    /// Terminals that drew a screen: a first attach shows the skeleton too, and is no flash.
    /// Kept by `next_part`, so a terminal rebuilt later still counts.
    drew: HashSet<String>,
    /// (terminal, show to the end of its catch-up, empty when it missed nothing) of each
    /// terminal that came on screen.
    shown: Vec<(String, Duration)>,
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

    /// A new part of a run (`--gate2` phases): the samples start again, the run's counts stay.
    pub fn next_part(&mut self) {
        let kept = Self {
            bench: self.bench.take(),
            link_lost: self.link_lost,
            reconnects: std::mem::take(&mut self.reconnects),
            skeletons: std::mem::take(&mut self.skeletons),
            drew: std::mem::take(&mut self.drew),
            ..Self::default()
        };
        *self = kept;
    }

    pub fn reconnect(&mut self, terminal: &str) {
        *self.reconnects.entry(terminal.to_string()).or_default() += 1;
    }

    pub fn drew(&mut self, terminal: &str) {
        self.drew.insert(terminal.to_string());
    }

    /// Counts only for a terminal that drew before.
    pub fn skeleton(&mut self, terminal: &str) {
        if self.drew.contains(terminal) {
            *self.skeletons.entry(terminal.to_string()).or_default() += 1;
        }
    }

    /// A terminal came on screen and its catch-up ended after `latency`.
    pub fn shown(&mut self, terminal: &str, latency: Duration) {
        self.shown.push((terminal.to_string(), latency));
    }

    pub fn shown_count(&self) -> usize {
        self.shown.len()
    }

    /// (count, p50, p95) of the show latencies.
    pub fn show_latency(&self) -> (usize, Option<Duration>, Option<Duration>) {
        let mut times: Vec<Duration> = self.shown.iter().map(|(_, took)| *took).collect();
        times.sort();
        let at = |p| (!times.is_empty()).then(|| percentile(&times, p));
        (times.len(), at(0.5), at(0.95))
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
#[path = "stats_tests.rs"]
mod tests;
