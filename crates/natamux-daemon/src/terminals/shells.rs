//! The open shells of the daemon and what happens to them on their own: output, exit, quiet
//! moments, idle ends. `TerminalService` is the public side.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use natamux_pty::{Env, PtyEvents};

use super::limits::{ENDED_IDLE_MS, MAX_SESSIONS, SHELL_PROGRAMS, WORKSPACE_QUIET_MS};
use super::service::TerminalOptions;
use super::session_input::set_size;
use super::session_lifecycle::{
    NewSession, close_session, session_key, shell_exited, spawn_session, take_output,
};
use super::tickets::Tickets;
use super::types::{ClientRef, Session, SessionRef, Size, TerminalError, same_client};
use crate::shared::event_bus::DaemonMessage;

pub(super) struct Inner {
    pub(super) options: TerminalOptions,
    pub(super) env: Env,
    pub(super) sessions: Mutex<HashMap<String, SessionRef>>,
    pub(super) tickets: Mutex<Tickets>,
    /// By task: whether output came since the quiet timer was set.
    pub(super) output_seen: Mutex<HashMap<String, bool>>,
    pub(super) next_attach: AtomicU64,
    /// Held while a shell starts, so two attaches at once start one shell, and the limit holds.
    pub(super) starting: Mutex<()>,
    /// Held while one shell's output is parsed: shells parse one at a time, as on Node's one
    /// loop, so floods in several shells take one core, not every core the dashboards need too.
    pub(super) parsing: Mutex<()>,
    pub(super) this: Weak<Inner>,
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in one shell (a bug) must not stop all the others.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Ends the attach of one client.
pub struct Detach {
    pub(super) inner: Weak<Inner>,
    pub(super) session: Weak<Mutex<Session>>,
    pub(super) client: ClientRef,
    pub(super) attach: u64,
}

impl Detach {
    pub fn detach(&self) {
        let (Some(inner), Some(session)) = (self.inner.upgrade(), self.session.upgrade()) else {
            return;
        };
        let mut guard = lock(&session);
        let Some(index) = guard.clients.iter().position(|(client, state)| {
            same_client(client, &self.client) && state.attach == self.attach
        }) else {
            return;
        };
        guard.clients.remove(index);
        if let Some(job) = guard.snapshot.as_mut() {
            job.clients
                .retain(|client| !same_client(client, &self.client));
        }
        // The client that claimed the size last decides it again.
        let last = guard
            .clients
            .iter()
            .rev()
            .find(|(_, state)| state.claimed)
            .map(|(_, state)| state.size);
        if let Some(size) = last {
            set_size(&mut guard, size);
        } else if guard.clients.is_empty() {
            inner.close_when_idle(&mut guard);
        }
    }
}

impl Inner {
    pub(super) fn now(&self) -> u64 {
        self.options.scheduler.now()
    }

    pub(super) fn get(&self, task_id: &str, terminal: &str) -> Option<SessionRef> {
        lock(&self.sessions)
            .get(&session_key(task_id, terminal))
            .cloned()
    }

    pub(super) fn is_current(&self, key: &str, session: &SessionRef) -> bool {
        lock(&self.sessions)
            .get(key)
            .is_some_and(|known| Arc::ptr_eq(known, session))
    }

    pub(super) fn sessions_where(&self, wanted: impl Fn(&Session) -> bool) -> Vec<SessionRef> {
        let all: Vec<SessionRef> = lock(&self.sessions).values().cloned().collect();
        all.into_iter()
            .filter(|session| wanted(&lock(session)))
            .collect()
    }

    pub(super) fn publish_shells_changed(&self, task_id: &str) {
        if let Some(bus) = &self.options.bus {
            bus.publish(DaemonMessage::ShellsChanged {
                task_id: task_id.into(),
            });
        }
    }

    pub(super) fn with_client(
        &self,
        task_id: &str,
        terminal: &str,
        client: &ClientRef,
        run: impl FnOnce(&mut Session),
    ) {
        if let Some(session) = self.get(task_id, terminal) {
            let mut guard = lock(&session);
            if guard
                .clients
                .iter()
                .any(|(known, _)| same_client(known, client))
            {
                run(&mut guard);
            }
        }
    }

    /// The shell, and whether this call started it. The check and the start are one step: two
    /// clients that open a terminal that does not run yet get the same shell.
    pub(super) fn ensure(
        &self,
        task_id: &str,
        terminal: &str,
        size: Size,
    ) -> Result<(SessionRef, bool), TerminalError> {
        if let Some(session) = self.get(task_id, terminal) {
            return Ok((session, false));
        }
        let _starting = lock(&self.starting);
        match self.get(task_id, terminal) {
            Some(session) => Ok((session, false)),
            None => self
                .start(task_id, terminal, size)
                .map(|session| (session, true)),
        }
    }

    pub(super) fn start(
        &self,
        task_id: &str,
        terminal: &str,
        size: Size,
    ) -> Result<SessionRef, TerminalError> {
        let cwd = (self.options.workspace_of)(task_id)?;
        if lock(&self.sessions).len() >= MAX_SESSIONS {
            return Err(TerminalError::Limit(MAX_SESSIONS));
        }
        let session = spawn_session(NewSession {
            spawn: self.options.spawn.as_ref(),
            env: &self.env,
            cwd: &cwd,
            task_id,
            terminal,
            size,
            scheduler: &self.options.scheduler,
            trace_input: self.options.trace_input,
        })?;
        let key = session_key(task_id, terminal);
        lock(&self.sessions).insert(key.clone(), Arc::clone(&session));
        self.publish_shells_changed(task_id);
        let events = self.events(key, &session);
        lock(&session).pty.start(events);
        Ok(session)
    }

