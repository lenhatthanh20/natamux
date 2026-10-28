//! One terminal socket and its reconnects, on the client's runtime. The rules live in
//! `protocol.rs`; this file only moves bytes, keeps the timers and tells the app.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::stream::Peekable;
use futures::{FutureExt, Stream, StreamExt};
use natamux_proto::terminals::messages::{
    CATCH_UP_TIMEOUT, ClientMessage, HEARTBEAT, ServerMessage,
};
use natamux_proto::terminals::query::encode;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use super::backoff::reconnect_delay;
use super::protocol::{AfterClose, Protocol, Step, TerminalEvent, TerminalStatus};
use crate::config::Config;
use crate::terminals::api;

/// The most output joined into one event: frames that already wait in the socket go to the app
/// together, so a busy shell wakes it once per burst, not once per frame.
const JOIN_BYTES: usize = 256 * 1024;

/// Why the last socket was lost, for the app's log.
type Loss = Arc<Mutex<Option<String>>>;

/// What the app asks of a terminal.
#[derive(Debug)]
enum Command {
    Input(String),
    Resize(u16, u16),
    Visible(bool),
    Claim,
    Parsed(u64),
    CaughtUp(u64),
    Reconnect,
}

/// The app's end of a terminal. Dropping it closes the socket; the shell keeps running.
pub struct TerminalHandle {
    commands: mpsc::UnboundedSender<Command>,
    pub events: UnboundedReceiver<TerminalEvent>,
    loss: Loss,
}

impl TerminalHandle {
    pub fn input(&self, data: impl Into<String>) {
        self.send(Command::Input(data.into()));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.send(Command::Resize(cols, rows));
    }

    pub fn set_visible(&self, visible: bool) {
        self.send(Command::Visible(visible));
    }

    /// The terminal took the focus: it takes the shell's size.
    pub fn claim(&self) {
        self.send(Command::Claim);
    }

    /// The screen parsed output up to `offset` (the `end` of an Output event).
    pub fn parsed(&self, offset: u64) {
        self.send(Command::Parsed(offset));
    }

    /// The screen parsed the whole catch-up (a CaughtUp event).
    pub fn caught_up(&self, offset: u64) {
        self.send(Command::CaughtUp(offset));
    }

    /// Drops the socket and resumes the shell on a new one, as after a network loss.
    pub fn reconnect(&self) {
        self.send(Command::Reconnect);
    }

