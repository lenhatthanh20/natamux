//! The task routes (`routes/tasks.ts`). The store does file and SQLite work, so each call runs on
//! a blocking thread.

use std::sync::Arc;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderValue,
};
use hyper::{Response, StatusCode};
use natamux_core::events::derive::enters_branch;
use natamux_core::events::schema::GATE_EVENT_TYPES;
use natamux_core::stages::position::StagePosition;
use natamux_core::tasks::attachments::{base64_length, preview_type_of};
use natamux_core::tasks::inputs::{
    apply_event_input, attachment_params, attachments_body, issue_link_body, list_query,
    task_id_param, task_repo_body, template_id_param,
};
use natamux_proto::tasks::attachments::MAX_TASK_ATTACHMENT_SIZE;
use natamux_proto::tasks::task::TaskState;
use natamux_store::attachments::{list_attachments, read_attachment};
use serde_json::{Value, json};

use crate::http::errors::{
    Answer, error_answer, forbidden, internal_error, json_answer, task_error, zod_answer,
};
use crate::http::server::AppState;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::helpers::position_of;
use crate::tasks::store::Store;
use crate::tasks::upload::decode_attachments;

/// A new task may bring its attachments in its JSON body, as base64.
pub fn new_task_body_limit() -> usize {
    (base64_length(MAX_TASK_ATTACHMENT_SIZE) + 1_000_000) as usize
}

/// Runs store work on a blocking thread and answers with its JSON, or its error.
pub async fn answer(
    status: StatusCode,
    work: impl FnOnce() -> TaskResult<Value> + Send + 'static,
) -> Answer {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(body)) => json_answer(status, &body),
        Ok(Err(error)) => task_error(&error),
        Err(_) => internal_error(),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub async fn list_tasks(state: Arc<AppState>, state_query: Option<Value>) -> Answer {
    let wanted = match list_query(state_query.as_ref()) {
        Ok(wanted) => wanted,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        let filter = wanted.and_then(|name| serde_json::from_value::<TaskState>(json!(name)).ok());
        Ok(json!({ "tasks": to_value(&state.tasks.list_tasks(filter)?) }))
    })
    .await
}

pub async fn create_task(state: Arc<AppState>, body: Option<Value>) -> Answer {
    answer(StatusCode::CREATED, move || {
        let object = body.clone().unwrap_or_else(|| json!({}));
        let repo = task_repo_body(&object)?;
        let uploads = attachments_body(&object)?;
        state.repos.get(repo.as_deref())?;
        let files = decode_attachments(&uploads)?;
        let task = state.tasks.create_task(body.as_ref(), &files, true)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

pub async fn get_task(state: Arc<AppState>, id: String) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        Ok(json!({ "task": to_value(&state.tasks.get_task(&id)?) }))
    })
    .await
}

/// The task and everything stored with it. A task with a workspace is refused until workspaces
/// come (part 5): Node removes the worktree first.
pub async fn delete_task(state: Arc<AppState>, id: String) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        let tasks = state.tasks.delete_task(&id)?;
        Ok(json!({ "id": id, "tasks": to_value(&tasks) }))
    })
    .await
}

pub async fn update_task(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        Ok(json!({ "task": to_value(&state.tasks.update_task(&id, body.as_ref())?) }))
    })
    .await
}

pub async fn list_events(state: Arc<AppState>, id: String) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        Ok(json!({ "events": to_value(&state.tasks.list_events(&id)?) }))
    })
    .await
}

/// An event from a person or an agent. The agent token may not pass a gate, and what it sends is
/// the agent's whatever source it names.
pub async fn post_event(
    state: Arc<AppState>,
    id: String,
    body: Option<Value>,
    by_agent: bool,
) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let mut input = match apply_event_input(body.as_ref()) {
        Ok(input) => input,
        Err(error) => return zod_answer(&error),
    };
    if by_agent {
        let kind = input.get("type").and_then(Value::as_str).unwrap_or("");
        if GATE_EVENT_TYPES.contains(&kind) {
            return forbidden(&format!("send {kind}"));
        }
        input.insert("source".into(), json!("agent"));
    }
    answer(StatusCode::OK, move || {
        state.tasks.with(|store| {
            let event = with_path(store, &id, Value::Object(input))?;
            let applied = store.apply_event(&id, Some(&event))?;
            Ok(json!({ "task": to_value(&applied.task), "accepted": applied.accepted, "moved": applied.moved }))
        })
    })
    .await
}

