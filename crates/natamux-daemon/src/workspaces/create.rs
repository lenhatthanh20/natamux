//! `create` of `workspace-service.ts`: makes the worktree of a task, and tells the steps it is at
//! (`workspace.preparing`).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use natamux_git::branches::list_branches;
use natamux_git::graph::{fetch_upstream, start_ref};
use natamux_git::worktrees::{add_worktree, remove_worktree};
use natamux_proto::tasks::task::Task;

use crate::shared::event_bus::DaemonMessage;
use crate::tasks::records::WorkspaceChange;
use crate::workspaces::branch_name::branch_name_for;
use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::service::WorkspaceService;

/// How long each step of a new workspace took, in ms, in the order the steps ended.
type StepTimes = Vec<(&'static str, i64)>;

/// The job label of a new worktree.
pub const CREATING: &str = "Creating worktree";

struct Made {
    repo_dir: PathBuf,
    worktree_path: PathBuf,
    branch: String,
}

impl WorkspaceService {
    /// Makes the worktree from `base` when given (a local branch), or else from the parent's
    /// branch for a subtask, or else from the default branch, as its remote has it now. The local
    /// branch when it has commits that are not pushed, or has no remote. HEAD when the repository
    /// has no branch to start from.
    pub fn create(&self, task_id: &str, base: Option<&str>) -> WorkspaceResult<Task> {
        self.exclusive(task_id, CREATING, || {
            let task = self.begin_create(task_id)?;
            self.finish_create(&task, base)
        })
    }

    /// The part of `create` that runs before Node's first `await`: the checks and the `fetch`
    /// step. A new workspace runs it before its answer, under a `claim` of the task.
    pub fn begin_create(&self, task_id: &str) -> WorkspaceResult<Task> {
        let task = self.tasks.get_task(task_id)?;
        if let Some(workspace) = &task.workspace {
            return Err(WorkspaceError::Conflict(format!(
                "{task_id} already has a workspace at {workspace}"
            )));
        }
        if task.state.is_finished() {
            return Err(WorkspaceError::Conflict(format!(
                "{task_id} is {}, so it gets no workspace",
                task.state.name()
            )));
        }
        // Told at once, so a dashboard that opens the task right after the answer sees it.
        self.set_step(task_id, "fetch");
        Ok(task)
    }

    /// The rest of `create`: git, the record, the setup script.
    pub fn finish_create(&self, task: &Task, base: Option<&str>) -> WorkspaceResult<Task> {
        let task_id = task.id.as_str();
        let mut times = StepTimes::new();
        let made = match self.add_task_worktree(task, base, &mut times) {
            Ok(made) => made,
            Err(error) => {
                self.end_steps(task_id, &times, Some(&error));
                return Err(error);
            }
        };
        let recorded = self.tasks.record_workspace(
            task_id,
            WorkspaceChange {
                workspace: Some(made.worktree_path.to_string_lossy().into_owned()),
                branch: Some(made.branch),
            },
        );
        let created = match recorded {
            Ok(created) => created,
            Err(error) => {
                let _ = remove_worktree(&made.repo_dir, &made.worktree_path, false);
                let error = WorkspaceError::from(error);
                self.end_steps(task_id, &times, Some(&error));
                return Err(error);
            }
        };
        self.end_steps(task_id, &times, None);
        // The workspace is made either way; a failing setup shows up in its terminal.
        if let Some(on_created) = &self.on_created {
            let _ = catch_unwind(AssertUnwindSafe(|| on_created(&created)));
        }
        Ok(created)
    }

    // The git part of create. `times` gets how long each step took.
    fn add_task_worktree(
        &self,
        task: &Task,
        base: Option<&str>,
        times: &mut StepTimes,
    ) -> WorkspaceResult<Made> {
        let repo_dir = self.repo_dir_of(task.repo.as_deref())?;
        let listed = list_branches(&repo_dir)?;
        let branches = &listed.branches;
        if let Some(base) = base
            && !branches.iter().any(|branch| branch == base)
        {
            return Err(WorkspaceError::Conflict(format!(
                "There is no local branch named {base}"
            )));
        }
        // A workspace removed with its branch kept leaves the task's work there: go on with it.
        let kept = task
            .branch
            .as_ref()
            .filter(|branch| branches.contains(branch))
            .cloned();
        let worktree_path = self.worktree_path_for(task);
        let branch = kept
            .clone()
            .unwrap_or_else(|| branch_name_for(&task.id, &task.title));
        let made = |branch: String| Made {
            repo_dir: repo_dir.clone(),
            worktree_path: worktree_path.clone(),
            branch,
        };
        if kept.is_some() {
            self.timed(&task.id, "checkout", times, || {
                add_worktree(&repo_dir, &worktree_path, &branch, None, true)
            })?;
            return Ok(made(branch));
        }
        // A subtask's work is merged into its parent's branch, so it starts there.
        let parent = match &task.parent_id {
            Some(parent_id) => self.tasks.get_task(parent_id)?.branch,
            None => None,
        };
        let from = base
            .map(str::to_string)
            .or_else(|| parent.filter(|parent| branches.contains(parent)))
            .or(listed.default_branch.clone());
        let Some(from) = from else {
            self.timed(&task.id, "checkout", times, || {
                add_worktree(&repo_dir, &worktree_path, &branch, None, false)
            })?;
            return Ok(made(branch));
        };
        self.timed(&task.id, "fetch", times, || {
            fetch_upstream(&repo_dir, &from);
            Ok::<_, WorkspaceError>(())
        })?;
        let start = start_ref(&repo_dir, &from).unwrap_or(from);
        self.timed(&task.id, "checkout", times, || {
            add_worktree(&repo_dir, &worktree_path, &branch, Some(&start), false)
        })?;
        Ok(made(branch))
    }

    // Runs one step: tells it, and writes down how long it took, also when it fails.
    fn timed<T, E>(
        &self,
        task_id: &str,
        name: &'static str,
        times: &mut StepTimes,
        work: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        self.set_step(task_id, name);
        let started = (self.now)();
        let result = work();
        times.push((name, (self.now)() - started));
        result
    }

    // create tells `fetch` before it reads the branches, and again when the fetch starts.
    fn set_step(&self, task_id: &str, step: &'static str) {
        {
            let mut steps = self.lock_steps();
            match steps.iter_mut().find(|(id, _)| id == task_id) {
                Some((_, known)) if *known == step => return,
                Some((_, known)) => *known = step,
                None => steps.push((task_id.to_string(), step)),
            }
        }
        self.bus.publish(DaemonMessage::WorkspacePreparing {
            task_id: task_id.to_string(),
            step: Some(step.to_string()),
            error: None,
        });
    }

    // Tells that the steps of a new workspace ended, and writes down how long each one took.
    pub(crate) fn end_steps(
        &self,
        task_id: &str,
        times: &[(&'static str, i64)],
        error: Option<&WorkspaceError>,
    ) {
        {
            let mut steps = self.lock_steps();
            let Some(at) = steps.iter().position(|(id, _)| id == task_id) else {
                return;
            };
            steps.remove(at);
        }
        self.bus.publish(DaemonMessage::WorkspacePreparing {
            task_id: task_id.to_string(),
            step: None,
            error: error.map(ToString::to_string),
        });
        let mut parts: Vec<String> = times
            .iter()
            .map(|(name, ms)| format!("{name} {ms} ms"))
            .collect();
        parts.push(if error.is_none() { "ok" } else { "failed" }.to_string());
        (self.log)(&format!("{task_id} workspace: {}", parts.join(", ")));
    }
}
