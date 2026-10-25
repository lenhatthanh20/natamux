//! `remove` of `workspace-service.ts`.

use std::path::Path;

use natamux_git::changes::list_changes;
use natamux_git::worktrees::{WorktreeState, has_submodules, remove_worktree, worktree_state};
use natamux_stages::files::archive_stage_files;

use crate::tasks::records::WorkspaceChange;
use crate::workspaces::branch_name::delete_finished_branch;
use crate::workspaces::errors::{RemoveWorkspaceResult, WorkspaceError, WorkspaceResult};
use crate::workspaces::service::WorkspaceService;

impl WorkspaceService {
    /// Removes the worktree folder. The branch goes too when it is fully merged, otherwise it
    /// stays (with `delete_branch`). A folder that was already deleted outside Natamux is only
    /// cleared from git and the task.
    pub fn remove(
        &self,
        task_id: &str,
        delete_branch: bool,
    ) -> WorkspaceResult<RemoveWorkspaceResult> {
        self.exclusive(task_id, "Removing worktree", || {
            let task = self.tasks.get_task(task_id)?;
            let Some(workspace) = task.workspace.clone() else {
                return Err(WorkspaceError::Conflict(format!(
                    "{task_id} has no workspace"
                )));
            };
            let folder = Path::new(&workspace);
            let repo_dir = self.repo_dir_of(task.repo.as_deref())?;
            let state = worktree_state(&repo_dir, folder)?;
            // The folder may hold someone's own files, so it is never deleted.
            if state == WorktreeState::NotAWorktree {
                return Err(WorkspaceError::Conflict(format!(
                    "{workspace} is not a git worktree of this repository, so it is not removed"
                )));
            }
            let submodules = if state == WorktreeState::Present {
                clean_worktree(task_id, folder)?
            } else {
                false
            };
            // A terminal that runs in the folder would keep files open while git removes it.
            (self.close_terminals)(task_id);
            // A stale entry keeps git from deleting the branch or adding the worktree again.
            if state != WorktreeState::Missing {
                // After the terminals ended, so the agent writes no stage file after the copy.
                let removed = archive_stage_files(&self.data_dir, task_id, Some(&workspace))
                    .map_err(|error| WorkspaceError::Io(error.to_string()))
                    .and_then(|()| {
                        remove_worktree(&repo_dir, folder, submodules).map_err(Into::into)
                    });
                if let Err(error) = removed {
                    // The agent ended with its terminal. Left running, the task would wait for it
                    // forever.
                    let _ = self.tasks.stop_agents(task_id);
                    return Err(error);
                }
            }

            let branch_deleted = match (&task.branch, delete_branch) {
                (Some(branch), true) => {
                    delete_finished_branch(&repo_dir, branch, task.pr_status.as_ref())
                }
                _ => false,
            };
            self.drop_rollback(task_id);
            self.tasks.record_workspace(
                task_id,
                WorkspaceChange {
                    workspace: None,
                    branch: if branch_deleted {
                        None
                    } else {
                        task.branch.clone()
                    },
                },
            )?;
            // After the workspace is cleared: a hook of the ending agent may have moved the task
            // while git worked, and later hooks are ignored (see AgentHookService).
            Ok(RemoveWorkspaceResult {
                task: self.tasks.stop_agents(task_id)?,
                branch_deleted,
            })
        })
    }
}

/// Refuses a worktree with changes that are not committed (git would), before the agent and the
/// terminals of the task are ended for nothing. A status that git cannot read proves nothing, so
/// its error stops the removal too. True when submodules are checked out: git refuses those too,
/// and the status counts changes inside them, so forcing loses no work.
fn clean_worktree(task_id: &str, folder: &Path) -> WorkspaceResult<bool> {
    let changes = list_changes(folder, false)?.len();
    if changes > 0 {
        let count = if changes == 1 {
            "1 change that is not committed. Commit or discard it".to_string()
        } else {
            format!("{changes} changes that are not committed. Commit or discard them")
        };
        return Err(WorkspaceError::Conflict(format!(
            "{task_id} has {count} first"
        )));
    }
    Ok(has_submodules(folder)?)
}
