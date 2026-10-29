//! Who calls the daemon, and what the agent token of a task may call (S-01): its own task, reads
//! of its direct subtasks, and a few global reads. Everything else answers 403 before routing,
//! so a new route is closed to agents until it is added here.

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::{Method, Request, StatusCode};

use super::auth::{bearer_token, is_authorized};
use super::errors::{Answer, error_answer};
use super::server::{AppState, query_value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// The daemon token, or no token on the health check.
    User,
    /// A terminal socket with a one-time ticket, which the socket checks itself.
    Ticket,
    /// The token of a task's shells, with that task's id.
    Agent(String),
}

impl Caller {
    pub fn is_agent(&self) -> bool {
        matches!(self, Caller::Agent(_))
    }
}

/// The token of the request: the bearer header, or `?token=` on `/events` only (browsers cannot
/// set headers on a WebSocket).
fn provided_token(request: &Request<Incoming>, events: bool, query: &str) -> Option<String> {
    let header = request
        .headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    bearer_token(header).map(str::to_string).or_else(|| {
        if events {
            query_value(query, "token").and_then(|v| v.as_str().map(str::to_string))
        } else {
            None
        }
    })
}

/// Who calls; None for no one the daemon knows.
pub fn caller(
    request: &Request<Incoming>,
    state: &AppState,
    method: &Method,
    parts: &[&str],
    query: &str,
) -> Option<Caller> {
    if *method == Method::GET && parts == ["health"] {
        return Some(Caller::User);
    }
    let events = *method == Method::GET && parts == ["events"];
    let provided = provided_token(request, events, query);
    if is_authorized(provided.as_deref(), &state.token) {
        return Some(Caller::User);
    }
    if let Some(task) = provided
        .as_deref()
        .and_then(|token| state.terminals.agent_tokens().task_of(token))
    {
        return Some(Caller::Agent(task));
    }
    // The terminal socket checks its one-time ticket itself instead.
    let socket = *method == Method::GET && matches!(parts, ["tasks", _, "terminal"]);
    let has_ticket = query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("ticket"));
    (socket && has_ticket).then_some(Caller::Ticket)
}

/// What `task list`, `task show` and the event stream need, and what a dashboard part shows.
fn is_global_read(parts: &[&str]) -> bool {
    matches!(
        parts,
        ["tasks"]
            | ["jobs"]
            | ["templates"]
            | ["activity"]
            | ["usage"]
            | ["limits"]
            | ["diffstats"]
            | ["workspaces", "preparing"]
            | ["repos"]
            | ["repos", _, "settings" | "skills"]
            | ["git", "branches" | "remote-branches"]
            | ["events"]
    )
}

/// Whether the agent token of task `own` may call the route. `is_child(id)`: that task is a
/// direct subtask of `own`. `POST /tasks` passes here; `create_task` checks its `parentId`.
pub fn agent_may(
    method: &Method,
    parts: &[&str],
    own: &str,
    is_child: impl FnOnce(&str) -> bool,
) -> bool {
    match parts {
        ["tasks", id, ..] if *id == own => *method != Method::DELETE,
        ["tasks", id, ..] => *method == Method::GET && is_child(id),
        ["tasks"] => *method == Method::GET || *method == Method::POST,
        // What an agent's own tools send: hooks, and Claude Code's limits from its status line.
        ["hooks" | "limits", _] => *method == Method::POST,
        _ => *method == Method::GET && is_global_read(parts),
    }
}

/// The 403 for a route the agent token of `own` may not call. The subtask lookup reads the
/// store, so it runs on a blocking thread, and only for a read of another task.
pub async fn refusal(
    state: &Arc<AppState>,
    method: &Method,
    parts: &[&str],
    own: &str,
) -> Option<Answer> {
    let is_child = match parts {
        ["tasks", id, ..] if *id != own && *method == Method::GET => {
            let (state, id, own) = (Arc::clone(state), id.to_string(), own.to_string());
            tokio::task::spawn_blocking(move || {
                state
                    .tasks
                    .get_task(&id)
                    .is_ok_and(|task| task.parent_id.as_deref() == Some(own.as_str()))
            })
            .await
            .unwrap_or(false)
        }
        _ => false,
    };
    if agent_may(method, parts, own, |_| is_child) {
        return None;
    }
    Some(error_answer(
        StatusCode::FORBIDDEN,
        "forbidden",
        "The agent token of a task reaches only that task. Use the dashboard.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn may(method: Method, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').skip(1).collect();
        agent_may(&method, &parts, "T-1", |id| id == "T-2")
    }

    #[test]
    fn its_own_task_except_delete() {
        assert!(may(Method::GET, "/tasks/T-1"));
        assert!(may(Method::PATCH, "/tasks/T-1"));
        assert!(may(Method::POST, "/tasks/T-1/stage"));
        assert!(may(Method::POST, "/tasks/T-1/events"));
        assert!(may(Method::POST, "/tasks/T-1/terminal/ticket"));
        assert!(!may(Method::DELETE, "/tasks/T-1"));
        assert!(!may(Method::DELETE, "/tasks/T-1/workspace"));
        assert!(!may(Method::DELETE, "/tasks/T-1/terminals/2"));
    }

    #[test]
    fn reads_of_a_direct_subtask_only() {
        assert!(may(Method::GET, "/tasks/T-2"));
        assert!(may(Method::GET, "/tasks/T-2/events"));
        assert!(!may(Method::POST, "/tasks/T-2/stage"));
        assert!(!may(Method::PATCH, "/tasks/T-2"));
        assert!(!may(Method::GET, "/tasks/T-3"));
        assert!(!may(Method::POST, "/tasks/T-3/terminal/ticket"));
        assert!(!may(Method::POST, "/tasks/T-3/events"));
    }

    #[test]
    fn the_global_reads_the_task_list_and_hooks() {
        for path in [
            "/tasks",
            "/jobs",
            "/templates",
            "/activity",
            "/usage",
            "/limits",
            "/diffstats",
            "/workspaces/preparing",
            "/repos",
            "/repos/app/settings",
            "/repos/app/skills",
            "/git/branches",
            "/git/remote-branches",
            "/events",
        ] {
            assert!(may(Method::GET, path), "{path}");
        }
        assert!(may(Method::POST, "/tasks"));
        assert!(may(Method::POST, "/hooks/claude-code"));
        assert!(may(Method::POST, "/limits/claude-code"));
        assert!(!may(Method::GET, "/hooks/claude-code"));
    }

    #[test]
    fn no_global_write_and_no_other_read() {
        for (method, path) in [
            (Method::POST, "/workspaces"),
            (Method::POST, "/repos"),
            (Method::PUT, "/repos/app/settings"),
            (Method::POST, "/repos/app/trust"),
            (Method::GET, "/repos/app/trust"),
            (Method::DELETE, "/repos/app"),
            (Method::POST, "/templates"),
            (Method::POST, "/templates/x/tasks"),
            (Method::POST, "/github/issues/import"),
            (Method::GET, "/github/issues"),
            (Method::GET, "/nothing"),
        ] {
            assert!(!may(method.clone(), path), "{method} {path}");
        }
    }
}
