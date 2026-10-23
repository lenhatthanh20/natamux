//! The move an event makes in one stage (`packages/core/src/events/moves.ts`).

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState;
use serde_json::Value;

use crate::stages::position::{Paths, StagePosition, get};
use crate::stages::terminal_lane::{PinnedTerminals, stage_of_terminal};

use TaskState::{Blocked, Draft, Failed, NeedsInput, Ready, Review, Running};

/// Where a move leads: a state, the end of the stage, or its gate (end when the gate is auto,
/// wait for approval when it is manual).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveTo {
    State(TaskState),
    Finish,
    Gate,
}

/// The states a move may happen in, and where it leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: &'static [TaskState],
    pub to: MoveTo,
}

const fn to(from: &'static [TaskState], state: TaskState) -> Move {
    Move {
        from,
        to: MoveTo::State(state),
    }
}

/// `MOVES`.
fn table(kind: &str) -> Option<Move> {
    let finish = |from| Move {
        from,
        to: MoveTo::Finish,
    };
    Some(match kind {
        "task.readied" => to(&[Draft], Ready),
        "task.blocked" => to(&[Ready, Running], Blocked),
        "task.unblocked" => to(&[Blocked], Ready),
        "task.retried" => to(&[Failed], Ready),
        "agent.started" => to(&[Ready], Running),
        "stage.started" => to(&[Draft, Ready], Running),
        "agent.asked" => to(&[Running], NeedsInput),
        "agent.resumed" => to(&[NeedsInput, Review, Failed], Running),
        "agent.finished" => to(&[Running], NeedsInput),
        "agent.failed" => to(&[Running, NeedsInput], Failed),
        "agent.waiting" => to(&[Running], Running),
        "agent.stopped" => to(&[Running, NeedsInput], Ready),
        "stage.reported" => to(&[Running, NeedsInput], Running),
        "stage.checked" => Move {
            from: &[Running],
            to: MoveTo::Gate,
        },
        "stage.approved" | "review.approved" => finish(&[Review, NeedsInput]),
        "stage.passed" => finish(&[Ready, Running, Review]),
        "stage.failed" => to(&[Running], Failed),
        "stage.skipped" => finish(&[Draft, Ready, Running, NeedsInput, Review, Blocked, Failed]),
        _ => return None,
    })
}

/// A non-empty string of the payload.
pub fn payload_string<'a>(event: &'a TaskEvent, key: &str) -> Option<&'a str> {
    event
        .payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

/// The position with the record of the paths taken, when there is one.
pub fn with_paths(position: StagePosition, paths: Option<Paths>) -> StagePosition {
    match paths {
        None => position,
        Some(paths) => StagePosition {
            paths: Some(paths),
            ..position
        },
    }
}

/// The state of one stage of a step: its lane in a group, else the state of the step.
pub fn lane_state_of(position: &StagePosition, step: &Step, stage: &str) -> Option<TaskState> {
    if step.is_group() {
        get(&position.lanes, stage).copied()
    } else {
        Some(position.state)
    }
}

/// The move an event makes. A failed check keeps the stage running while the agent fixes it; the
/// last try waits for you.
pub fn move_of(event: &TaskEvent) -> Option<Move> {
    if event.kind == "stage.check-failed" {
        let last = event.payload.get("final") == Some(&Value::Bool(true));
        return Some(to(&[Running], if last { NeedsInput } else { Running }));
    }
    table(&event.kind)
}

/// The stage an event is about. Outside a group it is the current stage. Inside one: the stage it
/// names, else the stage of the terminal it came from, else the only stage the move fits.
pub fn lane_of(
    steps: &[Step],
    step: &Step,
    position: &StagePosition,
    event: &TaskEvent,
    movement: &Move,
    pinned: &PinnedTerminals,
) -> Option<String> {
    let named = payload_string(event, "stage");
    let Step::Group(group) = step else {
        return Some(named.map_or_else(|| step.id().to_string(), str::to_string));
    };
    if let Some(named) = named {
        return Some(named.to_string());
    }
    if let Some(terminal) = payload_string(event, "terminal") {
        return stage_of_terminal(steps, step, terminal, pinned).map(|stage| stage.id);
    }
    let fitting: Vec<&str> = group
        .parallel
        .iter()
        .filter(|stage| {
            get(&position.lanes, &stage.id).is_some_and(|state| movement.from.contains(state))
        })
        .map(|stage| stage.id.as_str())
        .collect();
    match fitting.as_slice() {
        [only] => Some((*only).to_string()),
        _ => None,
    }
}
