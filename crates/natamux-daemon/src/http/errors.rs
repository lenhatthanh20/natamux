//! Error answers in the shapes of the Node daemon (`errors.ts`, Fastify's 404).

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};
use serde_json::{Value, json};

use crate::terminals::types::TerminalError;

pub type Answer = Response<Full<Bytes>>;

pub fn json_answer(status: StatusCode, body: &Value) -> Answer {
    let mut answer = Response::new(Full::new(Bytes::from(body.to_string())));
    *answer.status_mut() = status;
    answer.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    answer
}

pub fn unauthorized() -> Answer {
    json_answer(
        StatusCode::UNAUTHORIZED,
        &json!({ "error": "unauthorized" }),
    )
}

/// Fastify's answer for an address no route serves.
pub fn not_found(method: &str, url: &str) -> Answer {
    json_answer(
        StatusCode::NOT_FOUND,
        &json!({
            "message": format!("Route {method}:{url} not found"),
            "error": "Not Found",
            "statusCode": 404,
        }),
    )
}

/// One rule of a zod schema that a value broke.
pub struct Issue {
    pub path: &'static str,
    pub message: &'static str,
    pub detail: Value,
}

/// A request that breaks the schema: zod's issues, and one line a client can show.
pub fn invalid(issues: &[Issue]) -> Answer {
    let message = issues
        .iter()
        .map(|issue| format!("{}: {}", issue.path, issue.message))
        .collect::<Vec<_>>()
        .join("; ");
    let issues: Vec<Value> = issues
        .iter()
        .map(|issue| {
            let mut detail = issue.detail.clone();
            if let Some(object) = detail.as_object_mut() {
                object.insert("path".into(), json!([issue.path]));
                object.insert("message".into(), json!(issue.message));
            }
            detail
        })
        .collect();
    json_answer(
        StatusCode::BAD_REQUEST,
        &json!({ "error": "invalid_request", "issues": issues, "message": message }),
    )
}

pub fn terminal_error(error: &TerminalError) -> Answer {
    let (status, code) = match error {
        TerminalError::NoWorkspace(_) | TerminalError::NoTerminal(_) | TerminalError::Limit(_) => {
            (StatusCode::CONFLICT, "conflict")
        }
        TerminalError::ShellGone { .. } | TerminalError::Spawn(_) => {
            return json_answer(
                StatusCode::INTERNAL_SERVER_ERROR,
                &json!({ "error": "internal_error" }),
            );
        }
    };
    json_answer(
        status,
        &json!({ "error": code, "message": error.to_string() }),
    )
}
