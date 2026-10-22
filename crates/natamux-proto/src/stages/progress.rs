//! The progress of every stage of a task (`packages/core/src/events/progress.ts`).

use serde_json::{Map, Value};

use crate::events::task_event::TaskEvent;
use crate::stages::capabilities::stage_name;
use crate::stages::position::step_index;
use crate::stages::schema::{BranchPath, Gate, Stage, StageKind, Step};
use crate::tasks::task::{Task, TaskState};

/// `not-taken`: a stage of a branch where the task took another path, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageStatus {
    Done,
    Current,
    Upcoming,
    Skipped,
    NotTaken,
}

/// One stage as the progress views show it: where it is, and what proves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageProgress {
    pub stage: String,
    pub name: String,
    pub kind: StageKind,
    pub gate: Gate,
    pub output: Option<String>,
    /// The group the stage runs in, side by side with others.
    pub group: Option<String>,
    /// The path of a branch the stage is on.
    pub path: Option<String>,
    /// Why the task took the path, on the first stage of the path it took.
    pub because: Option<String>,
    pub status: StageStatus,
    /// The state of a current stage.
    pub state: Option<TaskState>,
    /// How many times the stage started; more than one means it was sent back.
    pub rounds: u32,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    /// The `source` of the event that approved it.
    pub approved_by: Option<String>,
    /// What the last checks found.
    pub passes: Vec<String>,
    pub reasons: Vec<String>,
    /// Why the stage was sent back the last time.
    pub sent_back_for: Option<String>,
    /// The last send back came from a CI failure, not from a person or a review.
    pub sent_back_by_ci: bool,
}

const ENDS: [&str; 5] = [
    "stage.approved",
    "review.approved",
    "stage.passed",
    "stage.skipped",
    "stage.checked",
];

const APPROVALS: [&str; 2] = ["stage.approved", "review.approved"];

fn payload_str<'a>(event: &'a TaskEvent, key: &str) -> Option<&'a str> {
    event.payload.get(key).and_then(Value::as_str)
}

// A send back names the stage it goes to in `to`; the daemon fills it in when the sender did not.
fn sends_back_to(event: &TaskEvent) -> Option<&str> {
    let send_back = matches!(
        event.kind.as_str(),
        "stage.reopened" | "review.changes-requested" | "agent.resumed"
    );
    if send_back {
        payload_str(event, "to")
    } else {
        None
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// The ids of the stages whose end enters step `index`. The end of the step before a skipped
// branch enters the step after it.
fn enders_of(steps: &[Step], index: usize, paths: &Map<String, Value>) -> Vec<String> {
    let Some(before_index) = index.checked_sub(1) else {
        return Vec::new();
    };
    let Some(before) = steps.get(before_index) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = before
        .stages()
        .iter()
        .map(|stage| stage.id.clone())
        .collect();
    let skipped = matches!(before, Step::Branch(branch)
        if paths.get(&branch.id).and_then(Value::as_str) == Some(""));
    if skipped {
        ids.extend(enders_of(steps, before_index, paths));
    }
    ids
}

fn last_of<'a>(
    events: &[&'a TaskEvent],
    keep: impl Fn(&TaskEvent) -> bool,
) -> Option<&'a TaskEvent> {
    events.iter().rev().find(|event| keep(event)).copied()
}

// When a stage began: the end of the stage or step before it, or a send back into it. Stages that
// only wait for a person (review, release) have no start event of their own.
fn entered_at<'a>(events: &'a [TaskEvent], into: &[String], from: &[String]) -> Option<&'a str> {
    events
        .iter()
        .rev()
        .find(|event| {
            let ends = ENDS.contains(&event.kind.as_str())
                && payload_str(event, "stage").is_some_and(|id| from.iter().any(|f| f == id));
            ends || sends_back_to(event).is_some_and(|id| into.iter().any(|i| i == id))
        })
        .map(|event| event.at.as_str())
}

