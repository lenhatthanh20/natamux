//! When the times of the Progress tab change by the clock alone, so the panel draws again then.
//! The stat strip and the time bars of `features/stages/Overview.tsx` are gone from the app
//! (T-0257).

use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_time::parse_iso_ms;

/// How long until `format_duration(from, now)` shows another text: the minutes round at half a
/// minute, after "<1m". A time in days changes less often; it draws once a minute too.
pub fn next_duration_change_ms(from: &str, now_ms: i64) -> Option<i64> {
    const MINUTE_MS: i64 = 60_000;
    let span = (now_ms - parse_iso_ms(from)?).max(0);
    if span < MINUTE_MS {
        return Some(MINUTE_MS - span);
    }
    let wait = (MINUTE_MS / 2 - span % MINUTE_MS).rem_euclid(MINUTE_MS);
    Some(if wait == 0 { MINUTE_MS } else { wait })
}

/// How long until a time of the Progress tab changes by the clock alone: the figures of a task
/// that has not ended, and the bar of the stage that runs.
pub fn next_progress_change_ms(
    task: &Task,
    progress: &[StageProgress],
    now_ms: i64,
) -> Option<i64> {
    let running = progress
        .iter()
        .filter(|entry| entry.status == StageStatus::Current)
        .filter_map(|entry| entry.started_at.as_deref());
    let ended = matches!(task.state, TaskState::Done | TaskState::Cancelled);
    let figures = [Some(task.created_at.as_str()), task.stage_since.as_deref()]
        .into_iter()
        .flatten()
        .filter(|_| !ended);
    running
        .chain(figures)
        .filter_map(|from| next_duration_change_ms(from, now_ms))
        .min()
}

#[cfg(test)]
#[path = "overview_rules_tests.rs"]
mod tests;
