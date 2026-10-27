//! The review comment routes of the Changes tab (`/tasks/:id/review-comments`). A person leaves
//! comments on lines of the diff; the agent token may not touch them.

use std::sync::Arc;

use http_body_util::Full;
use hyper::StatusCode;
use hyper::body::Bytes;
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issue, Issues, Rule, ZodError, invalid_type, object, one_of, string};
use natamux_proto::workspaces::review_comments::{
    CommentSide, CommentState, MAX_COMMENT_BODY, NewReviewComment,
};
use serde_json::{Map, Value, json};

use crate::http::errors::{
    Answer, forbidden, internal_error, json_answer, stage_error, zod_answer,
};
use crate::http::server::{AppState, query_value};
use crate::shared::event_bus::DaemonMessage;
use crate::stages::errors::StageResult;
use crate::workspaces::review_comments as service;

const REFUSED: &str = "use review comments";

/// Runs the work on a blocking thread (files, git, the terminal). `None` answers with no body.
async fn run(
    status: StatusCode,
    work: impl FnOnce() -> StageResult<Option<Value>> + Send + 'static,
) -> Answer {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(Some(body))) => json_answer(status, &body),
        Ok(Ok(None)) => {
            let mut empty = Answer::new(Full::new(Bytes::new()));
            *empty.status_mut() = status;
            empty
        }
        Ok(Err(error)) => stage_error(&error),
        Err(_) => internal_error(),
    }
}

fn changed(state: &AppState, task_id: &str) {
    state.bus.publish(DaemonMessage::ReviewCommentsChanged {
        task_id: task_id.to_string(),
    });
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// A line number: a whole number from 0 up.
fn line(map: &Map<String, Value>, key: &str, issues: &mut Issues) -> Option<u32> {
    let value = map.get(key);
    let path = [json!(key)];
    match value {
        Some(Value::Number(number)) => {
            let line = number.as_u64().and_then(|n| u32::try_from(n).ok());
            if line.is_none() {
                issues.push(Issue::custom(&path, "Must be a whole number of 0 or more"));
            }
            line
        }
        _ => {
            issues.push(invalid_type("number", value, &path));
            None
        }
    }
}

fn comment_body(value: Option<&Value>, issues: &mut Issues) -> Option<String> {
    let rules = [Rule::Trim, Rule::Min(1), Rule::Max(MAX_COMMENT_BODY)];
    string(value, &[json!("body")], &rules, issues)
}

/// The body of `POST /tasks/:id/review-comments`.
fn new_comment(value: Option<&Value>) -> Result<NewReviewComment, ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        let path = string(
            map.get("path"),
            &[json!("path")],
            &[Rule::Min(1), Rule::Max(4096)],
            &mut issues,
        );
        let side = one_of(
            map.get("side"),
            &[json!("side")],
            &["old", "new"],
            &mut issues,
        );
        let start = line(map, "start", &mut issues);
        let end = line(map, "end", &mut issues);
        let body = comment_body(map.get("body"), &mut issues);
        if let (Some(path), Some(side), Some(start), Some(end), Some(body)) =
            (path, side, start, end, body)
        {
            parsed = CommentSide::parse(&side).map(|side| NewReviewComment {
                path,
                side,
                start,
                end,
                body,
            });
        }
    }
    issues.into_result(parsed)
}

type Patch = (Option<String>, Option<CommentState>);

/// The body of `PATCH`: a text, a state, or both.
fn patch_body(value: Option<&Value>) -> Result<Patch, ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        let body = map
            .get("body")
            .and_then(|body| comment_body(Some(body), &mut issues));
        let names = ["draft", "sent", "resolved"];
        let state = map.get("state").and_then(|state| {
            let name = one_of(Some(state), &[json!("state")], &names, &mut issues)?;
            CommentState::parse(&name)
        });
        if body.is_none() && state.is_none() && issues.is_empty() {
            issues.push(Issue::custom(&[], "Give a body or a state"));
        }
        parsed = Some((body, state));
    }
    issues.into_result(parsed)
}

/// Every comment of the task where its code is now; `?path=` keeps one file's.
pub async fn list(state: Arc<AppState>, id: String, query: String, by_agent: bool) -> Answer {
    if by_agent {
        return forbidden(REFUSED);
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let only = query_value(&query, "path").and_then(|path| path.as_str().map(str::to_string));
    run(StatusCode::OK, move || {
        let (comments, moved) = service::list(&state, &id, only.as_deref())?;
        if moved {
            changed(&state, &id);
        }
        Ok(Some(json!({ "comments": to_value(&comments) })))
    })
    .await
}

pub async fn add(state: Arc<AppState>, id: String, body: Option<Value>, by_agent: bool) -> Answer {
    if by_agent {
        return forbidden(REFUSED);
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let input = match new_comment(body.as_ref()) {
        Ok(input) => input,
        Err(error) => return zod_answer(&error),
    };
    run(StatusCode::CREATED, move || {
        let comment = service::add(&state, &id, &input)?;
        changed(&state, &id);
        Ok(Some(json!({ "comment": to_value(&comment) })))
    })
    .await
}

pub async fn update(
    state: Arc<AppState>,
    id: String,
    cid: String,
    body: Option<Value>,
    by_agent: bool,
) -> Answer {
    if by_agent {
        return forbidden(REFUSED);
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let (text, to) = match patch_body(body.as_ref()) {
        Ok(patch) => patch,
        Err(error) => return zod_answer(&error),
    };
    run(StatusCode::OK, move || {
        let comment = service::patch(&state, &id, &cid, text.as_deref(), to)?;
        changed(&state, &id);
        Ok(Some(json!({ "comment": to_value(&comment) })))
    })
    .await
}

pub async fn delete(state: Arc<AppState>, id: String, cid: String, by_agent: bool) -> Answer {
    if by_agent {
        return forbidden(REFUSED);
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    run(StatusCode::NO_CONTENT, move || {
        service::delete(&state, &id, &cid)?;
        changed(&state, &id);
        Ok(None)
    })
    .await
}

/// Every draft to the agent in one prompt.
pub async fn send(state: Arc<AppState>, id: String, by_agent: bool) -> Answer {
    if by_agent {
        return forbidden(REFUSED);
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    run(StatusCode::OK, move || {
        let (task, comments) = service::send(&state, &id)?;
        changed(&state, &id);
        Ok(Some(
            json!({ "task": to_value(&task), "comments": to_value(&comments) }),
        ))
    })
    .await
}
