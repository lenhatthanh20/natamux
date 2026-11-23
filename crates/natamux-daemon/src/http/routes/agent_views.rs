//! What the agent token of a task reads of other tasks and projects (F6): its own task in full,
//! its direct subtasks as a short view, its own project, and its own rows of the dashboard
//! reads. `agent_rules` already checked the route; this cuts the answer.

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::{Method, StatusCode};
use natamux_core::tasks::inputs::{list_query, task_id_param};
use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Value, json};

use super::projects::repo_query;
use super::tasks::answer;
use super::{agents, tasks, workspaces};
use crate::http::agent_actions::AgentAction;
use crate::http::agent_rules::denial_line;
use crate::http::errors::{
    Answer, error_answer, internal_error, json_answer, task_error, zod_answer,
};
use crate::http::server::{AppState, query_value};
use crate::projects::repos::DEFAULT_REPO;
use crate::tasks::errors::TaskResult;

/// What the agent of the parent sees of a direct subtask.
pub fn task_view(task: &Task) -> Value {
    json!({
        "id": task.id, "title": task.title, "state": task.state, "stage": task.stage,
        "parentId": task.parent_id, "pr": task.pr, "prStatus": task.pr_status,
        "stageSince": task.stage_since, "branch": task.branch,
    })
}

/// The answer of a read the agent token of `own` makes, cut to what it may see; None: the
/// normal route answers.
pub async fn scoped_read(
    method: &Method,
    parts: &[&str],
    query: &str,
    state: &Arc<AppState>,
    own: &str,
) -> Option<Answer> {
    if *method != Method::GET {
        return None;
    }
    let own = own.to_string();
    let answer = match parts {
        ["tasks"] => list_tasks(Arc::clone(state), own, query_value(query, "state")).await,
        ["tasks", id] if *id != own => child_task(Arc::clone(state), id.to_string()).await,
        ["jobs"] => own_rows(tasks::list_jobs(state), "jobs", &own).await,
        ["usage"] => own_rows(agents::usage(state), "usage", &own).await,
        ["activity"] => own_rows(agents::activity(state), "activity", &own).await,
        ["workspaces", "preparing"] => {
            own_rows(workspaces::preparing(state), "preparing", &own).await
        }
        ["diffstats"] => {
            let stats = workspaces::diff_stats(Arc::clone(state)).await;
            own_rows(stats, "stats", &own).await
        }
        ["repos"] => own_repo_list(Arc::clone(state), own).await,
        ["repos", id, "settings" | "skills"] => {
            let wanted = Some(id.to_string());
            return other_repo(state, wanted, method, parts, &own).await;
        }
        ["git", "branches" | "remote-branches"] => {
            let wanted = match repo_query(query) {
                Ok(wanted) => wanted,
                Err(error) => return Some(zod_answer(&error)),
            };
            return other_repo(state, wanted, method, parts, &own).await;
        }
        _ => return None,
    };
    Some(answer)
}

async fn list_tasks(state: Arc<AppState>, own: String, state_query: Option<Value>) -> Answer {
    let wanted = match list_query(state_query.as_ref()) {
        Ok(wanted) => wanted,
        Err(error) => return zod_answer(&error),
    };
    answer(StatusCode::OK, move || {
        let filter = wanted.and_then(|name| serde_json::from_value::<TaskState>(json!(name)).ok());
        let tasks: Vec<Value> = state
            .tasks
            .list_tasks(filter)?
            .iter()
            .filter_map(|task| {
                if task.id == own {
                    Some(serde_json::to_value(task).unwrap_or(Value::Null))
                } else {
                    (task.parent_id.as_deref() == Some(own.as_str())).then(|| task_view(task))
                }
            })
            .collect();
        Ok(json!({ "tasks": tasks }))
    })
    .await
}

/// `agent_rules` let it through, so the task is a direct subtask.
async fn child_task(state: Arc<AppState>, id: String) -> Answer {
    answer(StatusCode::OK, move || {
        let id = task_id_param(&id)?;
        Ok(json!({ "task": task_view(&state.tasks.get_task(&id)?) }))
    })
    .await
}

