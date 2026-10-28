use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn counts_reconnects_skeletons_and_show_latency() {
    let mut stats = PerfStats::default();
    assert_eq!(stats.show_latency(), (0, None, None));
    stats.reconnect("901");
    stats.reconnect("901");
    stats.reconnect("902");
    stats.drew("903");
    stats.skeleton("903");
    stats.link_lost += 1;
    for took in 1..=20 {
        stats.shown("901", ms(took));
    }
    assert_eq!(stats.reconnects.values().sum::<u32>(), 3);
    assert_eq!(stats.reconnects["901"], 2);
    assert_eq!(stats.skeletons.values().sum::<u32>(), 1);
    assert_eq!(stats.show_latency(), (20, Some(ms(10)), Some(ms(19))));
    assert_eq!(stats.shown_count(), 20);
}

#[test]
fn the_next_part_keeps_the_run_counts_and_drops_the_samples() {
    let mut stats = PerfStats {
        bench: Some(BenchInfo {
            terminals: 30,
            shown: 6,
            output: "agent",
        }),
        ..PerfStats::default()
    };
    stats.reconnect("901");
    stats.drew("902");
    stats.next_part();
    stats.skeleton("902");
    stats.link_lost = 2;
    stats.shown("901", ms(5));
    stats.frame(Instant::now());
    stats.next_part();
    assert_eq!(stats.bench.as_ref().map(|bench| bench.terminals), Some(30));
    assert_eq!(stats.reconnects["901"], 1);
    assert_eq!(stats.skeletons["902"], 1);
    assert_eq!(stats.link_lost, 2);
    assert_eq!(stats.shown_count(), 0);
    assert_eq!(stats.summary(Instant::now(), true).frames, 0);
}

// Review of T-0210: every first attach shows the skeleton, so 30 terminals counted 30 flashes.
#[test]
fn a_skeleton_counts_only_for_a_terminal_that_drew_before() {
    let mut stats = PerfStats::default();
    stats.skeleton("901");
    assert_eq!(stats.skeletons.values().sum::<u32>(), 0);
    stats.drew("901");
    stats.skeleton("901");
    stats.skeleton("902");
    assert_eq!(stats.skeletons.values().sum::<u32>(), 1);
    assert_eq!(stats.skeletons["901"], 1);
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

#[test]
fn flow_runs_are_kept_by_name_across_parts() {
    let mut stats = PerfStats::default();
    stats.flow("diff_click", ms(12));
    stats.flow("diff_click", ms(20));
    stats.flow("palette_key", ms(5));
    stats.next_part();
    assert_eq!(stats.flows["diff_click"], [ms(12), ms(20)]);
    assert_eq!(stats.flows["palette_key"], [ms(5)]);
}
