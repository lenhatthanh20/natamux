//! What the agent token of a task may set when it makes a subtask or edits its own task (F6):
//! `dependsOn` only on its own task and direct subtasks, never another project or parent.

use std::sync::Arc;

use hyper::Method;
use serde_json::Value;

use crate::http::agent_actions::AgentAction;
use crate::http::agent_rules::denial_line;
use crate::http::errors::{Answer, forbidden, internal_error, task_error};
use crate::http::server::AppState;
use crate::tasks::errors::TaskResult;

/// The 403 for a body the agent token of `own` may not send. `edited`: the id of the task a
/// `PATCH` edits; None: a `POST /tasks`.
pub async fn refused(
    state: &Arc<AppState>,
    own: &str,
    body: Option<&Value>,
    edited: Option<&str>,
) -> Option<Answer> {
    let body = body.cloned().unwrap_or(Value::Null);
    let (state, own_id, editing) = (Arc::clone(state), own.to_string(), edited.is_some());
    let checked =
        tokio::task::spawn_blocking(move || why_refused(&state, &own_id, &body, editing)).await;
    let why = match checked {
        Ok(Ok(why)) => why?,
        Ok(Err(error)) => return Some(task_error(&error)),
        Err(_) => return Some(internal_error()),
    };
    let line = match edited {
        Some(id) => denial_line(
            Some(AgentAction::TaskUpdateOwn),
            &Method::PATCH,
            &["tasks", id],
            own,
        ),
        None => denial_line(
            Some(AgentAction::SubtaskCreate),
            &Method::POST,
            &["tasks"],
            own,
        ),
    };
    eprintln!("{line}");
    Some(forbidden(why))
}

fn why_refused(
    state: &AppState,
    own: &str,
    body: &Value,
    editing: bool,
) -> TaskResult<Option<&'static str>> {
    if editing && (body.get("repo").is_some() || body.get("parentId").is_some()) {
        return Ok(Some("move a task to another project or parent"));
    }
    let tasks = state.tasks.list_tasks(None)?;
    let reachable = |id: &str| {
        id == own
            || tasks
                .iter()
                .any(|task| task.id == id && task.parent_id.as_deref() == Some(own))
    };
    let depends = body.get("dependsOn").and_then(Value::as_array);
    if depends.is_some_and(|ids| ids.iter().any(|id| !id.as_str().is_some_and(&reachable))) {
        return Ok(Some(
            "make a task wait on a task outside this one and its subtasks",
        ));
    }
    if !editing && let Some(repo) = body.get("repo").filter(|repo| !repo.is_null()) {
        let own_repo = state.tasks.get_task(own)?.repo;
        let mine = state.repos.get(own_repo.as_deref())?.id;
        if repo.as_str() != Some(mine.as_str()) {
            return Ok(Some("make a subtask in another project"));
        }
    }
    Ok(None)
}
