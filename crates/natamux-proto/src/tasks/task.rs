//! A task as `GET /tasks` sends it (`taskSchema` in `packages/core/src/tasks/task.ts`): every
//! field, in the schema's order, so the `natamux` command prints the JSON the TypeScript one
//! prints. Fields this build does not know are dropped, as zod does, so a newer daemon still
//! parses.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `TASK_STATES` in `packages/core/src/tasks/task-state.ts`.
pub const TASK_STATES: [&str; 9] = [
    "draft",
    "ready",
    "running",
    "needs-input",
    "review",
    "blocked",
    "failed",
    "done",
    "cancelled",
];

/// `TASK_PRIORITIES` in `packages/core/src/tasks/task.ts`.
pub const TASK_PRIORITIES: [&str; 4] = ["low", "normal", "high", "urgent"];

/// `PULL_REQUEST_STATES` in `packages/core/src/tasks/task.ts`.
pub const PULL_REQUEST_STATES: [&str; 4] = ["open", "draft", "merged", "closed"];

/// `CHECK_STATES` in `packages/core/src/tasks/task.ts`.
pub const CHECK_STATES: [&str; 4] = ["passing", "failing", "pending", "none"];

/// `title` of a task: at most this many characters, after trimming.
pub const MAX_TITLE_LENGTH: usize = 400;

/// `taskIdSchema`: `T-42` or `ABC-42`, at most 32 characters.
pub fn is_task_id(value: &str) -> bool {
    let Some((letters, digits)) = value.split_once('-') else {
        return false;
    };
    value.len() <= 32
        && !letters.is_empty()
        && letters.bytes().all(|byte| byte.is_ascii_uppercase())
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    #[default]
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

    /// The state's name, as the daemon writes it.
    pub fn name(self) -> &'static str {
        match self {
            TaskState::Draft => "draft",
            TaskState::Ready => "ready",
            TaskState::Running => "running",
            TaskState::NeedsInput => "needs-input",
            TaskState::Review => "review",
            TaskState::Blocked => "blocked",
            TaskState::Failed => "failed",
            TaskState::Done => "done",
            TaskState::Cancelled => "cancelled",
            TaskState::Unknown => "unknown",
        }
    }
}

/// Defaults are zod's, for a field the daemon leaves out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub state: TaskState,
    #[serde(default = "normal")]
    pub priority: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub pr: Option<String>,
    #[serde(default)]
    pub pr_status: Option<Value>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default = "implement")]
    pub stage: String,
    #[serde(default)]
    pub stages: Vec<Value>,
    /// Stage to state, in the daemon's order.
    #[serde(default)]
    pub lanes: Map<String, Value>,
    #[serde(default)]
    pub paths: Map<String, Value>,
    #[serde(default)]
    pub stage_since: Option<String>,
    #[serde(default)]
    pub background_since: Option<String>,
    #[serde(default)]
    pub workflow_stages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_stages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_stages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub without_agent: Option<bool>,
    #[serde(default)]
    pub context: Option<TaskContext>,
    #[serde(default)]
    pub issue: Option<IssueLink>,
    #[serde(default)]
    pub created_at: String,
    /// ISO 8601 in UTC, so it sorts as text.
    pub updated_at: String,
}

fn normal() -> String {
    "normal".into()
}

fn implement() -> String {
    "implement".into()
}

/// Where the context of a task came from (`taskContextSchema`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct TaskContext {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    #[serde(default, rename = "ref")]
    pub reference: String,
}

/// The issue or ticket of a task (`issueLinkSchema`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct IssueLink {
    pub provider: String,
    pub key: String,
    pub url: String,
}

impl Task {
    /// A live terminal makes sense only for a task with a workspace that is not finished.
    pub fn has_live_terminal(&self) -> bool {
        self.workspace.is_some() && !self.state.is_finished()
    }
}

/// What GitHub last said about a task's pull request and its checks (`pullRequestStatusSchema`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PullRequestStatus {
    pub state: String,
    #[serde(default = "no_checks")]
    pub checks: String,
}

fn no_checks() -> String {
    "none".into()
}

impl Task {
    /// None when GitHub was not asked yet, or the stored value is not a status.
    pub fn pull_request_status(&self) -> Option<PullRequestStatus> {
        serde_json::from_value(self.pr_status.clone()?).ok()
    }
}

/// The body of `POST /tasks`: only what the dashboard sends. Every other field takes its default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTaskInput {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// The body of `POST /templates/:id/tasks`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TemplateTaskInput {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// The body of `PATCH /tasks/:id` that renames a task. The route takes any of title, description,
/// priority, labels and dependsOn; screen 1 changes the title only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TitlePatch {
    pub title: String,
}

/// The answer of `POST /tasks`, `POST /templates/:id/tasks`, `PATCH /tasks/:id`,
/// `POST /tasks/:id/agent` and `POST /workspaces`: `{ task }`.
#[derive(Clone, Debug, Deserialize)]
pub struct TaskAnswer {
    pub task: Task,
}

