//! `create` of `workspace-service.ts`: makes the worktree of a task, and tells the steps it is at
//! (`workspace.preparing`).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use natamux_git::branches::list_branches;
use natamux_git::graph::{fetch_origin_branch, fetch_upstream, start_ref};
use natamux_git::worktrees::{WorktreeBranch, add_worktree, remove_worktree, worktree_of_branch};
use natamux_proto::projects::repo::ORIGIN_PREFIX;
use natamux_proto::tasks::task::Task;

use crate::shared::event_bus::DaemonMessage;
use crate::tasks::records::WorkspaceChange;
use crate::workspaces::branch_name::{branch_name_for, branch_name_from, is_branch_name};
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
    /// Makes the worktree from `base` when given (a local branch, or `origin/<name>` fetched now),
    /// or else from the parent's branch for a subtask, or else from the default branch, as its
    /// remote has it now. The local branch when it has commits that are not pushed, or has no
    /// remote. HEAD when the repository has no branch to start from.
    ///
    /// The branch is `branch` when given, else the project's `git.branchName`, else
    /// `natamux/<id>-<slug>`. A named branch that exists is checked out instead: a local one as it
    /// is, one only on origin as a local branch that follows it.
    pub fn create(
        &self,
        task_id: &str,
        base: Option<&str>,
        branch: Option<&str>,
    ) -> WorkspaceResult<Task> {
        self.exclusive(task_id, CREATING, || {
            let task = self.begin_create(task_id)?;
            self.finish_create(&task, base, branch)
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
    pub fn finish_create(
        &self,
        task: &Task,
        base: Option<&str>,
        branch: Option<&str>,
    ) -> WorkspaceResult<Task> {
        let task_id = task.id.as_str();
        let mut times = StepTimes::new();
        let made = match self.add_task_worktree(task, base, branch, &mut times) {
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
        branch: Option<&str>,
        times: &mut StepTimes,
    ) -> WorkspaceResult<Made> {
        let repo_dir = self.repo_dir_of(task.repo.as_deref())?;
        let listed = list_branches(&repo_dir)?;
        let branches = &listed.branches;
        let worktree_path = self.worktree_path_for(task);
        let made = |branch: String| Made {
            repo_dir: repo_dir.clone(),
            worktree_path: worktree_path.clone(),
            branch,
        };
        // A workspace removed with its branch kept leaves the task's work there: go on with it.
        if let Some(kept) = task.branch.as_ref().filter(|kept| branches.contains(kept)) {
            self.timed(&task.id, "checkout", times, || {
                add_worktree(&repo_dir, &worktree_path, kept, WorktreeBranch::Existing)
            })?;
            return Ok(made(kept.clone()));
        }
        let named = match branch {
            Some(branch) => Some(branch.to_string()),
            None => {
                let settings = self.repos.settings_of(task.repo.as_deref());
                let pattern = settings.git.branch_name;
                pattern.map(|pattern| branch_name_from(Some(&pattern), &task.id, &task.title))
            }
        };
        // A named branch that exists, here or on origin, is checked out with its work; the base
        // does not count then. The default name holds the task id, so it is never there.
        if let Some(name) = &named {
            if !is_branch_name(name) {
                return Err(WorkspaceError::Conflict(format!(
                    "Branch name {name} is not valid"
                )));
            }
            if branches.contains(name) {
                if let Some(path) = worktree_of_branch(&repo_dir, name)? {
                    return Err(WorkspaceError::Conflict(format!(
                        "Branch {name} is checked out in {path}"
                    )));
                }
                self.timed(&task.id, "checkout", times, || {
                    add_worktree(&repo_dir, &worktree_path, name, WorktreeBranch::Existing)
                })?;
                return Ok(made(name.clone()));
            }
            let on_origin = self.timed(&task.id, "fetch", times, || {
                Ok::<_, WorkspaceError>(fetch_origin_branch(&repo_dir, name))
            })?;
            if on_origin {
                // The full name: a tag called `origin/<name>` would win over the short one.
                let remote_ref = format!("refs/remotes/origin/{name}");
                let how = WorktreeBranch::Track(&remote_ref);
                self.timed(&task.id, "checkout", times, || {
                    add_worktree(&repo_dir, &worktree_path, name, how)
                })?;
                return Ok(made(name.clone()));
            }
        }
        let branch = named.unwrap_or_else(|| branch_name_for(&task.id, &task.title));
        // A base that is not a local branch can be a branch of origin, perhaps never fetched.
        if let Some(base) = base.filter(|base| !branches.iter().any(|known| known == base)) {
            let Some(name) = base.strip_prefix(ORIGIN_PREFIX) else {
                return Err(WorkspaceError::Conflict(format!(
                    "There is no local branch named {base}"
                )));
            };
            let start = self.origin_start(&task.id, &repo_dir, name, times)?;
            let how = WorktreeBranch::New(Some(&start));
            self.timed(&task.id, "checkout", times, || {
                add_worktree(&repo_dir, &worktree_path, &branch, how)
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
                add_worktree(&repo_dir, &worktree_path, &branch, WorktreeBranch::New(None))
            })?;
            return Ok(made(branch));
        };
        self.timed(&task.id, "fetch", times, || {
            fetch_upstream(&repo_dir, &from);
            Ok::<_, WorkspaceError>(())
        })?;
        let start = start_ref(&repo_dir, &from).unwrap_or(from);
        self.timed(&task.id, "checkout", times, || {
            add_worktree(&repo_dir, &worktree_path, &branch, WorktreeBranch::New(Some(&start)))
        })?;
        Ok(made(branch))
    }

    // Fetches branch `name` of origin, and gives where the worktree starts: its remote copy.
    fn origin_start(
        &self,
        task_id: &str,
        repo_dir: &Path,
        name: &str,
        times: &mut StepTimes,
    ) -> WorkspaceResult<String> {
        let known = self.timed(task_id, "fetch", times, || {
            Ok::<_, WorkspaceError>(fetch_origin_branch(repo_dir, name))
        })?;
        if !known {
            return Err(WorkspaceError::Conflict(format!(
                "There is no branch {name} on origin"
            )));
        }
        // The full name: a tag called `origin/<name>` would win over the short one.
        Ok(format!("refs/remotes/origin/{name}"))
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
