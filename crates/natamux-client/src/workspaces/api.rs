//! The routes of `packages/daemon/src/http/routes/workspaces.ts` that the app calls.

use std::collections::BTreeMap;

use hyper::Method;
use natamux_proto::tasks::task::{Task, TaskAnswer};
use natamux_proto::terminals::query::encode;
use natamux_proto::workspaces::workspace::{
    ChangeEntry, ChangeList, DiffStat, DiffStatList, GitGraph, NewWorkspaceInput, PreparingList,
    PreparingStep, RemoveWorkspaceAnswer,
};

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with};

/// A task, its worktree and its agent in one step. The answer comes before the worktree is made;
/// `workspace.preparing` tells when it is.
pub async fn new_workspace(
    config: &Config,
    input: &NewWorkspaceInput,
) -> Result<Task, ClientError> {
    let answer: TaskAnswer = call_with(config, Method::POST, "/workspaces", input).await?;
    Ok(answer.task)
}

pub async fn remove_workspace(
    config: &Config,
    task_id: &str,
    delete_branch: bool,
) -> Result<RemoveWorkspaceAnswer, ClientError> {
    let path = format!(
        "/tasks/{}/workspace?deleteBranch={delete_branch}",
        encode(task_id)
    );
    call(config, Method::DELETE, &path).await
}

/// The workspaces being made, by task id, and the step each is at.
pub async fn list_preparing(
    config: &Config,
) -> Result<BTreeMap<String, PreparingStep>, ClientError> {
    let list: PreparingList = call(config, Method::GET, "/workspaces/preparing").await?;
    Ok(list.preparing)
}

/// The size of every workspace's change, by task id.
pub async fn diff_stats(config: &Config) -> Result<BTreeMap<String, DiffStat>, ClientError> {
    let list: DiffStatList = call(config, Method::GET, "/diffstats").await?;
    Ok(list.stats)
}

/// The files that are not committed.
pub async fn list_changes(config: &Config, task_id: &str) -> Result<Vec<ChangeEntry>, ClientError> {
    let path = format!("/tasks/{}/changes", encode(task_id));
    let list: ChangeList = call(config, Method::GET, &path).await?;
    Ok(list.changes)
}

pub async fn graph(config: &Config, task_id: &str) -> Result<GitGraph, ClientError> {
    let path = format!("/tasks/{}/graph", encode(task_id));
    call(config, Method::GET, &path).await
}
