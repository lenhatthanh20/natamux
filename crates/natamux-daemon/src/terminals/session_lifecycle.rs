//! A shell's start and end (`session-lifecycle.ts`).

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use natamux_proto::tasks::ports::port_range;
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::{Env, ProcessTable, Pty, Signal, SpawnOptions};
use natamux_screen::Screen;
use natamux_screen::frames::whole_chars;

use super::limits::{EXIT_WAIT_MS, KILL_GRACE_MS, PARSE_TURN_BYTES};
use super::session_input::set_paused;
use super::session_output::{flush_output, queue_output};
use super::session_snapshot::{end_snapshots, frame_parsed};
use super::types::{Frame, Session, SessionRef, Size, TerminalError};
use crate::shared::scheduler::Scheduler;

pub type SpawnPty = dyn Fn(SpawnOptions) -> std::io::Result<Box<dyn Pty>> + Send + Sync;

pub fn session_key(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

/// Random bytes as base64url, for shell ids and tickets.
pub fn random_id(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    // Tickets guard the shells: without the OS random source the daemon must not go on.
    if getrandom::fill(&mut buffer).is_err() {
        eprintln!("No random bytes from the system; stopping.");
        std::process::abort();
    }
    URL_SAFE_NO_PAD.encode(buffer)
}

pub struct NewSession<'a> {
    pub spawn: &'a SpawnPty,
    pub env: &'a Env,
    pub cwd: &'a Path,
    pub task_id: &'a str,
    pub terminal: &'a str,
    pub size: Size,
    pub scheduler: &'a Arc<Scheduler>,
    pub trace_input: bool,
}

pub fn spawn_session(new: NewSession<'_>) -> Result<SessionRef, TerminalError> {
    let port = port_range(new.task_id).0.to_string();
    let mut env = new.env.clone();
    // The task id lets `natamux hook` name the task even when the agent changes folders, and the
    // terminal name tells which stage of a group the agent works on. PORT is the first port of
    // the task's own block, for a dev server.
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("NATAMUX_TASK_ID".into(), new.task_id.into());
    env.insert("NATAMUX_TERMINAL".into(), new.terminal.into());
    env.insert("NATAMUX_PORT".into(), port.clone());
    env.insert("PORT".into(), port);
    let pty = (new.spawn)(SpawnOptions {
        cols: new.size.cols,
        rows: new.size.rows,
        cwd: new.cwd.to_path_buf(),
        env,
    })
    .map_err(|error| TerminalError::Spawn(error.to_string()))?;
    let program = pty.foreground();
    let processes = pty.processes();
    Ok(Arc::new_cyclic(|this| {
        Mutex::new(Session {
            task_id: new.task_id.into(),
            terminal: new.terminal.into(),
            shell_id: random_id(16),
            pty,
            processes,
            screen: Screen::new(new.size.cols, new.size.rows),
            clients: Vec::new(),
            size: new.size,
            offset: 0,
            batch: Vec::new(),
            last_flush_at: None,
            flush_timer: None,
            ring: VecDeque::new(),
            ring_bytes: 0,
            ring_start: 0,
            snapshot: None,
            paused: false,
            recent: VecDeque::new(),
            recent_bytes: 0,
            resync_timer: None,
            kill_timer: None,
            kill_done: false,
            exited: false,
            idle_timer: None,
            pending: VecDeque::new(),
            last_write: None,
            write_timer: None,
            trace_input: new.trace_input,
            traced_at: None,
            partial: Vec::new(),
            program,
            scheduler: Arc::clone(new.scheduler),
            this: this.clone(),
        })
    }))
}

/// Shells parse one at a time: a read is parsed in turns of PARSE_TURN_BYTES, ranges of the read
/// with no copy, and the `parsing` lock is free between them, so another shell's read goes in
/// between. `parse` runs under the lock, `after` (the output listeners) without it.
pub fn parse_in_turns<'a, T>(
    parsing: &Mutex<()>,
    read: &'a [u8],
    mut parse: impl FnMut(&'a [u8]) -> T,
    mut after: impl FnMut(T),
) {
    for piece in read.chunks(PARSE_TURN_BYTES) {
        let taken = {
            let _turn = parsing
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            parse(piece)
        };
        after(taken);
    }
}

