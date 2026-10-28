//! The HTTP server (`server.ts`): `/api` stripped, the token checked, then the route. Routes not
//! ported yet answer 404 in Fastify's shape.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::auth::{bearer_token, is_authorized};
use super::body::{DEFAULT_BODY_LIMIT, read_body};
use super::errors::{Answer, json_answer, not_found, unauthorized};
use super::routes::stages::Mark;
use super::routes::{
    agents, github, projects, review_comments, stages, tasks, terminals, workspace_files,
    workspaces,
};
use super::web::WebAssets;
use super::{event_socket, terminal_socket};
use crate::agents::hook_service::HookService;
use crate::agents::idle_pause::IdlePause;
use crate::agents::runner::AgentRunner;
use crate::agents::usage_service::UsageService;
use crate::github::issue_importer::IssueImporter;
use crate::github::issue_linker::IssueLinker;
use crate::github::pull_request_watcher::PullRequestWatcher;
use crate::projects::repos::Repos;
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::Jobs;
use crate::shared::scheduler::Scheduler;
use crate::stages::runner::StageRunner;
use crate::tasks::service::TaskService;
use crate::tasks::templates::TemplateService;
use crate::terminals::open_shells::OpenShells;
use crate::terminals::service::TerminalService;
use crate::workspaces::service::WorkspaceService;

/// A text of 1 MB sent as JSON: escapes can make it a few times longer.
const WORKSPACE_FILE_BODY_LIMIT: usize = 4 * 1024 * 1024;

pub struct AppState {
    pub stages: StageRunner,
    pub agents: Arc<AgentRunner>,
    pub hooks: Arc<HookService>,
    pub usage: Arc<UsageService>,
    pub token: String,
    /// The token of the task shells.
    pub agent_token: String,
    pub data_dir: PathBuf,
    /// The home folder of the daemon's environment: where the skills of the agents are read.
    pub home: PathBuf,
    pub web: WebAssets,
    pub terminals: TerminalService,
    pub open_shells: Arc<OpenShells>,
    /// A socket that sent nothing for this long is dropped (`SILENT_CLOSE`; shorter in tests).
    pub silent_close: std::time::Duration,
    pub tasks: TaskService,
    pub templates: TemplateService,
    pub repos: Arc<Repos>,
    pub jobs: Arc<Jobs>,
    pub bus: Arc<EventBus>,
    pub scheduler: Arc<Scheduler>,
    pub workspaces: Arc<WorkspaceService>,
    pub pull_requests: Arc<PullRequestWatcher>,
    pub issues: Arc<IssueImporter>,
    pub issue_linker: Arc<IssueLinker>,
    pub idle_pause: Arc<IdlePause>,
}

/// Serves until the listener fails. Each connection runs on its own task.
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> std::io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let state = Arc::clone(&state);
                async move { Ok::<_, Infallible>(handle(request, state).await) }
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades()
                .await;
        });
    }
}

pub async fn bind(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(std::io::Error::other)?;
    TcpListener::bind(address).await
}

/// The dashboard asks the daemon through /api, as through Vite and nginx.
pub fn strip_api_prefix(url: &str) -> String {
    if url == "/api" || url.starts_with("/api?") {
        return format!("/{}", &url[4..]);
    }
    match url.strip_prefix("/api/") {
        Some(rest) => format!("/{rest}"),
        None => url.to_string(),
    }
}

/// A path segment as the router reads it: percent-decoded.
fn segment(raw: &str) -> String {
    // decodeURIComponent: a `+` stays a `+` in a path.
    natamux_proto::terminals::query::decode(&raw.replace('+', "%2B"))
}

/// A query value as Fastify's parser gives it: text, or a list when the key comes more than once.
pub fn query_value(query: &str, key: &str) -> Option<Value> {
    let values: Vec<Value> = query
        .split('&')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name) == key).then(|| Value::String(decode(value)))
        })
        .collect();
    match values.len() {
        0 => None,
        1 => values.into_iter().next(),
        _ => Some(Value::Array(values)),
    }
}

