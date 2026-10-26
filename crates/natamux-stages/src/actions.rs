//! Ports `packages/daemon/src/stages/stage-actions.ts` (constants and pure helpers) and
//! `AGENT_REFUSED_ACTIONS` of `http/routes/stages.ts`.

use natamux_core::events::schema::GATE_EVENT_TYPES;
use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::Value;

/// Events of an agent that leave it where it was: still at the end of its turn.
pub const KEEPS_TURN: [&str; 1] = ["agent.active"];

/// `STAGE_ACTIONS`.
pub const STAGE_ACTIONS: [StageAction; 7] = [
    StageAction::Approve,
    StageAction::Reopen,
    StageAction::Skip,
    StageAction::Pass,
    StageAction::Fail,
    StageAction::Report,
    StageAction::Start,
];

/// What only a person may do, so an agent cannot pass a manual gate by itself.
pub const AGENT_REFUSED_ACTIONS: [StageAction; 3] =
    [StageAction::Approve, StageAction::Skip, StageAction::Pass];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageAction {
    Approve,
    Reopen,
    Skip,
    Pass,
    Fail,
    Report,
    Start,
}

impl StageAction {
    pub fn parse(text: &str) -> Option<StageAction> {
        STAGE_ACTIONS
            .into_iter()
            .find(|action| action.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StageAction::Approve => "approve",
            StageAction::Reopen => "reopen",
            StageAction::Skip => "skip",
            StageAction::Pass => "pass",
            StageAction::Fail => "fail",
            StageAction::Report => "report",
            StageAction::Start => "start",
        }
    }
}

/// The states of a stage whose agent starts again when the daemon starts.
pub const RESUMED_STATES: [TaskState; 3] =
    [TaskState::Running, TaskState::NeedsInput, TaskState::Review];

/// Events that end a stage and so may move the task on to the next step. A send back does not
/// count: a person who sends the task back to a stage marked to skip wants it to run.
pub const ENDS_STAGE: [&str; 5] = [
    "stage.approved",
    "review.approved",
    "stage.passed",
    "stage.checked",
    "stage.skipped",
];

/// An agent asked for `done` rounds of "fix it" on its own; with `limit` (`agents.maxSendBacks`)
/// the next one waits for a person. No limit: it never does. The count starts again each time a
/// person steps in, see `agent_send_backs`.
pub fn send_back_limit_reached(limit: Option<usize>, done: usize) -> bool {
    limit.is_some_and(|limit| done >= limit)
}

/// The send backs by an agent (or by CI) from a stage since a person last stepped in, oldest
/// event first. A person who sends back, approves, passes, skips or sets the state has decided,
/// so the agents get their tries again; only a person's events count, so an agent cannot reset
/// its own limit.
pub fn agent_send_backs(events: &[StoredEvent], from: &str) -> usize {
    let since = events
        .iter()
        .rposition(is_person_decision)
        .map_or(0, |at| at + 1);
    events
        .iter()
        .skip(since)
        .filter(|stored| {
            let payload = &stored.event.payload;
            stored.event.kind == "stage.reopened"
                && payload.get("byAgent") == Some(&Value::Bool(true))
                && payload.get("from").and_then(Value::as_str) == Some(from)
        })
        .count()
}

fn is_person_decision(stored: &StoredEvent) -> bool {
    let event = &stored.event;
    let kind = event.kind.as_str();
    stored.accepted
        && event.source == "human"
        && event.payload.get("byAgent") != Some(&Value::Bool(true))
        && (kind == "stage.reopened"
            || kind == "review.changes-requested"
            || GATE_EVENT_TYPES.contains(&kind))
}

fn state_named(name: &str) -> TaskState {
    [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Running,
        TaskState::NeedsInput,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ]
    .into_iter()
    .find(|state| state.name() == name)
    .unwrap_or(TaskState::Unknown)
}

/// The state of a stage: its lane inside a group (done when it has none), else the task's state.
pub fn lane_state(task: &Task, step: &Step, stage_id: &str) -> TaskState {
    if !step.is_group() {
        return task.state;
    }
    task.lanes
        .get(stage_id)
        .and_then(|lane| lane.as_str())
        .map_or(TaskState::Done, state_named)
}

/// A stage an agent works on: an agent stage, or a testing stage with no command.
pub fn run_by_agent(stage: &Stage) -> bool {
    stage.kind == StageKind::Agent || (stage.kind == StageKind::Testing && stage.command.is_none())
}

/// Why a send back asked for changes, for the agent's next prompt.
pub fn send_back_reason(cause: Option<&TaskEvent>) -> Option<String> {
    let cause = cause?;
    if cause.kind != "stage.reopened" && cause.kind != "review.changes-requested" {
        return None;
    }
    cause
        .payload
        .get("reason")
        .and_then(|reason| reason.as_str())
        .filter(|reason| !reason.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
