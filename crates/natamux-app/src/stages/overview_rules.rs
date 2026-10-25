//! The figures and the time bars of the Progress tab (`features/stages/Overview.tsx`).

// Ported with their tests (spec D14); the Progress tab calls them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::tasks::task::{Task, TaskState};

use super::stage_view_rules::{format_duration, format_duration_to, stage_time};
use crate::tasks::task_time::parse_iso_ms;

pub const NO_STAGE_STARTED: &str = "No stage has started yet.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Figure {
    pub label: &'static str,
    pub value: String,
    pub tip: &'static str,
}

fn figure(label: &'static str, value: String, tip: &'static str) -> Figure {
    Figure { label, value, tip }
}

/// The overview row: stages done, the time since the start (or from start to finish), the time in
/// this stage while the task is open, and how often it was sent back, when it was.
pub fn overview_figures(
    task: &Task,
    done: usize,
    total: usize,
    sent_back: usize,
    now_ms: i64,
) -> Vec<Figure> {
    let finished = task.state.is_finished();
    let mut figures = vec![figure(
        "Stages",
        format!("{done}/{total}"),
        "Stages done out of all",
    )];
    figures.push(if finished {
        figure(
            "Start to finish",
            format_duration_to(&task.created_at, &task.updated_at),
            "Time from start to finish",
        )
    } else {
        figure(
            "Total",
            format_duration(&task.created_at, now_ms),
            "Time since the task started",
        )
    });
    if let (Some(since), false) = (&task.stage_since, finished) {
        figures.push(figure(
            "This stage",
            format_duration(since, now_ms),
            "Time in this stage",
        ));
    }
    if sent_back > 0 {
        figures.push(figure(
            "Sent back",
            sent_back.to_string(),
            "Times the task was sent back",
        ));
    }
    figures
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeBar {
    pub stage: String,
    pub name: String,
    /// The share of the longest bar, 4 to 100.
    pub width_percent: f64,
    /// "4m", "1h 5m"; no "so far".
    pub time: String,
    /// The state colour of a current stage; a stage that ended is done.
    pub state: TaskState,
}

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

/// One bar per stage that started. The longest stage has the full width, and a short one keeps a
/// sliver so it can be seen.
pub fn time_bars(progress: &[StageProgress], now_ms: i64) -> Vec<TimeBar> {
    let rows: Vec<(&StageProgress, f64)> = progress
        .iter()
        .filter_map(|entry| {
            let start = parse_iso_ms(entry.started_at.as_deref()?)?;
            let end = if entry.status == StageStatus::Current {
                now_ms
            } else {
                entry
                    .ended_at
                    .as_deref()
                    .and_then(parse_iso_ms)
                    .unwrap_or(start)
            };
            Some((entry, (end - start) as f64 / 60_000.0))
        })
        .collect();
    let longest = rows.iter().map(|(_, minutes)| *minutes).fold(1.0, f64::max);
    rows.into_iter()
        .map(|(entry, minutes)| TimeBar {
            stage: entry.stage.clone(),
            name: entry.name.clone(),
            width_percent: (minutes / longest * 100.0).max(4.0),
            time: stage_time(entry, now_ms)
                .map(|text| text.replace(" so far", ""))
                .unwrap_or_default(),
            state: if entry.status == StageStatus::Current {
                entry.state.unwrap_or(TaskState::Running)
            } else {
                TaskState::Done
            },
        })
        .collect()
}

#[cfg(test)]
#[path = "overview_rules_tests.rs"]
mod tests;
