//! Shells in the workspace of a task (`terminal-service.ts`). A shell keeps running when the
//! dashboard closes, so an agent started in it goes on; the next dashboard that connects gets the
//! screen so far first. One shell has one size, and several dashboards can show it. The one used
//! last decides it.
//!
//! Locks: the map of sessions is only held for a lookup, never while a session is locked by the
//! same call, and nothing is published on the bus while a lock is held.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use natamux_proto::terminals::messages::ServerMessage;
use natamux_proto::terminals::text::plain_text;
use natamux_pty::Env;

use super::session_input::{set_size, submit, touch, write};
use super::session_lifecycle::{EndWait, SpawnPty, session_key};
use super::session_output::{flush_output, ring_since};
use super::session_snapshot::{catch_up_behind, request_snapshot, resume};
use super::shells::{Detach, Inner, lock};
use super::tickets::Tickets;
use super::types::{ClientRef, ClientState, Frame, Phase, Size, TerminalError};
use crate::shared::event_bus::EventBus;
use crate::shared::scheduler::Scheduler;

pub type WorkspaceOf = dyn Fn(&str) -> Result<PathBuf, TerminalError> + Send + Sync;
pub type OutputListener = dyn Fn(&str, &str, &[u8]) + Send + Sync;
pub type TaskCheck = dyn Fn(&str) -> bool + Send + Sync;
pub type TaskListener = dyn Fn(&str) + Send + Sync;

pub struct TerminalOptions {
    pub spawn: Arc<SpawnPty>,
    /// The folder of the task's workspace. An error when the task has none.
    pub workspace_of: Arc<WorkspaceOf>,
    /// The environment of the shells, before the session adds its own variables.
    pub env: Env,
    pub scheduler: Arc<Scheduler>,
    /// Called with every piece of output, for signs of life of an agent without hooks.
    pub on_output: Option<Arc<OutputListener>>,
    /// True when the task is done or cancelled, so its shells are only there to look around.
    pub is_ended: Option<Arc<TaskCheck>>,
    pub bus: Option<Arc<EventBus>>,
    /// Called once the shells of a task went quiet after output: files may have changed.
    pub on_output_idle: Option<Arc<TaskListener>>,
}

#[derive(Default)]
pub struct AttachOptions {
    pub visible: Option<bool>,
    /// A resume of a client that lost its socket: never starts a shell, never takes the size,
    /// and gets only the output since `offset` while the ring still has it.
    pub shell_id: Option<String>,
    pub offset: Option<u64>,
}

#[derive(Clone)]
pub struct TerminalService {
    inner: Arc<Inner>,
}

impl TerminalService {
    pub fn new(options: TerminalOptions) -> TerminalService {
        let env = natamux_pty::no_proxy::with_loopback_no_proxy(&options.env);
        TerminalService {
            inner: Arc::new_cyclic(|this| Inner {
                options,
                env,
                sessions: Mutex::new(HashMap::new()),
                tickets: Mutex::new(Tickets::default()),
                output_seen: Mutex::new(HashMap::new()),
                next_attach: AtomicU64::new(1),
                starting: Mutex::new(()),
                this: this.clone(),
            }),
        }
    }

    pub fn issue_ticket(&self, task_id: &str) -> Result<String, TerminalError> {
        (self.inner.options.workspace_of)(task_id)?;
        let now = self.inner.now();
        Ok(lock(&self.inner.tickets).issue(task_id, now))
    }

    pub fn redeem_ticket(&self, ticket: &str, task_id: &str) -> bool {
        let now = self.inner.now();
        lock(&self.inner.tickets).redeem(ticket, task_id, now)
    }

    pub fn is_open(&self, task_id: &str, terminal: &str) -> bool {
        self.inner.get(task_id, terminal).is_some()
    }

