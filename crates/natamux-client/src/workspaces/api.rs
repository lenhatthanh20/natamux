//! The routes of `packages/daemon/src/http/routes/workspaces.ts` that the app calls.

use std::collections::BTreeMap;

use hyper::Method;
use natamux_proto::tasks::task::{Task, TaskAnswer};
use natamux_proto::terminals::query::encode;
use natamux_proto::workspaces::changes::{
    CommitAnswer, CommitInput, CommitMessageDraft, FileDiff, ImageSide, RollbackInput,
    RollbackResult,
};
use natamux_proto::workspaces::files::{FileText, SavedFile, TreeEntry, TreeList, WorkspaceFile};
use natamux_proto::workspaces::workspace::{
    ChangeEntry, ChangeList, CreateWorkspaceInput, DiffStat, DiffStatList, FanOutAnswer,
    FanOutInput, GitGraph, NewWorkspaceInput, PreparingList, PreparingStep, RemoveWorkspaceAnswer,
};

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with, send, send_with_headers};

/// A task, its worktree and its agent in one step. The answer comes before the worktree is made;
/// `workspace.preparing` tells when it is.
pub async fn new_workspace(
    config: &Config,
    input: &NewWorkspaceInput,
) -> Result<Task, ClientError> {
    let answer: TaskAnswer = call_with(config, Method::POST, "/workspaces", input).await?;
    Ok(answer.task)
}

/// The worktree of a task that has none, from `base` or the repo's default branch.
pub async fn create_workspace(
    config: &Config,
    task_id: &str,
    base: Option<&str>,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/workspace", encode(task_id));
    let answer: TaskAnswer = match base {
        Some(base) => {
            let input = CreateWorkspaceInput {
                base: Some(base.to_string()),
                branch: None,
            };
            call_with(config, Method::POST, &path, &input).await?
        }
        // The daemon accepts no body, and refuses an empty JSON one.
        None => call(config, Method::POST, &path).await?,
    };
    Ok(answer.task)
}

/// Pushes the task's branch. Git work can take long: this call has no time limit.
pub async fn push(config: &Config, task_id: &str) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/push", encode(task_id));
    let answer: TaskAnswer = call(config, Method::POST, &path).await?;
    Ok(answer.task)
}

/// Opens a pull request for the task's branch.
pub async fn open_pull_request(config: &Config, task_id: &str) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/pull-request", encode(task_id));
    let answer: TaskAnswer = call(config, Method::POST, &path).await?;
    Ok(answer.task)
}

