//! Messages of the `/events` socket (`DaemonMessage` in `packages/daemon/src/shared/event-bus.ts`).
//! The app reads the `kind` to know which data is out of date, and the few fields it needs to
//! show a step or read one task again.

use serde::Deserialize;

use crate::workspaces::workspace::PreparingStep;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    kind: String,
    #[serde(default)]
    task_id: Option<String>,
}

/// Every `kind` this module knows, to check against the daemon's `DaemonMessage`.
pub const KNOWN_KINDS: [&str; 13] = [
    "task.created",
    "task.updated",
    "task.changed",
    "task.deleted",
    "repos.changed",
    "jobs.changed",
    "activity.changed",
    "usage.changed",
    "limits.changed",
    "workspace.dirty",
    "shells.changed",
    "workspace.preparing",
    "resync",
];

/// The data a message makes out of date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Tasks,
    Repos,
    Jobs,
    Activity,
    Usage,
    /// The plan limits of the agents (`GET /limits`).
    Limits,
    /// The diff stats (`GET /diffstats`).
    DiffStats,
    /// The terminals of one task (`GET /tasks/:id/terminals`).
    Shells,
    /// The step of a new workspace, and its ready or failed toast.
    Preparing,
    /// The socket was behind and skipped messages: read everything again.
    Resync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// `task.created`, `task.updated`, `task.changed` or `task.deleted`.
    TaskChanged,
    ReposChanged,
    JobsChanged,
    ActivityChanged,
    UsageChanged,
    LimitsChanged,
    /// `workspace.dirty`: files in a task's workspace may have changed.
    WorkspaceDirty,
    ShellsChanged,
    WorkspacePreparing,
    /// The socket was behind and skipped messages: read everything again.
    Resync,
    /// Any other kind, also ones this build does not know.
    Other,
}

impl EventKind {
    /// None when the text is not an event message at all.
    pub fn parse(text: &str) -> Option<EventKind> {
        Some(Self::from_name(
            &serde_json::from_str::<Envelope>(text).ok()?.kind,
        ))
    }

    fn from_name(kind: &str) -> EventKind {
        match kind {
            "task.created" | "task.updated" | "task.changed" | "task.deleted" => {
                EventKind::TaskChanged
            }
            "repos.changed" => EventKind::ReposChanged,
            "jobs.changed" => EventKind::JobsChanged,
            "activity.changed" => EventKind::ActivityChanged,
            "usage.changed" => EventKind::UsageChanged,
            "limits.changed" => EventKind::LimitsChanged,
            "workspace.dirty" => EventKind::WorkspaceDirty,
            "shells.changed" => EventKind::ShellsChanged,
            "workspace.preparing" => EventKind::WorkspacePreparing,
            "resync" => EventKind::Resync,
            _ => EventKind::Other,
        }
    }

    /// The data this kind changes. None for a kind the app does not read.
    pub fn resource(self) -> Option<Resource> {
        match self {
            EventKind::TaskChanged => Some(Resource::Tasks),
            EventKind::ReposChanged => Some(Resource::Repos),
            EventKind::JobsChanged => Some(Resource::Jobs),
            EventKind::ActivityChanged => Some(Resource::Activity),
            EventKind::UsageChanged => Some(Resource::Usage),
            EventKind::LimitsChanged => Some(Resource::Limits),
            EventKind::WorkspaceDirty => Some(Resource::DiffStats),
            EventKind::ShellsChanged => Some(Resource::Shells),
            EventKind::WorkspacePreparing => Some(Resource::Preparing),
            EventKind::Resync => Some(Resource::Resync),
            EventKind::Other => None,
        }
    }

    pub fn changes_task_list(self) -> bool {
        matches!(self, EventKind::TaskChanged | EventKind::Resync)
    }
}

/// `task.updated`: the change added a line to the task's log (the web's `logged`). A sign of life
/// of its agent, `task.changed`, moves only its time.
pub fn is_task_updated(text: &str) -> bool {
    serde_json::from_str::<Envelope>(text).is_ok_and(|message| message.kind == "task.updated")
}

