//! `/events` (`event-socket.ts`): the messages of the bus as JSON text, one socket per dashboard.
//! A dashboard that is too far behind (a sleeping laptop, a slow link) does not make the daemon
//! hold every message for it: above `EVENTS_HIGH_WATER` queued bytes it skips messages, and gets
//! one `resync` once it drained.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::handshake::derive_accept_key;
use async_tungstenite::tungstenite::protocol::frame::CloseFrame;
use async_tungstenite::tungstenite::protocol::{Role, frame::coding::CloseCode};
use futures::StreamExt;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONNECTION, HeaderValue, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use natamux_proto::terminals::messages::CLOSE_UNAUTHORIZED;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::sync::oneshot;

use super::agent_rules::Caller;
use super::edge_rules;
use super::errors::{Answer, not_found};
use super::server::AppState;
use crate::shared::event_bus::{ClientScope, EventBus};
use crate::shared::scheduler::{Scheduler, TimerId};

pub const EVENTS_HIGH_WATER: usize = 1024 * 1024;
pub const DRAIN_CHECK_MS: u64 = 250;
const RESYNC: &str = r#"{"kind":"resync"}"#;
/// How long a revoked socket gets to take its close frame.
const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// The part of a WebSocket that /events uses.
pub trait EventSocket: Send + Sync {
    /// Bytes queued and not yet written.
    fn buffered(&self) -> usize;
    fn is_open(&self) -> bool;
    fn send(&self, text: &str);
}

/// Sends the messages of the bus to one socket (`eventSender`).
#[derive(Clone)]
pub struct EventSender {
    socket: Arc<dyn EventSocket>,
    scheduler: Arc<Scheduler>,
    timer: Arc<Mutex<Option<TimerId>>>,
}

impl EventSender {
    pub fn new(socket: Arc<dyn EventSocket>, scheduler: Arc<Scheduler>) -> EventSender {
        EventSender {
            socket,
            scheduler,
            timer: Arc::default(),
        }
    }

    pub fn send(&self, text: &str) {
        let skipping = self.timer.lock().map_or(true, |timer| timer.is_some());
        if !self.socket.is_open() || skipping {
            return;
        }
        let queued = self.socket.buffered();
        if queued <= EVENTS_HIGH_WATER {
            self.socket.send(text);
            return;
        }
        eprintln!(
            "events: a dashboard is behind ({queued} bytes queued), skipping until it drains"
        );
        self.check_later();
    }

    fn check_later(&self) {
        let sender = self.clone();
        let timer = self.scheduler.after(DRAIN_CHECK_MS, move || sender.check());
        if let Ok(mut slot) = self.timer.lock() {
            *slot = Some(timer);
        }
    }

    fn check(&self) {
        if !self.socket.is_open() {
            self.clear();
        } else if self.socket.buffered() <= EVENTS_HIGH_WATER {
            self.clear();
            eprintln!("events: the dashboard drained, sending resync");
            self.socket.send(RESYNC);
        } else {
            self.check_later();
        }
    }

    fn clear(&self) {
        if let Ok(mut slot) = self.timer.lock()
            && let Some(timer) = slot.take()
        {
            self.scheduler.cancel(timer);
        }
    }

    /// The dashboard left: no more checks.
    pub fn stop(&self) {
        self.clear();
    }

    /// Whether a drain check waits.
    pub fn is_checking(&self) -> bool {
        self.timer.lock().is_ok_and(|timer| timer.is_some())
    }
}

/// A socket whose writer counts the bytes it has not written yet.
struct QueuedSocket {
    out: tokio::sync::mpsc::UnboundedSender<String>,
    queued: Arc<AtomicUsize>,
    open: Arc<AtomicBool>,
}

