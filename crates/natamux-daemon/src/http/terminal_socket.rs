//! The terminal socket (`handleTerminalSocket` in `routes/terminals.ts`). Output is binary,
//! control messages are JSON text, see `natamux_proto::terminals::messages`.

use std::sync::Arc;

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

use super::errors::{Answer, not_found};
use super::routes::terminals::task_id_issues;
use super::server::AppState;
use crate::terminals::service::AttachOptions;
use crate::terminals::shells::Detach;
use crate::terminals::types::{ClientRef, Frame, Size, TerminalClient, TerminalError};

type Io = TokioAdapter<TokioIo<hyper::upgrade::Upgraded>>;
type Socket = WebSocketStream<Io>;
type SocketSink = WebSocketSender<Io>;
type SocketStream = WebSocketReceiver<Io>;

/// What the writer of a socket does next.
enum Out {
    Frame(Frame),
    Close(Option<(u16, &'static str)>),
    /// Drops the connection with no close frame: a silent peer may never answer one.
    Terminate,
}

struct SocketClient {
    out: UnboundedSender<Out>,
}

impl TerminalClient for SocketClient {
    fn send(&self, frame: Frame) {
        let exit = matches!(frame, Frame::Message(ServerMessage::Exit { .. }));
        let _ = self.out.send(Out::Frame(frame));
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
    let writer = tokio::spawn(write_frames(sink, outbox));
    if let Some((detach, client, terminal)) = open(&state, &id, &query, &out).await {
        read_messages(&mut stream, &state, &id, &terminal, &client, &out).await;
        tokio::task::spawn_blocking(move || detach.detach());
    }
    drop(out);
    let _ = writer.await;
}

/// Sends what the shell and the socket code queue, in order, until a close or a drop.
async fn write_frames(mut sink: SocketSink, mut outbox: UnboundedReceiver<Out>) {
    while let Some(next) = outbox.recv().await {
        let message = match next {
            Out::Frame(Frame::Bytes(bytes)) => Message::Binary(Bytes::from_owner(bytes)),
            Out::Frame(Frame::Message(message)) => Message::Text(message.to_json().into()),
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
    }
    let _ = SinkExt::flush(&mut sink).await;
}

/// The ticket, then the attach. None when the socket was refused; the close is queued then.
async fn open(
    state: &Arc<AppState>,
    id: &str,
    query: &str,
    out: &UnboundedSender<Out>,
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
    let client: ClientRef = Arc::new(SocketClient { out: out.clone() });
    let (attach_client, attach_state, task) =
        (Arc::clone(&client), Arc::clone(state), id.to_string());
    let terminal = query.terminal.clone();
    let attached = tokio::task::spawn_blocking(move || {
        // A client that lost its socket only resumes a shell that runs, never one that waits.
        // Any other attach opens the task: all its agents come back.
        if query.shell_id.is_none() {
            attach_state.stages.run.restore_task(&task, &query.terminal);
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
            let _ = out.send(Out::Frame(Frame::Message(message)));
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
) {
    let mut deadline = tokio::time::Instant::now() + state.silent_close;
    loop {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Err(_) => {
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
        } => {
            let size = cols.zip(rows).map(|(cols, rows)| Size { cols, rows });
            terminals.visible(id, terminal, client, visible, size);
        }
        ClientMessage::Ack { offset } => terminals.ack(id, terminal, client, offset),
        ClientMessage::Ping => client.send(Frame::Message(ServerMessage::Pong)),
    }
}
