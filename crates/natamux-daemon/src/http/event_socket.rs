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
use async_tungstenite::tungstenite::protocol::Role;
use futures::StreamExt;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONNECTION, HeaderValue, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::errors::{Answer, not_found};
use super::server::AppState;
use crate::shared::scheduler::{Scheduler, TimerId};

pub const EVENTS_HIGH_WATER: usize = 1024 * 1024;
pub const DRAIN_CHECK_MS: u64 = 250;
const RESYNC: &str = r#"{"kind":"resync"}"#;

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
        if self.socket.buffered() <= EVENTS_HIGH_WATER {
            self.socket.send(text);
            return;
        }
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

pub fn upgrade(request: Request<Incoming>, state: Arc<AppState>) -> Answer {
    let key = request.headers().get(SEC_WEBSOCKET_KEY).cloned();
    let wants_socket = request
        .headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let (true, Some(key)) = (wants_socket, key) else {
        return not_found("GET", "/events");
    };
    tokio::spawn(async move {
        if let Ok(upgraded) = hyper::upgrade::on(request).await {
            let io = TokioAdapter::new(TokioIo::new(upgraded));
            let socket = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            run(socket, state).await;
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

type Io = TokioAdapter<TokioIo<hyper::upgrade::Upgraded>>;

async fn run(socket: WebSocketStream<Io>, state: Arc<AppState>) {
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
    let id = state.bus.subscribe_client(move |text| listener.send(text));
    let writing = Arc::clone(&queued);
    let writer = tokio::spawn(async move {
        while let Some(text) = outbox.recv().await {
            let len = text.len();
            let sent = sink.send(Message::Text(text.into())).await;
            writing.fetch_sub(len, Ordering::SeqCst);
            if sent.is_err() {
                break;
            }
        }
    });
    // The dashboard sends nothing that matters; the socket lives until it closes.
    while let Some(message) = stream.next().await {
        if matches!(message, Err(_) | Ok(Message::Close(_))) {
            break;
        }
    }
    open.store(false, Ordering::SeqCst);
    state.bus.unsubscribe(id);
    sender.stop();
    writer.abort();
}

#[cfg(test)]
#[path = "event_socket_tests.rs"]
mod tests;
