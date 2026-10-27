//! `taskEventSchema` and `GATE_EVENT_TYPES` (`packages/core/src/events/schema.ts`). The lists of
//! types and sources are in `natamux-proto`.

use natamux_proto::events::task_event::{EVENT_SOURCES, EVENT_TYPES, TaskEvent};
use serde_json::{Map, Value, json};

use crate::tasks::inputs::payload;
use crate::zod::{Issue, Issues, Rule, ZodError, object, one_of, string};

/// Events that pass a manual gate or set a state by hand. Only a person may send them, never the
/// agent token of a task terminal.
pub const GATE_EVENT_TYPES: [&str; 5] = [
    "review.approved",
    "stage.approved",
    "stage.passed",
    "stage.skipped",
    "state.overridden",
];

pub fn validate_task_event(value: &Value) -> Result<TaskEvent, ZodError> {
    let mut issues = Issues::default();
    let mut out = Map::new();
    if let Some(map) = object(Some(value), &[], &mut issues) {
        let task_id = string(
            map.get("taskId"),
            &[json!("taskId")],
            &[Rule::Min(1)],
            &mut issues,
        );
        let source = one_of(
            map.get("source"),
            &[json!("source")],
            &EVENT_SOURCES,
            &mut issues,
        );
        let kind = one_of(map.get("type"), &[json!("type")], &EVENT_TYPES, &mut issues);
        let at = string(
            map.get("at"),
            &[json!("at")],
            &[Rule::Datetime],
            &mut issues,
        );
        let payload = payload(map.get("payload"), &[json!("payload")], &mut issues);
        for (key, value) in [
            ("taskId", task_id.map(Value::from)),
            ("source", source.map(Value::from)),
            ("type", kind.map(Value::from)),
            ("at", at.map(Value::from)),
            ("payload", payload),
        ] {
            if let Some(value) = value {
                out.insert(key.into(), value);
            }
        }
    }
    let fields = issues.into_result(Some(out))?;
    serde_json::from_value(Value::Object(fields))
        .map_err(|error| ZodError(vec![Issue::custom(&[], &error.to_string())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_event_and_fills_its_payload() {
        let event = validate_task_event(&json!({
            "taskId": "T-1", "source": "human", "type": "task.readied",
            "at": "2026-10-23T00:00:00.000Z", "extra": 1
        }))
        .unwrap();
        assert_eq!(event.kind, "task.readied");
        assert!(event.payload.is_empty());
        let error = validate_task_event(&json!({"taskId": "", "source": "bot"})).unwrap_err();
        assert_eq!(
            error.describe(),
            "taskId: Too small: expected string to have >=1 characters; source: Invalid option: \
             expected one of \"agent\"|\"human\"|\"git\"|\"ci\"|\"system\"; type: Invalid option: \
             expected one of \"task.created\"|\"task.readied\"|\"task.blocked\"|\"task.unblocked\"|\
             \"task.retried\"|\"task.cancelled\"|\"agent.started\"|\"agent.asked\"|\"agent.resumed\"|\
             \"agent.finished\"|\"agent.failed\"|\"review.approved\"|\"review.changes-requested\"|\
             \"state.overridden\"|\"workspace.created\"|\"workspace.removed\"|\"agent.active\"|\
             \"git.committed\"|\"git.pushed\"|\"pr.opened\"|\"agent.launched\"|\"task.edited\"|\
             \"pr.merged\"|\"pr.closed\"|\"ci.passed\"|\"ci.failed\"|\"review.commented\"|\
             \"stage.started\"|\"stage.reported\"|\"stage.check-failed\"|\"stage.checked\"|\
             \"stage.approved\"|\"stage.passed\"|\"stage.failed\"|\"stage.reopened\"|\
             \"stage.skipped\"|\"context.fetched\"|\"issue.linked\"|\"issue.created\"|\
             \"agent.waiting\"|\"agent.stopped\"|\"attachment.added\"|\"stage.workflow-set\"|\
             \"stage.ci-set\"|\"stage.ci-started\"|\"stage.skip-set\"|\"workspace.failed\"|\
             \"agent.paused\"; at: \
             Invalid input: expected string, received undefined"
        );
    }

    fn base() -> Value {
        json!({"taskId": "T-1", "source": "agent", "type": "agent.started", "at": "2026-10-08T09:00:00Z"})
    }

    #[test]
    fn fills_an_empty_payload_by_default() {
        assert!(validate_task_event(&base()).unwrap().payload.is_empty());
    }

    #[test]
    fn rejects_an_unknown_event_type() {
        let mut value = base();
        value["type"] = json!("agent.exploded");
        assert!(validate_task_event(&value).is_err());
    }

    #[test]
    fn rejects_a_timestamp_that_is_not_iso_8601() {
        let mut value = base();
        value["at"] = json!("yesterday");
        assert!(validate_task_event(&value).is_err());
    }

    // A change here changes what an agent may do by itself, so it must be on purpose.
    #[test]
    fn gate_event_types_hold_exactly_the_events_that_pass_a_gate_or_set_a_state_by_hand() {
        let mut sorted = GATE_EVENT_TYPES.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            [
                "review.approved",
                "stage.approved",
                "stage.passed",
                "stage.skipped",
                "state.overridden"
            ]
        );
    }
}
