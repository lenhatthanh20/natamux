//! Sending a task back to a stage (`packages/core/src/events/reopen.ts`).

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::position::{path_of, step_to_run};
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::TaskState;

use crate::events::moves::{lane_state_of, payload_string, with_paths};
use crate::stages::position::{
    Lanes, StagePosition, combined_state, entry_state, get, is_not_taken, step_index, taken_stages,
    with,
};

/// Events that send a task back to a stage. A prompt typed to the agent during review is a
/// request for changes.
pub fn sends_back(event: &TaskEvent, step: &Step) -> bool {
    event.kind == "stage.reopened"
        || event.kind == "review.changes-requested"
        || (event.kind == "agent.resumed"
            && matches!(step, Step::Stage(stage) if stage.kind == StageKind::Review))
}

/// The stage to go back to: `to`; for changes asked about an agent stage that waits for you,
/// that stage; else the last agent stage before the current step. Only stages on the way the
/// task went count.
pub fn reopen_target(
    steps: &[Step],
    position: &StagePosition,
    current: i64,
    event: &TaskEvent,
) -> Option<(i64, String)> {
    let here = path_of(steps, &position.stage);
    if let Some(to) = payload_string(event, "to") {
        let index = step_index(steps, to);
        let there = path_of(steps, to);
        let ahead = match (&here, &there) {
            (Some(here), Some(there)) => {
                std::ptr::eq(there.path, here.path) && there.index > here.index
            }
            _ => false,
        };
        let fits = index >= 0
            && index <= current
            && !ahead
            && !is_not_taken(steps, position.paths.as_ref(), to);
        return fits.then(|| (index, to.to_string()));
    }
    let step = step_to_run(steps, &position.stage);
    if let (true, Some(step)) = (event.kind == "review.changes-requested", &step) {
        let waiting: Vec<&Stage> = step
            .stages()
            .into_iter()
            .filter(|stage| {
                let state = lane_state_of(position, step, &stage.id);
                stage.kind == StageKind::Agent
                    && matches!(state, Some(TaskState::Review | TaskState::NeedsInput))
            })
            .collect();
        let code = agent_stage_of(step);
        let chosen = waiting
            .iter()
            .find(|stage| Some(&stage.id) == code.as_ref().map(|code| &code.id))
            .or(waiting.first());
        if let Some(chosen) = chosen {
            return Some((current, chosen.id.clone()));
        }
    }
    let mut earlier: Vec<(i64, Step)> = Vec::new();
    for (index, each) in steps.iter().take(current.max(0) as usize).enumerate() {
        if each.is_branch() {
            for unit in taken_stages(each, position.paths.as_ref()) {
                earlier.push((index as i64, Step::Stage(unit)));
            }
        } else {
            earlier.push((index as i64, each.clone()));
        }
    }
    if let Some(here) = &here {
        for unit in &here.path.stages[..here.index] {
            earlier.push((current, Step::Stage(unit.clone())));
        }
    }
    earlier
        .iter()
        .rev()
        .find_map(|(index, unit)| agent_stage_of(unit).map(|stage| (*index, stage.id)))
}

/// The stage of a step that changes code: an agent stage with no output file.
fn agent_stage_of(step: &Step) -> Option<Stage> {
    let agents: Vec<&Stage> = step
        .stages()
        .into_iter()
        .filter(|stage| stage.kind == StageKind::Agent)
        .collect();
    agents
        .iter()
        .find(|stage| stage.output.is_none())
        .or(agents.first())
        .map(|stage| (*stage).clone())
}

/// The position after going back to `stage` (a stage, or a whole group) at step `index`. The stage
/// gets `state`, or the state it starts in. The choices made at that step and after are dropped.
pub fn reopen(
    steps: &[Step],
    index: i64,
    stage: &str,
    position: &StagePosition,
    state: Option<TaskState>,
) -> Option<StagePosition> {
    let step = steps.get(usize::try_from(index).ok()?)?;
    let state_of = |target: &Stage| state.unwrap_or_else(|| entry_state(target));
    let paths = position.paths.as_ref().map(|paths| {
        paths
            .iter()
            .filter(|(id, _)| step_index(steps, id) < index)
            .cloned()
            .collect::<Vec<_>>()
    });
    match step {
        Step::Branch(branch) => {
            let place = path_of(steps, stage)?;
            let target = place.path.stages.get(place.index)?;
            if is_not_taken(steps, position.paths.as_ref(), stage) {
                return None;
            }
            let taken = with(
                &paths.unwrap_or_default(),
                &branch.id,
                place.path.id.clone(),
            );
            Some(StagePosition {
                stage: stage.to_string(),
                state: state_of(target),
                lanes: Vec::new(),
                paths: Some(taken),
            })
        }
        Step::Stage(each) => Some(with_paths(
            StagePosition::new(&each.id, state_of(each)),
            paths,
        )),
        Step::Group(group) => {
            let here = group.id == position.stage;
            let lanes: Lanes = group
                .parallel
                .iter()
                .map(|lane| {
                    let lane_state = if lane.id == stage || group.id == stage {
                        state_of(lane)
                    } else if here {
                        get(&position.lanes, &lane.id)
                            .copied()
                            .unwrap_or(TaskState::Done)
                    } else {
                        TaskState::Done
                    };
                    (lane.id.clone(), lane_state)
                })
                .collect();
            let state = combined_state(&lanes);
            Some(with_paths(
                StagePosition {
                    stage: group.id.clone(),
                    state,
                    lanes,
                    paths: None,
                },
                paths,
            ))
        }
    }
}

#[cfg(test)]
#[path = "reopen_tests.rs"]
mod tests;
