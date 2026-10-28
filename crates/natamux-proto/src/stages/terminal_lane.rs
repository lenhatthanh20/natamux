//! The terminal of a stage from the settings (`packages/core/src/stages/terminal-lane.ts`). The
//! pinned terminals, the lane of a terminal and `endsShell1` are daemon rules (phase 4).

use crate::stages::position::{code_stage, step_to_run};
use crate::stages::schema::{Stage, StageKind, Step};

/// The terminal of a stage that runs on its own apart from Terminal 1. Terminals 2 to 4 go to the
/// stages of a group, so it does not take over the agent of one.
pub const OWN_SHELL: &str = "5";

/// Each stage that starts its own agent or arguments gets Terminal 6 plus its index among all
/// stages of the task, so no two stages share one.
const FIRST_STAGE_SHELL: usize = 6;

/// `terminalOfStage` for a stage that never started: the terminal the settings give.
pub fn terminal_of_stage(steps: &[Step], stage_id: &str) -> String {
    let all: Vec<&Stage> = steps.iter().flat_map(Step::stages).collect();
    let found = all.iter().position(|stage| stage.id == stage_id);
    let own = found.filter(|index| runs_own_agent(all[*index]));
    if let Some(index) = own {
        return (FIRST_STAGE_SHELL + index).to_string();
    }
    let Some(step) = step_to_run(steps, stage_id) else {
        return "1".into();
    };
    match &step {
        Step::Branch(_) => "1".into(),
        Step::Stage(stage) => (if stage.own_shell == Some(true) {
            OWN_SHELL
        } else {
            "1"
        })
        .into(),
        Step::Group(group) => {
            let main = code_stage(&step).map(|stage| stage.id.as_str());
            if main == Some(stage_id) {
                return "1".into();
            }
            let others = group
                .parallel
                .iter()
                .filter(|stage| Some(stage.id.as_str()) != main);
            let at = others.into_iter().position(|stage| stage.id == stage_id);
            // A stage that is not in the group counts as before: -1 + 2.
            at.map_or_else(|| "1".into(), |at| (at + 2).to_string())
        }
    }
}

/// `canHaveOwnAgent`: a stage that an agent works on may use another one, wherever it runs. One
/// that would run in Terminal 1 then runs in a terminal of its own, so Terminal 1 keeps the task's
/// agent and its session.
pub fn can_have_own_agent(stage: &Stage) -> bool {
    matches!(stage.kind, StageKind::Agent | StageKind::Testing)
}

fn runs_own_agent(stage: &Stage) -> bool {
    (stage.agent.is_some() || stage.args.is_some()) && can_have_own_agent(stage)
}

#[cfg(test)]
#[path = "terminal_lane_tests.rs"]
mod tests;