fn own_repo(state: &AppState, own: &str) -> TaskResult<Repo> {
    let task = state.tasks.get_task(own)?;
    state.repos.get(task.repo.as_deref())
}

/// Only the id and name: the folders of the machine are not the agent's business.
async fn own_repo_list(state: Arc<AppState>, own: String) -> Answer {
    answer(StatusCode::OK, move || {
        let repo = own_repo(&state, &own)?;
        Ok(json!({ "repos": [{ "id": repo.id, "name": repo.name }] }))
    })
    .await
}

/// A 403 when `wanted` (None: the default project) is not the project of the own task. The
/// id is compared as given, so the answer tells nothing of which other projects exist.
async fn other_repo(
    state: &Arc<AppState>,
    wanted: Option<String>,
    method: &Method,
    parts: &[&str],
    own: &str,
) -> Option<Answer> {
    let (state, own_id) = (Arc::clone(state), own.to_string());
    let mine = tokio::task::spawn_blocking(move || own_repo(&state, &own_id).map(|repo| repo.id));
    let mine = match mine.await {
        Ok(Ok(mine)) => mine,
        Ok(Err(error)) => return Some(task_error(&error)),
        Err(_) => return Some(internal_error()),
    };
    if wanted.as_deref().unwrap_or(DEFAULT_REPO) == mine {
        return None;
    }
    eprintln!(
        "{}",
        denial_line(Some(AgentAction::RepoReadOwn), method, parts, own)
    );
    Some(error_answer(
        StatusCode::FORBIDDEN,
        "forbidden",
        "The agent token of a task reads only the project of that task.",
    ))
}

/// The answer with only the rows of task `own` under `key`: a map by task id, or a list of
/// rows with `taskId`.
async fn own_rows(answer: Answer, key: &str, own: &str) -> Answer {
    let status = answer.status();
    let bytes = match answer.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(never) => match never {},
    };
    let Ok(mut body) = serde_json::from_slice::<Value>(&bytes) else {
        return json_answer(status, &Value::Null);
    };
    match body.get_mut(key) {
        Some(Value::Object(rows)) => rows.retain(|id, _| id == own),
        Some(Value::Array(rows)) => {
            rows.retain(|row| row.get("taskId").and_then(Value::as_str) == Some(own))
        }
        _ => {}
    }
    json_answer(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_of(body: Value, key: &str) -> Value {
        let answer = json_answer(StatusCode::OK, &body);
        let cut = futures::executor::block_on(own_rows(answer, key, "T-1"));
        let bytes = futures::executor::block_on(cut.into_body().collect())
            .unwrap()
            .to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn keeps_only_the_own_rows_of_a_map_or_a_list() {
        let map = json!({ "usage": { "T-1": 1, "T-2": 2 } });
        assert_eq!(rows_of(map, "usage"), json!({ "usage": { "T-1": 1 } }));
        let list = json!({ "jobs": [
            { "id": "j1", "taskId": "T-1" }, { "id": "j2", "taskId": "T-2" }, { "id": "j3" },
        ] });
        assert_eq!(
            rows_of(list, "jobs"),
            json!({ "jobs": [{ "id": "j1", "taskId": "T-1" }] })
        );
    }

    #[test]
    fn a_subtask_view_has_only_the_status_fields() {
        let task: Task = serde_json::from_value(json!({
            "id": "T-3", "title": "C", "description": "SECRET-MARK", "repo": "other",
            "workspace": "/w/T-3", "parentId": "T-1", "updatedAt": "2026-11-23T00:00:00.000Z",
        }))
        .unwrap();
        let view = task_view(&task);
        let keys: Vec<&String> = view.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 9, "{keys:?}");
        assert!(!view.to_string().contains("SECRET-MARK"));
        assert!(view.get("workspace").is_none() && view.get("repo").is_none());
        assert_eq!(view["parentId"], "T-1");
    }
}
