//! The perf overlay of gate 2: on with NATAMUX_PERF=1 or Tools > Toggle Perf Overlay. While on,
//! the window draws every frame, so frame times can be measured; that costs power, so it is off by
//! default.

use std::time::{Duration, Instant};

use gpui::{App, Global, Window};
use ui::prelude::*;

use super::stats::{PerfStats, Summary};

pub struct Perf {
    pub enabled: bool,
    pub stats: PerfStats,
}

impl Global for Perf {}

pub fn init(cx: &mut App) {
    let enabled = std::env::var("NATAMUX_PERF").is_ok_and(|value| value == "1");
    cx.set_global(Perf {
        enabled,
        stats: PerfStats::default(),
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

pub fn enabled(cx: &App) -> bool {
    cx.global::<Perf>().enabled
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
    window.on_next_frame(|window, cx| {
        let perf = cx.global_mut::<Perf>();
        if perf.enabled {
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