/// `withPath` without the stage engine: only the daemon picks a path, so the payload's `path` and
/// `because` go. An event that would enter a branch needs its conditions checked, which part 3
/// brings; until then it is refused.
fn with_path(store: &mut Store, id: &str, mut input: Value) -> TaskResult<Value> {
    let task = store.get_task(id)?;
    let steps = store.steps_of(&task);
    let mut probe = input.clone();
    if let Some(map) = probe.as_object_mut() {
        map.insert("taskId".into(), json!(id));
        map.insert("at".into(), json!(store.timestamp()));
    }
    let event = natamux_core::events::schema::validate_task_event(&probe)?;
    let position: StagePosition = position_of(&task);
    let event = natamux_core::events::apply::complete_event(&position, event, &steps, &Vec::new());
    if enters_branch(&position, &event, &steps).is_some() {
        return Err(TaskError::Conflict(format!(
            "{id} is at a branch; its conditions need the stage engine (phase 4, part 3)"
        )));
    }
    if let Some(payload) = input.get_mut("payload").and_then(Value::as_object_mut) {
        payload.shift_remove("path");
        payload.shift_remove("because");
    }
    Ok(input)
}

pub async fn list_templates(state: Arc<AppState>) -> Answer {
    answer(StatusCode::OK, move || {
        Ok(json!({ "templates": to_value(&state.templates.list()?) }))
    })
    .await
}

pub async fn save_template(state: Arc<AppState>, body: Option<Value>) -> Answer {
    answer(StatusCode::CREATED, move || {
        Ok(json!({ "template": to_value(&state.templates.save(body.as_ref())?) }))
    })
    .await
}

pub async fn task_from_template(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    answer(StatusCode::CREATED, move || {
        let id = template_id_param(&id)?;
        let repo = task_repo_body(body.as_ref().unwrap_or(&json!({})))?;
        state.repos.get(repo.as_deref())?;
        Ok(json!({ "task": to_value(&state.templates.create_task(&id, body.as_ref())?) }))
    })
    .await
}

pub fn list_jobs(state: &AppState) -> Answer {
    json_answer(
        StatusCode::OK,
        &json!({ "jobs": to_value(&state.jobs.list()) }),
    )
}

pub async fn list_task_attachments(state: Arc<AppState>, id: String) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        state.tasks.get_task(&id)?;
        let attachments = list_attachments(&state.data_dir, &id)?;
        Ok(json!({ "attachments": to_value(&attachments) }))
    })
    .await
}

/// An image the dashboard shows comes with its type. Any other file, SVG and HTML too, only
/// downloads, so it never runs as a page of the dashboard.
pub async fn get_attachment(state: Arc<AppState>, id: String, name: String) -> Answer {
    let work = move || -> TaskResult<Result<(String, Vec<u8>), Answer>> {
        let (id, name) = attachment_params(&id, &name)?;
        state.tasks.get_task(&id)?;
        let bytes = read_attachment(&state.data_dir, &id, &name)
            .map_err(|error| TaskError::Attachment(error.to_string()))??;
        Ok(match bytes {
            Some(bytes) => Ok((name, bytes)),
            None => Err(error_answer(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("{id} has no {name}"),
            )),
        })
    };
    let (name, bytes) = match tokio::task::spawn_blocking(work).await {
        Ok(Ok(Ok(found))) => found,
        Ok(Ok(Err(missing))) => return missing,
        Ok(Err(error)) => return task_error(&error),
        Err(_) => return internal_error(),
    };
    let mut response = Response::new(Full::new(Bytes::from(bytes)));
    let headers = response.headers_mut();
    match preview_type_of(&name) {
        Some(kind) => {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind));
        }
        None => {
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
                headers.insert(CONTENT_DISPOSITION, value);
            }
        }
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
    );
    *response.status_mut() = StatusCode::OK;
    response
}

pub async fn link_issue(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        let link = issue_link_body(body.as_ref())?;
        Ok(json!({ "task": to_value(&state.tasks.link_issue(&id, &link, "human")?) }))
    })
    .await
}
