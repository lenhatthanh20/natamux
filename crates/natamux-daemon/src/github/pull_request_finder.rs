//! The part of the watcher that looks for pull requests and reads their status. Runs on the
//! worker thread, or on the thread of `check_all` and `check`.

use std::path::Path;

use natamux_git::gh::{
    PullRequestRef, find_pull_request, find_pull_requests, parse_pull_request_ref,
    read_pull_request_statuses,
};
use serde_json::json;

use super::{Inner, WatchError, candidate, is_watched, locked};

/// The watched tasks of one GitHub repository. `folder`: the Natamux repository of the first one.
struct RepoBatch {
    folder: Option<String>,
    repo: PullRequestRef,
    tasks: Vec<(String, u64)>,
}

/// The candidates of one project: the workspace gh runs in, and the (task, branch) pairs.
struct Candidates {
    repo: Option<String>,
    cwd: String,
    pairs: Vec<(String, String)>,
}

impl Inner {
    fn record_found(&self, task_id: &str, url: &str) -> Result<(), WatchError> {
        // A check may have linked it while gh ran.
        if self.options.tasks.get_task(task_id)?.pr.is_none() {
            let payload = json!({ "url": url, "detected": "true" });
            self.options
                .tasks
                .record_git(task_id, "pr.opened", payload)?;
            locked(&self.state).on_origin.remove(task_id);
        }
        Ok(())
    }

    /// Links the pull request from the task branch, when there is one. Asks GitHub only once
    /// origin has the branch.
    pub(super) fn find_one(&self, task_id: &str) -> Result<(), WatchError> {
        let task = (self.options.sync_branch)(task_id)?;
        let Some((workspace, branch)) = candidate(&task) else {
            return Ok(());
        };
        if !locked(&self.state).finding.insert(task_id.to_string()) {
            return Ok(());
        }
        let result = self.look(task_id, workspace, branch);
        locked(&self.state).finding.remove(task_id);
        result
    }

    fn look(&self, task_id: &str, workspace: &str, branch: &str) -> Result<(), WatchError> {
        let worktree = Path::new(workspace);
        if !(self.has_remote_branch)(worktree, branch).map_err(WatchError::Git)? {
            locked(&self.state).on_origin.remove(task_id);
            return Ok(());
        }
        locked(&self.state).on_origin.insert(task_id.to_string());
        match find_pull_request(worktree, branch, &self.options.gh)? {
            Some(url) => self.record_found(task_id, &url),
            None => Ok(()),
        }
    }

    /// Links the pull requests of all candidates, with one gh call per project: a call per task
    /// would use up the GitHub limit that gh shares with the user. Runs gh in the workspace of the
    /// first task of the project.
    pub(super) fn find_all(&self, failed: &mut bool) {
        let tasks = match self.options.tasks.list_tasks(None) {
            Ok(tasks) => tasks,
            Err(error) => return self.fail(failed, &error.into()),
        };
        let mut by_repo: Vec<Candidates> = Vec::new();
        for listed in &tasks {
            if candidate(listed).is_none() || locked(&self.state).finding.contains(&listed.id) {
                continue;
            }
            let task = match (self.options.sync_branch)(&listed.id) {
                Ok(task) => task,
                Err(error) => {
                    self.fail(failed, &error.into());
                    continue;
                }
            };
            let Some((workspace, branch)) = candidate(&task) else {
                continue;
            };
            match (self.has_remote_branch)(Path::new(workspace), branch) {
                Err(error) => self.fail(failed, &WatchError::Git(error)),
                Ok(false) => {
                    locked(&self.state).on_origin.remove(&task.id);
                }
                Ok(true) => {
                    locked(&self.state).on_origin.insert(task.id.clone());
                    let pair = (task.id.clone(), branch.to_string());
                    match by_repo.iter_mut().find(|group| group.repo == task.repo) {
                        Some(group) => group.pairs.push(pair),
                        None => by_repo.push(Candidates {
                            repo: task.repo.clone(),
                            cwd: workspace.to_string(),
                            pairs: vec![pair],
                        }),
                    }
                }
            }
        }
        for group in &by_repo {
            if let Err(error) = self.find_batch(&group.cwd, &group.pairs) {
                self.fail(failed, &error);
            }
        }
    }

    fn find_batch(&self, cwd: &str, pairs: &[(String, String)]) -> Result<(), WatchError> {
        let urls = find_pull_requests(Path::new(cwd), &self.options.gh)?;
        for (task_id, branch) in pairs {
            if let Some(url) = urls.get(branch) {
                self.record_found(task_id, url)?;
            }
        }
        Ok(())
    }

    /// Reads the status of every watched task, with one gh call per GitHub repository.
    pub(super) fn read_all(&self, failed: &mut bool) {
        let tasks = match self.options.tasks.list_tasks(None) {
            Ok(tasks) => tasks,
            Err(error) => return self.fail(failed, &error.into()),
        };
        let mut batches: Vec<RepoBatch> = Vec::new();
        for task in tasks.iter().filter(|task| is_watched(task)) {
            let Some(repo) = parse_pull_request_ref(task.pr.as_deref().unwrap_or("")) else {
                // Not an address gh gives; it is asked about alone.
                if let Err(error) = self.check(&task.id) {
                    self.fail(failed, &error);
                }
                continue;
            };
            let entry = (task.id.clone(), repo.number);
            let same = |batch: &RepoBatch| {
                (&batch.repo.host, &batch.repo.owner, &batch.repo.name)
                    == (&repo.host, &repo.owner, &repo.name)
            };
            match batches.iter_mut().find(|batch| same(batch)) {
                Some(batch) => batch.tasks.push(entry),
                None => batches.push(RepoBatch {
                    folder: task.repo.clone(),
                    repo: repo.clone(),
                    tasks: vec![entry],
                }),
            }
        }
        for batch in &batches {
            if let Err(error) = self.read_batch(batch) {
                self.fail(failed, &error);
            }
        }
    }

    fn read_batch(&self, batch: &RepoBatch) -> Result<(), WatchError> {
        let numbers: Vec<u64> = batch.tasks.iter().map(|(_, number)| *number).collect();
        let folder = (self.options.cwd_of)(batch.folder.as_deref());
        let statuses = read_pull_request_statuses(
            Path::new(&folder),
            &batch.repo.repo(),
            &numbers,
            &self.options.gh,
        )?;
        for (task_id, number) in &batch.tasks {
            if let Some(status) = statuses.get(number) {
                self.options
                    .tasks
                    .record_pull_request_status(task_id, status.clone())?;
            }
        }
        Ok(())
    }
}
