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

/// One line of a task's event log.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StoredEvent {
    pub seq: u64,
    #[serde(default)]
    pub accepted: bool,
    pub event: TaskEvent,
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
}
