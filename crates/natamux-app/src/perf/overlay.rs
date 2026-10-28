//! The perf overlay of gate 2: on with NATAMUX_PERF=1 or Tools > Toggle Perf Overlay. While on,
//! the window draws every frame, so frame times can be measured; that costs power, so it is off by
//! default.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{App, Global, Window};
use ui::prelude::*;

use super::stats::{PerfStats, Summary};

pub struct Perf {
    pub enabled: bool,
    pub stats: PerfStats,
    /// `--gate2` measures real draws: no forced frames and no refresh, but the samples go on.
    pub quiet: bool,
}

/// A frame watch is waiting for the next frame: two would count every frame twice.
static WATCHING: AtomicBool = AtomicBool::new(false);

impl Global for Perf {}

pub fn init(cx: &mut App) {
    let enabled = std::env::var("NATAMUX_PERF").is_ok_and(|value| value == "1");
    cx.set_global(Perf {
        enabled,
        stats: PerfStats::default(),
        quiet: false,
    });
}

/// A key pressed at `at` went to a terminal as input. Keys that send nothing to a terminal (the
/// filter box, shortcuts) are never counted.
pub fn key(terminal: &str, at: Instant, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.key(terminal, at);
    }
}

/// The overlay forces frames and refreshes. False while `--gate2` measures real draws.
pub fn enabled(cx: &App) -> bool {
    let perf = cx.global::<Perf>();
    perf.enabled && !perf.quiet
}

/// A terminal socket reconnected.
pub fn reconnect(terminal: &str, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.reconnect(terminal);
    }
}

/// The dashboard link went to Reconnecting.
pub fn link_lost(cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.link_lost += 1;
    }
}

/// A terminal drew a screen: a skeleton it shows later is a flash.
pub fn drew(terminal: &str, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.drew(terminal);
    }
}

/// A pane showed the skeleton. It counts only after `drew`, as a first attach always shows it;
/// the count should stay 0.
pub fn skeleton(terminal: &str, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.skeleton(terminal);
    }
}

/// A terminal came on screen and its catch-up ended after `latency` (an empty one when it missed
/// nothing).
pub fn shown(terminal: &str, latency: Duration, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.shown(terminal, latency);
    }
}

/// A terminal wrote output to its screen.
pub fn output(terminal: &str, bytes: usize, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    if perf.enabled {
        perf.stats.output(terminal, bytes, Instant::now());
    }
}

pub fn toggle(window: &mut Window, cx: &mut App) {
    let perf = cx.global_mut::<Perf>();
    perf.enabled = !perf.enabled;
    perf.stats.pause();
    if perf.enabled {
        watch_frames(window);
    }
}

/// Starts the frame clock, if the overlay is on. Each frame starts the clock of the next one.
pub fn watch_frames(window: &mut Window) {
    if WATCHING.swap(true, Ordering::Relaxed) {
        return;
    }
    window.on_next_frame(|window, cx| {
        WATCHING.store(false, Ordering::Relaxed);
        let perf = cx.global_mut::<Perf>();
        if perf.enabled && !perf.quiet {
            perf.stats.frame(Instant::now());
            watch_frames(window);
        }
    });
}

fn ms(duration: Duration) -> String {
    format!("{:.1}", duration.as_secs_f64() * 1000.0)
}

fn lines(summary: &Summary, rates: &[(String, f64)]) -> Vec<String> {
    let mut lines = vec![
        format!(
            "frames {} · p50 {} · p99 {} · p99.9 {} ms",
            summary.frames,
            ms(summary.p50),
            ms(summary.p99),
            ms(summary.p999)
        ),
        format!(
            "over 1 refresh: {} · over 2: {} · missed {}",
            summary.over_one_refresh, summary.over_two_refreshes, summary.missed_refreshes
        ),
        match (summary.key_to_glyph_p95, summary.key_to_echo_p95) {
            (Some(glyph), Some(echo)) => {
                format!(
                    "key to glyph p95 {} ms · echo p95 {} ms",
                    ms(glyph),
                    ms(echo)
                )
            }
            _ => "key to glyph: type in a terminal".to_string(),
        },
    ];
    for (terminal, rate) in rates.iter().take(10) {
        lines.push(format!("{terminal}: {:.2} MB/s", rate / 1_000_000.0));
    }
    lines
}

/// The overlay, top right, when it is on.
pub fn render(cx: &App) -> Option<impl IntoElement + use<>> {
    let perf = cx.global::<Perf>();
    if !perf.enabled {
        return None;
    }
    let now = Instant::now();
    let text = lines(
        &perf.stats.summary(now, false),
        &perf.stats.output_rates(now),
    );
    let colors = cx.theme().colors();
    Some(
        v_flex()
            .absolute()
            .top_2()
            .right_2()
            .p_2()
            .gap_0p5()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .children(
                text.into_iter()
                    .map(|line| Label::new(line).size(LabelSize::XSmall).buffer_font(cx)),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_frame_and_key_numbers_and_terminal_rates() {
        let summary = Summary {
            frames: 1200,
            p50: Duration::from_micros(8300),
            p99: Duration::from_micros(9100),
            p999: Duration::from_millis(15),
            over_one_refresh: 30,
            over_two_refreshes: 2,
            missed_refreshes: 35,
            over_16ms: 0,
            key_to_glyph_p95: None,
            key_to_echo_p95: None,
        };
        let text = lines(&summary, &[("T-1/2".into(), 1_048_576.0)]);
        assert_eq!(text[0], "frames 1200 · p50 8.3 · p99 9.1 · p99.9 15.0 ms");
        assert_eq!(text[1], "over 1 refresh: 30 · over 2: 2 · missed 35");
        assert_eq!(text[2], "key to glyph: type in a terminal");
        assert_eq!(text[3], "T-1/2: 1.05 MB/s");
        let typed = Summary {
            key_to_glyph_p95: Some(Duration::from_millis(20)),
            key_to_echo_p95: Some(Duration::from_micros(4500)),
            ..summary
        };
        assert_eq!(
            lines(&typed, &[])[2],
            "key to glyph p95 20.0 ms · echo p95 4.5 ms"
        );
    }
}
