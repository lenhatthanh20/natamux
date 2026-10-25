//! The pinned terminals of `packages/core/src/stages/terminal-lane.ts`: the terminal each stage got
//! at its last start, and the stage a terminal works on.

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::stages::terminal_lane::{OWN_SHELL, can_have_own_agent, terminal_of_stage};
use serde_json::Value;

/// Stage id to the terminal it got at its last start.
pub type PinnedTerminals = Vec<(String, String)>;

/// From a task's accepted `stage.started` events.
pub fn pinned_terminals_of<'a>(
    events: impl IntoIterator<Item = (bool, &'a TaskEvent)>,
) -> PinnedTerminals {
    let mut pinned: PinnedTerminals = Vec::new();
    for (accepted, event) in events {
        let stage = event.payload.get("stage").and_then(Value::as_str);
        let terminal = event.payload.get("terminal").and_then(Value::as_str);
        if let (true, "stage.started", Some(stage), Some(terminal)) =
            (accepted, event.kind.as_str(), stage, terminal)
        {
            pinned = crate::stages::position::with(&pinned, stage, terminal.to_string());
        }
    }
    pinned
}

/// `terminalOfStage`: a stage keeps its pinned terminal until it starts again.
pub fn terminal_of_stage_pinned(
    steps: &[Step],
    stage_id: &str,
    pinned: &PinnedTerminals,
) -> String {
    crate::stages::position::get(pinned, stage_id)
        .cloned()
        .unwrap_or_else(|| terminal_of_stage(steps, stage_id))
}

/// `stageOfTerminal`: the stage of the step that works in this terminal.
pub fn stage_of_terminal(
    steps: &[Step],
    step: &Step,
    terminal: &str,
    pinned: &PinnedTerminals,
) -> Option<Stage> {
    step.stages()
        .into_iter()
        .find(|stage| terminal_of_stage_pinned(steps, &stage.id, pinned) == terminal)
        .cloned()
}

/// The agent and words of a stage that starts its own agent. A stage without them uses the
/// task's agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageAgent {
    pub agent: Option<String>,
    pub args: Option<String>,
}

/// `stageAgent`.
pub fn stage_agent(step: &Step, stage: &Stage) -> StageAgent {
    let own = (stage.agent.is_some() || stage.args.is_some()) && can_have_own_agent(step, stage);
    if !own {
        return StageAgent::default();
    }
    StageAgent {
        agent: stage.agent.clone(),
        args: stage.args.clone(),
    }
}

// Same order as the Node kinds; only equality is used.
#[derive(PartialEq, Eq)]
enum TerminalKind {
    One,
    OwnShell,
    Lane,
    OwnAgent,
}

fn terminal_kind(terminal: &str) -> TerminalKind {
    // JS `Number()`: text that is no number is NaN, which is none of the cases below.
    let number = terminal.trim().parse::<f64>().unwrap_or(f64::NAN);
    if number >= 6.0 {
        TerminalKind::OwnAgent
    } else if terminal == OWN_SHELL {
        TerminalKind::OwnShell
    } else if number == 1.0 {
        TerminalKind::One
    } else {
        TerminalKind::Lane
    }
}

/// `terminalAtStart`: at a new start a stage keeps its pinned terminal while the settings give the
/// same kind of terminal. When the kind changed, it takes the terminal the settings give now.
pub fn terminal_at_start(steps: &[Step], stage_id: &str, pinned: &PinnedTerminals) -> String {
    let now = terminal_of_stage(steps, stage_id);
    match crate::stages::position::get(pinned, stage_id) {
        Some(pin) if terminal_kind(pin) == terminal_kind(&now) => pin.clone(),
        _ => now,
    }
}

/// `endsShell1`: Terminal 1 keeps the task's agent and its session, so endShell never closes it. A
/// testing stage with a command runs it in a terminal of its own, which it may end.
pub fn ends_shell1(steps: &[Step], stage: &Stage, pinned: &PinnedTerminals) -> bool {
    if stage.kind == StageKind::Testing && stage.command.is_some() {
        return false;
    }
    terminal_of_stage_pinned(steps, &stage.id, pinned) == "1"
}

#[cfg(test)]
#[path = "terminal_lane_tests.rs"]
mod tests;
