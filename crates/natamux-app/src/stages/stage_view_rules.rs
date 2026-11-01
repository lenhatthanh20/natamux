//! Pure rules of the stage views (`packages/web/src/features/stages/stage-view.ts`, and `markerLabel`,
//! `linkKind` and the even-pixel rule of `StageMarker.tsx`). Times are ISO strings; the caller
//! passes `now_ms`, so no rule reads the clock.

// Ported with their tests (spec D14); the stage views call them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{BranchPath, Gate, Stage, StageBranch, StageKind, Step};
use natamux_proto::tasks::task::TaskState;

use crate::tasks::task_rules::state_label;
use crate::tasks::task_time::parse_iso_ms;

const MINUTE_MS: i64 = 60_000;

/// "<1m", "4m", "1h 5m", "2d 3h": the time between two moments. Empty when a time does not
/// parse.
pub fn format_duration(from: &str, to_ms: i64) -> String {
    let Some(from_ms) = parse_iso_ms(from) else {
        return String::new();
    };
    let span = (to_ms - from_ms).max(0);
    if span < MINUTE_MS {
        return "<1m".into();
    }
    // JavaScript's Math.round: halves go up.
    let minutes = (span + MINUTE_MS / 2).div_euclid(MINUTE_MS);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return if minutes % 60 == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {}m", minutes % 60)
        };
    }
    let days = hours / 24;
    if hours % 24 == 0 {
        format!("{days}d")
    } else {
        format!("{days}d {}h", hours % 24)
    }
}

/// `formatDuration` with a time text as the end.
pub fn format_duration_to(from: &str, to: &str) -> String {
    match parse_iso_ms(to) {
        Some(to_ms) => format_duration(from, to_ms),
        None => String::new(),
    }
}

/// The time a stage took, or has taken so far.
pub fn stage_time(progress: &StageProgress, now_ms: i64) -> Option<String> {
    let started = progress.started_at.as_deref()?;
    if progress.status == StageStatus::Current {
        return Some(format!("{} so far", format_duration(started, now_ms)));
    }
    let ended = progress.ended_at.as_deref()?;
    Some(format_duration_to(started, ended))
}

/// The time of a step: from the first start to the last end of its stages (side by side stages
/// overlap). None until one has started, or while one has no end.
pub fn step_time(entries: &[Option<&StageProgress>], now_ms: i64) -> Option<String> {
    let started: Vec<&StageProgress> = entries
        .iter()
        .flatten()
        .copied()
        .filter(|entry| entry.started_at.is_some())
        .collect();
    let first = started
        .iter()
        .filter_map(|entry| entry.started_at.as_deref())
        .min_by_key(|time| parse_iso_ms(time))?;
    if started
        .iter()
        .any(|entry| entry.status == StageStatus::Current)
    {
        return Some(format!("{} so far", format_duration(first, now_ms)));
    }
    let mut ends = Vec::new();
    for entry in &started {
        ends.push(entry.ended_at.as_deref()?);
    }
    let last = ends.into_iter().max_by_key(|time| parse_iso_ms(time))?;
    Some(format_duration_to(first, last))
}

pub fn find_stage<'a>(steps: &'a [Step], id: &str) -> Option<&'a Stage> {
    steps
        .iter()
        .flat_map(Step::stages)
        .find(|stage| stage.id == id)
}

/// What finishes a stage, in one sentence, for a stage not reached yet.
pub fn finished_when(stage: &Stage) -> String {
    if let Some(done_when) = &stage.done_when {
        return done_when.clone();
    }
    let gate = if stage.gate == Gate::Manual {
        ", and you approve it"
    } else {
        ""
    };
    match stage.kind {
        StageKind::Context => format!(
            "Finished when {} is written{gate}.",
            stage.output.as_deref().unwrap_or("context.md")
        ),
        StageKind::Review => "Finished when you approve the changes.".into(),
        StageKind::Release => "Finished when the pull request is merged.".into(),
        StageKind::Manual => "Finished when you approve it.".into(),
        StageKind::Testing => match &stage.command {
            None => "Finished when the tests are written and pass.".into(),
            Some(command) => format!("Finished when {command} passes."),
        },
        StageKind::Agent => agent_finished_when(stage, gate),
    }
}

