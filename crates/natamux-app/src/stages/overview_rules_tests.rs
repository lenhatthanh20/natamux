use natamux_proto::stages::schema::{Gate, StageKind};

use super::*;

fn ms(text: &str) -> i64 {
    parse_iso_ms(text).unwrap()
}

fn task(state: TaskState) -> Task {
    Task {
        id: "T-0001".into(),
        state,
        stage: "spec".into(),
        stage_since: Some("2026-10-10T08:11:00.000Z".into()),
        created_at: "2026-10-10T08:00:00.000Z".into(),
        updated_at: "2026-10-10T08:20:00.000Z".into(),
        ..Task::default()
    }
}

fn labels(figures: &[Figure]) -> Vec<&str> {
    figures.iter().map(|figure| figure.label).collect()
}

fn value<'a>(figures: &'a [Figure], label: &str) -> &'a str {
    figures
        .iter()
        .find(|figure| figure.label == label)
        .map_or("", |figure| figure.value.as_str())
}

const NOW: &str = "2026-10-10T08:30:00.000Z";

#[test]
fn shows_the_figures_in_one_row_split_by_lines_with_a_tip_each_and_no_step_names() {
    let figures = overview_figures(&task(TaskState::Review), 2, 6, 1, ms(NOW));
    assert_eq!(
        labels(&figures),
        ["Stages", "Total", "This stage", "Sent back"]
    );
    let tips: Vec<&str> = figures.iter().map(|figure| figure.tip).collect();
    assert_eq!(
        tips,
        [
            "Stages done out of all",
            "Time since the task started",
            "Time in this stage",
            "Times the task was sent back"
        ]
    );
    assert_eq!(value(&figures, "Stages"), "2/6");
    assert_eq!(value(&figures, "Sent back"), "1");
}

#[test]
fn shows_the_time_from_start_to_finish_of_a_finished_task_and_no_time_in_this_stage() {
    let figures = overview_figures(&task(TaskState::Done), 3, 3, 0, ms(NOW));
    let finish = figures
        .iter()
        .find(|figure| figure.label == "Start to finish");
    assert_eq!(
        finish.map(|figure| figure.tip),
        Some("Time from start to finish")
    );
    assert!(!labels(&figures).contains(&"This stage"));
    assert!(figures.iter().all(|figure| !figure.value.contains("NaN")));
}

#[test]
fn moves_the_open_figures_with_the_clock_and_adds_nothing_else_to_the_card() {
    let one = task(TaskState::Running);
    let early = overview_figures(&one, 1, 3, 0, ms(NOW));
    assert_eq!(value(&early, "Total"), "30m");
    assert_eq!(value(&early, "This stage"), "19m");
    let late = overview_figures(&one, 1, 3, 0, ms("2026-10-10T09:00:00.000Z"));
    assert_eq!(value(&late, "Total"), "1h");
    assert_eq!(value(&late, "This stage"), "49m");
    assert_eq!(labels(&late), labels(&early));
}

#[test]
fn keeps_the_time_from_start_to_finish_of_a_finished_task_still_as_the_clock_moves() {
    for state in [TaskState::Done, TaskState::Cancelled] {
        let one = task(state);
        for now in [NOW, "2026-10-10T09:00:00.000Z"] {
            let figures = overview_figures(&one, 1, 3, 0, ms(now));
            assert_eq!(value(&figures, "Start to finish"), "20m", "{state:?}");
            assert!(!labels(&figures).contains(&"This stage"));
        }
    }
}

#[test]
fn shows_no_time_in_this_stage_for_a_task_whose_stage_has_not_started() {
    let one = Task {
        stage_since: None,
        state: TaskState::Ready,
        ..task(TaskState::Ready)
    };
    let figures = overview_figures(&one, 0, 3, 0, ms(NOW));
    assert!(!labels(&figures).contains(&"This stage"));
    assert!(labels(&figures).contains(&"Total"));
}

