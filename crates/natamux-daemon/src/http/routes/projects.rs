//! The project routes that exist in Rust so far (`routes/projects.ts`): the branches of a
//! repository, which a new workspace can start from.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::formats::REPO_ID;
use natamux_core::zod::Issues;
use natamux_core::zod::string;
use natamux_git::branches::branch_list_json;
use serde_json::json;

use crate::http::errors::{Answer, zod_answer};
use crate::http::routes::workspaces::answer;
use crate::http::server::{AppState, query_value};

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
