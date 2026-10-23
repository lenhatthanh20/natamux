//! The HTTP server (`server.ts`): `/api` stripped, the token checked, then the route. Part 1
//! serves the terminal routes only; any other address is a 404 in Fastify's shape.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::json;
use tokio::net::TcpListener;

use super::auth::{bearer_token, is_authorized};
use super::errors::{Answer, json_answer, not_found, unauthorized};
use super::routes::terminals;
use super::terminal_socket;
use crate::terminals::open_shells::OpenShells;
use crate::terminals::service::TerminalService;

pub struct AppState {
    pub token: String,
    /// The token of the task shells.
    pub agent_token: String,
    pub data_dir: PathBuf,
    pub terminals: TerminalService,
    pub open_shells: Arc<OpenShells>,
    /// A socket that sent nothing for this long is dropped (`SILENT_CLOSE`; shorter in tests).
    pub silent_close: std::time::Duration,
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
    if !(method == Method::GET && parts.as_slice() == ["health"]) {
        // Browsers cannot set headers on a WebSocket: the terminal socket checks its one-time
        // ticket itself instead.
        let has_ticket = query
            .split('&')
            .any(|pair| pair.split('=').next() == Some("ticket"));
        let header = request
            .headers()
            .get(hyper::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        let provided = bearer_token(header);
        let allowed = (socket && has_ticket)
            || is_authorized(provided, &state.token)
            || is_authorized(provided, &state.agent_token);
        if !allowed {
            return unauthorized();
        }
    }
    match (&method, parts.as_slice()) {
        (&Method::GET, ["health"]) => json_answer(StatusCode::OK, &json!({ "status": "ok" })),
        (&Method::GET, ["tasks", id, "ended-shells"]) => terminals::ended_shells(&state, id),
        (&Method::GET, ["tasks", id, "terminals"]) => terminals::list_terminals(&state, id),
        (&Method::DELETE, ["tasks", id, "terminals", terminal]) => {
            terminals::close_terminal(&state, id, terminal)
        }
        (&Method::POST, ["tasks", id, "terminal", "ticket"]) => terminals::ticket(&state, id),
        (&Method::GET, ["tasks", id, "terminal"]) => {
            let id = id.to_string();
            terminal_socket::upgrade(request, state, id, query.to_string())
        }
        _ => not_found(method.as_str(), &url),
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
