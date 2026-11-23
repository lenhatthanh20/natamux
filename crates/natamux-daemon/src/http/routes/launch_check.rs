//! `POST /tasks/:id/launch-check` with `{digest}`: the pre-exec check of a Strict launch script
//! (`natamux repo verify-launch`). 200 while the task's worktree still has the approved digest it
//! was started with, 409 after any change. An agent token reaches only its own task.

use std::path::Path;
use std::sync::Arc;

use hyper::StatusCode;
use serde_json::{Value, json};

use crate::http::errors::{Answer, error_answer, internal_error, json_answer, task_error};
use crate::http::server::AppState;
use crate::projects::repo_trust_worktree::TrustState;
use crate::tasks::errors::TaskError;

pub async fn launch_check(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let digest = body
        .as_ref()
        .and_then(|body| body.get("digest"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(digest) = digest else {
        let message = "Give the digest the launch was approved with";
        return error_answer(StatusCode::BAD_REQUEST, "invalid_request", message);
    };
    let checked = tokio::task::spawn_blocking(move || -> Result<TrustState, TaskError> {
        let task = state.tasks.get_task(&id)?;
        let workspace = task.workspace.as_deref().map(Path::new);
        let repos = &state.repos;
        Ok(repos.is_trusted_in_task(task.repo.as_deref(), workspace))
    })
    .await;
    match checked {
        Ok(Ok(trust)) if trust.is_trusted() && trust.digest() == Some(digest.as_str()) => {
            json_answer(StatusCode::OK, &json!({ "ok": true }))
        }
        Ok(Ok(trust)) => {
            let reason = match trust {
                TrustState::Trusted { .. } => {
                    natamux_proto::projects::repo_trust::CONFIG_CHANGED_REASON.to_string()
                }
                other => other.reason(),
            };
            error_answer(StatusCode::CONFLICT, "conflict", &reason)
        }
        Ok(Err(error)) => task_error(&error),
        Err(_) => internal_error(),
    }
}
