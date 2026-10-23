//! The GitHub issue routes of `packages/daemon/src/http/routes/projects.ts`.

use hyper::Method;
use natamux_proto::projects::issues::{GitHubIssue, ImportIssues, ImportedTasks, IssueList};
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with};

/// The open issues of a project. None: the daemon's default project. A 409 says why `gh` failed.
pub async fn list_issues(
    config: &Config,
    repo: Option<&str>,
) -> Result<Vec<GitHubIssue>, ClientError> {
    let path = match repo {
        Some(repo) => format!("/github/issues?repo={}", encode(repo)),
        None => "/github/issues".to_string(),
    };
    let list: IssueList = call(config, Method::GET, &path).await?;
    Ok(list.issues)
}

pub async fn import_issues(
    config: &Config,
    numbers: &[u64],
    repo: Option<&str>,
) -> Result<Vec<Task>, ClientError> {
    let body = ImportIssues {
        numbers: numbers.to_vec(),
        repo: repo.map(str::to_string),
    };
    let done: ImportedTasks =
        call_with(config, Method::POST, "/github/issues/import", &body).await?;
    Ok(done.tasks)
}
