//! The GitHub routes (`routes/projects.ts` and `routes/tasks.ts`): list and import issues, and
//! open the issue of a task. `gh` blocks, so each call runs on a blocking thread.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::formats::REPO_ID;
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issues, Length, ZodError, array, object, string};
use serde_json::{Value, json};

use crate::http::errors::{
    Answer, internal_error, issue_import_error, json_answer, link_error, zod_answer,
};
use crate::http::routes::projects::repo_query;
use crate::http::routes::workspaces::{Least, integer};
use crate::http::server::AppState;

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub async fn list_issues(state: Arc<AppState>, query: &str) -> Answer {
    let repo = match repo_query(query) {
        Ok(repo) => repo,
        Err(error) => return zod_answer(&error),
    };
    match tokio::task::spawn_blocking(move || state.issues.list(repo.as_deref())).await {
        Ok(Ok(issues)) => json_answer(StatusCode::OK, &json!({ "issues": to_value(&issues) })),
        Ok(Err(error)) => issue_import_error(&error),
        Err(_) => internal_error(),
    }
}

/// `importIssuesBodySchema`: 1 to 100 issue numbers, and the project (none: the default one).
fn import_body(body: Option<&Value>) -> Result<(Vec<u64>, Option<String>), ZodError> {
    let mut issues = Issues::default();
    let parsed = object(body, &[], &mut issues).and_then(|map| {
        let length = Length {
            min: Some(1),
            max: Some(100),
        };
        let numbers = array(
            map.get("numbers"),
            &[json!("numbers")],
            length,
            &mut issues,
            |each, path, issues| integer(Some(each), path, Least::Above(0), None, issues),
        );
        let repo = match map.get("repo") {
            None => Some(None),
            Some(repo) => string(Some(repo), &[json!("repo")], &REPO_ID, &mut issues).map(Some),
        };
        numbers.zip(repo)
    });
    issues.into_result(parsed)
}

pub async fn import_issues(state: Arc<AppState>, body: Option<Value>) -> Answer {
    let (numbers, repo) = match import_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    let imported =
        tokio::task::spawn_blocking(move || state.issues.import(&numbers, repo.as_deref()));
    match imported.await {
        Ok(Ok(tasks)) => json_answer(StatusCode::OK, &json!({ "tasks": to_value(&tasks) })),
        Ok(Err(error)) => issue_import_error(&error),
        Err(_) => internal_error(),
    }
}

/// Opens a GitHub issue for the task now ("Create issue").
pub async fn create_issue(state: Arc<AppState>, id: String) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    match tokio::task::spawn_blocking(move || state.issue_linker.create(&id)).await {
        Ok(Ok(task)) => json_answer(StatusCode::CREATED, &json!({ "task": to_value(&task) })),
        Ok(Err(error)) => link_error(&error),
        Err(_) => internal_error(),
    }
}
