//! The routes that read a workspace (`routes/workspaces.ts`): its changes, their diffs and images,
//! its graph, and the files explorer with its editor. Git and file reads run on blocking threads.

use std::sync::Arc;

use hyper::{HeaderMap, StatusCode};
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issues, Rule, ZodError, object, one_of, string};
use natamux_git::changes::change_json;
use natamux_proto::workspaces::changes::ImageSide;
use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit, PushStatus};
use serde_json::{Value, json};

use crate::http::body::sandboxed_bytes;
use crate::http::errors::{Answer, internal_error, workspace_error, zod_answer};
use crate::http::routes::workspaces::answer;
use crate::http::server::{AppState, query_value};
use crate::workspaces::errors::WorkspaceError;

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Runs one query schema: its value, or the 400 answer.
fn parsed<T>(read: impl FnOnce(&mut Issues) -> Option<T>) -> Result<T, Box<Answer>> {
    let mut issues = Issues::default();
    let value = read(&mut issues);
    issues
        .into_result(value)
        .map_err(|error| Box::new(zod_answer(&error)))
}

fn param(id: &str) -> Result<String, Box<Answer>> {
    task_id_param(id).map_err(|error| Box::new(zod_answer(&error)))
}

/// `changedFileQuerySchema`: the path of a file, 1 to 1024 characters.
fn path_query(query: &str, issues: &mut Issues) -> Option<String> {
    string(
        query_value(query, "path").as_ref(),
        &[json!("path")],
        &[Rule::Min(1), Rule::Max(1024)],
        issues,
    )
}

/// `freshQuerySchema`: `?fresh=1` reads git now.
fn fresh_query(query: &str, issues: &mut Issues) -> Option<bool> {
    match query_value(query, "fresh") {
        None => Some(false),
        Some(value) => one_of(Some(&value), &[json!("fresh")], &["1"], issues).map(|_| true),
    }
}

pub async fn changes(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let fresh = match parsed(|issues| fresh_query(query, issues)) {
        Ok(fresh) => fresh,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        let changes = state.workspaces.changes(&id, fresh)?;
        let list: Vec<Value> = changes
            .iter()
            .map(|entry| change_json(entry, true))
            .collect();
        Ok(json!({ "changes": list }))
    })
    .await
}

pub async fn changed_file(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let path = match parsed(|issues| path_query(query, issues)) {
        Ok(path) => path,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(to_value(&state.workspaces.file_diff(&id, &path)?))
    })
    .await
}

/// The bytes of one side of a changed image.
pub async fn changed_image(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let (path, side) = match parsed(|issues| {
        let path = path_query(query, issues);
        let side = one_of(
            query_value(query, "side").as_ref(),
            &[json!("side")],
            &["original", "modified"],
            issues,
        );
        path.zip(side)
    }) {
        Ok(found) => found,
        Err(answer) => return *answer,
    };
    let side = if side == "original" {
        ImageSide::Original
    } else {
        ImageSide::Modified
    };
    let read = tokio::task::spawn_blocking(move || state.workspaces.file_image(&id, &path, side));
    match read.await {
        Ok(Ok((kind, bytes))) => sandboxed_bytes(&kind, bytes),
        Ok(Err(error)) => workspace_error(&error),
        Err(_) => internal_error(),
    }
}

/// `treeQuerySchema`: one folder; the worktree root is the empty path.
pub async fn tree(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let path = match parsed(|issues| match query_value(query, "path") {
        None => Some(String::new()),
        Some(value) => string(Some(&value), &[json!("path")], &[Rule::Max(1024)], issues),
    }) {
        Ok(path) => path,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(json!({ "entries": to_value(&state.workspaces.tree(&id, &path)?) }))
    })
    .await
}

pub async fn read_file(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let path = match parsed(|issues| path_query(query, issues)) {
        Ok(path) => path,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(to_value(&state.workspaces.read_file(&id, &path)?))
    })
    .await
}

/// The text a save sends: `{ text: z.string() }`.
fn text_body(body: Option<&Value>) -> Result<String, ZodError> {
    let mut issues = Issues::default();
    let text = object(body, &[], &mut issues)
        .and_then(|map| string(map.get("text"), &[json!("text")], &[], &mut issues));
    issues.into_result(text)
}

/// `If-Match` holds the version the editor read; a file that changed since is not replaced.
pub async fn write_file(
    state: Arc<AppState>,
    id: String,
    query: String,
    headers: HeaderMap,
    body: Option<Value>,
) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let path = match parsed(|issues| path_query(&query, issues)) {
        Ok(path) => path,
        Err(answer) => return *answer,
    };
    let text = match text_body(body.as_ref()) {
        Ok(text) => text,
        Err(error) => return zod_answer(&error),
    };
    // `singleHeader`: a header sent twice is as good as none.
    let version = headers
        .get("if-match")
        .filter(|_| headers.get_all("if-match").iter().count() == 1)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let Some(version) = version else {
        return workspace_error(&WorkspaceError::BadRequest("If-Match is missing".into()));
    };
    answer(StatusCode::OK, move || {
        Ok(to_value(
            &state.workspaces.write_file(&id, &path, &text, &version)?,
        ))
    })
    .await
}

pub async fn graph(state: Arc<AppState>, id: String, query: &str) -> Answer {
    let id = match param(&id) {
        Ok(id) => id,
        Err(answer) => return *answer,
    };
    let fresh = match parsed(|issues| fresh_query(query, issues)) {
        Ok(fresh) => fresh,
        Err(answer) => return *answer,
    };
    answer(StatusCode::OK, move || {
        Ok(graph_json(&state.workspaces.graph(&id, fresh)?))
    })
    .await
}

/// A graph as Node sends it, in its key order (`GitGraph` only reads JSON).
fn graph_json(graph: &GitGraph) -> Value {
    let push = match graph.push {
        None => Value::Null,
        Some(PushStatus::None) => json!({ "remote": "none" }),
        Some(PushStatus::New { unpushed }) => json!({ "remote": "new", "unpushed": unpushed }),
        Some(PushStatus::Tracked { unpushed, unpulled }) => {
            json!({ "remote": "tracked", "unpushed": unpushed, "unpulled": unpulled })
        }
        // A kind of a later daemon is never made here.
        Some(PushStatus::Unknown) => Value::Null,
    };
    let commits: Vec<Value> = graph.commits.iter().map(commit_json).collect();
    json!({
        "branch": graph.branch,
        "head": graph.head,
        "base": graph.base,
        "ahead": graph.ahead,
        "behind": graph.behind,
        "push": push,
        "commits": commits,
        "truncated": graph.truncated,
    })
}

fn commit_json(commit: &GraphCommit) -> Value {
    json!({
        "hash": commit.hash,
        "parents": commit.parents,
        "author": commit.author,
        "date": commit.date,
        "subject": commit.subject,
        "refs": commit.refs,
        "onBranch": commit.on_branch,
    })
}