#[test]
fn leaves_out_the_sent_back_figure_while_the_task_was_never_sent_back() {
    let figures = overview_figures(&task(TaskState::Review), 1, 3, 0, ms(NOW));
    assert!(labels(&figures).contains(&"Stages"));
    assert!(!labels(&figures).contains(&"Sent back"));
}

fn bar_entry(
    id: &str,
    status: StageStatus,
    start: Option<&str>,
    end: Option<&str>,
) -> StageProgress {
    StageProgress {
        stage: id.into(),
        name: id.to_uppercase(),
        kind: StageKind::Agent,
        gate: Gate::Auto,
        output: None,
        group: None,
        path: None,
        because: None,
        status,
        state: None,
        rounds: 1,
        started_at: start.map(String::from),
        ended_at: end.map(String::from),
        approved_by: None,
        passes: vec![],
        reasons: vec![],
        sent_back_for: None,
        sent_back_by_ci: false,
    }
}

#[test]
fn sizes_each_bar_by_its_share_of_the_longest_stage_with_a_sliver_for_a_short_one() {
    let progress = [
        bar_entry(
            "a",
            StageStatus::Done,
            Some("2026-10-10T08:00:00.000Z"),
            Some("2026-10-10T08:10:00.000Z"),
        ),
        bar_entry(
            "b",
            StageStatus::Done,
            Some("2026-10-10T08:10:00.000Z"),
            Some("2026-10-10T08:15:00.000Z"),
        ),
        bar_entry(
            "c",
            StageStatus::Done,
            Some("2026-10-10T08:15:00.000Z"),
            Some("2026-10-10T08:15:10.000Z"),
        ),
        bar_entry("d", StageStatus::Upcoming, None, None),
    ];
    let bars = time_bars(&progress, ms(NOW));
    assert_eq!(bars.len(), 3);
    assert_eq!(bars[0].width_percent, 100.0);
    assert_eq!(bars[1].width_percent, 50.0);
    assert_eq!(bars[2].width_percent, 4.0);
    assert_eq!(bars[0].time, "10m");
    assert_eq!(bars[0].state, TaskState::Done);
}

#[test]
fn counts_a_current_stage_up_to_now_without_so_far() {
    let mut running = bar_entry(
        "a",
        StageStatus::Current,
        Some("2026-10-10T08:00:00.000Z"),
        None,
    );
    running.state = Some(TaskState::Review);
    let bars = time_bars(&[running], ms(NOW));
    assert_eq!(bars[0].time, "30m");
    assert_eq!(bars[0].state, TaskState::Review);
    assert!(time_bars(&[], ms(NOW)).is_empty());
}

#[test]
fn a_duration_changes_when_its_minutes_round_up() {
    let from = "2026-10-10T08:00:00.000Z";
    let after = |seconds: i64| next_duration_change_ms(from, ms(from) + seconds * 1000);
    // "<1m" until a minute, then each half minute past it rounds up.
    assert_eq!(after(0), Some(60_000));
    assert_eq!(after(59), Some(1_000));
    assert_eq!(after(60), Some(30_000));
    assert_eq!(after(89), Some(1_000));
    assert_eq!(after(90), Some(60_000));
    assert_eq!(next_duration_change_ms("soon", ms(from)), None);
}

#[test]
fn the_progress_tab_follows_the_stage_that_runs_and_the_figures_of_an_open_task() {
    let now = ms(NOW);
    let current = bar_entry(
        "a",
        StageStatus::Current,
        Some("2026-10-10T08:29:50.000Z"),
        None,
    );
    let done = bar_entry(
        "b",
        StageStatus::Done,
        Some("2026-10-10T08:00:00.000Z"),
        Some("2026-10-10T08:10:00.000Z"),
    );
    let ended = task(TaskState::Done);
    let both = [current, done.clone()];
    assert_eq!(next_progress_change_ms(&ended, &both, now), Some(50_000));
    assert_eq!(next_progress_change_ms(&ended, &[done], now), None);
    // The figures of an open task: "Total" from 08:00, "This stage" from 08:11.
    assert_eq!(
        next_progress_change_ms(&task(TaskState::Running), &[], now),
        Some(30_000)
    );
}