/// The answer of `DELETE /tasks/:id`: the id, and the tasks after the delete (its subtasks may
/// have changed).
#[derive(Clone, Debug, Deserialize)]
pub struct DeleteTaskAnswer {
    pub id: String,
    #[serde(default)]
    pub tasks: Vec<Task>,
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
    fn writes_every_field_back_in_the_schemas_order_and_drops_unknown_ones() {
        let task: Task = serde_json::from_str(TASK).unwrap();
        let written = serde_json::to_string(&task).unwrap();
        assert_eq!(
            written,
            concat!(
                r#"{"id":"T-0143","title":"Rust migration phase 1","description":"","#,
                r#""state":"needs-input","priority":"normal","labels":[],"agent":"claude-code","#,
                r#""branch":"natamux/t-0143","workspace":"/w/T-0143","pr":null,"prStatus":null,"#,
                r#""dependsOn":[],"parentId":null,"repo":"natamux","stage":"implement","#,
                r#""stages":["spec","implement"],"lanes":{},"paths":{},"stageSince":null,"#,
                r#""backgroundSince":null,"workflowStages":[],"context":null,"issue":null,"#,
                r#""createdAt":"2026-10-22T00:53:31.924Z","updatedAt":"2026-10-22T00:59:38.510Z"}"#
            )
        );
    }

    #[test]
    fn keeps_the_daemons_order_of_lanes_and_optional_fields_when_set() {
        let text = TASK.replace(
            r#""lanes": {}"#,
            r#""lanes": {"tests": "running", "docs": "review"}, "ciStages": ["implement"]"#,
        );
        let task: Task = serde_json::from_str(&text).unwrap();
        let written = serde_json::to_string(&task).unwrap();
        assert!(written.contains(r#""lanes":{"tests":"running","docs":"review"}"#));
        assert!(written.contains(r#""workflowStages":[],"ciStages":["implement"],"context""#));
        assert!(!written.contains("skipStages"));
    }

    #[test]
    fn fills_zods_defaults() {
        let task: Task =
            serde_json::from_str(r#"{"id":"T-1","title":"t","createdAt":"a","updatedAt":"b"}"#)
                .unwrap();
        assert_eq!(task.state, TaskState::Draft);
        assert_eq!(task.priority, "normal");
        assert_eq!(task.stage, "implement");
    }

    #[test]
    fn task_ids_are_upper_case_letters_a_dash_and_digits() {
        for id in ["T-42", "ABC-1", "T-0001"] {
            assert!(is_task_id(id), "{id}");
        }
        for id in [
            "t-42",
            "T42",
            "T-",
            "-1",
            "T-4a",
            "TÉ-1",
            "T-1-2",
            &format!("T-{}", "1".repeat(31)),
        ] {
            assert!(!is_task_id(id), "{id}");
        }
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

    #[test]
    fn inputs_send_only_what_is_set_in_the_routes_names() {
        let task = NewTaskInput {
            title: "Add subtask".into(),
            parent_id: Some("T-1".into()),
            repo: Some("natamux".into()),
        };
        assert_eq!(
            serde_json::to_string(&task).unwrap(),
            r#"{"title":"Add subtask","parentId":"T-1","repo":"natamux"}"#
        );
        let plain = NewTaskInput {
            title: "x".into(),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"title":"x"}"#);
        let template = TemplateTaskInput {
            title: "x".into(),
            repo: Some("a".into()),
        };
        assert_eq!(
            serde_json::to_string(&template).unwrap(),
            r#"{"title":"x","repo":"a"}"#
        );
        let patch = TitlePatch {
            title: "New".into(),
        };
        assert_eq!(serde_json::to_string(&patch).unwrap(), r#"{"title":"New"}"#);
    }

    #[test]
    fn reads_the_delete_answer_and_the_task_answer() {
        let answer: DeleteTaskAnswer =
            serde_json::from_str(&format!(r#"{{"id":"T-2","tasks":[{TASK}]}}"#)).unwrap();
        assert_eq!(answer.id, "T-2");
        assert_eq!(answer.tasks.len(), 1);
        let made: TaskAnswer = serde_json::from_str(&format!(r#"{{"task":{TASK}}}"#)).unwrap();
        assert_eq!(made.task.id, "T-0143");
    }

    #[test]
    fn reads_the_pull_request_status_of_a_task() {
        let mut task: Task = serde_json::from_str(TASK).unwrap();
        assert_eq!(task.pull_request_status(), None);
        task.pr_status = Some(serde_json::json!({"state":"merged","checks":"passing","x":1}));
        let status = task.pull_request_status().unwrap();
        assert_eq!(
            (status.state.as_str(), status.checks.as_str()),
            ("merged", "passing")
        );
    }
}