/// The progress of every stage of a task, in order. `events` are the task's accepted events,
/// oldest first.
pub fn stage_progress(task: &Task, events: &[TaskEvent], steps: &[Step]) -> Vec<StageProgress> {
    let current = step_index(steps, &task.stage);
    let mut result = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        // A stage in a path runs like a step of its own, after the stage before it in the path.
        let places: Vec<(&Stage, Option<(&BranchPath, usize)>)> = match step {
            Step::Branch(branch) => branch
                .paths
                .iter()
                .flat_map(|path| {
                    path.stages
                        .iter()
                        .enumerate()
                        .map(move |(place, stage)| (stage, Some((path, place))))
                })
                .collect(),
            other => other
                .stages()
                .into_iter()
                .map(|stage| (stage, None))
                .collect(),
        };
        for (stage, on_path) in places {
            let ctx = Where {
                task,
                events,
                steps,
                step,
                index,
                current,
            };
            result.push(progress_of(&ctx, stage, on_path));
        }
    }
    result
}

struct Where<'a> {
    task: &'a Task,
    events: &'a [TaskEvent],
    steps: &'a [Step],
    step: &'a Step,
    index: usize,
    current: Option<usize>,
}

fn status_of(
    ctx: &Where,
    stage: &Stage,
    on_path: Option<(&BranchPath, usize)>,
    skipped: bool,
) -> (StageStatus, Option<TaskState>) {
    let task = ctx.task;
    let lane = if matches!(ctx.step, Step::Group(_)) {
        task.lanes
            .get(&stage.id)
            .and_then(|value| serde_json::from_value::<TaskState>(value.clone()).ok())
    } else {
        Some(task.state)
    };
    let here =
        on_path.and_then(|(path, _)| path.stages.iter().position(|each| each.id == task.stage));
    let place = on_path.map(|(_, place)| place);
    let before = ctx.current.is_some_and(|current| ctx.index < current);
    let after = ctx.current.is_none_or(|current| ctx.index > current);
    let path_before = matches!((place, here), (Some(place), Some(here)) if place < here);
    // `place > here` with no `here` counts as after: the task is not in this path yet.
    let path_after = place.is_some() && here.is_none_or(|here| place > Some(here));
    let finished = if skipped {
        StageStatus::Skipped
    } else {
        StageStatus::Done
    };
    if before || task.state == TaskState::Done || path_before {
        (finished, None)
    } else if after || path_after {
        (StageStatus::Upcoming, None)
    } else if lane == Some(TaskState::Done) {
        (finished, None)
    } else {
        (StageStatus::Current, lane.or(Some(task.state)))
    }
}

fn progress_of(ctx: &Where, stage: &Stage, on_path: Option<(&BranchPath, usize)>) -> StageProgress {
    let (task, events, step) = (ctx.task, ctx.events, ctx.step);
    let path = on_path.map(|(path, _)| path);
    let place = on_path.map(|(_, place)| place);
    let taken = match step {
        Step::Branch(branch) => task.paths.get(&branch.id).and_then(Value::as_str),
        _ => None,
    };
    let not_taken = taken.is_some_and(|taken| Some(taken) != path.map(|path| path.id.as_str()));
    // The event that chose the path the task took.
    let chosen = match (path, place) {
        (Some(path), Some(0)) if taken == Some(path.id.as_str()) => events
            .iter()
            .rev()
            .find(|event| payload_str(event, "path") == Some(path.id.as_str())),
        _ => None,
    };
    let entered = entered_into(ctx, stage, on_path);

    let Own {
        starts,
        ended,
        approved,
        checked,
        failed,
        agent_failed,
    } = own_events(events, &stage.id);
    let last_start = starts.last().copied();
    let skipped = ended.is_some_and(|event| event.kind == "stage.skipped");
    let reopened = events
        .iter()
        .rev()
        .find(|event| sends_back_to(event) == Some(stage.id.as_str()));

    let (status, state) = if not_taken {
        (StageStatus::NotTaken, None)
    } else {
        status_of(ctx, stage, on_path, skipped)
    };
    let started_at = started_at_of(not_taken, last_start, status, chosen, entered);
    let reasons = reasons_of(failed, checked, agent_failed, state);
    StageProgress {
        stage: stage.id.clone(),
        name: stage_name(stage),
        kind: stage.kind,
        gate: stage.gate,
        output: stage.output.clone(),
        group: match step {
            Step::Group(group) => Some(group.id.clone()),
            _ => None,
        },
        path: path.map(|path| path.id.clone()),
        because: chosen
            .and_then(|event| payload_str(event, "because"))
            .map(str::to_owned),
        status,
        state,
        rounds: if status == StageStatus::Upcoming || not_taken {
            0
        } else {
            u32::try_from(starts.len().max(1)).unwrap_or(u32::MAX)
        },
        started_at,
        ended_at: match status {
            StageStatus::Done | StageStatus::Skipped => ended.map(|event| event.at.clone()),
            _ => None,
        },
        approved_by: approved.map(|event| event.source.clone()),
        passes: strings(checked.and_then(|event| event.payload.get("passes"))),
        reasons,
        sent_back_for: reopened
            .and_then(|event| payload_str(event, "reason"))
            .map(str::to_owned),
        sent_back_by_ci: reopened
            .is_some_and(|event| event.payload.get("ci") == Some(&Value::Bool(true))),
    }
}

