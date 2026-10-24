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
use super::routes::{agents, stages, tasks, terminals};
use super::{event_socket, terminal_socket};
use crate::agents::runner::AgentRunner;
use crate::projects::repos::Repos;
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::Jobs;
use crate::shared::scheduler::Scheduler;
use crate::stages::runner::StageRunner;
use crate::tasks::service::TaskService;
use crate::tasks::templates::TemplateService;
use crate::terminals::open_shells::OpenShells;
use crate::terminals::service::TerminalService;

pub struct AppState {
    pub stages: StageRunner,
    pub agents: Arc<AgentRunner>,
    pub token: String,
    /// The token of the task shells.
    pub agent_token: String,
    pub data_dir: PathBuf,
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
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let segments: Vec<String> = path.split('/').skip(1).map(segment).collect();
    let parts: Vec<&str> = segments.iter().map(String::as_str).collect();
    let method = request.method().clone();
    let socket = method == Method::GET && matches!(parts.as_slice(), ["tasks", _, "terminal"]);
    let events = method == Method::GET && parts.as_slice() == ["events"];
    let mut by_agent = false;
    if !(method == Method::GET && parts.as_slice() == ["health"]) {
        // The terminal socket checks its one-time ticket itself instead.
        let has_ticket = query
            .split('&')
            .any(|pair| pair.split('=').next() == Some("ticket"));
        let provided = provided_token(&request, events, query);
        by_agent = is_authorized(provided.as_deref(), &state.agent_token);
        let allowed =
            (socket && has_ticket) || is_authorized(provided.as_deref(), &state.token) || by_agent;
        if !allowed {
            return unauthorized();
        }
    }
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
        (&Method::GET, ["tasks"]) => tasks::list_tasks(state, query_value(query, "state")).await,
        (&Method::GET, ["templates"]) => tasks::list_templates(state).await,
        (&Method::GET, ["tasks", id]) => tasks::get_task(state, id.to_string()).await,
        (&Method::DELETE, ["tasks", id]) => tasks::delete_task(state, id.to_string()).await,
        (&Method::GET, ["tasks", id, "events"]) => tasks::list_events(state, id.to_string()).await,
        (&Method::GET, ["tasks", id, "attachments"]) => {
            tasks::list_task_attachments(state, id.to_string()).await
        }
        (&Method::GET, ["tasks", id, "attachments", name]) => {
            tasks::get_attachment(state, id.to_string(), name.to_string()).await
        }
        (&Method::POST, ["tasks"])
        | (&Method::POST, ["templates"])
        | (&Method::POST, ["templates", _, "tasks"])
        | (&Method::PATCH, ["tasks", _])
        | (&Method::POST, ["tasks", _, "events"])
        | (&Method::PUT, ["tasks", _, "issue"])
        | (&Method::POST, ["tasks", _, "stage"])
        | (&Method::PUT, ["tasks", _, "workflow" | "ci" | "skip"])
        | (&Method::PUT, ["tasks", _, "files", _])
        | (&Method::POST, ["tasks", _, "agent"])
        | (&Method::POST, ["tasks", _, "agent", "message"]) => {
            with_body(request, state, route, by_agent).await
        }
        _ => not_found(method.as_str(), &url),
    }
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

/// The routes that read a JSON body.
async fn with_body(
    request: Request<Incoming>,
    state: Arc<AppState>,
    route: (Method, Vec<String>),
    by_agent: bool,
) -> Answer {
    let (method, parts) = route;
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let limit = if method == Method::POST && parts.as_slice() == ["tasks"] {
        tasks::new_task_body_limit()
    } else {
        DEFAULT_BODY_LIMIT
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