fn agent_finished_when(stage: &Stage, gate: &str) -> String {
    let mut parts = Vec::new();
    if let Some(output) = &stage.output {
        parts.push(format!("{output} is written"));
    }
    if let Some(sections) = stage.sections.as_ref().filter(|list| !list.is_empty()) {
        parts.push(format!("it has {}", sections.join(", ")));
    }
    if stage.require_commit == Some(true) {
        parts.push("the work is committed".into());
    }
    if let Some(check) = &stage.check {
        parts.push(format!("{check} passes"));
    }
    if parts.is_empty() {
        format!("Finished when the agent says it is done{gate}.")
    } else {
        format!("Finished when {}{gate}.", parts.join(", "))
    }
}

/// How many stages are finished, of all. A stage of a path not taken is neither.
pub fn stage_count(progress: &[StageProgress]) -> (usize, usize) {
    let counted: Vec<_> = progress
        .iter()
        .filter(|stage| stage.status != StageStatus::NotTaken)
        .collect();
    let done = counted
        .iter()
        .filter(|stage| matches!(stage.status, StageStatus::Done | StageStatus::Skipped))
        .count();
    (done, counted.len())
}

pub fn path_label(path: &BranchPath) -> &str {
    path.name.as_deref().unwrap_or(&path.id)
}

/// The progress of a stage by its id; a stage with no entry is read as upcoming by the callers.
pub type ProgressLookup<'a> = dyn Fn(&str) -> Option<StageStatus> + 'a;

/// The path a task took: the one with a stage that is not waiting. None before the branch is
/// reached, and none when the branch was skipped.
pub fn taken_path<'a>(
    branch: &'a StageBranch,
    status_of: &ProgressLookup,
) -> Option<&'a BranchPath> {
    branch.paths.iter().find(|path| {
        path.stages.iter().any(|stage| {
            status_of(&stage.id).is_some_and(|status| {
                status != StageStatus::Upcoming && status != StageStatus::NotTaken
            })
        })
    })
}

/// The status of the diamond of a branch.
pub fn branch_status(branch: &StageBranch, status_of: &ProgressLookup) -> StageStatus {
    let Some(taken) = taken_path(branch, status_of) else {
        let all_not_taken = branch
            .paths
            .iter()
            .flat_map(|path| &path.stages)
            .all(|stage| status_of(&stage.id) == Some(StageStatus::NotTaken));
        return if all_not_taken {
            StageStatus::NotTaken
        } else {
            StageStatus::Upcoming
        };
    };
    let finished = taken.stages.iter().all(|stage| {
        matches!(
            status_of(&stage.id),
            Some(StageStatus::Done | StageStatus::Skipped)
        )
    });
    if finished {
        StageStatus::Done
    } else {
        StageStatus::Current
    }
}

/// A line joins two stages only when they are not in different paths.
pub fn same_lane(before: Option<&StageProgress>, after: Option<&StageProgress>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => match (&before.path, &after.path) {
            (Some(left), Some(right)) => left == right,
            _ => true,
        },
        _ => true,
    }
}

fn stage_of(event: &TaskEvent) -> Option<&str> {
    event.payload.get("stage")?.as_str()
}

/// Sets a key, keeping the place of a key that is already there (a JavaScript `Map`).
fn put<T>(list: &mut Vec<(String, T)>, key: &str, value: T) {
    match list.iter_mut().find(|(name, _)| name == key) {
        Some(entry) => entry.1 = value,
        None => list.push((key.to_string(), value)),
    }
}

/// The stages whose last start asked the agent for a workflow, in the order they first started.
pub fn ran_with_workflow(events: &[TaskEvent]) -> Vec<String> {
    let mut last = Vec::new();
    for event in events {
        if let (true, Some(stage)) = (event.kind == "stage.started", stage_of(event)) {
            let workflow = event.payload.get("workflow").and_then(|v| v.as_bool()) == Some(true);
            put(&mut last, stage, workflow);
        }
    }
    last.into_iter()
        .filter(|(_, workflow)| *workflow)
        .map(|(stage, _)| stage)
        .collect()
}

