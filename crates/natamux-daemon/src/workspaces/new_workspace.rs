//! `new-workspace.ts`: the quick way to start work: a task, its worktree, and its first stage
//! started when an agent is chosen.

use std::sync::Arc;

use natamux_proto::projects::repo::ORIGIN_PREFIX;
use natamux_proto::tasks::task::{Task, TaskContext};
use natamux_store::attachments::AttachmentFile;
use serde_json::{Map, Value, json};

use crate::stages::errors::StageResult;
use crate::tasks::service::TaskService;
use crate::workspaces::branch_name::is_branch_name;
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
    /// The branch to work on; none is the project's `git.branchName`, or `natamux/<id>-<slug>`.
    pub branch: Option<String>,
    /// The agent that works on the task's stages; none leaves a plain shell.
    pub agent: Option<String>,
    /// Passed to the agent as `--model`; none is the agent's own default.
    pub model: Option<String>,
    /// None uses the project's full pipeline; single-agent keeps Context and Agent only.
    pub mode: Option<String>,
    /// The task's notes. With a manual context source they are its context.
    pub prompt: Option<String>,
    /// Where the context comes from, when not from the notes: a GitHub issue, a skill, an MCP
    /// server.
    pub context: Option<TaskContext>,
    /// Files for the agents to read, already checked.
    pub attachments: Vec<AttachmentFile>,
    /// No worktree: the task works in the project folder itself, on the branch checked out there.
    pub no_worktree: bool,
}

/// The branch name and the base branch are checked first, so a typo leaves no task behind. The
/// answer comes once the task is made: the worktree of a big repository can take minutes, so it is
/// made after. A base `origin/<name>` is checked with the worktree: it needs the network.
pub fn new_workspace(parts: &NewWorkspaceParts, input: NewWorkspaceInput) -> WorkspaceResult<Task> {
    if let Some(branch) = &input.branch
        && !is_branch_name(branch)
    {
        return Err(WorkspaceError::BadRequest(format!(
            "Branch name {branch} is not valid"
        )));
    }
    if let Some(base) = &input.base {
        let listed = parts.workspaces.branches(input.repo.as_deref())?;
        if !listed.branches.contains(base) && !base.starts_with(ORIGIN_PREFIX) {
            return Err(WorkspaceError::Conflict(format!(
                "There is no local branch named {base}"
            )));
        }
    }

    // Before the task is made, so a refusal leaves no task behind.
    if input.no_worktree {
        // shortcut: two requests at the same moment can both pass; the app sends one at a time.
        parts
            .workspaces
            .check_main_checkout_free(input.repo.as_deref())?;
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
    if let Some(model) = &input.model {
        raw.insert("model".into(), json!(model));
    }
    if let Some(mode) = &input.mode {
        raw.insert("mode".into(), json!(mode));
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
    if input.no_worktree {
        return main_checkout(parts, &created.id, input.agent.is_some());
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
        branch: input.branch,
        agent: input.agent,
        claim,
    };
    parts.workspaces.background(move || prepare.run());
    Ok(answer)
}

/// The task works in the project folder: nothing to fetch or check out, so it is ready before the
/// answer. A failure stays on the task, as for a worktree.
fn main_checkout(parts: &NewWorkspaceParts, task_id: &str, agent: bool) -> WorkspaceResult<Task> {
    match parts.workspaces.use_main_checkout(task_id) {
        Ok(task) if agent => {
            let start = Arc::clone(&parts.start_stage);
            let log = Arc::clone(&parts.workspaces.log);
            let id = task_id.to_string();
            parts.workspaces.background(move || {
                if let Err(error) = start(&id) {
                    log(&format!("{id}: the first stage did not start: {error}"));
                }
            });
            Ok(task)
        }
        Ok(task) => Ok(task),
        Err(error) => {
            let _ = parts
                .tasks
                .record_workspace_failure(task_id, &error.to_string());
            Ok(parts.tasks.get_task(task_id)?)
        }
    }
}

struct Prepare {
    tasks: TaskService,
    workspaces: Arc<WorkspaceService>,
    start_stage: StartStage,
    task: Task,
    base: Option<String>,
    branch: Option<String>,
    agent: Option<String>,
    claim: Claim,
}

impl Prepare {
    // A failure stays on the task, so the person can try again from it: the dialog is gone by now.
    fn run(self) {
        let task_id = self.task.id.clone();
        let made =
            self.workspaces
                .finish_create(&self.task, self.base.as_deref(), self.branch.as_deref());
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
