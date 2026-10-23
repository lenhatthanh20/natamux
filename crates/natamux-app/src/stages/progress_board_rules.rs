//! Pure rules of the Progress layout of All tasks (`ProgressBoard.tsx`, `stepsKey` of
//! `stage-view.ts`): the sections, the order of the rows,
//! the counts in the stage headers, the Next column and the links between the cells. `now_ms` is
//! passed in, so no rule reads the clock.

use std::collections::BTreeMap;

use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::position::step_index;
use natamux_proto::stages::progress::StageProgress;
use natamux_proto::stages::resolve::{active_steps, resolve_steps, step_refs_of};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::Value;

use super::stage_view_rules::{LinkKind, link_kind, same_lane};
use crate::sidebar::sidebar_tree::steps_of;
use crate::sidebar::workspace_groups::{DEFAULT_REPO_ID, RepoRef, known_repos};
use crate::tasks::task_attention::is_quiet;
use crate::tasks::task_rules::{needs_me, state_from_name};

/// One box of the board: the open and the done tasks that share a set of steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub key: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub open: Vec<Task>,
    pub done: Vec<Task>,
}

/// The steps a task carries, as text: tasks with the same text share a section.
pub fn steps_key(task: &Task) -> String {
    Value::Array(task.stages.clone()).to_string()
}

/// The key a task made now with the stages that are on would have.
fn current_key(pipeline: &[Step]) -> String {
    let refs = step_refs_of(&active_steps(pipeline));
    serde_json::to_value(refs).map_or_else(|_| String::new(), |value| value.to_string())
}

fn step_position(steps: &[Step], stage: &str) -> i64 {
    step_index(steps, stage).map_or(-1, |index| index as i64)
}

/// Waiting for you first, then the furthest along. Equal rows keep the order of `tasks`, which the
/// view freezes, so a row never moves under the pointer.
pub fn sort_rows(tasks: &[Task], steps: &[Step], now_ms: i64) -> Vec<Task> {
    let waits = |task: &Task| needs_me(task.state) || is_quiet(task, now_ms);
    let mut rows = tasks.to_vec();
    rows.sort_by_key(|task| (!waits(task), -step_position(steps, &task.stage)));
    rows
}

/// One section per project with open tasks (cancelled ones are left out), and one more for its
/// tasks made before its pipeline changed. Without the settings the tasks show the stages they
/// carry.
pub fn board_sections(
    tasks: &[Task],
    repos: &[RepoRef],
    pipelines: &BTreeMap<String, Vec<Step>>,
    now_ms: i64,
) -> Vec<Section> {
    let mut sections = Vec::new();
    for repo in known_repos(repos, tasks) {
        let own: Vec<&Task> = tasks
            .iter()
            .filter(|task| {
                task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID) == repo.id
                    && task.state != TaskState::Cancelled
            })
            .collect();
        if own.is_empty() {
            continue;
        }
        let loaded = pipelines.get(&repo.id);
        let pipeline: &[Step] = loaded.map_or(&[], Vec::as_slice);
        let mut keys: Vec<String> = Vec::new();
        for task in &own {
            let key = steps_key(task);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        // New tasks get the stages that are on, so tasks made since the last change carry these.
        let current = current_key(pipeline);
        keys.sort_by_key(|key| *key != current);
        for (index, key) in keys.iter().enumerate() {
            let group: Vec<Task> = own
                .iter()
                .filter(|task| steps_key(task) == *key)
                .map(|task| (*task).clone())
                .collect();
            let Some(first) = group.first() else {
                continue;
            };
            let steps = resolve_steps(&steps_of(first), pipeline);
            let (done, open): (Vec<Task>, Vec<Task>) = group
                .into_iter()
                .partition(|task| task.state == TaskState::Done);
            let title = if index == 0 && (loaded.is_none() || *key == current) {
                repo.name.clone()
            } else {
                format!("{}, earlier pipeline", repo.name)
            };
            sections.push(Section {
                key: format!("{}:{key}", repo.id),
                title,
                open: sort_rows(&open, &steps, now_ms),
                steps,
                done,
            });
        }
    }
    sections
}

