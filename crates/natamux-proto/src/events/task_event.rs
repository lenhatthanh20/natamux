//! Task events (`packages/core/src/events/schema.ts`): the log that `GET /tasks/:id/events`
//! sends, and the names a client may send.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const EVENT_SOURCES: [&str; 5] = ["agent", "human", "git", "ci", "system"];

pub const EVENT_TYPES: [&str; 47] = [
    "task.created",
    "task.readied",
    "task.blocked",
    "task.unblocked",
    "task.retried",
    "task.cancelled",
    "agent.started",
    "agent.asked",
    "agent.resumed",
    "agent.finished",
    "agent.failed",
    "review.approved",
    "review.changes-requested",
    "state.overridden",
    "workspace.created",
    "workspace.removed",
    "agent.active",
    "git.committed",
    "git.pushed",
    "pr.opened",
    "agent.launched",
    "task.edited",
    "pr.merged",
    "pr.closed",
    "ci.passed",
    "ci.failed",
    "review.commented",
    "stage.started",
    "stage.reported",
    "stage.check-failed",
    "stage.checked",
    "stage.approved",
    "stage.passed",
    "stage.failed",
    "stage.reopened",
    "stage.skipped",
    "context.fetched",
    "issue.linked",
    "issue.created",
    "agent.waiting",
    "agent.stopped",
    "attachment.added",
    "stage.workflow-set",
    "stage.ci-set",
    "stage.ci-started",
    "stage.skip-set",
    "workspace.failed",
];

/// Events the daemon records itself, without a change of state: a client cannot send them.
pub const RECORD_ONLY_EVENT_TYPES: [&str; 22] = [
    "task.created",
    "workspace.created",
    "workspace.removed",
    "agent.active",
    "git.committed",
    "git.pushed",
    "pr.opened",
    "agent.launched",
    "task.edited",
    "pr.closed",
    "ci.passed",
    "ci.failed",
    "review.commented",
    "context.fetched",
    "issue.linked",
    "issue.created",
    "attachment.added",
    "stage.workflow-set",
    "stage.ci-set",
    "stage.ci-started",
    "stage.skip-set",
    "workspace.failed",
];

/// The event types a client may send, in `EVENT_TYPES` order.
pub fn sendable_event_types() -> impl Iterator<Item = &'static str> {
    EVENT_TYPES
        .into_iter()
        .filter(|kind| !RECORD_ONLY_EVENT_TYPES.contains(kind))
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task_id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub at: String,
    #[serde(default)]
    pub payload: Map<String, Value>,
}

/// The body of `POST /tasks/:id/events`. Screen 1 sends `source: human` only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskEventInput {
    pub source: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub payload: Map<String, Value>,
}

impl TaskEventInput {
    /// An event a person sends, such as `task.readied` or `task.cancelled`.
    pub fn human(kind: &str) -> TaskEventInput {
        TaskEventInput {
            source: "human".into(),
            kind: kind.into(),
            payload: Map::new(),
        }
    }
}

/// The body of `GET /tasks/:id/events`, oldest first.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct EventList {
    #[serde(default)]
    pub events: Vec<StoredEvent>,
}

/// One line of a task's event log.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StoredEvent {
    pub seq: u64,
    #[serde(default)]
    pub accepted: bool,
    pub event: TaskEvent,
}

impl TaskEvent {
    fn text(&self, name: &str) -> Option<&str> {
        self.payload.get(name)?.as_str()
    }

    /// The question of an `agent.asked` event, on one line (`latestQuestion`). None when it
    /// carries no message.
    pub fn question(&self) -> Option<String> {
        if self.kind != "agent.asked" {
            return None;
        }
        let words: Vec<&str> = self.text("message")?.split_whitespace().collect();
        (!words.is_empty()).then(|| words.join(" "))
    }

    /// Why an `agent.failed` or `stage.failed` event failed (`waitReason`). None when empty.
    pub fn failure_reason(&self) -> Option<&str> {
        let failed = matches!(self.kind.as_str(), "agent.failed" | "stage.failed");
        self.text("reason")
            .filter(|reason| failed && !reason.is_empty())
    }
}

/// What the agent asked, from the newest accepted-or-not `agent.asked` event that carries a
/// message (`latestQuestion`). Events are oldest first.
pub fn latest_question(events: &[StoredEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find_map(|stored| stored.event.question())
}

/// Why a task waits for the user, from its newest accepted event that says so (`waitReason`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitReason {
    Asked { question: Option<String> },
    CheckFailed { reasons: Vec<String> },
    Failed { reason: Option<String> },
    Stopped,
}

