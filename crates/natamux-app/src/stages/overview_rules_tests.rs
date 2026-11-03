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

const NOW: &str = "2026-10-10T08:30:00.000Z";

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
