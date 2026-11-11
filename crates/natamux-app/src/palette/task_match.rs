//! Scoring of tasks against a typed query, without cloning a task.

use natamux_proto::tasks::task::Task;

use super::fuzzy::{FuzzyMatch, Range, fuzzy_match};
use super::palette_rows::PaletteRow;

fn scaled(found: Option<FuzzyMatch>, by: f64) -> Option<FuzzyMatch> {
    found.map(|found| FuzzyMatch {
        score: found.score * by,
        ranges: found.ranges,
    })
}

fn best_of(matches: [Option<FuzzyMatch>; 3]) -> Option<FuzzyMatch> {
    let mut best: Option<FuzzyMatch> = None;
    for found in matches.into_iter().flatten() {
        if best.as_ref().is_none_or(|top| found.score > top.score) {
            best = Some(found);
        }
    }
    best
}

/// How a task matched a query: its score and the ranges to highlight. Light, so a list of
/// thousands can be scored without cloning a task.
pub struct TaskScore {
    pub score: f64,
    title_ranges: Vec<Range>,
    id_ranges: Vec<Range>,
}

impl TaskScore {
    pub fn into_row(self, task: &Task) -> PaletteRow {
        PaletteRow::Task {
            task: task.clone(),
            title_ranges: self.title_ranges,
            id_ranges: self.id_ranges,
        }
    }
}

pub fn score_task(task: &Task, query: &str) -> Option<TaskScore> {
    let by_id = fuzzy_match(query, &task.id);
    let by_title = fuzzy_match(query, &task.title);
    let by_branch = task.branch.as_deref().and_then(|b| fuzzy_match(query, b));
    let best = best_of([
        scaled(by_id.clone(), 1.2),
        by_title.clone(),
        scaled(by_branch, 0.8),
    ])?;
    Some(TaskScore {
        score: best.score,
        title_ranges: by_title.map(|found| found.ranges).unwrap_or_default(),
        id_ranges: by_id.map(|found| found.ranges).unwrap_or_default(),
    })
}

/// The matching tasks, best first; a tie keeps the input order.
pub fn ranked_tasks<'a>(tasks: &[&'a Task], query: &str) -> Vec<(&'a Task, TaskScore)> {
    let mut found: Vec<(&'a Task, TaskScore)> = tasks
        .iter()
        .filter_map(|task| Some((*task, score_task(task, query)?)))
        .collect();
    found.sort_by(|a, b| b.1.score.total_cmp(&a.1.score));
    found
}
