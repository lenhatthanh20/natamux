//! The terminal socket (`handleTerminalSocket` in `routes/terminals.ts`). Output is binary,
//! control messages are JSON text, see `natamux_proto::terminals::messages`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_tungstenite::tokio::TokioAdapter;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::handshake::derive_accept_key;
use async_tungstenite::tungstenite::protocol::frame::CloseFrame;
use async_tungstenite::tungstenite::protocol::{Role, frame::coding::CloseCode};
use async_tungstenite::{WebSocketReceiver, WebSocketSender, WebSocketStream};
use futures::{SinkExt, StreamExt};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONNECTION, HeaderValue, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use natamux_proto::terminals::messages::{
    CLOSE_GONE, CLOSE_REFUSED, CLOSE_UNAUTHORIZED, ClientMessage, ServerMessage,
};
use natamux_proto::terminals::query::TerminalQuery;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::edge_rules;
use super::errors::{Answer, not_found};
use super::routes::terminals::task_id_issues;
use super::server::AppState;
use crate::terminals::limits::CLIENT_HIGH_WATER;
use crate::terminals::service::AttachOptions;
use crate::terminals::shells::Detach;
use crate::terminals::types::{ClientRef, Frame, Size, TerminalClient, TerminalError};

type Io = TokioAdapter<TokioIo<hyper::upgrade::Upgraded>>;
type Socket = WebSocketStream<Io>;
type SocketSink = WebSocketSender<Io>;
type SocketStream = WebSocketReceiver<Io>;