/// The stages whose CI check runs now, with the CI branch it runs on.
pub fn waiting_for_ci(events: &[TaskEvent]) -> Vec<(String, String)> {
    let mut waits: Vec<(String, String)> = Vec::new();
    for event in events {
        let Some(stage) = stage_of(event) else {
            continue;
        };
        if event.kind == "stage.ci-started" {
            let branch = event.payload.get("branch").and_then(|v| v.as_str());
            put(&mut waits, stage, branch.unwrap_or_default().to_string());
        } else if matches!(
            event.kind.as_str(),
            "stage.started" | "stage.checked" | "stage.check-failed"
        ) {
            waits.retain(|(name, _)| name != stage);
        }
    }
    waits
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MissingSkills {
    pub names: Vec<String>,
    /// The agent the stage started with, which can differ from the task's agent.
    pub agent: Option<String>,
}

/// The skills that each stage's prompt named and its agent did not have, from the latest start of
/// the stage. The daemon still started the stage, with the skill's name in the prompt.
pub fn missing_skills(events: &[TaskEvent]) -> Vec<(String, MissingSkills)> {
    let mut missing = Vec::new();
    for event in events {
        let (true, Some(stage)) = (event.kind == "stage.started", stage_of(event)) else {
            continue;
        };
        let names = event
            .payload
            .get("missingSkills")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let agent = event.payload.get("agent").and_then(|v| v.as_str());
        let value = MissingSkills {
            names,
            agent: agent.map(String::from),
        };
        put(&mut missing, stage, value);
    }
    missing
}

/// What a stage dot says: its state while it is the current stage, else its status.
pub fn marker_label(status: StageStatus, state: Option<TaskState>) -> &'static str {
    match (status, state) {
        (StageStatus::Current, Some(state)) => state_label(state),
        (StageStatus::Done, _) => "Done",
        (StageStatus::Current, None) => "Now",
        (StageStatus::Upcoming, _) => "Not reached",
        (StageStatus::Skipped, _) => "Skipped",
        (StageStatus::NotTaken, _) => "Not taken",
    }
}

/// The colour family of a dot: the state chips' colours (`data-state` of `stage-node`).
pub fn marker_color_state(status: StageStatus, state: Option<TaskState>) -> &'static str {
    match status {
        StageStatus::Done => "done",
        StageStatus::Current => state.map_or("ready", TaskState::name),
        _ => "draft",
    }
}

/// The shape inside a dot: a check when done, the shape of its state while current, the dash when
/// skipped, nothing else.
pub fn marker_shape(status: StageStatus, state: Option<TaskState>) -> Option<&'static str> {
    match status {
        StageStatus::Done => Some("done"),
        StageStatus::Current => Some(state.map_or("ready", TaskState::name)),
        StageStatus::Skipped => Some("cancelled"),
        _ => None,
    }
}

/// An even number of pixels near `size * share`, so the shape sits on whole pixels in the node.
pub fn even_px(size: f32, share: f32) -> f32 {
    2.0 * (size * share / 2.0).round()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Done,
    Flow,
    Upcoming,
    Hidden,
}

/// The line between two stages: green behind the work, filling a running stage, dashed ahead.
pub fn link_kind(before: Option<&StageProgress>, after: Option<&StageProgress>) -> LinkKind {
    let (Some(before), Some(after)) = (before, after) else {
        return LinkKind::Hidden;
    };
    if before.status == StageStatus::NotTaken || after.status == StageStatus::NotTaken {
        return LinkKind::Upcoming;
    }
    if !matches!(before.status, StageStatus::Done | StageStatus::Skipped) {
        return LinkKind::Upcoming;
    }
    if after.status == StageStatus::Current && after.state == Some(TaskState::Running) {
        return LinkKind::Flow;
    }
    if after.status == StageStatus::Upcoming {
        LinkKind::Upcoming
    } else {
        LinkKind::Done
    }
}

#[cfg(test)]
#[path = "stage_view_rules_tests.rs"]
mod tests;
