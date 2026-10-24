//! `terminalLane` of `packages/core/src/stages/terminal-lane.ts`: the stage a terminal works or
//! worked on, and its state.

use natamux_proto::stages::position::step_to_run;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState;
use natamux_proto::terminals::list::TerminalLane;

use crate::stages::capabilities::stages_before;
use crate::stages::position::{Paths, StagePosition, get};
use crate::stages::terminal_lane::{PinnedTerminals, stage_of_terminal, terminal_of_stage_pinned};

/// A terminal that no stage up to now uses.
pub const NO_LANE: TerminalLane = TerminalLane {
    stage: None,
    state: None,
    agent: None,
};

fn lane(stage: &str, state: TaskState) -> TerminalLane {
    TerminalLane {
        stage: Some(stage.to_string()),
        state: Some(state),
        agent: None,
    }
}

/// In a group each terminal has the state of its own lane, not the state of the whole task.
pub fn terminal_lane(
    position: &StagePosition,
    steps: &[Step],
    terminal: &str,
    pinned: &PinnedTerminals,
) -> TerminalLane {
    let Some(step) = step_to_run(steps, &position.stage) else {
        return if terminal == "1" {
            lane(&position.stage, position.state)
        } else {
            NO_LANE
        };
    };
    match &step {
        Step::Stage(stage) => {
            if terminal == terminal_of_stage_pinned(steps, &stage.id, pinned) {
                return lane(&stage.id, position.state);
            }
        }
        Step::Group(_) => {
            if let Some(stage) = stage_of_terminal(steps, &step, terminal, pinned) {
                let state = get(&position.lanes, &stage.id).copied();
                return lane(&stage.id, state.unwrap_or(TaskState::Done));
            }
        }
        Step::Branch(_) => {}
    }
    earlier_lane(steps, &step, terminal, position.paths.as_ref(), pinned)
}

/// A stage's terminal stays open after its step, so it keeps the name of that stage. Terminal 1
/// serves many stages: the last one before the current step names it. Paths not taken never ran.
pub fn earlier_lane(
    steps: &[Step],
    current: &Step,
    terminal: &str,
    paths: Option<&Paths>,
    pinned: &PinnedTerminals,
) -> TerminalLane {
    stages_before(steps, current.id(), paths)
        .iter()
        .rfind(|each| terminal_of_stage_pinned(steps, &each.id, pinned) == terminal)
        .map_or(NO_LANE, |stage| lane(&stage.id, TaskState::Done))
}

#[cfg(test)]
#[path = "lanes_tests.rs"]
mod tests;
