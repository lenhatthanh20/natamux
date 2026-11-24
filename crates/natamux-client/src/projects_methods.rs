//! The `Client` calls of the Projects dialog and Import issues.

use natamux_proto::projects::issues::GitHubIssue;
use natamux_proto::projects::repo::{AddRepo, Repo};
use natamux_proto::projects::repo_trust::RepoTrustView;
use natamux_proto::tasks::task::Task;

use crate::{Answer, Client, projects};

impl Client {
    /// A 400 or 409 carries the daemon's reason (not a folder, added already, not a git repo).
    pub fn add_repo(&self, input: &AddRepo) -> Answer<Repo> {
        let input = input.clone();
        self.with_config(
            move |config| async move { projects::api::add_repo(&config, &input).await },
        )
    }

    /// None: the default project.
    pub fn list_issues(&self, repo: Option<&str>) -> Answer<Vec<GitHubIssue>> {
        let repo = repo.map(str::to_string);
        self.with_config(move |config| async move {
            projects::github_api::list_issues(&config, repo.as_deref()).await
        })
    }

    pub fn import_issues(&self, numbers: &[u64], repo: Option<&str>) -> Answer<Vec<Task>> {
        let (numbers, repo) = (numbers.to_vec(), repo.map(str::to_string));
        self.with_config(move |config| async move {
            projects::github_api::import_issues(&config, &numbers, repo.as_deref()).await
        })
    }
}

impl Client {
    /// The project's commands for the approval dialog. `task`: read in its worktree.
    pub fn repo_trust(&self, repo_id: &str, task: Option<&str>) -> Answer<RepoTrustView> {
        let (repo_id, task) = (repo_id.to_string(), task.map(str::to_string));
        self.with_config(move |config| async move {
            projects::api::repo_trust(&config, &repo_id, task.as_deref()).await
        })
    }

    /// `hash`: the one the dialog showed. A 409 means the commands changed since.
    pub fn approve_repo(
        &self,
        repo_id: &str,
        hash: &str,
        task: Option<&str>,
    ) -> Answer<RepoTrustView> {
        let (repo_id, hash) = (repo_id.to_string(), hash.to_string());
        let task = task.map(str::to_string);
        self.with_config(move |config| async move {
            projects::api::approve_repo(&config, &repo_id, &hash, task.as_deref()).await
        })
    }
}
