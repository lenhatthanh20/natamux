//! Applies one event to a task's position in its steps (`packages/core/src/events/apply.ts`).

use natamux_proto::events::task_event::{RECORD_ONLY_EVENT_TYPES, TaskEvent};
use natamux_proto::stages::position::{path_of, step_to_run};
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::TaskState;
use serde_json::Value;

use crate::events::moves::{Move, MoveTo, lane_of, move_of, payload_string, with_paths};
use crate::events::reopen::{reopen, reopen_target, sends_back};
use crate::stages::position::{
    StagePosition, combined_state, enter_step, get, step_index, taken_stages, with,
};
use crate::stages::terminal_lane::PinnedTerminals;
use crate::tasks::state::is_terminal_state;

/// The result of an event: the new position, whether it fit, and whether the step changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedEvent {
    pub position: StagePosition,
    pub accepted: bool,
    pub moved: bool,
}

fn keep(position: &StagePosition, accepted: bool) -> AppliedEvent {
    AppliedEvent {
        position: position.clone(),
        accepted,
        moved: false,
    }
}

fn state_named(value: Option<&Value>) -> Option<TaskState> {
    let text = value?.as_str()?;
    natamux_proto::tasks::task::TASK_STATES
        .contains(&text)
        .then(|| serde_json::from_value(Value::String(text.into())).ok())?
}

/// The event as it is logged: the stage it is about, and for a send back the stage it goes back
/// to, filled in when the sender left them out.
pub fn complete_event(
    position: &StagePosition,
    event: TaskEvent,
    steps: &[Step],
    pinned: &PinnedTerminals,
) -> TaskEvent {
    let index = step_index(steps, &position.stage);
    let Some(step) = step_to_run(steps, &position.stage) else {
        return event;
    };
    if is_terminal_state(position.state) {
        return event;
    }
    if sends_back(&event, &step) {
        if payload_string(&event, "to").is_some() {
            return event;
        }
        return match reopen_target(steps, position, index, &event) {
            None => event,
            Some((_, stage)) => with_payload(event, "to", stage),
        };
    }
    let Some(movement) = move_of(&event) else {
        return event;
    };
    if payload_string(&event, "stage").is_some() {
        return event;
    }
    match lane_of(steps, &step, position, &event, &movement, pinned) {
        None => event,
        Some(lane) => with_payload(event, "stage", lane),
    }
}

fn with_payload(mut event: TaskEvent, key: &str, value: String) -> TaskEvent {
    event.payload.insert(key.into(), Value::String(value));
    event
}

/// After a stage ends: the next stage of its path, the next step, or done after the last one.
fn finish_lane(
    steps: &[Step],
    index: i64,
    position: &StagePosition,
    lane: &str,
    path: Option<&str>,
) -> (StagePosition, bool) {
    if let Some(Step::Group(_)) = usize::try_from(index).ok().and_then(|at| steps.get(at)) {
        let lanes = with(&position.lanes, lane, TaskState::Done);
        if lanes.iter().any(|(_, state)| *state != TaskState::Done) {
            let state = combined_state(&lanes);
            return (
                StagePosition {
                    lanes,
                    state,
                    ..position.clone()
                },
                false,
            );
        }
    }
    let following = path_of(steps, lane).and_then(|place| place.path.stages.get(place.index + 1));
    if let Some(following) = following {
        let next = enter_step(&Step::Stage(following.clone()), None, None);
        return (with_paths(next, position.paths.clone()), true);
    }
    let mut paths = position.paths.clone();
    let mut chosen = path.map(str::to_string);
    let start = usize::try_from(index + 1).unwrap_or(0);
    for next in steps.iter().skip(start) {
        let Step::Branch(branch) = next else {
            return (with_paths(enter_step(next, None, None), paths), true);
        };
        // A branch takes the path the event names. Without one, or with a path the task does not
        // have, the task skips the branch. Both are recorded, a skip as ''.
        let taken = branch
            .paths
            .iter()
            .find(|each| Some(&each.id) == chosen.as_ref())
            .map_or(String::new(), |each| each.id.clone());
        paths = Some(with(&paths.unwrap_or_default(), &branch.id, taken.clone()));
        if !taken.is_empty() {
            return (
                with_paths(enter_step(next, None, Some(&taken)), paths),
                true,
            );
        }
        // The event chose a path for the first branch it reaches only.
        chosen = None;
    }
    let done = StagePosition {
        state: TaskState::Done,
        lanes: Vec::new(),
        ..position.clone()
    };
    (with_paths(done, paths), false)
}

/// A state set by hand: on a stage (it is reopened), on a group (every lane that is not done),
/// or on the current step.
fn override_state(
    position: &StagePosition,
    event: &TaskEvent,
    steps: &[Step],
    step: Option<&Step>,
) -> AppliedEvent {
    let Some(target) = state_named(event.payload.get("to")) else {
        return keep(position, false);
    };
    if let Some(stage) = payload_string(event, "stage") {
        let at = step_index(steps, stage);
        let moved = if at >= 0 {
            reopen(steps, at, stage, position, Some(target))
        } else {
            None
        };
        return match moved {
            None => keep(position, false),
            Some(moved) => AppliedEvent {
                position: moved,
                accepted: true,
                moved: true,
            },
        };
    }
    let mut next = position.clone();
    if let Some(Step::Group(_)) = step {
        next.lanes = position
            .lanes
            .iter()
            .map(|(id, state)| {
                let lane = if *state == TaskState::Done {
                    *state
                } else {
                    target
                };
                (id.clone(), lane)
            })
            .collect();
    }
    next.state = target;
    AppliedEvent {
        position: next,
        accepted: true,
        moved: false,
    }
}

