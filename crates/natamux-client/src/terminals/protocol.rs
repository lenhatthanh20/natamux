//! The client side of the terminal protocol, with no IO: what to send and what to tell the app,
//! for each thing that happens. Same rules as the web client
//! (packages/web/src/features/terminals/session/connection.ts and catch-up.ts).

use natamux_proto::terminals::messages::{
    ACK_EVERY_BYTES, ClientMessage, FINAL_CLOSE_CODES, ServerMessage, input_chunks,
};
use natamux_proto::terminals::query::TerminalQuery;

/// What the app sees of one terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    Status(TerminalStatus),
    /// A snapshot starts: clear the screen and take this size until the catch-up ends.
    Reset {
        cols: u16,
        rows: u16,
    },
    /// Bytes for the screen. `end` is the offset after them, None during a catch-up. Once the
    /// screen parsed them, call `parsed(end)`.
    Output {
        bytes: Vec<u8>,
        end: Option<u64>,
    },
    /// The catch-up's bytes all came. Once the screen parsed them, call `caught_up(offset)`.
    CaughtUp {
        offset: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalStatus {
    Connecting,
    Open,
    Reconnecting,
    /// The shell ended, with its exit code when known.
    Ended {
        code: Option<i32>,
    },
    /// The daemon ended the idle agent after `minutes` with no one looking. Showing the terminal
    /// or typing into it starts it again.
    Paused {
        minutes: u64,
    },
    /// It could not start, or the daemon refused it. No reconnect.
    Failed(String),
}

/// Off screen: the app parses nothing it does not show, so the daemon sends no live output until
/// the terminal is shown again, and then catches it up at once (T-0210).
fn parked() -> ClientMessage {
    ClientMessage::Visible {
        visible: false,
        cols: None,
        rows: None,
        park: true,
    }
}

/// What one call wants done: messages for the socket and events for the app.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub send: Vec<ClientMessage>,
    pub emit: Vec<TerminalEvent>,
}

/// After a socket closed: try again, or stop.
#[derive(Debug, PartialEq, Eq)]
pub enum AfterClose {
    Retry,
    /// The shell paused: connect again once the terminal is shown or typed into.
    Sleep,
    Stop(TerminalStatus),
}

#[derive(Debug)]
pub struct Protocol {
    terminal: String,
    shell_id: Option<String>,
    connected: bool,
    ever_opened: bool,
    ended: bool,
    paused: bool,
    /// Bytes of output since the shell started, counted from `replayed`.
    offset: u64,
    offset_known: bool,
    acked: u64,
    catch_up: bool,
    /// Typing during a catch-up waits for its end, as the screen is not shown meanwhile.
    typed: Vec<String>,
    size: (u16, u16),
    sent_size: Option<(u16, u16)>,
    /// The shell's size, from this socket's snapshot.
    shell_size: Option<(u16, u16)>,
    /// This client took the shell's size.
    claimed: bool,
    visible: bool,
    sent_visible: bool,
    claim_after_catch_up: bool,
    tries: u32,
}

impl Protocol {
    pub fn new(terminal: &str, cols: u16, rows: u16, visible: bool) -> Protocol {
        Protocol {
            terminal: terminal.to_string(),
            shell_id: None,
            connected: false,
            ever_opened: false,
            ended: false,
            paused: false,
            offset: 0,
            offset_known: false,
            acked: 0,
            catch_up: false,
            typed: Vec::new(),
            size: (cols, rows),
            sent_size: None,
            shell_size: None,
            claimed: false,
            visible,
            sent_visible: false,
            claim_after_catch_up: false,
            tries: 0,
        }
    }

    pub fn ever_opened(&self) -> bool {
        self.ever_opened
    }

