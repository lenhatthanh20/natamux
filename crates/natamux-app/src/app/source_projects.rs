//! The project and GitHub issue calls of screen 7 (Projects dialog, Import issues) and repo trust,
//! from the daemon or the fake.

use natamux_client::Answer;
use natamux_proto::projects::issues::GitHubIssue;
use natamux_proto::projects::repo::{AddRepo, Repo};
use natamux_proto::projects::repo_trust::RepoTrustView;
use natamux_proto::tasks::task::Task;

use super::Source;

impl Source {
    /// `POST /repos`. The projects list changes through the feed's `repos.changed`.
    pub fn add_repo(&self, input: &AddRepo) -> Answer<Repo> {
        self.ask(|c| c.add_repo(input), |f| f.add_repo(input))
    }

    /// `GET /repos/:id/trust`: the commands the project runs, and whether they are approved.
    pub fn repo_trust(&self, repo_id: &str) -> Answer<RepoTrustView> {
        self.ask(|c| c.repo_trust(repo_id), |f| f.repo_trust(repo_id))
    }

    /// `POST /repos/:id/trust` with the hash the dialog showed; 409 when the commands changed.
    pub fn approve_repo(&self, repo_id: &str, hash: &str) -> Answer<RepoTrustView> {
        self.ask(
            |c| c.approve_repo(repo_id, hash),
            |f| f.approve_repo(repo_id, hash),
        )
    }

    /// `GET /github/issues`. None: the daemon's default project.
    pub fn list_issues(&self, repo: Option<&str>) -> Answer<Vec<GitHubIssue>> {
        self.ask(|c| c.list_issues(repo), |f| f.list_issues(repo))
    }

    pub fn import_issues(&self, numbers: &[u64], repo: Option<&str>) -> Answer<Vec<Task>> {
        self.ask(
            |c| c.import_issues(numbers, repo),
            |f| f.import_issues(numbers, repo),
        )
    }
}