/// Output of a running shell: whole characters to the screen and the clients. Gives the bytes it
/// took, for the output listeners: a range of `read`, unless the read before cut a character.
pub fn take_output<'a>(session: &mut Session, read: &'a [u8]) -> Cow<'a, [u8]> {
    let all = if session.partial.is_empty() {
        Cow::Borrowed(read)
    } else {
        let mut all = std::mem::take(&mut session.partial);
        all.extend_from_slice(read);
        Cow::Owned(all)
    };
    let end = whole_chars(&all);
    session.partial = all[end..].to_vec();
    let all = match all {
        Cow::Borrowed(read) => Cow::Borrowed(&read[..end]),
        Cow::Owned(mut all) => {
            all.truncate(end);
            Cow::Owned(all)
        }
    };
    if all.is_empty() {
        return all;
    }
    session.screen.write(&all);
    queue_output(session, &all);
    frame_parsed(session);
    all
}

/// The process table for `shell_exited`, read with no lock held: only a shell being closed needs
/// one, to tell if a program outlived it.
pub fn table_at_exit(session: &SessionRef) -> Option<Arc<ProcessTable>> {
    let reader = {
        let session = session.lock().ok()?;
        session.kill_timer.and(session.processes.clone())?
    };
    Some(reader(true))
}

/// The shell ended. A program that ignored SIGHUP can outlive its shell: it still gets the
/// SIGKILL. `table`: from `table_at_exit`.
pub fn shell_exited(session: &mut Session, code: Option<i32>, table: Option<&ProcessTable>) {
    let leftover = match table {
        Some(table) => session.pty.leftover_in(table),
        None => session.processes.is_none() && session.pty.leftover(),
    };
    if !leftover {
        stop_kill_timer(session);
    }
    session.exited = true;
    let scheduler = Arc::clone(&session.scheduler);
    scheduler.clear(&mut session.idle_timer);
    scheduler.clear(&mut session.write_timer);
    scheduler.clear(&mut session.resync_timer);
    // After a close, nothing is pending and no client is left, so nothing goes out twice.
    flush_output(session);
    end_snapshots(session);
    set_paused(session, false);
    for (client, _) in session.clients.drain(..) {
        client.send(Frame::Message(ServerMessage::Exit { code }));
    }
}

fn stop_kill_timer(session: &mut Session) {
    let scheduler = Arc::clone(&session.scheduler);
    scheduler.clear(&mut session.kill_timer);
    session.kill_done = true;
}

/// Ends a shell that the service already forgot. `strong` keeps the session alive until the
/// SIGKILL went out or is not needed.
pub fn close_session(session: &mut Session, strong: SessionRef) {
    let scheduler = Arc::clone(&session.scheduler);
    scheduler.clear(&mut session.idle_timer);
    scheduler.clear(&mut session.write_timer);
    scheduler.clear(&mut session.resync_timer);
    flush_output(session);
    end_snapshots(session);
    for (client, _) in session.clients.drain(..) {
        client.send(Frame::Message(ServerMessage::Exit { code: None }));
    }
    // A shell that ignores SIGHUP (or a program in it that does) must not stay alive.
    let killing = Arc::clone(&strong);
    session.kill_timer = Some(scheduler.after(KILL_GRACE_MS, move || {
        let Ok(mut session) = killing.lock() else {
            return;
        };
        session.kill_timer = None;
        drop(session);
        kill(&killing, Signal::Kill);
    }));
    // A paused shell blocks on its output and might never get to end.
    set_paused(session, false);
    match session.processes.clone() {
        None => session.pty.kill(Signal::Hup),
        Some(read) => {
            scheduler.off_thread(move || {
                let table = read(true);
                if let Ok(mut session) = strong.lock() {
                    session.pty.kill_in(Signal::Hup, &table);
                }
            });
        }
    }
}