    pub fn in_catch_up(&self) -> bool {
        self.catch_up
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The shell paused, and the terminal was not shown or typed into since.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// The number of the next try, for the backoff. `hello` starts it again from 0.
    pub fn next_try(&mut self) -> u32 {
        let tries = self.tries;
        self.tries = self.tries.saturating_add(1);
        tries
    }

    /// The first socket asks for the shell as it is, and starts it when none runs. A later one
    /// resumes the shell it had: from its offset when known, else with a snapshot. A resume never
    /// takes the shell's size; its next claim sends the size.
    pub fn query(&mut self, ticket: String) -> TerminalQuery {
        self.claimed = self.visible && self.shell_id.is_none();
        self.sent_visible = self.visible;
        self.sent_size = Some(self.size);
        self.shell_size = None;
        TerminalQuery {
            ticket,
            cols: self.size.0,
            rows: self.size.1,
            terminal: self.terminal.clone(),
            visible: self.visible,
            shell_id: self.shell_id.clone(),
            offset: self.offset_known.then_some(self.offset),
        }
    }

    pub fn opened(&mut self) -> Step {
        self.connected = true;
        self.ever_opened = true;
        let mut step = Step::default();
        step.emit.push(TerminalEvent::Status(TerminalStatus::Open));
        if self.claimed {
            self.send_size(false, &mut step);
        }
        if !self.visible {
            // The query says hidden, which still gets live output up to a budget; parked gets
            // none until it is shown.
            step.send.push(parked());
        }
        step
    }

    pub fn server(&mut self, message: ServerMessage) -> Step {
        let mut step = Step::default();
        match message {
            ServerMessage::Hello { shell_id } => {
                self.shell_id = Some(shell_id);
                // The daemon answered: the next loss starts the backoff from the first delay.
                self.tries = 0;
            }
            ServerMessage::Snapshot { cols, rows } => {
                self.shell_size = Some((cols, rows));
                self.begin_catch_up();
                step.emit.push(TerminalEvent::Reset { cols, rows });
            }
            ServerMessage::Resume { .. } => self.begin_catch_up(),
            ServerMessage::Replayed { offset } => {
                // Live output after this counts from here, in the order of the frames.
                self.offset = offset;
                self.offset_known = true;
                step.emit.push(TerminalEvent::CaughtUp { offset });
            }
            ServerMessage::Pong => {}
            ServerMessage::Exit { code } => {
                self.end();
                step.emit
                    .push(TerminalEvent::Status(TerminalStatus::Ended { code }));
            }
            ServerMessage::Paused { minutes } => {
                self.paused = true;
                self.catch_up = false;
                step.emit
                    .push(TerminalEvent::Status(TerminalStatus::Paused { minutes }));
            }
            ServerMessage::Error { message } => {
                self.end();
                step.emit
                    .push(TerminalEvent::Status(TerminalStatus::Failed(message)));
            }
        }
        step
    }

    pub fn output(&mut self, bytes: Vec<u8>) -> TerminalEvent {
        // Bytes of a catch-up are not counted: snapshot and live bytes come mixed, and
        // `replayed` gives the offset.
        if self.catch_up && !self.offset_known {
            return TerminalEvent::Output { bytes, end: None };
        }
        self.offset += bytes.len() as u64;
        TerminalEvent::Output {
            bytes,
            end: Some(self.offset),
        }
    }

    /// The socket closed. `code` is its close code, None when it just went away.
    pub fn closed(&mut self, code: Option<u16>) -> AfterClose {
        let was_connected = self.connected;
        self.connected = false;
        // The next socket starts its own catch-up; typing stays queued for it. An offset that a
        // cut catch-up left unknown stays unknown, so that socket asks for a snapshot.
        self.catch_up = false;
        if self.ended {
            return AfterClose::Stop(TerminalStatus::Ended { code: None });
        }
        if self.paused {
            return AfterClose::Sleep;
        }
        let final_code = code.is_some_and(|code| FINAL_CLOSE_CODES.contains(&code));
        if self.ever_opened && !final_code {
            return AfterClose::Retry;
        }
        self.end();
        AfterClose::Stop(if final_code || !was_connected {
            TerminalStatus::Failed(close_reason(code))
        } else {
            TerminalStatus::Ended { code: None }
        })
    }

    pub fn input(&mut self, data: &str) -> Step {
        let mut step = Step::default();
        if self.paused {
            // The keys go to the agent once it runs again.
            self.typed.push(data.to_string());
            self.wake();
            return step;
        }
        if self.ended || !self.connected {
            return step;
        }
        if self.catch_up {
            self.typed.push(data.to_string());
            return step;
        }
        if !self.claimed {
            self.send_size(true, &mut step);
        }
        for chunk in input_chunks(data) {
            step.send.push(ClientMessage::Input { data: chunk });
        }
        step
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Step {
        self.size = (cols, rows);
        let mut step = Step::default();
        if self.connected && !self.catch_up && self.visible {
            self.send_size(false, &mut step);
        }
        step
    }

    /// The terminal came on screen or left it. Coming on screen claims the shell's size.
    pub fn set_visible(&mut self, visible: bool) -> Step {
        self.visible = visible;
        let mut step = Step::default();
        if visible && self.paused {
            self.wake();
        }
        if !self.connected || self.ended {
            return step;
        }
        if visible && self.catch_up {
            // Its size is the snapshot's until the catch-up ends.
            self.claim_after_catch_up = true;
        } else if visible {
            self.claim(&mut step);
        } else if self.sent_visible {
            self.sent_visible = false;
            step.send.push(parked());
        }
        step
    }

    /// The terminal took the focus: it claims the shell's size, as the web's textarea focus
    /// does. During a catch-up the claim waits for its end.
    pub fn claim_size(&mut self) -> Step {
        let mut step = Step::default();
        if !self.connected || self.ended || !self.visible {
            return step;
        }
        if self.catch_up {
            self.claim_after_catch_up = true;
        } else {
            self.send_size(true, &mut step);
        }
        step
    }

    /// The screen parsed the output up to `offset`.
    pub fn parsed(&mut self, offset: u64) -> Step {
        let mut step = Step::default();
        if offset.saturating_sub(self.acked) >= ACK_EVERY_BYTES {
            self.ack(offset, &mut step);
        }
        step
    }

    /// The screen parsed a whole catch-up, up to the offset of its `replayed`.
    pub fn caught_up(&mut self, offset: u64) -> Step {
        let mut step = Step::default();
        self.ack(offset, &mut step);
        self.finish_catch_up(&mut step);
        step
    }

    /// No `replayed` came in time: show what there is and go on.
    pub fn catch_up_timed_out(&mut self) -> Step {
        let mut step = Step::default();
        self.finish_catch_up(&mut step);
        step
    }

    fn begin_catch_up(&mut self) {
        self.catch_up = true;
        self.offset_known = false;
    }

    fn finish_catch_up(&mut self, step: &mut Step) {
        if !self.catch_up {
            return;
        }
        self.catch_up = false;
        if !self.connected {
            return;
        }
        if self.claimed {
            self.send_size(false, step);
        }
        if std::mem::take(&mut self.claim_after_catch_up) && self.visible {
            self.claim(step);
        }
        // A resume on screen with no snapshot, or one at another size: the shell may have
        // another client's size, so the pane takes its own (T-0173).
        if !self.claimed && self.visible && self.shell_size != Some(self.size) {
            self.send_size(true, step);
        }
        for data in std::mem::take(&mut self.typed) {
            step.send.extend(
                input_chunks(&data)
                    .into_iter()
                    .map(|data| ClientMessage::Input { data }),
            );
        }
    }

    fn claim(&mut self, step: &mut Step) {
        self.sent_visible = true;
        self.claimed = true;
        self.sent_size = Some(self.size);
        step.send.push(ClientMessage::Visible {
            visible: true,
            cols: Some(self.size.0),
            rows: Some(self.size.1),
            park: false,
        });
    }

    fn send_size(&mut self, always: bool, step: &mut Step) {
        if self.catch_up || !self.connected {
            return;
        }
        if always || self.sent_size != Some(self.size) {
            self.sent_size = Some(self.size);
            self.claimed = true;
            step.send.push(ClientMessage::Resize {
                cols: self.size.0,
                rows: self.size.1,
            });
        }
    }

    fn ack(&mut self, offset: u64, step: &mut Step) {
        self.acked = offset;
        if self.connected {
            step.send.push(ClientMessage::Ack { offset });
        }
    }

    fn end(&mut self) {
        self.ended = true;
        self.catch_up = false;
        self.typed.clear();
    }

    /// The paused shell is gone, so the next socket asks for a new one, which brings the agent
    /// back on its session.
    fn wake(&mut self) {
        self.paused = false;
        self.shell_id = None;
        self.offset = 0;
        self.offset_known = false;
        self.acked = 0;
    }
}

fn close_reason(code: Option<u16>) -> String {
    match code {
        Some(4401) => "The daemon refused the terminal: its ticket was not valid.".into(),
        Some(4409) => "The daemon refused the terminal.".into(),
        Some(4410) => "The shell of this terminal no longer runs.".into(),
        _ => "Could not connect to the terminal.".into(),
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
