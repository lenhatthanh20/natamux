//! The activity lines of a task (`features/tasks/task-events.ts`): one sentence per kind of event,
//! consecutive events of one kind grouped into a line, and the Timeline's list of lines.

// Ported with their tests (spec D14); the details panel calls them.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::BTreeMap;

use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use serde_json::{Map, Value};

use super::task_time::parse_iso_ms;

/// One short sentence for every kind of event in `EVENT_TYPES`; a test keeps the two in step.
const EVENT_SENTENCE: [(&str, &str); 48] = [
    ("task.created", "Task created"),
    ("task.readied", "Marked ready"),
    ("task.blocked", "Blocked"),
    ("task.unblocked", "Unblocked"),
    ("task.retried", "Retried"),
    ("task.cancelled", "Cancelled"),
    ("agent.started", "Agent started"),
    ("agent.asked", "Agent is waiting for you"),
    ("agent.resumed", "Agent resumed"),
    ("agent.finished", "Agent finished its turn"),
    ("agent.failed", "Agent failed"),
    ("review.approved", "Approved"),
    ("review.changes-requested", "Changes requested"),
    ("state.overridden", "State set by hand"),
    ("workspace.created", "Workspace created"),
    ("workspace.removed", "Workspace removed"),
    ("agent.active", "Agent is working"),
    ("git.committed", "Committed"),
    ("git.pushed", "Pushed"),
    ("pr.opened", "Pull request opened"),
    ("agent.launched", "Agent launched"),
    ("task.edited", "Details edited"),
    ("pr.merged", "Pull request merged"),
    ("pr.closed", "Pull request closed"),
    ("ci.passed", "Checks passed"),
    ("ci.failed", "Checks failed"),
    ("review.commented", "Review comment sent to the agent"),
    ("stage.started", "Stage started"),
    ("stage.reported", "Stage reported done"),
    ("stage.check-failed", "A stage check failed"),
    ("stage.checked", "Stage checks passed"),
    ("stage.approved", "Stage approved"),
    ("stage.passed", "Stage passed"),
    ("stage.failed", "Stage failed"),
    ("stage.reopened", "Sent back to an earlier stage"),
    ("stage.skipped", "Stage skipped"),
    ("context.fetched", "Context fetched"),
    ("issue.linked", "Issue linked"),
    ("issue.created", "Issue created"),
    ("agent.waiting", "Agent waits for its background work"),
    ("agent.stopped", "Agent stopped"),
    ("attachment.added", "File attached"),
    ("stage.workflow-set", "Workflow choice changed"),
    ("stage.ci-set", "CI check choice changed"),
    ("stage.ci-started", "CI check started"),
    ("stage.skip-set", "Skip choice changed"),
    ("workspace.failed", "Workspace failed"),
    ("agent.paused", "Agent paused while idle"),
];

/// The name of a path of a branch by its id.
pub type PathNames = BTreeMap<String, String>;

pub const NO_ACTIVITY: &str = "No activity yet.";
pub const IGNORED_EVENT: &str = " (did not fit, ignored)";

pub fn event_sentence(kind: &str) -> Option<&'static str> {
    EVENT_SENTENCE
        .iter()
        .find(|(known, _)| *known == kind)
        .map(|(_, sentence)| *sentence)
}

/// A CI send back can go to the stage it came from, so "an earlier stage" would be wrong for it.
/// An event that chose a path of a branch ends with "-> <path name>"; without a name the id is
/// shown. A kind with no sentence reads as its own name.
pub fn describe_event(kind: &str, payload: &Map<String, Value>, path_names: &PathNames) -> String {
    if kind == "agent.paused"
        && let Some(minutes) = payload.get("idleMinutes").and_then(Value::as_u64)
    {
        return format!("Agent paused after {minutes} min idle");
    }
    let ci = payload.get("ci") == Some(&Value::Bool(true));
    let sentence = if kind == "stage.reopened" && ci {
        "Sent back by CI"
    } else {
        event_sentence(kind).unwrap_or(kind)
    };
    match payload.get("path").and_then(Value::as_str) {
        Some(path) if !path.is_empty() => {
            let name = path_names.get(path).map_or(path, String::as_str);
            format!("{sentence} → {name}")
        }
        _ => sentence.to_string(),
    }
}

