//! Messages of the `/events` socket (`DaemonMessage` in `packages/daemon/src/shared/event-bus.ts`).
//! The app only needs to know when its task list is out of date, so it reads the `kind` alone.

use serde::Deserialize;

#[derive(Deserialize)]
struct Envelope {
    kind: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// `task.created`, `task.updated`, `task.changed` or `task.deleted`.
    TaskChanged,
    /// The socket was behind and skipped messages: read everything again.
    Resync,
    /// Any other kind, also ones this build does not know.
    Other,
}

impl EventKind {
    /// None when the text is not an event message at all.
    pub fn parse(text: &str) -> Option<EventKind> {
        let envelope: Envelope = serde_json::from_str(text).ok()?;
        Some(match envelope.kind.as_str() {
            "task.created" | "task.updated" | "task.changed" | "task.deleted" => {
                EventKind::TaskChanged
            }
            "resync" => EventKind::Resync,
            _ => EventKind::Other,
        })
    }

    pub fn changes_task_list(self) -> bool {
        matches!(self, EventKind::TaskChanged | EventKind::Resync)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_kinds_and_resync_change_the_list() {
        for kind in [
            "task.created",
            "task.updated",
            "task.changed",
            "task.deleted",
        ] {
            let text = format!(r#"{{"kind":"{kind}","task":{{"id":"T-1"}},"taskId":"T-1"}}"#);
            assert_eq!(EventKind::parse(&text), Some(EventKind::TaskChanged));
        }
        let resync = EventKind::parse(r#"{"kind":"resync"}"#);
        assert_eq!(resync, Some(EventKind::Resync));
        assert!(resync.is_some_and(EventKind::changes_task_list));
    }

    #[test]
    fn other_kinds_are_kept_but_change_nothing() {
        for text in [
            r#"{"kind":"shells.changed","taskId":"T-1"}"#,
            r#"{"kind":"usage.changed","taskId":"T-1","usage":{}}"#,
            r#"{"kind":"a.kind.from.later"}"#,
        ] {
            let kind = EventKind::parse(text);
            assert_eq!(kind, Some(EventKind::Other));
            assert!(!kind.is_some_and(EventKind::changes_task_list));
        }
    }

    #[test]
    fn text_that_is_no_event_is_none() {
        assert_eq!(EventKind::parse("{}"), None);
        assert_eq!(EventKind::parse("nope"), None);
    }
}
