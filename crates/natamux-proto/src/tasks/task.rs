//! A task as `GET /tasks` sends it (`taskSchema`): every
//! field, in the schema's order, so the `natamux` command prints the JSON the TypeScript one
//! prints. Fields this build does not know are dropped, as zod does, so a newer daemon still
//! parses.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `TASK_STATES`.
pub const TASK_STATES: [&str; 10] = [
    "draft",
    "ready",
    "running",
    "idle",
    "needs-input",
    "review",
    "blocked",
    "failed",
    "done",
    "cancelled",
];

/// `TASK_PRIORITIES`.
pub const TASK_PRIORITIES: [&str; 4] = ["low", "normal", "high", "urgent"];

/// `PULL_REQUEST_STATES`.
pub const PULL_REQUEST_STATES: [&str; 4] = ["open", "draft", "merged", "closed"];

/// `CHECK_STATES`.
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
    /// The agent ended its turn without a question: nothing waits for the user.
    Idle,
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
            TaskState::Idle => "Idle",
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

    /// The agent is there: it works, ended its turn, or waits for an answer.
    pub fn has_live_agent(self) -> bool {
        matches!(
            self,
            TaskState::Running | TaskState::Idle | TaskState::NeedsInput
        )
    }

    /// The state's name, as the daemon writes it.
    pub fn name(self) -> &'static str {
        match self {
            TaskState::Draft => "draft",
            TaskState::Ready => "ready",
            TaskState::Running => "running",
            TaskState::Idle => "idle",
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
    /// Passed to the task's agent as `--model`; None: the agent's own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub pr: Option<String>,
    /// A stored value that is not a status reads as None, so the task still reads.
    #[serde(default, deserialize_with = "status_or_none")]
    pub pr_status: Option<PullRequestStatus>,
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
    /// When a turn that background work started last ended (T-0390).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_result_at: Option<String>,
    #[serde(default)]
    pub workflow_stages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_stages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_stages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub without_agent: Option<bool>,
    /// The task works in the project folder itself, not in a worktree: Natamux never makes a
    /// worktree for it nor deletes that folder or its branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_checkout: Option<bool>,
    #[serde(default)]
    pub context: Option<TaskContext>,
    #[serde(default)]
    pub issue: Option<IssueLink>,
    #[serde(default)]
    pub created_at: String,
    /// ISO 8601 in UTC, so it sorts as text. Absent in the subtask view an agent gets.
    #[serde(default)]
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
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestStatus {
    pub state: String,
    #[serde(default = "no_checks")]
    pub checks: String,
}

fn no_checks() -> String {
    "none".into()
}

fn status_or_none<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PullRequestStatus>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

impl Task {
    /// None when GitHub was not asked yet, or the stored value is not a status.
    pub fn pull_request_status(&self) -> Option<PullRequestStatus> {
        self.pr_status.clone()
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

/// `EDITABLE_FIELDS`: what a person may change after a task is created.
pub const EDITABLE_FIELDS: [&str; 6] = [
    "title",
    "description",
    "priority",
    "labels",
    "dependsOn",
    "model",
];

/// The body of `PATCH /tasks/:id` (`updateTaskInputSchema`): a field left out stays as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    /// An empty name clears the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
#[path = "task_tests.rs"]
mod tests;