/// A merged pull request finishes a task that is at review or later.
fn complete_on_merge(position: &StagePosition, steps: &[Step], index: i64) -> AppliedEvent {
    let review_or_later =
        |stage: &Stage| matches!(stage.kind, StageKind::Review | StageKind::Release);
    let first_review = steps
        .iter()
        .position(|candidate| {
            taken_stages(candidate, position.paths.as_ref())
                .iter()
                .any(review_or_later)
        })
        .map_or(-1, |at| at as i64);
    // The stages of a path share the index of their branch, so the stage must also be at or
    // after the review of its path.
    let early = index == first_review
        && path_of(steps, &position.stage).is_some_and(|place| {
            !place.path.stages[..=place.index]
                .iter()
                .any(review_or_later)
        });
    if first_review < 0 || index < first_review || early {
        return keep(position, false);
    }
    // A branch is no stage to be at: at the end, the task ends on the last stage of its path.
    let end = match steps.last() {
        Some(last @ Step::Branch(_)) => {
            taken_stages(last, Some(&position.paths.clone().unwrap_or_default()))
                .last()
                .map(|stage| stage.id.clone())
        }
        Some(last) => Some(last.id().to_string()),
        None => None,
    };
    let done = StagePosition {
        stage: end.clone().unwrap_or_else(|| position.stage.clone()),
        state: TaskState::Done,
        lanes: Vec::new(),
        paths: None,
    };
    AppliedEvent {
        position: with_paths(done, position.paths.clone()),
        accepted: true,
        moved: end.is_some_and(|end| end != position.stage),
    }
}

fn send_back(
    position: &StagePosition,
    event: &TaskEvent,
    steps: &[Step],
    index: i64,
) -> AppliedEvent {
    // The stage starts again, so the runner gives it its prompt and it gets a new round.
    let moved = reopen_target(steps, position, index, event)
        .and_then(|(at, stage)| reopen(steps, at, &stage, position, None));
    match moved {
        None => keep(position, false),
        Some(moved) => AppliedEvent {
            position: moved,
            accepted: true,
            moved: true,
        },
    }
}

/// A plain move of a stage or a lane: start, ask, end. An end goes to the gate or on to the next
/// stage, by the gate of the stage.
fn move_stage(
    position: &StagePosition,
    event: &TaskEvent,
    steps: &[Step],
    index: i64,
    step: &Step,
    movement: &Move,
) -> AppliedEvent {
    let group = step.is_group();
    let lane = lane_of(steps, step, position, event, movement, &Vec::new());
    let stage = lane.as_ref().and_then(|lane| {
        step.stages()
            .into_iter()
            .find(|candidate| &candidate.id == lane)
    });
    let (Some(lane), Some(stage)) = (lane.clone(), stage) else {
        return keep(position, false);
    };
    let current = if group {
        get(&position.lanes, &lane).copied()
    } else {
        Some(position.state)
    };
    if !current.is_some_and(|state| movement.from.contains(&state)) {
        return keep(position, false);
    }
    let to = match movement.to {
        MoveTo::Gate if stage.gate == Gate::Auto => MoveTo::Finish,
        MoveTo::Gate => MoveTo::State(TaskState::Review),
        other => other,
    };
    let state = match to {
        MoveTo::State(state) => state,
        _ => {
            let path = payload_string(event, "path");
            let (position, moved) = finish_lane(steps, index, position, &lane, path);
            return AppliedEvent {
                position,
                accepted: true,
                moved,
            };
        }
    };
    let mut next = position.clone();
    if group {
        next.lanes = with(&position.lanes, &lane, state);
        next.state = combined_state(&next.lanes);
    } else {
        next.state = state;
    }
    AppliedEvent {
        position: next,
        accepted: true,
        moved: false,
    }
}

/// Applies one event. An event that does not fit is not accepted and leaves the position as it is.
pub fn apply_task_event(
    position: &StagePosition,
    event: &TaskEvent,
    steps: &[Step],
) -> AppliedEvent {
    if RECORD_ONLY_EVENT_TYPES.contains(&event.kind.as_str()) {
        return keep(position, true);
    }
    let index = step_index(steps, &position.stage);
    let step = step_to_run(steps, &position.stage);
    if event.kind == "state.overridden" {
        return override_state(position, event, steps, step.as_ref());
    }
    if is_terminal_state(position.state) {
        return keep(position, false);
    }
    if event.kind == "task.cancelled" {
        let mut next = position.clone();
        next.state = TaskState::Cancelled;
        return AppliedEvent {
            position: next,
            accepted: true,
            moved: false,
        };
    }
    if event.kind == "pr.merged" {
        return complete_on_merge(position, steps, index);
    }
    let Some(step) = step else {
        return keep(position, false);
    };
    if sends_back(event, &step) {
        return send_back(position, event, steps, index);
    }
    match move_of(event) {
        None => keep(position, false),
        Some(movement) => move_stage(position, event, steps, index, &step, &movement),
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "apply_branch_tests.rs"]
mod branch_tests;