impl EventSocket for QueuedSocket {
    fn buffered(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    fn send(&self, text: &str) {
        self.queued.fetch_add(text.len(), Ordering::SeqCst);
        let _ = self.out.send(text.to_string());
    }
}

/// An agent token gets only the messages of its own task and its direct subtasks (F6).
pub fn upgrade(request: Request<Incoming>, state: Arc<AppState>, who: &Caller) -> Answer {
    if let Some(refused) = edge_rules::check_origin(request.headers(), &state.allowed_hosts) {
        return refused;
    }
    let key = request.headers().get(SEC_WEBSOCKET_KEY).cloned();
    let wants_socket = request
        .headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let (true, Some(key)) = (wants_socket, key) else {
        return not_found("GET", "/events");
    };
    let scope = match who {
        Caller::Agent(own) => ClientScope::Task(own.clone()),
        _ => ClientScope::All,
    };
    tokio::spawn(async move {
        if let Ok(upgraded) = hyper::upgrade::on(request).await {
            let io = TokioAdapter::new(TokioIo::new(upgraded));
            let socket = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            run(socket, state, scope).await;
        }
    });
    let mut answer = Response::new(Full::new(Bytes::new()));
    *answer.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    let headers = answer.headers_mut();
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(accept) = HeaderValue::from_str(&derive_accept_key(key.as_bytes())) {
        headers.insert(SEC_WEBSOCKET_ACCEPT, accept);
    }
    answer
}

/// Ends a socket that an agent token opened once that token is revoked (`EventBus::end_agent`).
/// Dropped, it leaves the bus.
pub(super) struct AgentEnd {
    bus: Arc<EventBus>,
    hook: Option<u64>,
    ended: Option<oneshot::Receiver<()>>,
}

impl AgentEnd {
    /// `agent`: the task whose token opened the socket; None: a person's socket.
    pub(super) fn new(bus: &Arc<EventBus>, agent: Option<&str>) -> AgentEnd {
        let mut end = AgentEnd {
            bus: Arc::clone(bus),
            hook: None,
            ended: None,
        };
        if let Some(agent) = agent {
            let (revoked, ended) = oneshot::channel();
            end.hook = Some(bus.on_agent_end(agent, move || {
                let _ = revoked.send(());
            }));
            end.ended = Some(ended);
        }
        end
    }

    /// Returns once the token is revoked; never for a person's socket.
    pub(super) async fn revoked(&mut self) {
        match self.ended.as_mut() {
            Some(ended) => {
                let _ = ended.await;
            }
            None => std::future::pending().await,
        }
    }
}

impl Drop for AgentEnd {
    fn drop(&mut self) {
        if let Some(hook) = self.hook {
            self.bus.unsubscribe(hook);
        }
    }
}

type Io = TokioAdapter<TokioIo<hyper::upgrade::Upgraded>>;

async fn run(socket: WebSocketStream<Io>, state: Arc<AppState>, scope: ClientScope) {
    // Before the subscription, so a revoke in between still ends the socket.
    let agent = match &scope {
        ClientScope::Task(own) => Some(own.clone()),
        ClientScope::All => None,
    };
    let mut end = AgentEnd::new(&state.bus, agent.as_deref());
    let (mut sink, mut stream) = socket.split();
    let (out, mut outbox): (_, UnboundedReceiver<String>) = unbounded_channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let open = Arc::new(AtomicBool::new(true));
    let socket: Arc<dyn EventSocket> = Arc::new(QueuedSocket {
        out,
        queued: Arc::clone(&queued),
        open: Arc::clone(&open),
    });
    let sender = EventSender::new(socket, Arc::clone(&state.scheduler));
    let listener = sender.clone();
    let id = state
        .bus
        .subscribe_client(scope, move |text| listener.send(text));
    // The pull request statuses may be old: nobody looked while no dashboard listened. Returns at
    // once; the watcher's own thread asks GitHub.
    if state.bus.size() == 1 {
        state.pull_requests.wake();
    }
    let writing = Arc::clone(&queued);
    let (close, mut closing) = oneshot::channel::<()>();
    let mut writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                text = outbox.recv() => {
                    let Some(text) = text else { break };
                    let len = text.len();
                    let sent = sink.send(Message::Text(text.into())).await;
                    writing.fetch_sub(len, Ordering::SeqCst);
                    if sent.is_err() {
                        break;
                    }
                }
                _ = &mut closing => {
                    // A close frame: a dropped connection alone does not reach every client.
                    let frame = CloseFrame {
                        code: CloseCode::from(CLOSE_UNAUTHORIZED),
                        reason: "unauthorized".into(),
                    };
                    let _ = sink.send(Message::Close(Some(frame))).await;
                    break;
                }
            }
        }
    });
    // The dashboard sends nothing that matters; the socket lives until it closes, or until the
    // agent token that opened it is revoked.
    let revoked = loop {
        tokio::select! {
            message = stream.next() => {
                if matches!(message, None | Some(Err(_) | Ok(Message::Close(_)))) {
                    break false;
                }
            }
            () = end.revoked() => break true,
        }
    };
    open.store(false, Ordering::SeqCst);
    state.bus.unsubscribe(id);
    sender.stop();
    // A peer that stops reading keeps the writer in `send`: it must not outlive the wait.
    if revoked && close.send(()).is_ok() {
        if tokio::time::timeout(CLOSE_WAIT, &mut writer).await.is_err() {
            writer.abort();
        }
    } else {
        writer.abort();
    }
}

#[cfg(test)]
#[path = "event_socket_tests.rs"]
mod tests;
