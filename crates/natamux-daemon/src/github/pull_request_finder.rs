//! The part of the watcher that looks for pull requests and reads their status. Runs on the
//! worker thread, or on the thread of `check_all` and `check`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use natamux_git::gh::{
    PullRequestRef, find_pull_request, find_pull_requests, parse_pull_request_ref,
    read_pull_request_statuses,
};
use serde_json::json;

use super::{Inner, WatchError, candidate, is_watched, locked};
use crate::tasks::helpers::compare_text;

/// The watched tasks of one GitHub repository. `folder`: the Natamux repository of the first one.
struct RepoBatch {
    folder: Option<String>,
    repo: PullRequestRef,
    tasks: Vec<(String, u64)>,
}

/// The fields of a watched task that `read_all` needs.
struct Watched {
    updated_at: String,
    id: String,
    pr: String,
    repo: Option<String>,
}

/// The order of `list_tasks`: the first task of a group picks where gh runs.
fn newest_first<T>(items: &mut [T], key: impl Fn(&T) -> (&str, &str)) {
    items.sort_by(|a, b| {
        let ((a_at, a_id), (b_at, b_id)) = (key(a), key(b));
        compare_text(b_at, a_at).then_with(|| compare_text(a_id, b_id))
    });
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
        let on_origin = (self.remote_branches)(worktree).map_err(WatchError::Git)?;
        if !on_origin.contains(branch) {
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
        let scanned = self
            .options
            .tasks
            .scan(|task| candidate(task).map(|_| (task.updated_at.clone(), task.id.clone())));
        let mut ids = match scanned {
            Ok(ids) => ids,
            Err(error) => return self.fail(failed, &error.into()),
        };
        newest_first(&mut ids, |(updated_at, id)| {
            (updated_at.as_str(), id.as_str())
        });
        // The branches of origin, by project: its worktrees share the refs, so git runs once.
        let mut remote: HashMap<String, Result<HashSet<String>, String>> = HashMap::new();
        let mut by_repo: Vec<Candidates> = Vec::new();
        for (_, id) in &ids {
            if locked(&self.state).finding.contains(id) {
                continue;
            }
            let task = match (self.options.sync_branch)(id) {
                Ok(task) => task,
                Err(error) => {
                    self.fail(failed, &error.into());
                    continue;
                }
            };
            let Some((workspace, branch)) = candidate(&task) else {
                continue;
            };
            // A task without a project has its own refs, for all this code knows.
            let key = task.repo.clone().unwrap_or_else(|| workspace.to_string());
            let on_origin = remote
                .entry(key)
                .or_insert_with(|| (self.remote_branches)(Path::new(workspace)))
                .as_ref()
                .map(|branches| branches.contains(branch));
            match on_origin {
                Err(error) => self.fail(failed, &WatchError::Git(error.clone())),
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
        let scanned = self.options.tasks.scan(|task| {
            is_watched(task).then(|| Watched {
                updated_at: task.updated_at.clone(),
                id: task.id.clone(),
                pr: task.pr.clone().unwrap_or_default(),
                repo: task.repo.clone(),
            })
        });
        let mut tasks = match scanned {
            Ok(tasks) => tasks,
            Err(error) => return self.fail(failed, &error.into()),
        };
        newest_first(&mut tasks, |task| {
            (task.updated_at.as_str(), task.id.as_str())
        });
        let mut batches: Vec<RepoBatch> = Vec::new();
        for task in &tasks {
            let Some(repo) = parse_pull_request_ref(&task.pr) else {
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