/// What the writer of a socket does next.
enum Out {
    /// The frame and the bytes it counts in the backlog.
    Frame(Frame, usize),
    Close(Option<(u16, &'static str)>),
    /// Drops the connection with no close frame: a silent peer may never answer one.
    Terminate,
}

/// A client that never reads may queue this much, then it is dropped. Acks cannot raise it: an
/// ack only moves the resend point, the queue is what the socket has not taken yet.
const MAX_QUEUED_BYTES: usize = 4 * CLIENT_HIGH_WATER as usize;

/// The bytes queued for one socket and not yet written.
#[derive(Default)]
struct Backlog {
    bytes: AtomicUsize,
    over: AtomicBool,
}

impl Backlog {
    /// False once the queue is over the cap, for this frame and every later one.
    fn add(&self, size: usize) -> bool {
        if self.over.load(Ordering::Relaxed) {
            return false;
        }
        if self.bytes.fetch_add(size, Ordering::Relaxed) + size > MAX_QUEUED_BYTES {
            self.over.store(true, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn written(&self, size: usize) {
        self.bytes.fetch_sub(size, Ordering::Relaxed);
    }

    fn is_over(&self) -> bool {
        self.over.load(Ordering::Relaxed)
    }
}

struct SocketClient {
    out: UnboundedSender<Out>,
    backlog: Arc<Backlog>,
}

impl TerminalClient for SocketClient {
    fn send(&self, frame: Frame) {
        let exit = matches!(
            frame,
            Frame::Message(ServerMessage::Exit { .. } | ServerMessage::Paused { .. })
        );
        let size = match &frame {
            Frame::Bytes(bytes) => bytes.len(),
            Frame::Message(message) => message.to_json().len(),
        };
        if !self.backlog.add(size) {
            let _ = self.out.send(Out::Terminate);
            return;
        }
        let _ = self.out.send(Out::Frame(frame, size));
        if exit {
            let _ = self.out.send(Out::Close(None));
        }
    }
}

pub fn upgrade(
    request: Request<Incoming>,
    state: Arc<AppState>,
    id: String,
    query: String,
) -> Answer {
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
        return not_found("GET", &format!("/tasks/{id}/terminal"));
    };
    tokio::spawn(async move {
        if let Ok(upgraded) = hyper::upgrade::on(request).await {
            let io = TokioAdapter::new(TokioIo::new(upgraded));
            let socket = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            run(socket, state, id, query).await;
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

async fn run(socket: Socket, state: Arc<AppState>, id: String, query: String) {
    let (sink, mut stream) = socket.split();
    let (out, outbox) = unbounded_channel::<Out>();
    let backlog = Arc::new(Backlog::default());
    let writer = tokio::spawn(write_frames(sink, outbox, Arc::clone(&backlog)));
    if let Some((detach, client, terminal)) = open(&state, &id, &query, &out, &backlog).await {
        read_messages(&mut stream, &state, &id, &terminal, &client, &out, &backlog).await;
        tokio::task::spawn_blocking(move || detach.detach());
    }
    drop(out);
    // A writer stuck on a peer that never reads would never see the queued Terminate.
    if backlog.is_over() {
        writer.abort();
    }
    let _ = writer.await;
}

/// Sends what the shell and the socket code queue, in order, until a close or a drop.
async fn write_frames(
    mut sink: SocketSink,
    mut outbox: UnboundedReceiver<Out>,
    backlog: Arc<Backlog>,
) {
    while let Some(next) = outbox.recv().await {
        let (message, size) = match next {
            Out::Frame(Frame::Bytes(bytes), size) => (Message::Binary(bytes), size),
            Out::Frame(Frame::Message(message), size) => {
                (Message::Text(message.to_json().into()), size)
            }
            Out::Close(code) => {
                let frame = code.map(|(code, reason)| CloseFrame {
                    code: CloseCode::from(code),
                    reason: reason.into(),
                });
                let _ = sink.send(Message::Close(frame)).await;
                break;
            }
            Out::Terminate => return,
        };
        if sink.send(message).await.is_err() {
            return;
        }
        backlog.written(size);
    }
    let _ = SinkExt::flush(&mut sink).await;
}

/// The ticket, then the attach. None when the socket was refused; the close is queued then.
async fn open(
    state: &Arc<AppState>,
    id: &str,
    query: &str,
    out: &UnboundedSender<Out>,
    backlog: &Arc<Backlog>,
) -> Option<(Detach, ClientRef, String)> {
    let query = TerminalQuery::parse(query);
    // The ticket is only spent once the query is right, as in Node.
    let redeemed = task_id_issues(id).is_empty()
        && query
            .as_ref()
            .is_some_and(|query| state.terminals.redeem_ticket(&query.ticket, id));
    let Some(query) = query.filter(|_| redeemed) else {
        let _ = out.send(Out::Close(Some((CLOSE_UNAUTHORIZED, "unauthorized"))));
        return None;
    };
    let client: ClientRef = Arc::new(SocketClient {
        out: out.clone(),
        backlog: Arc::clone(backlog),
    });
    let (attach_client, attach_state, task) =
        (Arc::clone(&client), Arc::clone(state), id.to_string());
    let terminal = query.terminal.clone();
    let attached = tokio::task::spawn_blocking(move || {
        // A client that lost its socket only resumes a shell that runs, never one that waits.
        // Any other attach opens the task: all its agents come back.
        if query.shell_id.is_none() {
            let run = &attach_state.stages.run;
            run.restore_task(&task, &query.terminal, query.visible);
        }
        let options = AttachOptions {
            visible: Some(query.visible),
            shell_id: query.shell_id,
            offset: query.offset,
        };
        let size = Size {
            cols: query.cols,
            rows: query.rows,
        };
        attach_state
            .terminals
            .attach(&task, attach_client, size, &query.terminal, options)
    })
    .await;
    match attached {
        Ok(Ok(detach)) => Some((detach, client, terminal)),
        Ok(Err(TerminalError::ShellGone { .. })) => {
            let _ = out.send(Out::Close(Some((CLOSE_GONE, "gone"))));
            None
        }
        Ok(Err(error)) => {
            let message = ServerMessage::Error {
                message: error.to_string(),
            };
            let _ = out.send(Out::Frame(Frame::Message(message), 0));
            let _ = out.send(Out::Close(Some((CLOSE_REFUSED, "refused"))));
            None
        }
        Err(_) => None,
    }
}

/// Reads the client's messages until it closes, or sends nothing for the silence limit: a
/// client gone or asleep is dropped with no close frame, as a silent peer may never answer one.
/// Only text and binary frames count, as Node's `message` event; protocol pings do not.
async fn read_messages(
    stream: &mut SocketStream,
    state: &AppState,
    id: &str,
    terminal: &str,
    client: &ClientRef,
    out: &UnboundedSender<Out>,
    backlog: &Backlog,
) {
    let mut deadline = tokio::time::Instant::now() + state.silent_close;
    loop {
        if backlog.is_over() {
            eprintln!("terminal socket {id}/{terminal} dropped: it does not read its output");
            return;
        }
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Err(_) => {
                let silent = state.silent_close.as_secs_f64();
                eprintln!("terminal socket {id}/{terminal} dropped: silent for {silent} s");
                let _ = out.send(Out::Terminate);
                return;
            }
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return,
            Ok(Some(Ok(Message::Text(text)))) => {
                deadline = tokio::time::Instant::now() + state.silent_close;
                handle_message(state, id, terminal, client, text.as_str());
            }
            // The dashboard sends only JSON: a binary frame is heard, and ignored.
            Ok(Some(Ok(Message::Binary(_)))) => {
                deadline = tokio::time::Instant::now() + state.silent_close;
            }
            Ok(Some(Ok(_))) => {}
        }
    }
}

fn handle_message(state: &AppState, id: &str, terminal: &str, client: &ClientRef, text: &str) {
    let Ok(message) = serde_json::from_str::<ClientMessage>(text) else {
        return;
    };
    if !message.is_valid() {
        return;
    }
    let terminals = &state.terminals;
    match message {
        ClientMessage::Input { data } => terminals.input(id, terminal, Some(client), &data),
        ClientMessage::Resize { cols, rows } => {
            terminals.resize(id, terminal, Some(client), Size { cols, rows });
        }
        ClientMessage::Visible {
            visible,
            cols,
            rows,
            park,
        } => {
            let size = cols.zip(rows).map(|(cols, rows)| Size { cols, rows });
            terminals.visible(id, terminal, client, visible, size, park);
        }
        ClientMessage::Ack { offset } => terminals.ack(id, terminal, client, offset),
        ClientMessage::Ping => client.send(Frame::Message(ServerMessage::Pong)),
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    fn client() -> (SocketClient, UnboundedReceiver<Out>) {
        let (out, outbox) = unbounded_channel();
        let backlog = Arc::default();
        (SocketClient { out, backlog }, outbox)
    }

    fn bytes(size: usize) -> Frame {
        Frame::Bytes(Bytes::from(vec![0; size]))
    }

    #[test]
    fn queues_frames_while_the_backlog_is_under_the_cap() {
        let (client, mut outbox) = client();
        client.send(bytes(MAX_QUEUED_BYTES));
        assert!(matches!(outbox.try_recv(), Ok(Out::Frame(_, size)) if size == MAX_QUEUED_BYTES));
        assert!(!client.backlog.is_over());
    }

    #[test]
    fn terminates_a_client_that_is_over_the_cap() {
        let (client, mut outbox) = client();
        client.send(bytes(MAX_QUEUED_BYTES));
        client.send(bytes(1));
        assert!(matches!(outbox.try_recv(), Ok(Out::Frame(..))));
        assert!(matches!(outbox.try_recv(), Ok(Out::Terminate)));
        assert!(client.backlog.is_over());
        // Nothing more is queued for it.
        client.send(bytes(1));
        assert!(matches!(outbox.try_recv(), Ok(Out::Terminate)));
        assert!(outbox.try_recv().is_err());
    }

    #[test]
    fn written_frames_leave_the_backlog() {
        let (client, _outbox) = client();
        client.send(bytes(MAX_QUEUED_BYTES));
        client.backlog.written(MAX_QUEUED_BYTES);
        assert!(client.backlog.add(MAX_QUEUED_BYTES));
    }
}
