//! The routes of `packages/daemon/src/http/routes/stages.ts` that the app calls.

use hyper::Method;
use natamux_proto::stages::actions::{StageActionInput, StageFile, StageFileList, StageToggle};
use natamux_proto::tasks::task::{Task, TaskAnswer};
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with};

/// Approves, skips, sends back or starts a stage by hand.
pub async fn stage_action(
    config: &Config,
    task_id: &str,
    input: &StageActionInput,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/stage", encode(task_id));
    let answer: TaskAnswer = call_with(config, Method::POST, &path, input).await?;
    Ok(answer.task)
}

async fn toggle(
    config: &Config,
    task_id: &str,
    route: &str,
    stage: &str,
    on: bool,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/{route}", encode(task_id));
    let body = StageToggle {
        stage: stage.to_string(),
        on,
    };
    let answer: TaskAnswer = call_with(config, Method::PUT, &path, &body).await?;
    Ok(answer.task)
}

/// Whether the stage's next start asks the agent for a workflow.
pub async fn set_workflow(
    config: &Config,
    task_id: &str,
    stage: &str,
    on: bool,
) -> Result<Task, ClientError> {
    toggle(config, task_id, "workflow", stage, on).await
}

/// Whether the stage waits for CI.
pub async fn set_ci(
    config: &Config,
    task_id: &str,
    stage: &str,
    on: bool,
) -> Result<Task, ClientError> {
    toggle(config, task_id, "ci", stage, on).await
}

/// Marks a stage that is not reached yet to be skipped, or clears the mark.
pub async fn set_skip(
    config: &Config,
    task_id: &str,
    stage: &str,
    on: bool,
) -> Result<Task, ClientError> {
    toggle(config, task_id, "skip", stage, on).await
}

/// The files the stages of the task wrote.
pub async fn list_stage_files(
    config: &Config,
    task_id: &str,
) -> Result<Vec<StageFile>, ClientError> {
    let path = format!("/tasks/{}/files", encode(task_id));
    let list: StageFileList = call(config, Method::GET, &path).await?;
    Ok(list.files)
}