/// The time of day with seconds in the local zone, like `09:05:07 AM`.
pub fn format_clock(iso_time: &str, offset_minutes: i32) -> String {
    let Some(ms) = parse_iso_ms(iso_time) else {
        return String::new();
    };
    let seconds = (ms + i64::from(offset_minutes) * 60_000)
        .div_euclid(1000)
        .rem_euclid(86_400);
    let (hour, minute, second) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    let shown = match hour % 12 {
        0 => 12,
        other => other,
    };
    let suffix = if hour < 12 { "AM" } else { "PM" };
    format!("{shown:02}:{minute:02}:{second:02} {suffix}")
}

pub fn actor_of(source: &str) -> &'static str {
    match source {
        "human" => "You",
        "agent" => "Agent",
        _ => "Daemon",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityLine {
    /// The first event of the run, which keys the line.
    pub seq: u64,
    pub accepted: bool,
    pub kind: String,
    pub sentence: String,
    pub actor: &'static str,
    /// How many events in a row were the same.
    pub count: usize,
    /// The newest event of the run: its time is the time of the line.
    pub at: String,
    /// More about the newest event, such as the background work an agent waits for, or why a
    /// workspace could not be made.
    pub detail: Option<String>,
}

fn detail_of(event: &TaskEvent) -> Option<String> {
    if event.kind == "workspace.failed" {
        let message = event.payload.get("message")?.as_str()?;
        return (!message.is_empty()).then(|| message.to_string());
    }
    if event.kind != "agent.waiting" {
        return None;
    }
    let entries: Vec<&str> = event
        .payload
        .get("waitingFor")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    (!entries.is_empty()).then(|| entries.join("; "))
}

/// Consecutive events of the same kind, from the same actor, become one line with a count, so
/// "Details edited" four times in a row reads as "Details edited ×4". Oldest first in, oldest
/// first out.
pub fn group_events(events: &[StoredEvent], path_names: &PathNames) -> Vec<ActivityLine> {
    let mut lines: Vec<ActivityLine> = Vec::new();
    for stored in events {
        let event = &stored.event;
        let actor = actor_of(&event.source);
        let detail = detail_of(event);
        let sentence = describe_event(&event.kind, &event.payload, path_names);
        match lines.last_mut() {
            Some(last)
                if last.sentence == sentence
                    && last.actor == actor
                    && last.accepted == stored.accepted =>
            {
                last.count += 1;
                last.at = event.at.clone();
                if detail.is_some() {
                    last.detail = detail;
                }
            }
            _ => lines.push(ActivityLine {
                seq: stored.seq,
                accepted: stored.accepted,
                kind: event.kind.clone(),
                sentence,
                actor,
                count: 1,
                at: event.at.clone(),
                detail,
            }),
        }
    }
    lines
}

/// "Details edited ×4 · You"
pub fn describe_line(line: &ActivityLine) -> String {
    let times = if line.count > 1 {
        format!(" ×{}", line.count)
    } else {
        String::new()
    };
    let detail = line
        .detail
        .as_ref()
        .map_or(String::new(), |detail| format!(": {detail}"));
    format!("{}{times}{detail} · {}", line.sentence, line.actor)
}

/// The Activity section's lines, newest first. "Agent is working" lines, in logs written before
/// they were dropped, are not news.
pub fn timeline_lines(events: &[StoredEvent]) -> Vec<ActivityLine> {
    let shown: Vec<StoredEvent> = events
        .iter()
        .filter(|stored| stored.event.kind != "agent.active")
        .cloned()
        .collect();
    let mut lines = group_events(&shown, &PathNames::new());
    lines.reverse();
    lines
}

#[cfg(test)]
#[path = "task_events_rules_tests.rs"]
mod tests;
