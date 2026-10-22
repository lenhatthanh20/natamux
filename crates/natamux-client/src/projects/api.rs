//! The routes of `packages/daemon/src/http/routes/projects.ts` that the app calls.

use hyper::Method;
use natamux_proto::projects::repo::{BranchList, LoadedSettings, RepoList, SkillList};
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, send};

pub async fn list_repos(config: &Config) -> Result<RepoList, ClientError> {
    call(config, Method::GET, "/repos").await
}

/// Forgets a project. Its files and worktrees stay.
pub async fn remove_repo(config: &Config, repo_id: &str) -> Result<(), ClientError> {
    let path = format!("/repos/{}", encode(repo_id));
    send(config, Method::DELETE, &path, None).await.map(|_| ())
}

/// The branches of a project. None: the daemon's default project.
pub async fn list_branches(config: &Config, repo: Option<&str>) -> Result<BranchList, ClientError> {
    let path = match repo {
        Some(repo) => format!("/git/branches?repo={}", encode(repo)),
        None => "/git/branches".to_string(),
    };
    call(config, Method::GET, &path).await
}

pub async fn load_settings(config: &Config, repo_id: &str) -> Result<LoadedSettings, ClientError> {
    let path = format!("/repos/{}/settings", encode(repo_id));
    call(config, Method::GET, &path).await
}

pub async fn list_skills(config: &Config, repo_id: &str) -> Result<SkillList, ClientError> {
    let path = format!("/repos/{}/skills", encode(repo_id));
    call(config, Method::GET, &path).await
}
