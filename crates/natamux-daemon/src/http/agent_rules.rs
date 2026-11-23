//! Who calls the daemon, and what the agent token of a task may call (S-01, F6): the routes
//! `agent_actions` classifies, on its own task or a direct subtask, and in Strict mode only what
//! its grants allow. Everything else answers 403 before routing.

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::{Method, Request, StatusCode};
use natamux_core::security::{self, SecurityMode};

use super::agent_actions::{AgentAction, Scope, classify};
use super::agent_grants::AgentGrants;
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

/// The token of the request and whether it came in the query: the bearer header, or `?token=`
/// on `/events` only (browsers cannot set headers on a WebSocket).
fn provided_token(
    request: &Request<Incoming>,
    events: bool,
    query: &str,
) -> Option<(String, bool)> {
    let header = request
        .headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if let Some(token) = bearer_token(header) {
        return Some((token.to_string(), false));
    }
    if !events {
        return None;
    }
    query_value(query, "token").and_then(|v| v.as_str().map(|token| (token.to_string(), true)))
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
    let token = provided.as_ref().map(|(token, _)| token.as_str());
    if is_authorized(token, &state.token) {
        return Some(Caller::User);
    }
    if let Some((token, in_query)) = &provided
        && let Some(task) = state.terminals.agent_tokens().task_of(token)
    {
        // Strict: an address ends up in logs and shell history, so agents send the header.
        if *in_query && security::mode().is_strict() {
            return None;
        }
        return Some(Caller::Agent(task));
    }
    // The terminal socket checks its one-time ticket itself instead.
    let socket = *method == Method::GET && matches!(parts, ["tasks", _, "terminal"]);
    let has_ticket = query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("ticket"));
    (socket && has_ticket).then_some(Caller::Ticket)
}

/// What the agent token of task `own` does with the call, or why not (None: the route is not
/// classified). `is_child(id)`: that task is a direct subtask of `own`.
pub fn agent_decision(
    method: &Method,
    parts: &[&str],
    own: &str,
    is_child: impl FnOnce(&str) -> bool,
    mode: &SecurityMode,
) -> Result<AgentAction, Option<AgentAction>> {
    let (action, scope) = classify(method, parts).ok_or(None)?;
    let target = match parts {
        ["tasks", id, ..] => Some(*id),
        _ => None,
    };
    let action = match (scope, target) {
        (Scope::Global, _) => action,
        (_, Some(id)) if id == own => action,
        (Scope::Child, Some(id)) => {
            if !is_child(id) {
                return Err(Some(action));
            }
            action.on_child()
        }
        _ => return Err(Some(action)),
    };
    if AgentGrants::of(mode).allows(action) {
        Ok(action)
    } else {
        Err(Some(action))
    }
}

/// Whether the agent token of task `own` may call the route in Personal mode.
pub fn agent_may(
    method: &Method,
    parts: &[&str],
    own: &str,
    is_child: impl FnOnce(&str) -> bool,
) -> bool {
    agent_decision(method, parts, own, is_child, &SecurityMode::Personal).is_ok()
}

/// The log line of a refused agent call: never the body, the query or the token.
pub fn denial_line(
    action: Option<AgentAction>,
    method: &Method,
    parts: &[&str],
    own: &str,
) -> String {
    let action = action.map_or("unclassified", AgentAction::name);
    format!(
        "agent-api: denied {action} {method} /{} task={own}",
        parts.join("/")
    )
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
    let denied = agent_decision(method, parts, own, |_| is_child, &security::mode()).err()?;
    eprintln!("{}", denial_line(denied, method, parts, own));
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
        // Security T-0346 (F-02).
        assert!(!may(Method::POST, "/tasks/T-1/fan-out"));
        assert!(!may(Method::POST, "/tasks/T-1/stages/implement/ci-approve"));
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
            "/activity",
            "/usage",
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
            // T-0373 (F6): no task to cut these to.
            (Method::GET, "/templates"),
            (Method::GET, "/limits"),
            (Method::GET, "/tasks/T-1/nothing"),
        ] {
            assert!(!may(method.clone(), path), "{method} {path}");
        }
    }

    fn strict(grants: &[&str]) -> SecurityMode {
        use natamux_core::security::{AgentApi, StrictSettings};
        SecurityMode::Strict(StrictSettings {
            agent_api: AgentApi {
                grants: grants.iter().map(|name| name.to_string()).collect(),
                ..AgentApi::default()
            },
            ..StrictSettings::default()
        })
    }

    fn strict_may(grants: &[&str], method: Method, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').skip(1).collect();
        agent_decision(&method, &parts, "T-1", |id| id == "T-2", &strict(grants)).is_ok()
    }

    #[test]
    fn strict_gives_the_minimum_and_the_listed_grants_only() {
        for (method, path) in [
            (Method::GET, "/tasks/T-1"),
            (Method::PATCH, "/tasks/T-1"),
            (Method::POST, "/tasks/T-1/stage"),
            (Method::GET, "/tasks/T-1/tree"),
            (Method::GET, "/tasks/T-2"),
            (Method::POST, "/tasks"),
            (Method::POST, "/hooks/claude-code"),
        ] {
            assert!(strict_may(&[], method.clone(), path), "{method} {path}");
        }
        for (method, path) in [
            (Method::GET, "/tasks"),
            (Method::GET, "/jobs"),
            (Method::GET, "/usage"),
            (Method::GET, "/repos"),
            (Method::GET, "/events"),
            (Method::POST, "/tasks/T-1/push"),
            (Method::POST, "/tasks/T-1/pull-request"),
            (Method::POST, "/tasks/T-1/issue"),
            (Method::GET, "/tasks/T-2/tree"),
            (Method::GET, "/tasks/T-2/file"),
        ] {
            assert!(!strict_may(&[], method.clone(), path), "{method} {path}");
        }
        assert!(strict_may(
            &["subtask.read.workspace"],
            Method::GET,
            "/tasks/T-2/tree"
        ));
        assert!(strict_may(
            &["pr.create.own"],
            Method::POST,
            "/tasks/T-1/pull-request"
        ));
        // A grant never reaches another task.
        assert!(!strict_may(
            &["pr.create.own"],
            Method::POST,
            "/tasks/T-3/pull-request"
        ));
        assert!(!strict_may(
            &["git.push.own"],
            Method::POST,
            "/tasks/T-1/push"
        ));
    }

    #[test]
    fn the_denial_line_names_the_action_route_and_task_only() {
        let parts = ["tasks", "T-2", "push"];
        let line = denial_line(Some(AgentAction::GitPushOwn), &Method::POST, &parts, "T-1");
        assert_eq!(
            line,
            "agent-api: denied git.push.own POST /tasks/T-2/push task=T-1"
        );
        let line = denial_line(None, &Method::GET, &["nothing"], "T-1");
        assert_eq!(line, "agent-api: denied unclassified GET /nothing task=T-1");
    }
}
