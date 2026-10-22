//! A task as `GET /tasks` sends it (`taskSchema` in `packages/core/src/tasks/task.ts`). Only the
//! fields the app reads; the others are ignored, so a newer daemon still parses.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Draft,
    Ready,
    Running,
    NeedsInput,
    Review,
    Blocked,
    Failed,
    Done,
    Cancelled,
    /// A state this build does not know yet.
    #[serde(other)]
    Unknown,
}

impl TaskState {
    pub fn label(self) -> &'static str {
        match self {
            TaskState::Draft => "Draft",
            TaskState::Ready => "Ready",
            TaskState::Running => "Running",
            TaskState::NeedsInput => "Needs input",
            TaskState::Review => "Review",
            TaskState::Blocked => "Blocked",
            TaskState::Failed => "Failed",
            TaskState::Done => "Done",
            TaskState::Cancelled => "Cancelled",
            TaskState::Unknown => "Unknown",
        }
    }

    /// Done and cancelled tasks show a wrap-up, not a live terminal.
    pub fn is_finished(self) -> bool {
        matches!(self, TaskState::Done | TaskState::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    pub stage: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    /// ISO 8601 in UTC, so it sorts as text.
    pub updated_at: String,
}

impl Task {
    /// A live terminal makes sense only for a task with a workspace that is not finished.
    pub fn has_live_terminal(&self) -> bool {
        self.workspace.is_some() && !self.state.is_finished()
    }
}

/// The body of `GET /tasks`.
#[derive(Clone, Debug, Deserialize)]
pub struct TaskList {
    pub tasks: Vec<Task>,
}

/// The body of `POST /tasks/:id/terminal/ticket`.
#[derive(Clone, Debug, Deserialize)]
pub struct Ticket {
    pub ticket: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // A task as the daemon of 0.1.43 sends it, with fields this crate does not read.
    const TASK: &str = r#"{
        "id": "T-0143", "title": "Rust migration phase 1", "description": "", "state": "needs-input",
        "priority": "normal", "labels": [], "agent": "claude-code", "branch": "natamux/t-0143",
        "workspace": "/w/T-0143", "pr": null, "prStatus": null, "dependsOn": [], "parentId": null,
        "repo": "natamux", "stage": "implement", "stages": ["spec", "implement"], "lanes": {},
        "paths": {}, "stageSince": null, "backgroundSince": null, "workflowStages": [],
        "context": null, "issue": null, "createdAt": "2026-10-22T00:53:31.924Z",
        "updatedAt": "2026-10-22T00:59:38.510Z", "aFieldFromLater": {"x": 1}
    }"#;

    #[test]
    fn reads_a_task_and_ignores_other_fields() {
        let task: Task = serde_json::from_str(TASK).unwrap();
        assert_eq!(task.id, "T-0143");
        assert_eq!(task.state, TaskState::NeedsInput);
        assert_eq!(task.stage, "implement");
        assert_eq!(task.workspace.as_deref(), Some("/w/T-0143"));
        assert_eq!(task.parent_id, None);
        assert_eq!(task.updated_at, "2026-10-22T00:59:38.510Z");
        assert!(task.has_live_terminal());
    }

    #[test]
    fn a_state_from_a_newer_daemon_is_unknown() {
        let task: Task = serde_json::from_str(&TASK.replace("needs-input", "paused")).unwrap();
        assert_eq!(task.state, TaskState::Unknown);
    }

    #[test]
    fn finished_or_workspace_less_tasks_have_no_live_terminal() {
        let mut task: Task = serde_json::from_str(TASK).unwrap();
        task.state = TaskState::Done;
        assert!(!task.has_live_terminal());
        task.state = TaskState::Running;
        task.workspace = None;
        assert!(!task.has_live_terminal());
    }

    #[test]
    fn reads_the_list_and_the_ticket() {
        let list: TaskList = serde_json::from_str(&format!(r#"{{"tasks":[{TASK}]}}"#)).unwrap();
        assert_eq!(list.tasks.len(), 1);
        let ticket: Ticket = serde_json::from_str(r#"{"ticket":"abc"}"#).unwrap();
        assert_eq!(ticket.ticket, "abc");
    }
}
