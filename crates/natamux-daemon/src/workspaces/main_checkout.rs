//! A task that works in the project folder itself, with no worktree (`mainCheckout`). Natamux
//! never deletes that folder, its changes or its branch: removing the workspace only detaches it.

use std::fs;
use std::io;
use std::path::Path;

use natamux_git::branches::list_branches;
use natamux_git::worktrees::tracks_path;
use natamux_proto::tasks::task::Task;
use natamux_stages::files::archive_stage_files;
use natamux_store::task_files::STAGE_DIR;

use crate::tasks::records::WorkspaceChange;
use crate::workspaces::errors::{RemoveWorkspaceResult, WorkspaceError, WorkspaceResult};
use crate::workspaces::service::WorkspaceService;

impl WorkspaceService {
    /// The tasks, finished ones too, that work in the main checkout of the project now.
    fn main_checkout_holders(&self, repo: Option<&str>) -> WorkspaceResult<Vec<Task>> {
        let folder = self.repo_dir_of(repo)?;
        let holders = self.tasks.list_tasks(None)?.into_iter().filter(|task| {
            task.main_checkout == Some(true)
                && task.workspace.is_some()
                && self
                    .repo_dir_of(task.repo.as_deref())
                    .is_ok_and(|dir| dir == folder)
        });
        Ok(holders.collect())
    }

    /// Refuses while an open task works in the main checkout: two tasks would share the stage
    /// files and the folder that hooks find a task by.
    pub fn check_main_checkout_free(&self, repo: Option<&str>) -> WorkspaceResult<()> {
        let holders = self.main_checkout_holders(repo)?;
        match holders.iter().find(|task| !task.state.is_finished()) {
            Some(open) => Err(WorkspaceError::Conflict(format!(
                "{} already works in the main checkout of this project. Finish it or use a \
                 worktree",
                open.id
            ))),
            None => Ok(()),
        }
    }

    /// Gives the task the project folder as its workspace, on the branch checked out there (none
    /// on a detached HEAD). Nothing is checked out. A finished task that still holds the folder is
    /// detached first, so its stage files do not reach the new task.
    pub fn use_main_checkout(&self, task_id: &str) -> WorkspaceResult<Task> {
        let task = self.tasks.get_task(task_id)?;
        let repo = task.repo.as_deref();
        let holders = self.main_checkout_holders(repo)?;
        for old in holders.iter().filter(|task| task.state.is_finished()) {
            self.remove(&old.id, false)?;
        }
        let folder = self.repo_dir_of(repo)?;
        let branch = list_branches(&folder).ok().and_then(|list| list.current);
        let workspace = folder.to_string_lossy().into_owned();
        Ok(self.tasks.record_main_checkout(task_id, workspace, branch)?)
    }

    /// `remove` for a main checkout task: the terminals end and the stage files are archived and
    /// cleared. The folder, its changes and its branch stay.
    pub(crate) fn detach_main_checkout(
        &self,
        task: &Task,
        workspace: &str,
    ) -> WorkspaceResult<RemoveWorkspaceResult> {
        let task_id = task.id.as_str();
        (self.close_terminals)(task_id);
        let cleared = archive_stage_files(&self.data_dir, task_id, Some(workspace))
            .and_then(|()| remove_stage_dir(Path::new(workspace)))
            .map_err(|error| WorkspaceError::Io(error.to_string()));
        if let Err(error) = cleared {
            // The agent ended with its terminal. Left running, the task would wait for it forever.
            let _ = self.tasks.stop_agents(task_id);
            return Err(error);
        }
        self.drop_rollback(task_id);
        self.tasks.record_workspace(
            task_id,
            WorkspaceChange {
                workspace: None,
                branch: task.branch.clone(),
            },
        )?;
        Ok(RemoveWorkspaceResult {
            task: self.tasks.stop_agents(task_id)?,
            branch_deleted: false,
        })
    }
}

/// The stage folder is ours (git ignores it), unless the project commits it: then it is the user's.
fn remove_stage_dir(folder: &Path) -> io::Result<()> {
    let dir = folder.join(STAGE_DIR);
    match fs::symlink_metadata(&dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) if tracks_path(folder, STAGE_DIR) => Ok(()),
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(&dir),
        Ok(_) => fs::remove_file(&dir),
    }
}