/// Signals the shell and what runs in it. The `ps` this needs runs off the timer thread and with
/// no lock held; a pty that reads no table is signalled at once.
fn kill(session: &SessionRef, signal: Signal) {
    let Ok(mut guard) = session.lock() else {
        return;
    };
    let Some(read) = guard.processes.clone() else {
        guard.pty.kill(signal);
        guard.kill_done = true;
        return;
    };
    let scheduler = Arc::clone(&guard.scheduler);
    drop(guard);
    let session = Arc::clone(session);
    scheduler.off_thread(move || {
        let table = read(true);
        if let Ok(mut session) = session.lock() {
            session.pty.kill_in(signal, &table);
            session.kill_done = true;
        }
    });
}

/// The end of closing shells: done once each ended and its SIGKILL went out or is not needed, or
/// once KILL_GRACE_MS + EXIT_WAIT_MS passed.
pub struct EndWait {
    sessions: Vec<SessionRef>,
    deadline: u64,
    scheduler: Arc<Scheduler>,
}

impl EndWait {
    pub fn new(sessions: Vec<SessionRef>, scheduler: &Arc<Scheduler>) -> EndWait {
        EndWait {
            sessions,
            deadline: scheduler.now() + KILL_GRACE_MS + EXIT_WAIT_MS,
            scheduler: Arc::clone(scheduler),
        }
    }

    pub fn is_done(&self) -> bool {
        self.scheduler.now() >= self.deadline
            || self.sessions.iter().all(|session| {
                session
                    .lock()
                    .map(|session| session.exited && session.kill_done)
                    .unwrap_or(true)
            })
    }

    /// Blocks until done, on the real clock.
    pub fn wait(&self) {
        while !self.is_done() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use super::{PARSE_TURN_BYTES, parse_in_turns};

    #[test]
    fn a_big_read_is_parsed_in_turns_of_16_kib_and_keeps_every_byte() {
        let read: Vec<u8> = (0..40 * 1024).map(|at| (at % 251) as u8).collect();
        let parsing = Mutex::new(());
        let mut turns: Vec<&[u8]> = Vec::new();
        parse_in_turns(&parsing, &read, |piece| piece, |piece| turns.push(piece));
        let sizes: Vec<usize> = turns.iter().map(|turn| turn.len()).collect();
        assert_eq!(sizes, [PARSE_TURN_BYTES, PARSE_TURN_BYTES, 8 * 1024]);
        assert_eq!(turns.concat(), read);
        // Ranges of the read, not copies.
        assert_eq!(turns[0].as_ptr(), read.as_ptr());
    }

    #[test]
    fn a_small_read_is_one_turn() {
        let parsing = Mutex::new(());
        let mut turns = 0;
        parse_in_turns(&parsing, b"ls\r\n", |_| (), |()| turns += 1);
        assert_eq!(turns, 1);
        let full = vec![7; PARSE_TURN_BYTES];
        parse_in_turns(&parsing, &full, |_| (), |()| turns += 1);
        assert_eq!(turns, 2);
    }

    /// The 64 KiB read of one shell does not hold the others: between two of its turns another
    /// shell takes the lock and parses.
    #[test]
    fn another_shell_parses_between_the_turns_of_a_flood() {
        let parsing = Mutex::new(());
        let flood = vec![b'x'; 64 * 1024];
        let mut order = Vec::new();
        std::thread::scope(|scope| {
            parse_in_turns(
                &parsing,
                &flood,
                |piece| {
                    assert!(parsing.try_lock().is_err(), "a turn holds the lock");
                    piece.len()
                },
                |len| {
                    order.push(format!("flood {len}"));
                    if order.len() == 1 {
                        let (done, heard) = channel();
                        let parsing = &parsing;
                        scope.spawn(move || {
                            let _turn = parsing.lock().unwrap();
                            done.send(()).unwrap();
                        });
                        heard.recv_timeout(Duration::from_secs(5)).unwrap();
                        order.push("other shell".to_string());
                    }
                },
            );
        });
        let turn = format!("flood {PARSE_TURN_BYTES}");
        let turn = turn.as_str();
        assert_eq!(order, [turn, "other shell", turn, turn, turn]);
    }
}
