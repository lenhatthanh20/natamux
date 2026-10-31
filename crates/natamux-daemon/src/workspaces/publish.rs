//! `workspace-publish.ts`: takes the work of a workspace out: its commits, the pushes of its
//! branch, its pull request, and the draft of a commit message.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use natamux_agents::custom::agent_kind;
use natamux_git::changes::list_changes;
use natamux_git::commit_drafter::{DraftInput, MAX_DIFF_CHARS, draft_commit_message};
use natamux_git::commit_push::{commit_all, push_branch, read_commit_diff};
use natamux_git::gh::create_pull_request;
use natamux_git::graph::has_remote_branch;
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::changes::{CommitMessageDraft, DraftSource};
use serde_json::json;

use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::service::WorkspaceService;

/// What a draft that runs gives the callers that waited for it. The error is kept as its status,
/// code and text, since `WorkspaceError` cannot be copied.
type DraftResult = Result<CommitMessageDraft, (u16, &'static str, String)>;

/// One draft that runs, and the threads that wait for its answer.
#[derive(Default)]
pub struct DraftCell {
    result: Mutex<Option<DraftResult>>,
    done: Condvar,
}

impl DraftCell {
    fn lock(&self) -> MutexGuard<'_, Option<DraftResult>> {
        self.result
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set(&self, result: DraftResult) {
        *self.lock() = Some(result);
        self.done.notify_all();
    }

    fn wait(&self) -> DraftResult {
        let mut result = self.lock();
        loop {
            if let Some(ready) = result.as_ref() {
                return ready.clone();
            }
            result = self
                .done
                .wait(result)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Ends the draft of a task also when its thread panics, so the ones that wait do not hang.
struct DraftRun<'a> {
    service: &'a WorkspaceService,
    task_id: &'a str,
    cell: Arc<DraftCell>,
}

impl Drop for DraftRun<'_> {
    fn drop(&mut self) {
        if self.cell.lock().is_none() {
            self.cell.set(Err((
                500,
                "internal_error",
                "The commit message draft failed".to_string(),
            )));
        }
        self.service
            .drafts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(self.task_id);
    }
}

// The answer a waiting request gets has the status and code of the first request.
fn waited(cell: &DraftCell) -> WorkspaceResult<CommitMessageDraft> {
    cell.wait()
        .map_err(|(status, code, text)| WorkspaceError::Rebuilt { status, code, text })
}

impl WorkspaceService {
    /// Commits every change of the workspace on the task branch.
    pub fn commit(&self, task_id: &str, message: &str) -> WorkspaceResult<(Task, String)> {
        self.exclusive(task_id, "Committing", || {
            let committed = commit_all(&self.workspace_of(task_id)?, message)?;
            // So an older rollback cannot be undone over committed files.
            self.drop_rollback(task_id);
            let task = self.tasks.record_git(
                task_id,
                "git.committed",
                json!({ "hash": committed.hash, "subject": committed.subject }),
            )?;
            Ok((task, committed.hash))
        })
    }

    /// A commit message for the changes of the workspace, by the task's agent. It only reads, so a
    /// push that runs does not stop it. A second ask while one runs gets the same answer.
    pub fn draft_commit_message(&self, task_id: &str) -> WorkspaceResult<CommitMessageDraft> {
        let (cell, leader) = {
            let mut drafts = self
                .drafts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match drafts.get(task_id) {
                Some(running) => (Arc::clone(running), false),
                None => {
                    let cell = Arc::new(DraftCell::default());
                    drafts.insert(task_id.to_string(), Arc::clone(&cell));
                    (cell, true)
                }
            }
        };
        if !leader {
            return waited(&cell);
        }
        let _run = DraftRun {
            service: self,
            task_id,
            cell: Arc::clone(&cell),
        };
        let result = self.draft_now(task_id);
        cell.set(match &result {
            Ok(draft) => Ok(draft.clone()),
            Err(error) => {
                let (status, code) = error.status_and_code();
                Err((status, code, error.to_string()))
            }
        });
        result
    }

    fn draft_now(&self, task_id: &str) -> WorkspaceResult<CommitMessageDraft> {
        let workspace = self.workspace_of(task_id)?;
        let task = self.tasks.get_task(task_id)?;
        // A custom agent drafts as its base with the base's own program: its command may be a
        // function of the user's interactive shell, which this run does not load.
        let agent = task
            .agent
            .as_deref()
            .map(agent_kind)
            .filter(|agent| LAUNCHABLE_AGENTS.contains(&agent.as_str()))
            .unwrap_or_else(|| "claude-code".to_string());
        let diff = read_commit_diff(&workspace, MAX_DIFF_CHARS)?;
        let changes = list_changes(&workspace, false)?;
        let draft = draft_commit_message(
            &DraftInput {
                program: (self.program_of)(&agent),
                agent: agent.clone(),
                cwd: workspace,
                title: task.title,
                changes,
                diff,
                timeout_ms: None,
            },
            &*self.run_agent_once,
        );
        if draft.source == DraftSource::Draft {
            (self.log)(&format!(
                "No commit message draft by {agent} for {task_id}: {}",
                draft.reason.as_deref().unwrap_or("")
            ));
        }
        Ok(draft)
    }

    /// Pushes the task branch to `origin` and makes it the upstream of the branch.
    pub fn push(&self, task_id: &str) -> WorkspaceResult<Task> {
        self.exclusive(task_id, "Pushing", || self.push_now(task_id))
    }

    /// Pushes, then opens a pull request into the default branch, or into the parent's branch for
    /// a subtask. A task that has one keeps it.
    pub fn open_pull_request(&self, task_id: &str) -> WorkspaceResult<Task> {
        self.exclusive(task_id, "Opening pull request", || {
            let task = self.tasks.get_task(task_id)?;
            if let Some(pr) = &task.pr {
                return Err(WorkspaceError::Conflict(format!(
                    "{task_id} has a pull request already: {pr}"
                )));
            }
            let pushed = self.push_now(task_id)?;
            let base = match &task.parent_id {
                Some(parent_id) => self.tasks.get_task(parent_id)?.branch,
                None => None,
            };
            let workspace = self.workspace_of(task_id)?;
            // The base of a pull request must be on the remote. Pushed only once: after a subtask
            // is merged, the local copy is behind origin and a push would be refused.
            if let Some(base) = &base
                && !has_remote_branch(&workspace, base)?
            {
                push_branch(&workspace, base)?;
            }
            // Node opens the issue of the body first; when that fails, no pull request is opened.
            let body = (self.pull_request_body)(&pushed)?;
            let url = create_pull_request(
                &workspace,
                pushed.branch.as_deref().unwrap_or(""),
                &pushed.title,
                &body,
                base.as_deref(),
                &self.run_gh,
            )
            .map_err(|error| WorkspaceError::GitHub(error.message))?;
            Ok(self
                .tasks
                .record_git(task_id, "pr.opened", json!({ "url": url }))?)
        })
    }

    fn push_now(&self, task_id: &str) -> WorkspaceResult<Task> {
        let workspace = self.workspace_of(task_id)?;
        // The agent may have switched the worktree to another branch: push that one.
        let Some(branch) = self.sync_branch(task_id)?.branch else {
            return Err(WorkspaceError::Conflict(format!("{task_id} has no branch")));
        };
        push_branch(&workspace, &branch)?;
        Ok(self
            .tasks
            .record_git(task_id, "git.pushed", json!({ "branch": branch }))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The first request cannot hold and then fail with a 404 through the service, so the cell is
    // set by hand; it is the same path a waiting request takes.
    #[test]
    fn a_waiting_draft_gets_the_same_404() {
        let cell = Arc::new(DraftCell::default());
        let waiting = {
            let cell = Arc::clone(&cell);
            std::thread::spawn(move || waited(&cell))
        };
        cell.set(Err((404, "not_found", "T-1 does not exist".to_string())));
        let error = waiting.join().unwrap().unwrap_err();
        assert_eq!(error.status_and_code(), (404, "not_found"));
        assert_eq!(error.to_string(), "T-1 does not exist");
    }
}