/// Events before the last start or resume are old news: then the agent stopped without saying a
/// thing. No events read yet (None): assume a question.
pub fn wait_reason(events: Option<&[StoredEvent]>) -> WaitReason {
    let Some(events) = events else {
        return WaitReason::Asked { question: None };
    };
    for stored in events.iter().rev().filter(|stored| stored.accepted) {
        let event = &stored.event;
        match event.kind.as_str() {
            "agent.asked" => {
                return WaitReason::Asked {
                    question: event.question(),
                };
            }
            "agent.failed" | "stage.failed" => {
                return WaitReason::Failed {
                    reason: event.failure_reason().map(String::from),
                };
            }
            "stage.check-failed" if event.payload.get("final") != Some(&Value::Bool(false)) => {
                let reasons = match event.payload.get("reasons") {
                    Some(Value::Array(items)) => items
                        .iter()
                        .filter_map(|item| item.as_str().map(String::from))
                        .collect(),
                    _ => Vec::new(),
                };
                return WaitReason::CheckFailed { reasons };
            }
            "agent.resumed" | "agent.started" | "stage.started" => break,
            _ => {}
        }
    }
    WaitReason::Stopped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_sends_only_events_that_change_a_task() {
        let sendable: Vec<&str> = sendable_event_types().collect();
        assert_eq!(
            sendable.len(),
            EVENT_TYPES.len() - RECORD_ONLY_EVENT_TYPES.len()
        );
        assert_eq!(sendable.first(), Some(&"task.readied"));
        assert!(!sendable.contains(&"task.created"));
        assert!(sendable.contains(&"agent.asked"));
    }

    #[test]
    fn writes_an_event_back_in_the_schemas_order() {
        let stored: StoredEvent = serde_json::from_str(
            r#"{"seq":3,"accepted":true,"event":{"type":"agent.asked","taskId":"T-1","at":"t",
                "source":"agent","payload":{"question":"Why?","stage":"spec"},"extra":1}}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&stored).unwrap(),
            r#"{"seq":3,"accepted":true,"event":{"taskId":"T-1","source":"agent","type":"agent.asked","at":"t","payload":{"question":"Why?","stage":"spec"}}}"#
        );
    }

    fn stored(accepted: bool, kind: &str, payload: &str) -> StoredEvent {
        serde_json::from_str(&format!(
            r#"{{"seq":1,"accepted":{accepted},"event":{{"taskId":"T-1","source":"agent",
                "type":"{kind}","at":"2026-10-22T01:00:00.000Z","payload":{payload}}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn a_human_event_sends_its_source_and_type_and_no_empty_payload() {
        let input = TaskEventInput::human("task.cancelled");
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"source":"human","type":"task.cancelled"}"#
        );
        assert!(sendable_event_types().any(|kind| kind == "task.cancelled"));
    }

    #[test]
    fn reads_the_event_list() {
        let list: EventList = serde_json::from_str(
            r#"{"events":[{"seq":1,"accepted":true,"event":{"taskId":"T-1","source":"system",
                "type":"task.created","at":"t","payload":{}}}]}"#,
        )
        .unwrap();
        assert_eq!(list.events[0].event.kind, "task.created");
    }

    #[test]
    fn the_question_is_the_newest_message_on_one_line() {
        let events = [
            stored(true, "agent.asked", r#"{"message":"Old?"}"#),
            stored(true, "agent.asked", r#"{"message":"  Which\n  branch?  "}"#),
            stored(true, "agent.asked", r#"{}"#),
        ];
        assert_eq!(latest_question(&events).as_deref(), Some("Which branch?"));
        assert_eq!(latest_question(&events[2..]), None);
        let other = stored(true, "agent.failed", r#"{"message":"x"}"#);
        assert_eq!(other.event.question(), None);
    }

    #[test]
    fn a_task_that_waits_is_told_by_its_newest_accepted_event() {
        assert_eq!(wait_reason(None), WaitReason::Asked { question: None });
        let asked = [stored(true, "agent.asked", r#"{"message":"Why?"}"#)];
        assert_eq!(
            wait_reason(Some(&asked)),
            WaitReason::Asked {
                question: Some("Why?".into())
            }
        );
        let failed = [
            stored(true, "agent.asked", r#"{"message":"Why?"}"#),
            stored(true, "agent.failed", r#"{"reason":"exit 1"}"#),
            stored(false, "agent.asked", r#"{"message":"ignored"}"#),
        ];
        assert_eq!(
            wait_reason(Some(&failed)),
            WaitReason::Failed {
                reason: Some("exit 1".into())
            }
        );
        let silent = [stored(true, "stage.failed", r#"{"reason":""}"#)];
        assert_eq!(
            wait_reason(Some(&silent)),
            WaitReason::Failed { reason: None }
        );
    }

    #[test]
    fn a_failed_check_lists_its_reasons_unless_more_tries_follow() {
        let events = [stored(
            true,
            "stage.check-failed",
            r#"{"reasons":["lint","tests",3]}"#,
        )];
        assert_eq!(
            wait_reason(Some(&events)),
            WaitReason::CheckFailed {
                reasons: vec!["lint".into(), "tests".into()]
            }
        );
        let retry = [stored(true, "stage.check-failed", r#"{"final":false}"#)];
        assert_eq!(wait_reason(Some(&retry)), WaitReason::Stopped);
    }

    #[test]
    fn events_before_the_last_start_are_old_news() {
        let events = [
            stored(true, "agent.asked", r#"{"message":"Why?"}"#),
            stored(true, "agent.resumed", r#"{}"#),
        ];
        assert_eq!(wait_reason(Some(&events)), WaitReason::Stopped);
        assert_eq!(wait_reason(Some(&[])), WaitReason::Stopped);
    }
}
