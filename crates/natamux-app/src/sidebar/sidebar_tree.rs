//! The rules of the sidebar tree (`features/sidebar/sidebar-tree.ts`): folds, urgency, the stage
//! a row shows. Rows under a task are `shell_rows.rs`; the progress line is `comet.rs`.

// Ported with their tests (spec D11); the stage rules serve the details panel of screen 3.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::stages::capabilities::stage_name_of_id;
pub use natamux_proto::stages::schema::{PathRef, StepRef};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::Value;

/// Which project groups, finished lists and task rows the user opened (true) or closed (false),
/// oldest choice first.
pub type Folds = Vec<(String, bool)>;

// Old groups and tasks do not need a remembered fold forever.
const MAX_FOLDS: usize = 300;

/// Reads saved folds and drops anything that is not true or false.
pub fn parse_folds(raw: Option<&str>) -> Folds {
    match serde_json::from_str::<Value>(raw.unwrap_or("{}")) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(key, open)| open.as_bool().map(|open| (key, open)))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn with_fold(folds: &[(String, bool)], key: &str, open: bool) -> Folds {
    let mut next: Folds = folds
        .iter()
        .filter(|(each, _)| each != key)
        .cloned()
        .collect();
    next.push((key.to_owned(), open));
    let extra = next.len().saturating_sub(MAX_FOLDS);
    next.drain(..extra);
    next
}

pub fn is_fold_open(folds: &[(String, bool)], key: &str, fallback: bool) -> bool {
    folds
        .iter()
        .find(|(each, _)| each == key)
        .map_or(fallback, |(_, open)| *open)
}

// The order of the most urgent first.
const URGENT: [TaskState; 6] = [
    TaskState::NeedsInput,
    TaskState::Failed,
    TaskState::Blocked,
    TaskState::Review,
    TaskState::Running,
    TaskState::Ready,
];

/// The state a folded row shows for its children, so a failed or waiting child is never hidden.
pub fn urgent_state(states: &[TaskState]) -> Option<TaskState> {
    URGENT.into_iter().find(|state| states.contains(state))
}

/// How much room a row takes: what needs the user is a card, what is finished is one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTier {
    Attention,
    Live,
    Quiet,
}

pub fn row_tier(state: TaskState) -> RowTier {
    match state {
        TaskState::NeedsInput | TaskState::Failed | TaskState::Blocked => RowTier::Attention,
        TaskState::Done | TaskState::Cancelled | TaskState::Draft => RowTier::Quiet,
        _ => RowTier::Live,
    }
}

/// A row opens by itself while one of its children works, waits or failed.
pub fn opens_by_default(states: &[TaskState]) -> bool {
    states.iter().any(|state| {
        matches!(
            state,
            TaskState::Running | TaskState::NeedsInput | TaskState::Failed
        )
    })
}

/// `STATE_LABEL` in `lib/tasks.ts`. The daemon's own label for a review is "Review".
pub fn state_label(state: TaskState) -> &'static str {
    match state {
        TaskState::Review => "In review",
        other => other.label(),
    }
}

/// `agentLabel` in `lib/agents.ts`: the name the pickers show, else the id, else "No agent".
pub fn agent_label(id: Option<&str>) -> String {
    match id {
        Some("claude-code") => "Claude Code".into(),
        Some("codex") => "Codex".into(),
        Some("gemini") => "Gemini CLI".into(),
        Some("copilot") => "GitHub Copilot".into(),
        Some("cursor") => "Cursor".into(),
        Some(other) => other.to_owned(),
        None => "No agent".into(),
    }
}

/// `stageName` in core, for a stage with no name of its own (the rules are in `natamux_proto`).
pub fn stage_name(id: &str) -> String {
    stage_name_of_id(id)
}

fn strings(value: Option<&Value>) -> Vec<String> {
    let items = value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn step_of(value: &Value) -> StepRef {
    let Some(object) = value.as_object() else {
        return StepRef::Stage(value.as_str().unwrap_or_default().to_owned());
    };
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if let Some(parallel) = object.get("parallel") {
        return StepRef::Group {
            id,
            parallel: strings(Some(parallel)),
        };
    }
    if let Some(paths) = object.get("paths").and_then(Value::as_array) {
        let paths = paths
            .iter()
            .map(|path| PathRef {
                id: path
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                stages: strings(path.get("stages")),
            })
            .collect();
        return StepRef::Branch { id, paths };
    }
    StepRef::Stage(id)
}

/// The steps of the task's pipeline.
pub fn steps_of(task: &Task) -> Vec<StepRef> {
    task.stages.iter().map(step_of).collect()
}

/// The stages a step holds: itself, the lanes of a group, or the stages of every path.
pub fn stages_of_ref(step: &StepRef) -> Vec<&str> {
    match step {
        StepRef::Stage(id) => vec![id.as_str()],
        StepRef::Group { parallel, .. } => parallel.iter().map(String::as_str).collect(),
        StepRef::Branch { paths, .. } => paths
            .iter()
            .flat_map(|path| path.stages.iter().map(String::as_str))
            .collect(),
    }
}

/// Whether the step is where a task at `stage` is: the stage itself, or the id of its group or
/// branch, or a stage inside it.
pub fn step_holds(step: &StepRef, stage: &str) -> bool {
    match step {
        StepRef::Stage(id) => id == stage,
        StepRef::Group { id, .. } | StepRef::Branch { id, .. } => {
            id == stage || stages_of_ref(step).contains(&stage)
        }
    }
}

/// The state of one stage of a task, from `task.lanes`.
pub fn lane_state(task: &Task, stage: &str) -> Option<TaskState> {
    serde_json::from_value(task.lanes.get(stage)?.clone()).ok()
}

/// The stage a row shows. None for a task with no stage to show (`stageLabel` in `lib/tasks.ts`).
pub fn stage_label(task: &Task) -> Option<String> {
    match task.state {
        TaskState::Draft | TaskState::Done | TaskState::Cancelled => None,
        _ => Some(stage_name(&task.stage)),
    }
}

/// "Step 3 of 5".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagePosition {
    pub at: usize,
    pub of: usize,
}

/// "Step 3 of 5", from the steps the task has. A group of parallel stages is one step.
pub fn stage_position(task: &Task) -> Option<StagePosition> {
    let steps = steps_of(task);
    if steps.len() < 2 || stage_label(task).is_none() {
        return None;
    }
    let index = steps
        .iter()
        .position(|step| step_holds(step, &task.stage))?;
    Some(StagePosition {
        at: index + 1,
        of: steps.len(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubtaskCount {
    pub done: usize,
    pub total: usize,
}

pub fn subtask_count(id: &str, tasks: &[Task]) -> Option<SubtaskCount> {
    let own: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.parent_id.as_deref() == Some(id))
        .collect();
    if own.is_empty() {
        return None;
    }
    Some(SubtaskCount {
        done: own
            .iter()
            .filter(|task| task.state == TaskState::Done)
            .count(),
        total: own.len(),
    })
}

/// The filtered tasks plus the parent of each subtask among them, so a match is shown in its place.
pub fn with_parents(shown: &[Task], all: &[Task]) -> Vec<Task> {
    let mut result = shown.to_vec();
    for task in shown {
        let parent = all
            .iter()
            .find(|other| Some(other.id.as_str()) == task.parent_id.as_deref());
        if let Some(parent) = parent
            && parent.workspace.is_some()
            && !result.iter().any(|each| each.id == parent.id)
        {
            result.push(parent.clone());
        }
    }
    result
}

#[cfg(test)]
#[path = "sidebar_tree_tests.rs"]
mod tests;
