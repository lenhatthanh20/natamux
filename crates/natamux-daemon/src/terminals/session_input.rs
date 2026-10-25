//! Input to a shell and its size (`session-input.ts`).

use natamux_pty::input_gap::{InputKind, pieces, wait_before};

use super::input_trace::{Step, trace};
use super::session_output::flush_output;
use super::types::{ClientRef, ClientState, Input, Session, Size, same_client};

/// Pieces of DEL and pieces of other input go to the shell INPUT_GAP_MS apart, see `input_gap`.
pub fn write(session: &mut Session, data: &str) {
    for (data, kind) in pieces(data) {
        session.pending.push_back(Input { data, kind });
    }
    flush(session);
}

/// Text, then its Enter ENTER_AFTER_PASTE_MS later.
pub fn submit(session: &mut Session) {
    session.pending.push_back(Input {
        data: "\r".into(),
        kind: InputKind::Submit,
    });
    flush(session);
}

pub fn flush(session: &mut Session) {
    if session.write_timer.is_some() {
        return;
    }
    while let Some(part) = session.pending.front() {
        let kind = part.kind;
        let wait = wait_before(kind, session.last_write, session.now());
        if wait > 0 {
            session.write_timer = Some(session.later(wait, |session| {
                session.write_timer = None;
                flush(session);
            }));
            return;
        }
        if let Some(part) = session.pending.pop_front() {
            trace(session, Step::Write(kind), &part.data);
            session.pty.write(part.data.as_bytes());
            session.last_write = Some((session.now(), kind));
        }
    }
}

/// Makes a client the one used last, and the one that claimed the size.
pub fn touch(session: &mut Session, client: &ClientRef) -> Option<ClientState> {
    let index = session
        .clients
        .iter()
        .position(|(known, _)| same_client(known, client))?;
    let (client, mut state) = session.clients.remove(index);
    state.claimed = true;
    session.clients.push((client, state.clone()));
    Some(state)
}

/// Only the daemon's screen pauses a shell; a slow client never does.
pub fn set_paused(session: &mut Session, paused: bool) {
    if session.paused == paused {
        return;
    }
    session.paused = paused;
    if paused {
        session.pty.pause();
    } else {
        session.pty.resume();
    }
}

/// The shell is told only when the size really changes. Output drawn for the old size goes out
/// first.
pub fn set_size(session: &mut Session, size: Size) {
    if session.size != size {
        flush_output(session);
        session.size = size;
        session.pty.resize(size.cols, size.rows);
        session.screen.resize(size.cols, size.rows);
    }
}
