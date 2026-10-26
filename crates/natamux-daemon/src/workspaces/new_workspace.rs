//! `new-workspace.ts`: the quick way to start work: a task, its worktree, and its first stage
//! started when an agent is chosen.

use std::sync::Arc;

use natamux_proto::projects::repo::ORIGIN_PREFIX;
use natamux_proto::tasks::task::{Task, TaskContext};
use natamux_store::attachments::AttachmentFile;
use serde_json::{Map, Value, json};

use crate::stages::errors::StageResult;
use crate::tasks::service::TaskService;
use crate::workspaces::create::CREATING;
use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::fan_out::stage_failure;
use crate::workspaces::service::{Claim, WorkspaceService};

/// Makes the task stay without an agent until a person starts one: `StageRunner::leave_to_person`.
pub type LeaveToPerson = Arc<dyn Fn(&str) -> StageResult<()> + Send + Sync>;
/// Starts the first stage of a task: the `start` action of the stage runner.
pub type StartStage = Arc<dyn Fn(&str) -> StageResult<Task> + Send + Sync>;

pub struct NewWorkspaceParts {
    pub tasks: TaskService,
    pub workspaces: Arc<WorkspaceService>,
    pub leave_to_person: LeaveToPerson,
    pub start_stage: StartStage,
}

#[derive(Clone, Debug, Default)]
pub struct NewWorkspaceInput {
    pub title: String,
    /// The repository id; none is the default repository.
    pub repo: Option<String>,
    /// A local branch to start from, or `origin/<name>`; none is the repository's default branch.
    pub base: Option<String>,
    /// The agent that works on the task's stages; none leaves a plain shell.
    pub agent: Option<String>,
    /// The task's notes. With a manual context source they are its context.
    pub prompt: Option<String>,
    /// Where the context comes from, when not from the notes: a GitHub issue, a skill, an MCP
    /// server.
    pub context: Option<TaskContext>,
    /// Files for the agents to read, already checked.
    pub attachments: Vec<AttachmentFile>,
}

/// The base branch is checked first, so a typo leaves no task behind. The answer comes once the
/// task is made: the worktree of a big repository can take minutes, so it is made after. A base
/// `origin/<name>` is checked with the worktree: it needs the network.
pub fn new_workspace(parts: &NewWorkspaceParts, input: NewWorkspaceInput) -> WorkspaceResult<Task> {
    if let Some(base) = &input.base {
        let listed = parts.workspaces.branches(input.repo.as_deref())?;
        if !listed.branches.contains(base) && !base.starts_with(ORIGIN_PREFIX) {
            return Err(WorkspaceError::Conflict(format!(
                "There is no local branch named {base}"
            )));
        }
    }

    let mut raw = Map::new();
    raw.insert("title".into(), json!(input.title));
    raw.insert(
        "description".into(),
        json!(input.prompt.clone().unwrap_or_default()),
    );
    if let Some(repo) = &input.repo {
        raw.insert("repo".into(), json!(repo));
    }
    if let Some(agent) = &input.agent {
        raw.insert("agent".into(), json!(agent));
    }
    if let Some(context) = &input.context {
        raw.insert(
            "context".into(),
            serde_json::to_value(context).unwrap_or(Value::Null),
        );
    }
    let created = parts
        .tasks
        .create_task(Some(&Value::Object(raw)), &input.attachments, true)?;
    if input.agent.is_none() {
        // A first stage that passes by itself, such as a manual context, would start the next
        // stage's agent in the new workspace.
        (parts.leave_to_person)(&created.id).map_err(stage_failure)?;
    }
    // As Node's `create` up to its first `await`: the task is taken, its job listed and its
    // `fetch` step told before the answer, so a dashboard that opens the task sees it and a
    // second workspace for it gets the 409.
    let started = parts
        .workspaces
        .claim(&created.id, CREATING)
        .and_then(|claim| Ok((claim, parts.workspaces.begin_create(&created.id)?)));
    // Read before the worktree is made: the answer is the task as it was made.
    let answer = parts.tasks.get_task(&created.id)?;
    let (claim, task) = match started {
        Ok(started) => started,
        Err(error) => {
            // Node's promise of `create` rejects after the answer, into the failure of the task.
            let _ = parts
                .tasks
                .record_workspace_failure(&created.id, &error.to_string());
            return Ok(answer);
        }
    };
    let prepare = Prepare {
        tasks: parts.tasks.clone(),
        workspaces: Arc::clone(&parts.workspaces),
        start_stage: Arc::clone(&parts.start_stage),
        task,
        base: input.base,
        agent: input.agent,
        claim,
    };
    parts.workspaces.background(move || prepare.run());
    Ok(answer)
}

struct Prepare {
    tasks: TaskService,
    workspaces: Arc<WorkspaceService>,
    start_stage: StartStage,
    task: Task,
    base: Option<String>,
    agent: Option<String>,
    claim: Claim,
}

impl Prepare {
    // A failure stays on the task, so the person can try again from it: the dialog is gone by now.
    fn run(self) {
        let task_id = self.task.id.clone();
        let made = self
            .workspaces
            .finish_create(&self.task, self.base.as_deref());
        // Freed before the stage starts, as Node's `create` ends before `act(start)`.
        drop(self.claim);
        if let Err(error) = made {
            let _ = self
                .tasks
                .record_workspace_failure(&task_id, &error.to_string());
            return;
        }
        if self.agent.is_some()
            && let Err(error) = (self.start_stage)(&task_id)
        {
            (self.workspaces.log)(&format!(
                "{task_id}: the first stage did not start: {error}"
            ));
        }
    }
}
