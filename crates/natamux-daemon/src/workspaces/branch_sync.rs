//! The agent of a task may switch its worktree to another branch (the head of a pull request, or a
//! name a team rule asks for). The pull request finder, the CI gate and the branch delete follow
//! the branch the work is on, not the one the worktree was made with.

use std::path::Path;

use natamux_git::branches::list_branches;
use natamux_git::exec::current_branch;
use natamux_proto::projects::repo::BranchList;
use natamux_proto::tasks::task::Task;

use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::service::TaskService;
use crate::workspaces::errors::WorkspaceResult;
use crate::workspaces::service::WorkspaceService;

/// Records the branch checked out in the task's worktree when it is not `task.branch`, and gives
/// the task as it is now. Nothing changes when the task has no worktree or no branch, its folder
/// is gone, HEAD is detached, the branch is not one of the repository, or it is a main one (see
/// `is_main_branch`). For the services that have no `WorkspaceService`. `repo_dir`: the folder
/// of a repository by its id (None: the default one).
pub fn sync_branch<F>(tasks: &TaskService, repo_dir: &F, task_id: &str) -> TaskResult<Task>
where
    F: Fn(Option<&str>) -> Result<String, TaskError> + ?Sized,
{
    let task = tasks.get_task(task_id)?;
    let list_of = || branch_list(repo_dir(task.repo.as_deref()));
    let Some(to) = switched_branch(&task, list_of) else {
        return Ok(task);
    };
    let from = task.branch.clone();
    tasks.record_branch(task_id, from, to)
}

fn switched_branch(task: &Task, list_of: impl FnOnce() -> Option<BranchList>) -> Option<String> {
    let folder = Path::new(task.workspace.as_deref()?);
    if !folder.is_dir() {
        return None;
    }
    // A task without a branch has nothing to follow; the CI check refuses it.
    let old = task.branch.as_deref()?;
    let branch = current_branch(folder)?;
    if branch == old {
        return None;
    }
    let list = list_of()?;
    // A folder that is not a worktree of the repository may be on a branch of its own.
    let ours = list.branches.contains(&branch);
    (ours && !is_main(&list, &branch)).then_some(branch)
}

/// The default branch of the task's repository, or the one its main checkout is on. Such a branch
/// is never taken as the task's own, and never deleted with its workspace. A repository git cannot
/// read counts as main, so nothing is taken or deleted on a guess.
pub fn is_main_branch<F>(repo_dir: &F, task: &Task, branch: &str) -> bool
where
    F: Fn(Option<&str>) -> Result<String, TaskError> + ?Sized,
{
    branch_list(repo_dir(task.repo.as_deref())).is_none_or(|list| is_main(&list, branch))
}

fn branch_list(dir: Result<String, TaskError>) -> Option<BranchList> {
    list_branches(Path::new(&dir.ok()?)).ok()
}

fn is_main(list: &BranchList, branch: &str) -> bool {
    list.default_branch.as_deref() == Some(branch) || list.current.as_deref() == Some(branch)
}

impl WorkspaceService {
    /// `sync_branch` for the task.
    pub fn sync_branch(&self, task_id: &str) -> WorkspaceResult<Task> {
        let task = sync_branch(&self.tasks, &|repo| self.repos.path_of(repo), task_id)?;
        Ok(task)
    }

    pub(crate) fn is_main_branch(&self, task: &Task, branch: &str) -> bool {
        is_main_branch(&|repo| self.repos.path_of(repo), task, branch)
    }
}