    pub(super) fn events(&self, key: String, session: &SessionRef) -> PtyEvents {
        let (inner, weak) = (self.this.clone(), Arc::downgrade(session));
        let (exit_inner, exit_weak, exit_key) = (inner.clone(), weak.clone(), key.clone());
        PtyEvents {
            on_data: Box::new(move |read| {
                let (Some(inner), Some(session)) = (inner.upgrade(), weak.upgrade()) else {
                    return;
                };
                // A closed shell may still print while it ends. Nobody looks at it any more.
                if !inner.is_current(&key, &session) {
                    return;
                }
                let (data, task_id, terminal) = {
                    let _turn = lock(&inner.parsing);
                    let mut guard = lock(&session);
                    let data = take_output(&mut guard, read);
                    (data, guard.task_id.clone(), guard.terminal.clone())
                };
                if !data.is_empty() {
                    inner.output(&task_id, &terminal, &data);
                }
            }),
            on_exit: Box::new(move |code| {
                let (Some(inner), Some(session)) = (exit_inner.upgrade(), exit_weak.upgrade())
                else {
                    return;
                };
                let task_id = {
                    let mut guard = lock(&session);
                    shell_exited(&mut guard, code);
                    guard.task_id.clone()
                };
                let removed = {
                    let mut sessions = lock(&inner.sessions);
                    let current = sessions
                        .get(&exit_key)
                        .is_some_and(|known| Arc::ptr_eq(known, &session));
                    current && sessions.remove(&exit_key).is_some()
                };
                if removed {
                    inner.publish_shells_changed(&task_id);
                }
            }),
        }
    }

    pub(super) fn output(&self, task_id: &str, terminal: &str, data: &[u8]) {
        if self.options.on_output_idle.is_some() {
            self.note_output(task_id);
        }
        if let Some(listener) = &self.options.on_output {
            listener(task_id, terminal, data);
        }
    }

    /// A flag per task, not a new timer per chunk, so a flood stays cheap. The quiet moment is
    /// between WORKSPACE_QUIET_MS and twice that.
    pub(super) fn note_output(&self, task_id: &str) {
        let mut seen = lock(&self.output_seen);
        if let Some(flag) = seen.get_mut(task_id) {
            *flag = true;
            return;
        }
        seen.insert(task_id.to_string(), false);
        drop(seen);
        self.check_quiet(task_id.to_string());
    }

    pub(super) fn check_quiet(&self, task_id: String) {
        let inner = self.this.clone();
        self.options.scheduler.after(WORKSPACE_QUIET_MS, move || {
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let mut seen = lock(&inner.output_seen);
            if seen.get(&task_id) == Some(&true) {
                seen.insert(task_id.clone(), false);
                drop(seen);
                inner.check_quiet(task_id);
                return;
            }
            seen.remove(&task_id);
            drop(seen);
            if let Some(listener) = &inner.options.on_output_idle {
                listener(&task_id);
            }
            inner.note_foreground(&task_id);
        });
    }

    /// A program that starts or ends in a shell prints something. The dashboards show it: tell
    /// them when it changed.
    pub(super) fn note_foreground(&self, task_id: &str) {
        let mut changed = false;
        for session in self.sessions_where(|session| session.task_id == task_id) {
            let mut guard = lock(&session);
            let program = guard.pty.foreground();
            changed = changed || program != guard.program;
            guard.program = program;
        }
        if changed {
            self.publish_shells_changed(task_id);
        }
    }

    pub(super) fn close_one(&self, task_id: &str, terminal: &str) {
        let Some(session) = lock(&self.sessions).remove(&session_key(task_id, terminal)) else {
            return;
        };
        self.publish_shells_changed(task_id);
        let strong = Arc::clone(&session);
        close_session(&mut lock(&session), strong);
    }

    /// An ended task needs no shell that nobody looks at. One that runs a program, such as a dev
    /// server, is checked again later.
    pub(super) fn close_when_idle(&self, session: &mut Session) {
        let ended = self
            .options
            .is_ended
            .as_ref()
            .is_some_and(|is_ended| is_ended(&session.task_id));
        if !ended {
            return;
        }
        let scheduler = Arc::clone(&session.scheduler);
        scheduler.clear(&mut session.idle_timer);
        let (inner, weak) = (self.this.clone(), session.this.clone());
        session.idle_timer = Some(scheduler.after(ENDED_IDLE_MS, move || {
            let (Some(inner), Some(session)) = (inner.upgrade(), weak.upgrade()) else {
                return;
            };
            let key = {
                let guard = lock(&session);
                session_key(&guard.task_id, &guard.terminal)
            };
            if !inner.is_current(&key, &session) {
                return;
            }
            let mut guard = lock(&session);
            guard.idle_timer = None;
            if !guard.clients.is_empty() {
                return;
            }
            let program = guard.pty.foreground();
            let busy = program.as_deref().is_some_and(|program| {
                let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
                !SHELL_PROGRAMS.contains(&name)
            });
            if busy {
                inner.close_when_idle(&mut guard);
                return;
            }
            let (task_id, terminal) = (guard.task_id.clone(), guard.terminal.clone());
            drop(guard);
            inner.close_one(&task_id, &terminal);
        }));
    }
}