/// A stage column of a section, with the tasks that are in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageColumn {
    pub stage_id: String,
    pub name: String,
    pub in_group: bool,
    pub in_path: bool,
    /// Open tasks whose current step holds the stage and are not finished there.
    pub here: usize,
    /// Of those, the ones that need the user.
    pub waiting: usize,
}

/// The state of one stage of a group; a lane with no entry is done.
fn lane_state(task: &Task, stage_id: &str) -> TaskState {
    task.lanes
        .get(stage_id)
        .and_then(Value::as_str)
        .map_or(TaskState::Done, state_from_name)
}

pub fn stage_columns(steps: &[Step], open: &[Task]) -> Vec<StageColumn> {
    steps
        .iter()
        .flat_map(|step| {
            let in_group = matches!(step, Step::Group(_));
            let in_path = matches!(step, Step::Branch(_));
            step.stages()
                .into_iter()
                .map(move |stage| (stage, in_group, in_path))
        })
        .map(|(stage, in_group, in_path)| {
            let here: Vec<&Task> = open
                .iter()
                .filter(|task| {
                    let Some(step) = step_index(steps, &task.stage).and_then(|at| steps.get(at))
                    else {
                        return false;
                    };
                    // A task is in one path of a branch only, the one of its stage.
                    let holds = if in_path {
                        task.stage == stage.id
                    } else {
                        step.stages()
                            .iter()
                            .any(|candidate| candidate.id == stage.id)
                    };
                    holds && (!in_group || lane_state(task, &stage.id) != TaskState::Done)
                })
                .collect();
            let waiting = here
                .iter()
                .filter(|task| {
                    needs_me(if in_group {
                        lane_state(task, &stage.id)
                    } else {
                        task.state
                    })
                })
                .count();
            StageColumn {
                stage_id: stage.id.clone(),
                name: stage_name(stage),
                in_group,
                in_path,
                here: here.len(),
                waiting,
            }
        })
        .collect()
}

/// The label of the Next button of a task in this state.
pub fn next_label(state: TaskState) -> Option<&'static str> {
    match state {
        TaskState::NeedsInput => Some("Answer"),
        TaskState::Review => Some("Review"),
        TaskState::Failed | TaskState::Blocked => Some("Open"),
        _ => None,
    }
}

/// The button of a task that waits for the user (a quiet task has no label, so it has none).
pub fn next_button(task: &Task, now_ms: i64) -> Option<&'static str> {
    let waits = needs_me(task.state) || is_quiet(task, now_ms);
    next_label(task.state).filter(|_| waits)
}

/// What a task that shows no button says: what its agent does, or "Working".
pub fn next_text(task: &Task, activity: Option<&str>) -> String {
    if task.state == TaskState::Running {
        activity.unwrap_or("Working").to_owned()
    } else {
        String::new()
    }
}

/// The line before and the line after each cell. None between the stages of one group, and none
/// between stages of two paths.
pub fn cell_links(progress: &[StageProgress]) -> Vec<(LinkKind, LinkKind)> {
    progress
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let before = index.checked_sub(1).and_then(|at| progress.get(at));
            let after = progress.get(index + 1);
            let side_by_side = |other: Option<&StageProgress>| {
                entry.group.is_some() && other.is_some_and(|other| other.group == entry.group)
            };
            let left = if side_by_side(before) || !same_lane(before, Some(entry)) {
                None
            } else {
                before
            };
            let right = if side_by_side(after) || !same_lane(Some(entry), after) {
                None
            } else {
                after
            };
            (link_kind(left, Some(entry)), link_kind(Some(entry), right))
        })
        .collect()
}

#[cfg(test)]
#[path = "progress_board_rules_tests.rs"]
mod tests;
