//! A shell's start and end (`session-lifecycle.ts`).

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use natamux_proto::tasks::ports::port_range;
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::{Env, Pty, Signal, SpawnOptions};
use natamux_screen::Screen;
use natamux_screen::frames::whole_chars;

use super::limits::{EXIT_WAIT_MS, KILL_GRACE_MS};
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
    Ok(Arc::new_cyclic(|this| {
        Mutex::new(Session {
            task_id: new.task_id.into(),
            terminal: new.terminal.into(),
            shell_id: random_id(16),
            pty,
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
            partial: Vec::new(),
            program,
            scheduler: Arc::clone(new.scheduler),
            this: this.clone(),
        })
    }))
}

/// Output of a running shell: whole characters to the screen and the clients. Gives the bytes it
/// took, for the output listeners.
pub fn take_output(session: &mut Session, read: Vec<u8>) -> Vec<u8> {
    // Most reads end on a whole character: then no copy.
    let mut all = if session.partial.is_empty() {
        read
    } else {
        let mut all = std::mem::take(&mut session.partial);
        all.extend(read);
        all
    };
    let end = whole_chars(&all);
    session.partial = all.split_off(end);
    if all.is_empty() {
        return all;
    }
    session.screen.write(&all);
    queue_output(session, &all);
    frame_parsed(session);
    all
}

/// The shell ended. `removed`: the service forgot it now (it had not closed it). A program that
/// ignored SIGHUP can outlive its shell: it still gets the SIGKILL.
pub fn shell_exited(session: &mut Session, code: Option<i32>) {
    if !session.pty.leftover() {
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
    session.kill_timer = Some(scheduler.after(KILL_GRACE_MS, move || {
        if let Ok(mut session) = strong.lock() {
            session.kill_timer = None;
            session.pty.kill(Signal::Kill);
            session.kill_done = true;
        }
    }));
    // A paused shell blocks on its output and might never get to end.
    set_paused(session, false);
    session.pty.kill(Signal::Hup);
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