fn decode(text: &str) -> String {
    natamux_proto::terminals::query::decode(text)
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

pub async fn handle(request: Request<Incoming>, state: Arc<AppState>) -> Answer {
    let raw = request
        .uri()
        .path_and_query()
        .map_or("/", |path| path.as_str());
    let url = strip_api_prefix(raw);
    let accept = request
        .headers()
        .get(hyper::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if let Some(answer) = state.web.answer(request.method(), raw, accept) {
        return answer;
    }
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let segments: Vec<String> = path.split('/').skip(1).map(segment).collect();
    let parts: Vec<&str> = segments.iter().map(String::as_str).collect();
    let method = request.method().clone();
    let Some(by_agent) = caller(&request, &state, &method, &parts, query) else {
        return unauthorized();
    };
    let route = (
        method.clone(),
        parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>(),
    );
    if let Some(answer) = stage_route(&method, &parts, Arc::clone(&state)).await {
        return answer;
    }
    if let Some(answer) = workspace_route(&method, &parts, query, Arc::clone(&state)).await {
        return answer;
    }
    match (&method, parts.as_slice()) {
        (&Method::GET, ["health"]) => json_answer(StatusCode::OK, &json!({ "status": "ok" })),
        (&Method::GET, ["events"]) => event_socket::upgrade(request, state),
        (&Method::GET, ["tasks", id, "terminal"]) => {
            let id = id.to_string();
            terminal_socket::upgrade(request, state, id, query.to_string())
        }
        (&Method::GET, ["tasks", id, "ended-shells"]) => terminals::ended_shells(&state, id),
        (&Method::GET, ["tasks", id, "terminals"]) => {
            terminals::list_terminals(state, id.to_string()).await
        }
        (&Method::DELETE, ["tasks", id, "terminals", terminal]) => {
            terminals::close_terminal(&state, id, terminal)
        }
        (&Method::POST, ["tasks", id, "terminal", "ticket"]) => terminals::ticket(&state, id),
        (&Method::GET, ["jobs"]) => tasks::list_jobs(&state),
        (&Method::GET, ["activity"]) => agents::activity(&state),
        (&Method::GET, ["usage"]) => agents::usage(&state),
        (&Method::GET, ["limits"]) => agents::limits(&state),
        (&Method::POST, ["hooks", agent]) => {
            agents::post_hook(request, state, agent.to_string(), query.to_string()).await
        }
        (&Method::GET, ["tasks"]) => tasks::list_tasks(state, query_value(query, "state")).await,
        (&Method::GET, ["templates"]) => tasks::list_templates(state).await,
        (&Method::GET, ["tasks", id]) => tasks::get_task(state, id.to_string()).await,
        (&Method::DELETE, ["tasks", id]) => tasks::delete_task(state, id.to_string(), query).await,
        (&Method::GET, ["tasks", id, "events"]) => tasks::list_events(state, id.to_string()).await,
        (&Method::GET, ["tasks", id, "attachments"]) => {
            tasks::list_task_attachments(state, id.to_string()).await
        }
        (&Method::GET, ["tasks", id, "attachments", name]) => {
            tasks::get_attachment(state, id.to_string(), name.to_string()).await
        }
        (&Method::GET, ["tasks", id, "review-comments"]) => {
            let (id, query) = (id.to_string(), query.to_string());
            review_comments::list(state, id, query, by_agent).await
        }
        (&Method::DELETE, ["tasks", id, "review-comments", cid]) => {
            review_comments::delete(state, id.to_string(), cid.to_string(), by_agent).await
        }
        (&Method::POST, ["tasks", id, "review-comments", "send"]) => {
            review_comments::send(state, id.to_string(), by_agent).await
        }
        (&Method::POST, ["tasks"])
        | (&Method::POST, ["tasks", _, "review-comments"])
        | (&Method::PATCH, ["tasks", _, "review-comments", _])
        | (&Method::POST, ["templates"])
        | (&Method::POST, ["templates", _, "tasks"])
        | (&Method::PATCH, ["tasks", _])
        | (&Method::POST, ["tasks", _, "events"])
        | (&Method::PUT, ["tasks", _, "issue"])
        | (&Method::POST, ["tasks", _, "stage"])
        | (&Method::PUT, ["tasks", _, "workflow" | "ci" | "skip"])
        | (&Method::PUT, ["tasks", _, "files", _])
        | (&Method::POST, ["tasks", _, "agent"])
        | (&Method::POST, ["tasks", _, "agent", "message"])
        | (&Method::POST, ["limits", _])
        | (&Method::POST, ["tasks", _, "workspace" | "commit" | "rollback" | "fan-out"])
        | (&Method::POST, ["workspaces"])
        | (&Method::POST, ["repos"])
        | (&Method::PUT, ["repos", _, "settings"])
        | (&Method::POST, ["github", "issues", "import"])
        | (&Method::PUT, ["tasks", _, "file"]) => {
            with_body(request, state, route, query.to_string(), by_agent).await
        }
        _ => not_found(method.as_str(), &url),
    }
}

/// Who calls: `Some(true)` for the agent token, `Some(false)` for the daemon token, a terminal
/// ticket or the health check, `None` for no one the daemon knows.
fn caller(
    request: &Request<Incoming>,
    state: &AppState,
    method: &Method,
    parts: &[&str],
    query: &str,
) -> Option<bool> {
    if *method == Method::GET && parts == ["health"] {
        return Some(false);
    }
    let socket = *method == Method::GET && matches!(parts, ["tasks", _, "terminal"]);
    let events = *method == Method::GET && parts == ["events"];
    // The terminal socket checks its one-time ticket itself instead.
    let has_ticket = query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("ticket"));
    let provided = provided_token(request, events, query);
    let by_agent = is_authorized(provided.as_deref(), &state.agent_token);
    let allowed =
        (socket && has_ticket) || is_authorized(provided.as_deref(), &state.token) || by_agent;
    allowed.then_some(by_agent)
}

/// The stage routes that read no body (part 3).
async fn stage_route(method: &Method, parts: &[&str], state: Arc<AppState>) -> Option<Answer> {
    Some(match (method, parts) {
        (&Method::GET, ["tasks", id, "files"]) => stages::list_files(state, id.to_string()).await,
        (&Method::GET, ["tasks", id, "files", name]) => {
            stages::read_file(state, id.to_string(), name.to_string()).await
        }
        (&Method::POST, ["tasks", id, "agent", "reopen"]) => {
            agents::reopen_agent(state, id.to_string()).await
        }
        _ => return None,
    })
}

/// The workspace, git and GitHub routes that read no body (part 5).
async fn workspace_route(
    method: &Method,
    parts: &[&str],
    query: &str,
    state: Arc<AppState>,
) -> Option<Answer> {
    Some(match (method, parts) {
        (&Method::GET, ["diffstats"]) => workspaces::diff_stats(state).await,
        (&Method::GET, ["workspaces", "preparing"]) => workspaces::preparing(&state),
        (&Method::DELETE, ["tasks", id, "workspace"]) => {
            workspaces::remove(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "changes"]) => {
            workspace_files::changes(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "changes", "file"]) => {
            workspace_files::changed_file(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "changes", "image"]) => {
            workspace_files::changed_image(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "tree"]) => {
            workspace_files::tree(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "file"]) => {
            workspace_files::read_file(state, id.to_string(), query).await
        }
        (&Method::GET, ["tasks", id, "graph"]) => {
            workspace_files::graph(state, id.to_string(), query).await
        }
        (&Method::POST, ["tasks", id, "rollback", "undo"]) => {
            workspaces::undo_rollback(state, id.to_string()).await
        }
        (&Method::POST, ["tasks", id, "commit-message"]) => {
            workspaces::commit_message(state, id.to_string()).await
        }
        (&Method::POST, ["tasks", id, "push"]) => workspaces::push(state, id.to_string()).await,
        (&Method::POST, ["tasks", id, "pull-request"]) => {
            workspaces::open_pull_request(state, id.to_string()).await
        }
        (&Method::POST, ["tasks", id, "pull-request", "refresh"]) => {
            workspaces::refresh_pull_request(state, id.to_string()).await
        }
        (&Method::GET, ["repos"]) => projects::list_repos(state).await,
        (&Method::DELETE, ["repos", id]) => projects::remove_repo(state, id.to_string()).await,
        (&Method::GET, ["repos", id, "settings"]) => {
            projects::get_settings(state, id.to_string()).await
        }
        (&Method::GET, ["repos", id, "skills"]) => {
            projects::list_skills(state, id.to_string()).await
        }
        (&Method::GET, ["git", "branches"]) => projects::branches(state, query).await,
        (&Method::GET, ["git", "remote-branches"]) => projects::remote_branches(state, query).await,
        (&Method::GET, ["github", "issues"]) => github::list_issues(state, query).await,
        (&Method::POST, ["tasks", id, "issue"]) => {
            github::create_issue(state, id.to_string()).await
        }
        _ => return None,
    })
}

/// The routes that read a JSON body.
async fn with_body(
    request: Request<Incoming>,
    state: Arc<AppState>,
    route: (Method, Vec<String>),
    query: String,
    by_agent: bool,
) -> Answer {
    let (method, parts) = route;
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let limit = match (&method, parts.as_slice()) {
        (&Method::POST, ["tasks"] | ["workspaces"]) => tasks::new_task_body_limit(),
        (&Method::PUT, ["tasks", _, "file"]) => WORKSPACE_FILE_BODY_LIMIT,
        _ => DEFAULT_BODY_LIMIT,
    };
    let (head, body) = request.into_parts();
    let body = match read_body(&head.headers, body, limit).await {
        Ok(body) => body,
        Err(error) => return error.answer(),
    };
    match (&method, parts.as_slice()) {
        (&Method::POST, ["tasks"]) => tasks::create_task(state, body).await,
        (&Method::POST, ["templates"]) => tasks::save_template(state, body).await,
        (&Method::POST, ["templates", id, "tasks"]) => {
            tasks::task_from_template(state, id.to_string(), body).await
        }
        (&Method::PATCH, ["tasks", id]) => tasks::update_task(state, id.to_string(), body).await,
        (&Method::POST, ["tasks", id, "events"]) => {
            tasks::post_event(state, id.to_string(), body, by_agent).await
        }
        (&Method::PUT, ["tasks", id, "issue"]) => {
            tasks::link_issue(state, id.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "stage"]) => {
            stages::act(state, id.to_string(), body, by_agent).await
        }
        (&Method::PUT, ["tasks", id, mark @ ("workflow" | "ci" | "skip")]) => {
            let mark = match *mark {
                "workflow" => Mark::Workflow,
                "ci" => Mark::Ci,
                _ => Mark::Skip,
            };
            stages::set_mark(state, id.to_string(), body, by_agent, mark).await
        }
        (&Method::PUT, ["tasks", id, "files", name]) => {
            stages::write_file(state, id.to_string(), name.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "agent"]) => {
            agents::start_agent(state, id.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "agent", "message"]) => {
            agents::message(state, id.to_string(), body).await
        }
        (&Method::POST, ["limits", agent]) => agents::post_limits(&state, agent, body),
        (&Method::POST, ["tasks", id, "review-comments"]) => {
            review_comments::add(state, id.to_string(), body, by_agent).await
        }
        (&Method::PATCH, ["tasks", id, "review-comments", cid]) => {
            let (id, cid) = (id.to_string(), cid.to_string());
            review_comments::update(state, id, cid, body, by_agent).await
        }
        (&Method::POST, ["repos"]) => projects::add_repo(state, body).await,
        (&Method::PUT, ["repos", id, "settings"]) => {
            projects::put_settings(state, id.to_string(), head.headers, body, by_agent).await
        }
        _ => workspace_body_route(&method, &parts, state, query, head.headers, body).await,
    }
}

/// The workspace and GitHub routes that read a JSON body.
async fn workspace_body_route(
    method: &Method,
    parts: &[&str],
    state: Arc<AppState>,
    query: String,
    headers: hyper::HeaderMap,
    body: Option<Value>,
) -> Answer {
    match (method, parts) {
        (&Method::POST, ["tasks", id, "workspace"]) => {
            workspaces::create(state, id.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "commit"]) => {
            workspaces::commit(state, id.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "rollback"]) => {
            workspaces::rollback(state, id.to_string(), body).await
        }
        (&Method::POST, ["tasks", id, "fan-out"]) => {
            workspaces::fan_out_tries(state, id.to_string(), body).await
        }
        (&Method::POST, ["workspaces"]) => workspaces::start_workspace(state, body).await,
        (&Method::PUT, ["tasks", id, "file"]) => {
            workspace_files::write_file(state, id.to_string(), query, headers, body).await
        }
        (&Method::POST, ["github", "issues", "import"]) => github::import_issues(state, body).await,
        _ => not_found(method.as_str(), &parts.join("/")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_api_prefix_as_the_node_daemon() {
        assert_eq!(strip_api_prefix("/api/tasks"), "/tasks");
        assert_eq!(strip_api_prefix("/api"), "/");
        assert_eq!(strip_api_prefix("/api?x=1"), "/?x=1");
        assert_eq!(strip_api_prefix("/apix"), "/apix");
        assert_eq!(strip_api_prefix("/health"), "/health");
    }
}
