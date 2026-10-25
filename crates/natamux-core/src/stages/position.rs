//! Where a task is in its steps (`packages/core/src/stages/position.ts`): what `natamux-proto`'s
//! `position.rs` lacks. Lanes and paths keep the order of a JS object.

use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::TaskState;

/// A JS object of stage id to state, in insertion order.
pub type Lanes = Vec<(String, TaskState)>;
/// Branch id to path id, '' when the task skipped the branch.
pub type Paths = Vec<(String, String)>;

pub fn get<'a, T>(entries: &'a [(String, T)], key: &str) -> Option<&'a T> {
    entries
        .iter()
        .find(|(id, _)| id == key)
        .map(|(_, value)| value)
}

/// `{ ...entries, [key]: value }`: a key that is there keeps its place.
pub fn with<T: Clone>(entries: &[(String, T)], key: &str, value: T) -> Vec<(String, T)> {
    let mut next = entries.to_vec();
    match next.iter_mut().find(|(id, _)| id == key) {
        Some(entry) => entry.1 = value,
        None => next.push((key.to_string(), value)),
    }
    next
}

/// Where a task is: its step, the state of that step, and for a group the state of each stage.
/// Inside a branch the step is the current stage of the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagePosition {
    pub stage: String,
    pub state: TaskState,
    pub lanes: Lanes,
    pub paths: Option<Paths>,
}

impl StagePosition {
    pub fn new(stage: &str, state: TaskState) -> StagePosition {
        StagePosition {
            stage: stage.into(),
            state,
            lanes: Vec::new(),
            paths: None,
        }
    }
}

// The lane states, the most urgent first. A group's state is its most urgent lane.
const URGENCY: [TaskState; 7] = [
    TaskState::NeedsInput,
    TaskState::Failed,
    TaskState::Blocked,
    TaskState::Review,
    TaskState::Running,
    TaskState::Ready,
    TaskState::Draft,
];

pub fn combined_state(lanes: &[(String, TaskState)]) -> TaskState {
    URGENCY
        .into_iter()
        .find(|state| lanes.iter().any(|(_, lane)| lane == state))
        .unwrap_or(TaskState::Done)
}

/// The state a stage starts in. A stage that only waits for a person waits at once.
pub fn entry_state(stage: &Stage) -> TaskState {
    if matches!(stage.kind, StageKind::Review | StageKind::Manual) {
        TaskState::Review
    } else {
        TaskState::Ready
    }
}

/// A branch is entered at the first stage of `path`; without a path of the branch there is
/// nothing to enter and the caller moves past it.
pub fn enter_step(step: &Step, state: Option<TaskState>, path: Option<&str>) -> StagePosition {
    match step {
        Step::Branch(branch) => {
            let first = branch
                .paths
                .iter()
                .find(|each| Some(each.id.as_str()) == path)
                .and_then(|each| each.stages.first());
            match first {
                Some(first) => enter_step(&Step::Stage(first.clone()), state, None),
                None => StagePosition::new(&branch.id, state.unwrap_or(TaskState::Ready)),
            }
        }
        Step::Stage(stage) => {
            StagePosition::new(&stage.id, state.unwrap_or_else(|| entry_state(stage)))
        }
        Step::Group(group) => {
            let lanes: Lanes = group
                .parallel
                .iter()
                .map(|stage| {
                    (
                        stage.id.clone(),
                        state.unwrap_or_else(|| entry_state(stage)),
                    )
                })
                .collect();
            StagePosition {
                stage: group.id.clone(),
                state: combined_state(&lanes),
                lanes,
                paths: None,
            }
        }
    }
}

/// A new task waits at its first step until it is readied.
pub fn first_position(steps: &[Step]) -> StagePosition {
    match steps.first() {
        None => StagePosition::new("implement", TaskState::Draft),
        Some(first) => enter_step(first, Some(TaskState::Draft), None),
    }
}

/// `takenStages`: the stages of a step on the way the task went.
pub fn taken_stages(step: &Step, paths: Option<&Paths>) -> Vec<Stage> {
    match (step, paths) {
        (Step::Branch(branch), Some(paths)) => {
            let taken = get(paths, &branch.id);
            branch
                .paths
                .iter()
                .find(|path| Some(&path.id) == taken)
                .map(|path| path.stages.clone())
                .unwrap_or_default()
        }
        _ => step.stages().into_iter().cloned().collect(),
    }
}

/// `isNotTaken` with typed paths.
pub fn is_not_taken(steps: &[Step], paths: Option<&Paths>, stage_id: &str) -> bool {
    let Some(place) = natamux_proto::stages::position::path_of(steps, stage_id) else {
        return false;
    };
    paths
        .and_then(|paths| get(paths, &place.branch.id))
        .is_some_and(|taken| *taken != place.path.id)
}

/// `stepIndex` as JS gives it: -1 for none.
pub fn step_index(steps: &[Step], id: &str) -> i64 {
    natamux_proto::stages::position::step_index(steps, id).map_or(-1, |index| index as i64)
}

#[cfg(test)]
#[path = "position_tests.rs"]
mod tests;