    /// The names of the open terminals of a task, in order.
    pub fn list(&self, task_id: &str) -> Vec<String> {
        let prefix = session_key(task_id, "");
        let mut names: Vec<String> = lock(&self.inner.sessions)
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(String::from))
            .collect();
        names.sort_by_key(|name| name.parse::<u32>().unwrap_or(u32::MAX));
        names
    }

    /// Starts the shell when there is none. A client that is not on screen does not take the
    /// shell's size.
    pub fn attach(
        &self,
        task_id: &str,
        client: ClientRef,
        size: Size,
        terminal: &str,
        options: AttachOptions,
    ) -> Result<Detach, TerminalError> {
        let visible = options.visible.unwrap_or(true);
        let resumed = match &options.shell_id {
            None => None,
            Some(shell_id) => {
                let existed = self.inner.get(task_id, terminal);
                let same = existed
                    .as_ref()
                    .is_some_and(|session| lock(session).shell_id == *shell_id);
                if !same {
                    return Err(TerminalError::ShellGone {
                        task_id: task_id.into(),
                        terminal: terminal.into(),
                    });
                }
                existed
            }
        };
        let (session, started) = match resumed.clone() {
            Some(session) => (session, false),
            None => self.inner.ensure(task_id, terminal, size)?,
        };
        let claims = visible && resumed.is_none();
        let attach = self.inner.next_attach.fetch_add(1, Ordering::SeqCst);
        let mut guard = lock(&session);
        let now = guard.now();
        let state = ClientState {
            attach,
            size,
            claimed: claims,
            visible,
            phase: Phase::Snapshot,
            sent: guard.offset,
            acked: guard.offset,
            acked_at: now,
            behind_from: None,
            behind_since: now,
            snapshot_at: now,
            budget_from: now,
            budget_bytes: 0,
        };
        if claims {
            guard.clients.push((Arc::clone(&client), state));
            if !started {
                // The screen is drawn for this size, so the shell must know it before the snapshot.
                set_size(&mut guard, size);
            }
        } else {
            // First, as the one used least: another client leaving never gives it the size.
            guard.clients.insert(0, (Arc::clone(&client), state));
        }
        let scheduler = Arc::clone(&guard.scheduler);
        scheduler.clear(&mut guard.idle_timer);
        // No output reaches it before: it is in the snapshot phase.
        client.send(Frame::Message(ServerMessage::Hello {
            shell_id: guard.shell_id.clone(),
        }));
        if resumed.is_none() || !resume(&mut guard, &client, options.offset) {
            request_snapshot(&mut guard, &client);
        }
        drop(guard);
        Ok(Detach {
            inner: self.inner.this.clone(),
            session: Arc::downgrade(&session),
            client,
            attach,
        })
    }

    /// A client came on screen or left it. Only a terminal that came on screen in the page sends
    /// its size, and only that claims the shell's size.
    pub fn visible(
        &self,
        task_id: &str,
        terminal: &str,
        client: &ClientRef,
        visible: bool,
        size: Option<Size>,
    ) {
        self.inner
            .with_client(task_id, terminal, client, |session| {
                let now = session.now();
                let Some(state) = session.client_state(client) else {
                    return;
                };
                if state.visible && !visible {
                    state.budget_from = now;
                    state.budget_bytes = 0;
                }
                state.visible = visible;
                let behind = state.phase == Phase::Behind;
                if let (true, Some(size)) = (visible, size) {
                    touch(session, client);
                    if let Some((_, state)) = session.clients.last_mut() {
                        state.size = size;
                    }
                    set_size(session, size);
                }
                if visible && behind {
                    catch_up_behind(session);
                }
            });
    }

    /// The client parsed its output up to `offset`. An offset past what it was sent counts as
    /// all of it, and an older one changes nothing.
    pub fn ack(&self, task_id: &str, terminal: &str, client: &ClientRef, offset: u64) {
        self.inner
            .with_client(task_id, terminal, client, |session| {
                let now = session.now();
                let Some(state) = session.client_state(client) else {
                    return;
                };
                let acked = offset.max(state.acked).min(state.sent);
                if acked > state.acked {
                    state.acked = acked;
                    state.acked_at = now;
                }
                if state.phase == Phase::Behind {
                    catch_up_behind(session);
                }
            });
    }

    /// The output sent since `offset`, while it is still kept.
    pub fn output_since(
        &self,
        task_id: &str,
        offset: u64,
        terminal: &str,
    ) -> Option<Vec<Arc<[u8]>>> {
        let session = self.inner.get(task_id, terminal)?;
        ring_since(&lock(&session), offset)
    }

    /// The last lines of a shell as plain text. None when the shell is not open.
    pub fn last_screen(&self, task_id: &str, terminal: &str, max_lines: usize) -> Option<String> {
        let session = self.inner.get(task_id, terminal)?;
        let text = lock(&session).screen.last_lines(max_lines);
        Some(text)
    }

    /// A shell that a finished task kept, because a program runs in it, ends once it is idle,
    /// also when no dashboard ever attaches to it.
    pub fn end_when_idle(&self, task_id: &str, terminal: &str) {
        if let Some(session) = self.inner.get(task_id, terminal) {
            let mut guard = lock(&session);
            if guard.clients.is_empty() {
                self.inner.close_when_idle(&mut guard);
            }
        }
    }

    /// All output so far is sent. The screen parses at once, so only the batch waits.
    pub fn settled(&self, task_id: &str, terminal: &str) {
        if let Some(session) = self.inner.get(task_id, terminal) {
            flush_output(&mut lock(&session));
        }
    }

    /// Types a command line into a shell of the task, starting the shell first when needed.
    pub fn run(
        &self,
        task_id: &str,
        line: &str,
        terminal: &str,
        size: Size,
    ) -> Result<(), TerminalError> {
        let (session, _) = self.inner.ensure(task_id, terminal, size)?;
        write(&mut lock(&session), &format!("{line}\r"));
        Ok(())
    }

    /// Pastes text into a shell and presses Enter, so the agent there takes it as its next
    /// prompt. Bracketed paste keeps line breaks from sending the text early. The text loses
    /// escapes and control bytes: one could end the paste early.
    pub fn paste(&self, task_id: &str, text: &str, terminal: &str) -> Result<(), TerminalError> {
        let session = self
            .inner
            .get(task_id, terminal)
            .ok_or_else(|| TerminalError::NoTerminal(task_id.into()))?;
        let mut guard = lock(&session);
        write(
            &mut guard,
            &format!("\x1b[200~{}\x1b[201~", plain_text(text)),
        );
        submit(&mut guard);
        Ok(())
    }

    pub fn foreground(&self, task_id: &str, terminal: &str) -> Option<String> {
        let session = self.inner.get(task_id, terminal)?;
        lock(&session).pty.foreground()
    }

    pub fn commands(&self, task_id: &str, terminal: &str) -> Option<Vec<String>> {
        let session = self.inner.get(task_id, terminal)?;
        lock(&session).pty.commands()
    }

    /// Input from a client gives the shell that client's size first: the user types where they
    /// look.
    pub fn input(&self, task_id: &str, terminal: &str, client: Option<&ClientRef>, data: &str) {
        let Some(session) = self.inner.get(task_id, terminal) else {
            return;
        };
        let mut guard = lock(&session);
        if let Some(state) = client.and_then(|client| touch(&mut guard, client)) {
            set_size(&mut guard, state.size);
        }
        write(&mut guard, data);
    }

    /// A client sends its size when it changes and when it takes the focus.
    pub fn resize(&self, task_id: &str, terminal: &str, client: Option<&ClientRef>, size: Size) {
        let Some(session) = self.inner.get(task_id, terminal) else {
            return;
        };
        let mut guard = lock(&session);
        if client
            .and_then(|client| touch(&mut guard, client))
            .is_some()
            && let Some((_, state)) = guard.clients.last_mut()
        {
            state.size = size;
        }
        set_size(&mut guard, size);
    }

    /// Ends one shell of a task. It is forgotten at once, so a new shell with the same name
    /// starts a new process with an empty screen, even when the old one is slow to end.
    pub fn close_one(&self, task_id: &str, terminal: &str) {
        self.inner.close_one(task_id, terminal);
    }

    /// Ends every shell of a task, for example before its workspace is removed.
    pub fn close(&self, task_id: &str) {
        for terminal in self.list(task_id) {
            self.close_one(task_id, &terminal);
        }
    }

    /// Ends every shell of a task; the wait ends a moment after a stubborn shell is killed.
    pub fn close_and_wait(&self, task_id: &str) -> EndWait {
        let sessions = self
            .inner
            .sessions_where(|session| session.task_id == task_id);
        self.close(task_id);
        EndWait::new(sessions, &self.inner.options.scheduler)
    }

    /// At shutdown: the daemon must not exit before a shell that ignores SIGHUP is killed, or it
    /// lives on with no parent.
    pub fn close_all_and_wait(&self) -> EndWait {
        let sessions = self.inner.sessions_where(|_| true);
        self.close_all();
        EndWait::new(sessions, &self.inner.options.scheduler)
    }

    pub fn close_all(&self) {
        let keys: Vec<(String, String)> = self
            .inner
            .sessions_where(|_| true)
            .iter()
            .map(|session| {
                let guard = lock(session);
                (guard.task_id.clone(), guard.terminal.clone())
            })
            .collect();
        for (task_id, terminal) in keys {
            self.close_one(&task_id, &terminal);
        }
    }
}
