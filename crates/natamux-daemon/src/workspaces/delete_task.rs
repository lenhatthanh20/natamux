//! The delete of a task with its worktree and branch: one flow for `DELETE /tasks/:id` and the
//! retention job, so retention never takes a shortcut the person's delete does not.

use natamux_proto::tasks::task::Task;

use crate::workspaces::errors::WorkspaceResult;
use crate::workspaces::service::WorkspaceService;

impl WorkspaceService {
    /// Uncommitted changes refuse the delete, so the task, the worktree and the branch all stay;
    /// `force` discards them instead. Gives the tasks the delete changed (parent, dependencies).
    pub fn delete_task_and_workspace(&self, id: &str, force: bool) -> WorkspaceResult<Vec<Task>> {
        // The branch the agent switched to goes, not the one the worktree was made with.
        let current = self.sync_branch(id)?;
        if current.workspace.is_some() {
            self.remove_for_delete(id, force)?;
        }
        self.delete_branches_of(&current)?;
        Ok(self.tasks.delete_task(id)?)
    }
}
