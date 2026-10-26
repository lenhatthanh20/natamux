//! The routes of `packages/daemon/src/http/routes/projects.ts` that the app calls.

use hyper::Method;
use natamux_proto::projects::repo::{
    AddRepo, AddedRepo, BranchList, LoadedSettings, RemoteBranchList, Repo, RepoList,
    SavedSettings, SkillList,
};
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with, send, send_with_headers};

pub async fn list_repos(config: &Config) -> Result<RepoList, ClientError> {
    call(config, Method::GET, "/repos").await
}

/// Adds a project. The daemon answers 201, or 400 / 409 with its reason.
pub async fn add_repo(config: &Config, input: &AddRepo) -> Result<Repo, ClientError> {
    let added: AddedRepo = call_with(config, Method::POST, "/repos", input).await?;
    Ok(added.repo)
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

/// The branches of origin of a project. The daemon asks origin, so it can take seconds.
pub async fn list_remote_branches(
    config: &Config,
    repo: &str,
) -> Result<RemoteBranchList, ClientError> {
    let path = format!("/git/remote-branches?repo={}", encode(repo));
    call(config, Method::GET, &path).await
}

pub async fn load_settings(config: &Config, repo_id: &str) -> Result<LoadedSettings, ClientError> {
    let path = format!("/repos/{}/settings", encode(repo_id));
    call(config, Method::GET, &path).await
}

/// Saves the settings. `raw` is the file as a JSON object, its key order kept. `version` is the
/// one `load_settings` gave: the daemon answers 409 when the file changed since. Without one the
/// file is written whatever is on disk (Overwrite).
pub async fn save_settings(
    config: &Config,
    repo_id: &str,
    raw: &serde_json::Map<String, serde_json::Value>,
    version: Option<&str>,
) -> Result<SavedSettings, ClientError> {
    let path = format!("/repos/{}/settings", encode(repo_id));
    let body =
        serde_json::to_vec(raw).map_err(|error| ClientError::BadAnswer(error.to_string()))?;
    let headers: Vec<(&str, &str)> = version
        .map(|version| ("if-match", version))
        .into_iter()
        .collect();
    let answer = send_with_headers(config, Method::PUT, &path, &headers, Some(body)).await?;
    serde_json::from_slice(&answer).map_err(|error| ClientError::BadAnswer(error.to_string()))
}

pub async fn list_skills(config: &Config, repo_id: &str) -> Result<SkillList, ClientError> {
    let path = format!("/repos/{}/skills", encode(repo_id));
    call(config, Method::GET, &path).await
}