pub async fn remove_workspace(
    config: &Config,
    task_id: &str,
    delete_branch: bool,
    force: bool,
) -> Result<RemoveWorkspaceAnswer, ClientError> {
    let force = if force { "&force=true" } else { "" };
    let path = format!(
        "/tasks/{}/workspace?deleteBranch={delete_branch}{force}",
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

/// `?fresh=1` makes the daemon read git again instead of using its short cache.
fn fresh_query(fresh: bool) -> &'static str {
    if fresh { "?fresh=1" } else { "" }
}

/// The files that are not committed.
pub async fn list_changes(
    config: &Config,
    task_id: &str,
    fresh: bool,
) -> Result<Vec<ChangeEntry>, ClientError> {
    let path = format!("/tasks/{}/changes{}", encode(task_id), fresh_query(fresh));
    let list: ChangeList = call(config, Method::GET, &path).await?;
    Ok(list.changes)
}

pub async fn graph(config: &Config, task_id: &str, fresh: bool) -> Result<GitGraph, ClientError> {
    let path = format!("/tasks/{}/graph{}", encode(task_id), fresh_query(fresh));
    call(config, Method::GET, &path).await
}

/// The two texts of one changed file.
pub async fn file_diff(
    config: &Config,
    task_id: &str,
    file: &str,
) -> Result<FileDiff, ClientError> {
    let path = format!(
        "/tasks/{}/changes/file?path={}",
        encode(task_id),
        encode(file)
    );
    call(config, Method::GET, &path).await
}

/// The raw bytes of one side of a changed image. 413: too large, 404: that side does not exist.
pub async fn file_image(
    config: &Config,
    task_id: &str,
    file: &str,
    side: ImageSide,
) -> Result<Vec<u8>, ClientError> {
    let path = format!(
        "/tasks/{}/changes/image?path={}&side={}",
        encode(task_id),
        encode(file),
        side.name()
    );
    Ok(send(config, Method::GET, &path, None).await?.to_vec())
}

/// Commits every change. Git work can take long: this call has no time limit.
pub async fn commit(
    config: &Config,
    task_id: &str,
    input: &CommitInput,
) -> Result<CommitAnswer, ClientError> {
    let path = format!("/tasks/{}/commit", encode(task_id));
    call_with(config, Method::POST, &path, input).await
}

/// Asks for a commit message. The daemon waits up to 60 s for the agent, so this is a write call
/// (no client limit).
pub async fn commit_message(
    config: &Config,
    task_id: &str,
) -> Result<CommitMessageDraft, ClientError> {
    let path = format!("/tasks/{}/commit-message", encode(task_id));
    call(config, Method::POST, &path).await
}

pub async fn rollback(
    config: &Config,
    task_id: &str,
    input: &RollbackInput,
) -> Result<RollbackResult, ClientError> {
    let path = format!("/tasks/{}/rollback", encode(task_id));
    call_with(config, Method::POST, &path, input).await
}

/// Puts back what the last rollback of the task took.
pub async fn undo_rollback(config: &Config, task_id: &str) -> Result<RollbackResult, ClientError> {
    let path = format!("/tasks/{}/rollback/undo", encode(task_id));
    call(config, Method::POST, &path).await
}

/// Makes `input.count` copies of the task, to let several agents try the same work.
pub async fn fan_out(
    config: &Config,
    task_id: &str,
    input: &FanOutInput,
) -> Result<Vec<Task>, ClientError> {
    let path = format!("/tasks/{}/fan-out", encode(task_id));
    let answer: FanOutAnswer = call_with(config, Method::POST, &path, input).await?;
    Ok(answer.tasks)
}

/// Asks GitHub for the pull request's state and checks now.
pub async fn refresh_pull_request(config: &Config, task_id: &str) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/pull-request/refresh", encode(task_id));
    let answer: TaskAnswer = call(config, Method::POST, &path).await?;
    Ok(answer.task)
}

/// One folder of the worktree; the root is the empty path.
pub async fn list_folder(
    config: &Config,
    task_id: &str,
    folder: &str,
) -> Result<Vec<TreeEntry>, ClientError> {
    let path = format!("/tasks/{}/tree?path={}", encode(task_id), encode(folder));
    let list: TreeList = call(config, Method::GET, &path).await?;
    Ok(list.entries)
}

/// A file of the worktree. 409: the path leaves the worktree (`OutsideWorkspaceError`), 404: no
/// such file.
pub async fn read_workspace_file(
    config: &Config,
    task_id: &str,
    file: &str,
) -> Result<WorkspaceFile, ClientError> {
    let path = format!("/tasks/{}/file?path={}", encode(task_id), encode(file));
    call(config, Method::GET, &path).await
}

/// Saves a file. `version`: the one `read_workspace_file` gave; 409 when the file changed since.
pub async fn write_workspace_file(
    config: &Config,
    task_id: &str,
    file: &str,
    text: &str,
    version: &str,
) -> Result<SavedFile, ClientError> {
    let path = format!("/tasks/{}/file?path={}", encode(task_id), encode(file));
    let body = serde_json::to_vec(&FileText {
        text: text.to_string(),
    })
    .map_err(|error| ClientError::BadAnswer(error.to_string()))?;
    let answer = send_with_headers(
        config,
        Method::PUT,
        &path,
        &[("if-match", version)],
        Some(body),
    )
    .await?;
    serde_json::from_slice(&answer).map_err(|error| ClientError::BadAnswer(error.to_string()))
}