/// When the stage last began: its last start, else when it was entered; none ahead of the task.
fn started_at_of(
    not_taken: bool,
    last_start: Option<&TaskEvent>,
    status: StageStatus,
    chosen: Option<&TaskEvent>,
    entered: Option<&str>,
) -> Option<String> {
    if not_taken {
        None
    } else if let Some(start) = last_start {
        Some(start.at.clone())
    } else if status == StageStatus::Upcoming {
        None
    } else {
        chosen
            .map(|event| event.at.as_str())
            .or(entered)
            .map(str::to_owned)
    }
}

/// When a stage on `on_path` (or in its step) began: the end of the stage before it, or a send
/// back into it.
fn entered_into<'a>(
    ctx: &Where<'a>,
    stage: &Stage,
    on_path: Option<(&BranchPath, usize)>,
) -> Option<&'a str> {
    let (task, events, steps, step) = (ctx.task, ctx.events, ctx.steps, ctx.step);
    let path = on_path.map(|(path, _)| path);
    let place = on_path.map(|(_, place)| place);
    let previous = match (path, place) {
        (Some(path), Some(place)) if place > 0 => path.stages.get(place - 1),
        _ => None,
    };
    let into: Vec<String> = if path.is_none() {
        step.stages().iter().map(|each| each.id.clone()).collect()
    } else {
        vec![stage.id.clone()]
    };
    let from = match previous {
        Some(previous) => vec![previous.id.clone()],
        None => enders_of(steps, ctx.index, &task.paths),
    };
    entered_at(events, &into, &from)
}

/// A stage's own events since its last start.
struct Own<'a> {
    starts: Vec<&'a TaskEvent>,
    ended: Option<&'a TaskEvent>,
    approved: Option<&'a TaskEvent>,
    checked: Option<&'a TaskEvent>,
    failed: Option<&'a TaskEvent>,
    agent_failed: Option<&'a TaskEvent>,
}

fn own_events<'a>(events: &'a [TaskEvent], stage_id: &str) -> Own<'a> {
    let own: Vec<&TaskEvent> = events
        .iter()
        .filter(|event| payload_str(event, "stage") == Some(stage_id))
        .collect();
    let starts: Vec<&TaskEvent> = own
        .iter()
        .filter(|e| e.kind == "stage.started")
        .copied()
        .collect();
    let since_start: &[&TaskEvent] = match own.iter().rposition(|e| e.kind == "stage.started") {
        Some(at) => &own[at..],
        None => &own,
    };
    let ended = last_of(since_start, |e| ENDS.contains(&e.kind.as_str()));
    let approved = last_of(since_start, |e| APPROVALS.contains(&e.kind.as_str()));
    let checked = last_of(since_start, |e| e.kind == "stage.checked");
    let failed = last_of(since_start, |e| e.kind == "stage.check-failed");
    // Why the stage's agent failed, such as a shell that could not find it.
    let agent_failed = last_of(since_start, |e| {
        e.kind == "agent.failed" || e.kind == "stage.failed"
    });
    Own {
        starts,
        ended,
        approved,
        checked,
        failed,
        agent_failed,
    }
}

/// The last checks' reasons, unless a later check passed; else why the agent failed.
fn reasons_of(
    failed: Option<&TaskEvent>,
    checked: Option<&TaskEvent>,
    agent_failed: Option<&TaskEvent>,
    state: Option<TaskState>,
) -> Vec<String> {
    match (failed, checked) {
        (Some(failed), checked) if checked.is_none_or(|checked| failed.at > checked.at) => {
            strings(failed.payload.get("reasons"))
        }
        _ => match agent_failed.and_then(|event| payload_str(event, "reason")) {
            Some(reason) if state == Some(TaskState::Failed) => vec![reason.to_owned()],
            _ => Vec::new(),
        },
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
