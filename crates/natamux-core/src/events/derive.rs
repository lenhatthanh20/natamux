//! `entersBranch` and `deriveTaskState` (`packages/core/src/events/derive.ts`).

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{StageBranch, Step};

use crate::events::apply::apply_task_event;
use crate::stages::position::{StagePosition, first_position, get};

/// The branch an event would enter: the event ends the current step, and the next step is a
/// branch. A dry run that ignores any path the event names.
pub fn enters_branch(
    position: &StagePosition,
    event: &TaskEvent,
    steps: &[Step],
) -> Option<StageBranch> {
    let before = position.paths.clone().unwrap_or_default();
    let mut dry = event.clone();
    dry.payload.remove("path");
    let start = StagePosition {
        paths: Some(before.clone()),
        ..position.clone()
    };
    let after = apply_task_event(&start, &dry, steps)
        .position
        .paths
        .unwrap_or_default();
    let entered = steps.iter().find(|step| {
        get(&after, step.id()).is_some_and(String::is_empty) && get(&before, step.id()).is_none()
    })?;
    match entered {
        Step::Branch(branch) => Some(branch.clone()),
        _ => None,
    }
}

/// Events applied in order from the first step; those not accepted come back in the second list.
pub fn derive_task_state(events: &[TaskEvent], steps: &[Step]) -> (StagePosition, Vec<TaskEvent>) {
    let mut position = first_position(steps);
    let mut rejected = Vec::new();
    for event in events {
        let applied = apply_task_event(&position, event, steps);
        position = applied.position;
        if !applied.accepted {
            rejected.push(event.clone());
        }
    }
    (position, rejected)
}

#[cfg(test)]
#[path = "derive_tests.rs"]
mod tests;