/// The task a message is about (`taskId`), when it has one. `task.created` and `task.updated`
/// carry the whole task instead, so read its id from there.
pub fn task_id_of(text: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct WithTask {
        #[serde(default)]
        id: Option<String>,
    }
    #[derive(Deserialize)]
    struct Message {
        #[serde(flatten)]
        envelope: Envelope,
        #[serde(default)]
        task: Option<WithTask>,
    }
    let message: Message = serde_json::from_str(text).ok()?;
    message
        .envelope
        .task_id
        .or_else(|| message.task.and_then(|task| task.id))
}

/// `workspace.preparing`: the step a new workspace is at. `step` None: it ended, with `error` when
/// it failed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparingMessage {
    pub task_id: String,
    #[serde(default)]
    pub step: Option<PreparingStep>,
    #[serde(default)]
    pub error: Option<String>,
}

impl PreparingMessage {
    /// None when the text is no `workspace.preparing` message.
    pub fn parse(text: &str) -> Option<PreparingMessage> {
        let envelope: Envelope = serde_json::from_str(text).ok()?;
        if envelope.kind != "workspace.preparing" {
            return None;
        }
        serde_json::from_str(text).ok()
    }

    pub fn failed(&self) -> bool {
        self.step.is_none() && self.error.is_some()
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
            r#"{"kind":"task.something.from.later","taskId":"T-1"}"#,
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

    #[test]
    fn every_known_kind_is_not_other() {
        for name in KNOWN_KINDS {
            assert_ne!(EventKind::from_name(name), EventKind::Other, "{name}");
        }
    }

    #[test]
    fn each_kind_changes_one_resource() {
        let cases = [
            (r#"{"kind":"task.deleted","taskId":"T-1"}"#, Resource::Tasks),
            (r#"{"kind":"repos.changed"}"#, Resource::Repos),
            (r#"{"kind":"jobs.changed","jobs":[]}"#, Resource::Jobs),
            (
                r#"{"kind":"activity.changed","taskId":"T-1","activity":{}}"#,
                Resource::Activity,
            ),
            (
                r#"{"kind":"usage.changed","taskId":"T-1","usage":{}}"#,
                Resource::Usage,
            ),
            (r#"{"kind":"limits.changed","limits":[]}"#, Resource::Limits),
            (
                r#"{"kind":"workspace.dirty","taskId":"T-1"}"#,
                Resource::DiffStats,
            ),
            (
                r#"{"kind":"shells.changed","taskId":"T-1"}"#,
                Resource::Shells,
            ),
            (
                r#"{"kind":"workspace.preparing","taskId":"T-1","step":"fetch"}"#,
                Resource::Preparing,
            ),
            (r#"{"kind":"resync"}"#, Resource::Resync),
        ];
        for (text, resource) in cases {
            let kind = EventKind::parse(text).unwrap();
            assert_eq!(kind.resource(), Some(resource), "{text}");
        }
        let unknown = EventKind::parse(r#"{"kind":"a.kind.from.later"}"#).unwrap();
        assert_eq!(unknown.resource(), None);
    }

    #[test]
    fn finds_the_task_of_a_message() {
        assert_eq!(
            task_id_of(r#"{"kind":"shells.changed","taskId":"T-7"}"#).as_deref(),
            Some("T-7")
        );
        assert_eq!(
            task_id_of(r#"{"kind":"task.created","task":{"id":"T-8","title":"t"}}"#).as_deref(),
            Some("T-8")
        );
        assert_eq!(task_id_of(r#"{"kind":"resync"}"#), None);
        assert_eq!(task_id_of("nope"), None);
    }

    #[test]
    fn reads_the_steps_of_a_workspace_that_is_made_and_how_it_ends() {
        let step = PreparingMessage::parse(
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":"checkout"}"#,
        )
        .unwrap();
        assert_eq!(step.step, Some(PreparingStep::Checkout));
        assert!(!step.failed());
        let ready =
            PreparingMessage::parse(r#"{"kind":"workspace.preparing","taskId":"T-1","step":null}"#)
                .unwrap();
        assert_eq!(ready.step, None);
        assert!(!ready.failed());
        let failed = PreparingMessage::parse(
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":null,"error":"no space"}"#,
        )
        .unwrap();
        assert!(failed.failed());
        assert_eq!(failed.error.as_deref(), Some("no space"));
    }

    #[test]
    fn another_message_is_no_preparing_message() {
        assert_eq!(PreparingMessage::parse(r#"{"kind":"resync"}"#), None);
        assert_eq!(PreparingMessage::parse("nope"), None);
    }
}
