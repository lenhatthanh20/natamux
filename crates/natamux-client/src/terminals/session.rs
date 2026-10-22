//! One terminal socket and its reconnects, on the client's runtime. The rules live in
//! `protocol.rs`; this file only moves bytes, keeps the timers and tells the app.

use std::sync::Arc;
use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
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

/// What the app asks of a terminal.
#[derive(Debug)]
enum Command {
    Input(String),
    Resize(u16, u16),
    Visible(bool),
    Parsed(u64),
    CaughtUp(u64),
    Reconnect,
}

/// The app's end of a terminal. Dropping it closes the socket; the shell keeps running.
pub struct TerminalHandle {
    commands: mpsc::UnboundedSender<Command>,
    pub events: UnboundedReceiver<TerminalEvent>,
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

    /// A handle with no socket: the app shows `events`, and what it sends goes nowhere. For the
    /// app's screenshots in CI, which run with no daemon.
    pub fn detached(events: UnboundedReceiver<TerminalEvent>) -> TerminalHandle {
        let (commands, _) = mpsc::unbounded_channel();
        TerminalHandle { commands, events }
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
    runtime.spawn(run(config, options, command_rx, event_tx));
    TerminalHandle { commands, events }
}

enum Outcome {
    /// The socket is gone and the shell may still run: try again.
    Lost,
    /// The session is over, or the app let go of it.
    Done,
}

async fn run(
    config: Arc<Config>,
    options: SessionOptions,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: UnboundedSender<TerminalEvent>,
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
                )
                .await
                {
                    Outcome::Done => return,
                    Outcome::Lost => {}
                }
            }
            Err(reason) if !protocol.ever_opened() => {
                emit(TerminalEvent::Status(TerminalStatus::Failed(reason)));
                return;
            }
            Err(_) => {}
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

async fn drive(
    socket: WebSocketStream<ConnectStream>,
    first: Step,
    heartbeat: Duration,
    protocol: &mut Protocol,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &UnboundedSender<TerminalEvent>,
) -> Outcome {
    let (mut sink, mut stream) = socket.split();
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
                return lost(protocol, None, events);
            }
        }
        if protocol.ended() {
            let _ = sink.close(None).await;
            return Outcome::Done;
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
                    step.emit.push(protocol.output(bytes.to_vec()));
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
                    return lost(protocol, frame.map(|frame| u16::from(frame.code)), events);
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return lost(protocol, None, events),
            },
            command = commands.recv() => match command {
                Some(Command::Reconnect) => return lost(protocol, None, events),
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
                return lost(protocol, None, events);
            }
            _ = sleep_until(catch_up_deadline.unwrap_or(far)), if catch_up_deadline.is_some() => {
                catch_up_deadline = None;
                step = protocol.catch_up_timed_out();
            }
        }
    }
}

fn apply(protocol: &mut Protocol, command: Command) -> Step {
    match command {
        Command::Input(data) => protocol.input(&data),
        Command::Resize(cols, rows) => protocol.resize(cols, rows),
        Command::Visible(visible) => protocol.set_visible(visible),
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
        AfterClose::Stop(status) => {
            let _ = events.unbounded_send(TerminalEvent::Status(status));
            Outcome::Done
        }
    }
}
