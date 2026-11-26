//! Pure rules of the Progress layout of All tasks (`ProgressBoard.tsx`, `stepsKey` of
//! `stage-view.ts`): the sections, the order of the rows,
//! the counts in the stage headers, the Next column and the links between the cells. `now_ms` is
//! passed in, so no rule reads the clock.

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::position::{step_index, step_to_run};
use natamux_proto::stages::progress::StageProgress;
use natamux_proto::stages::resolve::{active_steps, resolve_steps, step_refs_of};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::Value;

use super::stage_view_rules::{LinkKind, link_kind, same_lane};
use crate::sidebar::sidebar_tree::steps_of;
use crate::sidebar::workspace_groups::{DEFAULT_REPO_ID, RepoRef, known_repos};
use crate::tasks::task_attention::{Attention, is_quiet};
use crate::tasks::task_changes::task_of;
use crate::tasks::task_rules::{needs_me, state_from_name};

/// One box of the board: the open and the done tasks that share a set of steps. The board
/// shares its tasks with the dashboard, so a section costs no copy of a task.
#[derive(Clone, Debug, PartialEq)]
pub struct Section<T = Arc<Task>> {
    pub key: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub open: Vec<T>,
    pub done: Vec<T>,
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
pub fn sort_rows<T: Borrow<Task> + Clone>(
    tasks: &[T],
    steps: &[Step],
    at: impl Into<Attention>,
) -> Vec<T> {
    let at = at.into();
    let waits = |task: &Task| needs_me(task.state) || is_quiet(task, at);
    let mut rows = tasks.to_vec();
    rows.sort_by_key(|held| {
        let task = task_of(held);
        (!waits(task), -step_position(steps, &task.stage))
    });
    rows
}

/// One section per project with open tasks (cancelled ones are left out), and one more for its
/// tasks made before its pipeline changed. Without the settings the tasks show the stages they
/// carry. The tasks are held by value or shared, and a section holds them the same way.
pub fn board_sections<T: Borrow<Task> + Clone>(
    tasks: &[T],
    repos: &[RepoRef],
    pipelines: &BTreeMap<String, Vec<Step>>,
    attention: impl Into<Attention>,
) -> Vec<Section<T>> {
    let attention = attention.into();
    let mut sections = Vec::new();
    for repo in known_repos(repos, tasks) {
        let own: Vec<&T> = tasks
            .iter()
            .filter(|held| {
                let task = task_of(*held);
                task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID) == repo.id
                    && task.state != TaskState::Cancelled
            })
            .collect();
        if own.is_empty() {
            continue;
        }
        let loaded = pipelines.get(&repo.id);
        let pipeline: &[Step] = loaded.map_or(&[], Vec::as_slice);
        // One pass: each task is keyed once, and the groups keep the order the keys first show.
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<(String, Vec<&T>)> = Vec::new();
        for task in own {
            let key = steps_key(task_of(task));
            let index = *at.entry(key.clone()).or_insert_with(|| {
                groups.push((key, Vec::new()));
                groups.len() - 1
            });
            if let Some((_, members)) = groups.get_mut(index) {
                members.push(task);
            }
        }
        // New tasks get the stages that are on, so tasks made since the last change carry these.
        let current = current_key(pipeline);
        groups.sort_by_key(|(key, _)| *key != current);
        for (index, (key, members)) in groups.iter().enumerate() {
            let Some(first) = members.first() else {
                continue;
            };
            let steps = resolve_steps(&steps_of(task_of(*first)), pipeline);
            let (done, open): (Vec<T>, Vec<T>) = members
                .iter()
                .copied()
                .cloned()
                .partition(|task| task_of(task).state == TaskState::Done);
            let title = if index == 0 && (loaded.is_none() || key == &current) {
                repo.name.clone()
            } else {
                format!("{}, earlier pipeline", repo.name)
            };
            sections.push(Section {
                key: format!("{}:{key}", repo.id),
                title,
                open: sort_rows(&open, &steps, attention),
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

pub fn stage_columns<T: Borrow<Task>>(steps: &[Step], open: &[T]) -> Vec<StageColumn> {
    steps
        .iter()
        .flat_map(|step| {
            let in_path = matches!(step, Step::Branch(_));
            step.stages().into_iter().map(move |stage| (stage, in_path))
        })
        .map(|(stage, in_path)| {
            // The group the stage runs in: its step, or a group inside a path.
            let group = step_to_run(steps, &stage.id)
                .filter(Step::is_group)
                .map(|group| group.id().to_string());
            let in_group = group.is_some();
            let here: Vec<&Task> = open
                .iter()
                .map(task_of)
                .filter(|task| {
                    let Some(step) = step_index(steps, &task.stage).and_then(|at| steps.get(at))
                    else {
                        return false;
                    };
                    // A task is in one path of a branch only, the one of its stage.
                    let holds = if in_path {
                        task.stage == stage.id || group.as_deref() == Some(task.stage.as_str())
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
pub fn next_button(task: &Task, at: impl Into<Attention>) -> Option<&'static str> {
    let waits = needs_me(task.state) || is_quiet(task, at);
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
