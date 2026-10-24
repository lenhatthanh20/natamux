//! Error answers in the shapes of the Node daemon (`errors.ts`, Fastify's 404).

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};
use serde_json::{Value, json};

use natamux_core::json::to_js_string;
use natamux_core::zod::ZodError;

use crate::agents::errors::{AgentError, HookError};
use crate::github::issue_importer::IssueImportError;
use crate::github::issue_linker::LinkError;
use crate::github::pull_request_watcher::WatchError;
use crate::stages::errors::StageError;
use crate::tasks::errors::TaskError;
use crate::terminals::types::TerminalError;
use crate::workspaces::errors::WorkspaceError;

pub type Answer = Response<Full<Bytes>>;

pub fn json_answer(status: StatusCode, body: &Value) -> Answer {
    let mut answer = Response::new(Full::new(Bytes::from(to_js_string(body))));
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
        TerminalError::TaskNotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
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

/// A body that breaks a zod schema: its issues, and one line a client can show.
pub fn zod_answer(error: &ZodError) -> Answer {
    json_answer(
        StatusCode::BAD_REQUEST,
        &json!({ "error": "invalid_request", "issues": error.issues(), "message": error.describe() }),
    )
}

/// `{ error: code, message }` with a status.
pub fn error_answer(status: StatusCode, code: &str, message: &str) -> Answer {
    json_answer(status, &json!({ "error": code, "message": message }))
}

/// A Fastify error with a 4xx status, as the error handler answers it.
pub fn bad_request(status: StatusCode, message: &str) -> Answer {
    error_answer(status, "bad_request", message)
}

/// The agent token may not do what only a person may (`AgentRefusedError`).
pub fn forbidden(what: &str) -> Answer {
    error_answer(
        StatusCode::FORBIDDEN,
        "forbidden",
        &format!("Only a person can {what}. Use the dashboard."),
    )
}

pub fn internal_error() -> Answer {
    json_answer(
        StatusCode::INTERNAL_SERVER_ERROR,
        &json!({ "error": "internal_error" }),
    )
}

/// The answer of each task error, as `ERROR_ANSWERS` in `errors.ts`.
pub fn task_error(error: &TaskError) -> Answer {
    let message = error.to_string();
    match error {
        TaskError::Invalid(issues) => zod_answer(issues),
        TaskError::Edit(_) | TaskError::Attachment(_) => {
            error_answer(StatusCode::BAD_REQUEST, "invalid_request", &message)
        }
        TaskError::NotFound(_) | TaskError::TemplateNotFound(_) | TaskError::RepoNotFound(_) => {
            error_answer(StatusCode::NOT_FOUND, "not_found", &message)
        }
        TaskError::FileChanged(_) | TaskError::Conflict(_) => {
            error_answer(StatusCode::CONFLICT, "conflict", &message)
        }
        TaskError::Failed(_) => {
            eprintln!("{message}");
            internal_error()
        }
    }
}

/// The answer of each stage engine error, as `ERROR_ANSWERS` in `errors.ts`.
pub fn stage_error(error: &StageError) -> Answer {
    let message = error.to_string();
    match error {
        StageError::Action(_) | StageError::Agent(AgentError::Waiting { .. }) => {
            error_answer(StatusCode::CONFLICT, "conflict", &message)
        }
        StageError::Input(_) | StageError::File(_) => {
            error_answer(StatusCode::BAD_REQUEST, "invalid_request", &message)
        }
        StageError::Agent(AgentError::Unknown(_)) => {
            error_answer(StatusCode::NOT_FOUND, "unknown_agent", &message)
        }
        StageError::Task(error) | StageError::Agent(AgentError::Task(error)) => task_error(error),
        StageError::Terminal(error) | StageError::Agent(AgentError::Terminal(error)) => {
            terminal_error(error)
        }
    }
}

/// The answer of each hook error (`UnknownAgentError` is 404 `unknown_agent`).
pub fn hook_error(error: &HookError) -> Answer {
    match error {
        HookError::UnknownAgent(_) => {
            error_answer(StatusCode::NOT_FOUND, "unknown_agent", &error.to_string())
        }
        HookError::Invalid(issues) => zod_answer(issues),
        HookError::Task(error) => task_error(error),
        HookError::Failed(message) => {
            eprintln!("{message}");
            internal_error()
        }
    }
}

/// The answer of each workspace error (`status_and_code` has Node's mapping). A task error keeps
/// its own answer, which carries zod's issues.
pub fn workspace_error(error: &WorkspaceError) -> Answer {
    if let WorkspaceError::Task(error) = error {
        return task_error(error);
    }
    let (status, code) = error.status_and_code();
    if status == 500 {
        eprintln!("{error}");
        return internal_error();
    }
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    error_answer(status, code, &error.to_string())
}

/// `GitHubError` and the issue errors are 409 `conflict`.
fn conflict(error: &impl std::fmt::Display) -> Answer {
    error_answer(StatusCode::CONFLICT, "conflict", &error.to_string())
}

pub fn issue_import_error(error: &IssueImportError) -> Answer {
    match error {
        IssueImportError::GitHub(_) => conflict(error),
        IssueImportError::Task(error) => task_error(error),
    }
}

pub fn link_error(error: &LinkError) -> Answer {
    match error {
        LinkError::Link(_) | LinkError::GitHub(_) => conflict(error),
        LinkError::Task(error) => task_error(error),
    }
}

pub fn watch_error(error: &WatchError) -> Answer {
    match error {
        WatchError::GitHub(_) | WatchError::Git(_) => conflict(error),
        WatchError::Task(error) => task_error(error),
    }
}