    /// Why the socket was lost last, once a `Reconnecting` status came.
    pub fn last_loss(&self) -> Option<String> {
        self.loss
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A handle with no socket: the app shows `events`, and what it sends goes nowhere. For the
    /// app's screenshots in CI, which run with no daemon.
    pub fn detached(events: UnboundedReceiver<TerminalEvent>) -> TerminalHandle {
        let (commands, _) = mpsc::unbounded_channel();
        TerminalHandle {
            commands,
            events,
            loss: Loss::default(),
        }
    }

    fn send(&self, command: Command) {
        // Fails only once the session ended, when there is nothing left to tell.
        let _ = self.commands.send(command);
    }
}

pub struct SessionOptions {
    pub task_id: String,
    pub terminal: String,
    pub cols: u16,
    pub rows: u16,
    pub visible: bool,
    /// HEARTBEAT, except in tests that cannot wait 30 s for a stale socket.
    pub heartbeat: Duration,
}

impl SessionOptions {
    pub fn new(task_id: &str, terminal: &str, cols: u16, rows: u16, visible: bool) -> Self {
        SessionOptions {
            task_id: task_id.to_string(),
            terminal: terminal.to_string(),
            cols,
            rows,
            visible,
            heartbeat: HEARTBEAT,
        }
    }
}

pub fn open(
    runtime: &tokio::runtime::Handle,
    config: Arc<Config>,
    options: SessionOptions,
) -> TerminalHandle {
    let (commands, command_rx) = mpsc::unbounded_channel();
    let (event_tx, events) = unbounded();
    let loss = Loss::default();
    runtime.spawn(run(config, options, command_rx, event_tx, loss.clone()));
    TerminalHandle {
        commands,
        events,
        loss,
    }
}

fn note_loss(loss: &Loss, reason: impl Into<String>) {
    *loss.lock().unwrap_or_else(PoisonError::into_inner) = Some(reason.into());
}

enum Outcome {
    /// The socket is gone and the shell may still run: try again.
    Lost,
    /// The shell paused: connect again once the terminal is shown or typed into.
    Paused,
    /// The session is over, or the app let go of it.
    Done,
}

async fn run(
    config: Arc<Config>,
    options: SessionOptions,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: UnboundedSender<TerminalEvent>,
    loss: Loss,
) {
    let emit = |event: TerminalEvent| {
        let _ = events.unbounded_send(event);
    };
    let mut protocol = Protocol::new(
        &options.terminal,
        options.cols,
        options.rows,
        options.visible,
    );
    emit(TerminalEvent::Status(TerminalStatus::Connecting));
    loop {
        match connect(&config, &options.task_id, &mut protocol).await {
            Ok(socket) => {
                let step = protocol.opened();
                let heartbeat = options.heartbeat;
                match drive(
                    socket,
                    step,
                    heartbeat,
                    &mut protocol,
                    &mut commands,
                    &events,
                    &loss,
                )
                .await
                {
                    Outcome::Done => return,
                    Outcome::Lost => {}
                    Outcome::Paused => {
                        if !wake(&mut protocol, &mut commands).await {
                            return;
                        }
                        emit(TerminalEvent::Status(TerminalStatus::Connecting));
                        continue;
                    }
                }
            }
            Err(reason) if !protocol.ever_opened() => {
                emit(TerminalEvent::Status(TerminalStatus::Failed(reason)));
                return;
            }
            Err(reason) => note_loss(&loss, reason),
        }
        emit(TerminalEvent::Status(TerminalStatus::Reconnecting));
        let delay = reconnect_delay(protocol.next_try(), fastrand::f64());
        if !wait(delay, &mut protocol, &mut commands).await {
            return;
        }
    }
}

async fn connect(
    config: &Config,
    task_id: &str,
    protocol: &mut Protocol,
) -> Result<WebSocketStream<ConnectStream>, String> {
    // A new ticket for every socket: each is good for one use and 30 s.
    let ticket = api::terminal_ticket(config, task_id)
        .await
        .map_err(|error| error.to_string())?;
    let query = protocol.query(ticket);
    let url = config
        .ws_url(&format!(
            "/tasks/{}/terminal?{}",
            encode(task_id),
            query.to_query_string()
        ))
        .map_err(|error| error.to_string())?;
    let (socket, _) = connect_async(url.as_str())
        .await
        .map_err(|error| format!("Could not connect to the terminal: {error}"))?;
    Ok(socket)
}

/// Waits before the next try. Commands keep coming meanwhile: a resize or a visibility change
/// goes into the next socket's query. False when the app let go.
async fn wait(
    delay: Duration,
    protocol: &mut Protocol,
    commands: &mut mpsc::UnboundedReceiver<Command>,
) -> bool {
    let until = Instant::now() + delay;
    loop {
        tokio::select! {
            _ = sleep_until(until) => return true,
            command = commands.recv() => match command {
                Some(command) => {
                    apply(protocol, command);
                }
                None => return false,
            },
        }
    }
}

/// Waits while the shell is paused. Commands keep coming meanwhile; showing the terminal or
/// typing into it ends the wait. False when the app let go.
async fn wake(protocol: &mut Protocol, commands: &mut mpsc::UnboundedReceiver<Command>) -> bool {
    while protocol.paused() {
        match commands.recv().await {
            Some(command) => {
                apply(protocol, command);
            }
            None => return false,
        }
    }
    true
}

async fn drive(
    socket: WebSocketStream<ConnectStream>,
    first: Step,
    heartbeat: Duration,
    protocol: &mut Protocol,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &UnboundedSender<TerminalEvent>,
    loss: &Loss,
) -> Outcome {
    let (mut sink, stream) = socket.split();
    let mut stream = stream.peekable();
    let mut ping_at = Instant::now() + heartbeat;
    // No pong for two heartbeats: the connection is dead even if the OS has not noticed yet.
    let mut pong_deadline: Option<Instant> = None;
    let mut catch_up_deadline: Option<Instant> = None;
    let mut step = first;
    loop {
        for event in step.emit.drain(..) {
            let _ = events.unbounded_send(event);
        }
        for message in step.send.drain(..) {
            if sink.send(Message::text(message.to_json())).await.is_err() {
                note_loss(loss, "could not send to the socket");
                return lost(protocol, None, events);
            }
        }
        if protocol.ended() {
            let _ = sink.close(None).await;
            return Outcome::Done;
        }
        if protocol.paused() {
            let _ = sink.close(None).await;
            return Outcome::Paused;
        }
        catch_up_deadline = match (protocol.in_catch_up(), catch_up_deadline) {
            (true, None) => Some(Instant::now() + CATCH_UP_TIMEOUT),
            (true, deadline) => deadline,
            (false, _) => None,
        };
        let far = Instant::now() + Duration::from_secs(3600);
        tokio::select! {
            frame = stream.next() => match frame {
                Some(Ok(Message::Binary(bytes))) => {
                    let joined = join_binary(bytes.to_vec(), &mut stream);
                    step.emit.push(protocol.output(joined));
                }
                Some(Ok(Message::Text(text))) => {
                    if let Some(message) = ServerMessage::parse(text.as_str()) {
                        if message == ServerMessage::Pong {
                            pong_deadline = None;
                        }
                        step = protocol.server(message);
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    let code = frame.map(|frame| u16::from(frame.code));
                    note_loss(loss, format!("the daemon closed the socket (code {code:?})"));
                    return lost(protocol, code, events);
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    note_loss(loss, format!("socket error: {error}"));
                    return lost(protocol, None, events);
                }
                None => {
                    note_loss(loss, "the socket ended");
                    return lost(protocol, None, events);
                }
            },
            command = commands.recv() => match command {
                Some(Command::Reconnect) => {
                    note_loss(loss, "the app asked for a new socket");
                    return lost(protocol, None, events);
                }
                Some(command) => step = apply(protocol, command),
                None => {
                    let _ = sink.close(None).await;
                    return Outcome::Done;
                }
            },
            _ = sleep_until(ping_at) => {
                ping_at = Instant::now() + heartbeat;
                pong_deadline.get_or_insert(Instant::now() + 2 * heartbeat);
                step.send.push(ClientMessage::Ping);
            }
            _ = sleep_until(pong_deadline.unwrap_or(far)), if pong_deadline.is_some() => {
                note_loss(loss, "no pong for two heartbeats");
                return lost(protocol, None, events);
            }
            _ = sleep_until(catch_up_deadline.unwrap_or(far)), if catch_up_deadline.is_some() => {
                catch_up_deadline = None;
                step = protocol.catch_up_timed_out();
            }
        }
    }
}

/// Adds the binary frames that already wait in the stream to `joined`, up to `JOIN_BYTES`. Only
/// binary frames join: a control message keeps its place between them, for the next select.
fn join_binary<S, E>(mut joined: Vec<u8>, stream: &mut Peekable<S>) -> Vec<u8>
where
    S: Stream<Item = Result<Message, E>> + Unpin,
{
    while joined.len() < JOIN_BYTES
        && matches!(
            std::pin::Pin::new(&mut *stream).peek().now_or_never(),
            Some(Some(Ok(Message::Binary(_))))
        )
    {
        if let Some(Some(Ok(Message::Binary(more)))) = stream.next().now_or_never() {
            joined.extend_from_slice(&more);
        }
    }
    joined
}

fn apply(protocol: &mut Protocol, command: Command) -> Step {
    match command {
        Command::Input(data) => protocol.input(&data),
        Command::Resize(cols, rows) => protocol.resize(cols, rows),
        Command::Visible(visible) => protocol.set_visible(visible),
        Command::Claim => protocol.claim_size(),
        Command::Parsed(offset) => protocol.parsed(offset),
        Command::CaughtUp(offset) => protocol.caught_up(offset),
        // Only a socket can be dropped; while waiting there is none.
        Command::Reconnect => Step::default(),
    }
}

fn lost(
    protocol: &mut Protocol,
    code: Option<u16>,
    events: &UnboundedSender<TerminalEvent>,
) -> Outcome {
    match protocol.closed(code) {
        AfterClose::Retry => Outcome::Lost,
        AfterClose::Sleep => Outcome::Paused,
        AfterClose::Stop(status) => {
            let _ = events.unbounded_send(TerminalEvent::Status(status));
            Outcome::Done
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Frames = Peekable<futures::stream::Iter<std::vec::IntoIter<Result<Message, ()>>>>;

    fn frames(items: Vec<Message>) -> Frames {
        futures::stream::iter(items.into_iter().map(Ok).collect::<Vec<_>>()).peekable()
    }

    fn next(stream: &mut Frames) -> Option<Message> {
        match stream.next().now_or_never() {
            Some(Some(Ok(message))) => Some(message),
            _ => None,
        }
    }

    #[test]
    fn ready_binary_frames_join_into_one() {
        let mut stream = frames(vec![
            Message::binary(b"b".to_vec()),
            Message::binary(b"c".to_vec()),
        ]);
        assert_eq!(join_binary(b"a".to_vec(), &mut stream), b"abc");
        assert_eq!(next(&mut stream), None);
    }

    #[test]
    fn a_text_frame_ends_the_join_and_keeps_its_place() {
        let mut stream = frames(vec![
            Message::binary(b"b".to_vec()),
            Message::text("control"),
            Message::binary(b"c".to_vec()),
        ]);
        assert_eq!(join_binary(b"a".to_vec(), &mut stream), b"ab");
        assert_eq!(next(&mut stream), Some(Message::text("control")));
        assert_eq!(next(&mut stream), Some(Message::binary(b"c".to_vec())));
    }

    #[test]
    fn the_join_stops_once_it_has_256_kib() {
        let part = vec![1u8; 100 * 1024];
        let mut stream = frames(vec![Message::binary(part.clone()); 3]);
        let joined = join_binary(part.clone(), &mut stream);
        assert_eq!(joined.len(), 300 * 1024);
        assert_eq!(next(&mut stream), Some(Message::binary(part)));
        assert_eq!(next(&mut stream), None);
    }

    #[test]
    fn a_single_frame_over_the_cap_passes_whole_and_joins_nothing() {
        let big = vec![2u8; JOIN_BYTES * 2];
        let mut stream = frames(vec![Message::binary(b"x".to_vec())]);
        assert_eq!(join_binary(big.clone(), &mut stream), big);
        assert_eq!(next(&mut stream), Some(Message::binary(b"x".to_vec())));
    }
}
