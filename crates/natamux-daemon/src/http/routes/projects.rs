//! The project routes (`routes/projects.ts`): the list, add and remove of projects, their
//! settings and skills, and the branches of a repository, which a new workspace can start from.

use std::path::Path;
use std::sync::Arc;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{HeaderMap, StatusCode};
use natamux_core::formats::REPO_ID;
use natamux_core::zod::Issues;
use natamux_core::zod::string;
use natamux_git::branches::branch_list_json;
use serde_json::{Value, json};

use crate::http::errors::{
    Answer, forbidden, internal_error, json_answer, repo_error_answer, zod_answer,
};
use crate::http::routes::workspace_files::if_match;
use crate::http::routes::workspaces::answer;
use crate::http::server::{AppState, query_value};
use crate::projects::errors::RepoError;
use crate::shared::event_bus::DaemonMessage;

/// `repoQuerySchema`: the project, by id; none is the default one.
pub fn repo_query(query: &str) -> Result<Option<String>, natamux_core::zod::ZodError> {
    let mut issues = Issues::default();
    let repo = match query_value(query, "repo") {
        None => Some(None),
        Some(value) => string(Some(&value), &[json!("repo")], &REPO_ID, &mut issues).map(Some),
    };
    issues.into_result(repo)
}

pub async fn branches(state: Arc<AppState>, query: &str) -> Answer {
    let repo = match repo_query(query) {
        Ok(repo) => repo,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        Ok(branch_list_json(
            &state.workspaces.branches(repo.as_deref())?,
        ))
    })
    .await
}

/// The branches of origin, also the ones never fetched, for the remote part of **Branch from**.
pub async fn remote_branches(state: Arc<AppState>, query: &str) -> Answer {
    let repo = match repo_query(query) {
        Ok(repo) => repo,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        let branches = state.workspaces.remote_branches(repo.as_deref())?;
        Ok(json!({ "branches": branches }))
    })
    .await
}

/// `repoParamsSchema`: the id of a project in the address.
fn repo_param(id: &str) -> Result<String, RepoError> {
    let mut issues = Issues::default();
    let id = string(
        Some(&Value::String(id.to_string())),
        &[json!("id")],
        &REPO_ID,
        &mut issues,
    );
    Ok(issues.into_result(id)?)
}

/// Runs project work on a blocking thread (it reads and writes files, and runs git). `None` is
/// an answer with no body.
async fn run(
    status: StatusCode,
    work: impl FnOnce() -> Result<Option<Value>, RepoError> + Send + 'static,
) -> Answer {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(Some(body))) => json_answer(status, &body),
        Ok(Ok(None)) => {
            let mut empty = Answer::new(Full::new(Bytes::new()));
            *empty.status_mut() = status;
            empty
        }
        Ok(Err(error)) => repo_error_answer(&error),
        Err(_) => internal_error(),
    }
}

pub async fn list_repos(state: Arc<AppState>) -> Answer {
    run(StatusCode::OK, move || {
        let mut body = json!({ "repos": state.repos.list()? });
        // `JSON.stringify` leaves out `undefined`.
        if let Some(dir) = state.repos.worktrees_dir() {
            body["worktreesDir"] = json!(dir.display().to_string());
        }
        Ok(Some(body))
    })
    .await
}

pub async fn add_repo(state: Arc<AppState>, body: Option<Value>) -> Answer {
    let answer = run(StatusCode::CREATED, {
        let state = Arc::clone(&state);
        move || {
            let repo = state.repos.add(&body.unwrap_or(Value::Null))?;
            Ok(Some(json!({ "repo": repo })))
        }
    })
    .await;
    if answer.status() == StatusCode::CREATED {
        state.bus.publish(DaemonMessage::ReposChanged);
    }
    answer
}

pub async fn remove_repo(state: Arc<AppState>, id: String) -> Answer {
    let answer = run(StatusCode::NO_CONTENT, {
        let state = Arc::clone(&state);
        move || {
            state.repos.remove(&repo_param(&id)?)?;
            Ok(None)
        }
    })
    .await;
    if answer.status() == StatusCode::NO_CONTENT {
        state.bus.publish(DaemonMessage::ReposChanged);
    }
    answer
}

pub async fn get_settings(state: Arc<AppState>, id: String) -> Answer {
    run(StatusCode::OK, move || {
        Ok(Some(state.repos.load(&repo_param(&id)?)?))
    })
    .await
}

/// `If-Match` holds the version the editor read; a file that changed since is not replaced.
pub async fn put_settings(
    state: Arc<AppState>,
    id: String,
    headers: HeaderMap,
    body: Option<Value>,
    by_agent: bool,
) -> Answer {
    if by_agent {
        return forbidden("change project settings");
    }
    let version = if_match(&headers);
    run(StatusCode::OK, move || {
        let id = repo_param(&id)?;
        let (settings, version) =
            state
                .repos
                .save(&id, &body.unwrap_or(Value::Null), version.as_deref())?;
        Ok(Some(json!({ "settings": settings, "version": version })))
    })
    .await
}

/// The skills of every agent for a project, for the skill picker of the settings page.
pub async fn list_skills(state: Arc<AppState>, id: String) -> Answer {
    run(StatusCode::OK, move || {
        let path = state.repos.path_of(Some(&repo_param(&id)?))?;
        Ok(Some(
            natamux_agents::skill_list::list_skills(Path::new(&path), &state.home).to_json(),
        ))
    })
    .await
}
