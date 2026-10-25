//! The open shells of each task, kept in a file beside the task as they change, so a crash or a
//! kill without SIGTERM loses nothing (`open-shells.ts`). After a restart, the shells that no stage
//! restarts wait until a dashboard opens the task.
//!
//! Locks: the state lock is never held while publishing on the bus, as this listens to it too.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use natamux_proto::agents::start::LAUNCHABLE_AGENTS;

use super::limits::SHELL_PROGRAMS;
use super::shell_files::{OpenShell, delete_open_shells, note_ended_session, save_open_shells};
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::scheduler::{Scheduler, TimerId};

/// A restored or restarted agent shows a spinner until it is in the foreground, at most this long.
pub const RESTORE_SPINNER_MS: u64 = 15_000;
pub const RESTORE_POLL_MS: u64 = 500;
/// An agent that quit, or a shell that closed, is written down only after this long. When the app
/// is killed or the machine shuts down, they can end a moment before the daemon, and must come
/// back.
pub const EXIT_CONFIRM_MS: u64 = 3000;

/// What `OpenShells` needs to know of the open shells (`TerminalService`, or a fake).
pub trait ShellsView: Send + Sync {
    fn list(&self, task_id: &str) -> Vec<String>;
    fn is_open(&self, task_id: &str, terminal: &str) -> bool;
    fn foreground(&self, task_id: &str, terminal: &str) -> Option<String>;
}

pub type AgentRuns = dyn Fn(&str, &str) -> bool + Send + Sync;
/// The agent whose process runs in a shell; may run `ps`, so it is called on the timer thread.
pub type AgentIn = dyn Fn(&str, &str) -> Option<String> + Send + Sync;
pub type SaveShells = dyn Fn(&Path, &str, &[OpenShell]) -> std::io::Result<()> + Send + Sync;

pub struct OpenShellsOptions {
    pub data_dir: PathBuf,
    pub terminals: Arc<dyn ShellsView>,
    pub bus: Arc<EventBus>,
    pub scheduler: Arc<Scheduler>,
    /// True for a task that is done, cancelled or gone: it keeps no shells.
    pub is_ended: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    /// Whether an agent runs in a shell. Without it, no quit is noticed.
    pub agent_runs: Option<Arc<AgentRuns>>,
    pub agent_in: Option<Arc<AgentIn>>,
    pub save: Option<Arc<SaveShells>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) agent: Option<String>,
    pub(super) session: Option<String>,
}

pub(super) const PLAIN: Entry = Entry {
    agent: None,
    session: None,
};

pub(super) struct Starting {
    pub(super) task_id: String,
    pub(super) terminal: String,
    pub(super) agent: String,
    pub(super) until: u64,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) entries: HashMap<String, BTreeMap<String, Entry>>,
    /// Shells from the file that have not started again yet, by task, in the order of the file.
    pub(super) waiting: HashMap<String, Vec<String>>,
    /// Shells from the file whose stage starts its agent again, by task.
    pub(super) held: HashMap<String, Vec<String>>,
    /// "<task>/<terminal>" of the shells where an agent was launched or seen in this run. Only
    /// their quit is written down.
    pub(super) active: HashSet<String>,
    pub(super) exit_timers: HashMap<String, TimerId>,
    /// Shells that closed, until the close is written down.
    pub(super) closing: HashMap<String, TimerId>,
    /// The program last asked about in each shell with no agent, so `ps` runs once per program.
    pub(super) probed: HashMap<String, String>,
    /// Agents just started again, until they are in the foreground.
    pub(super) starting: Vec<(String, Starting)>,
    pub(super) poll: Option<TimerId>,
    pub(super) stopped: bool,
    pub(super) subscription: Option<u64>,
}

pub struct OpenShells {
    pub(super) options: OpenShellsOptions,
    pub(super) state: Mutex<State>,
    pub(super) this: Weak<OpenShells>,
}

pub(super) fn key_of(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

/// An agent comes back on the session it reported, or with none on the newest of the folder.
pub(super) fn resumable(entry: &Entry) -> Option<&str> {
    entry
        .agent
        .as_deref()
        .filter(|agent| LAUNCHABLE_AGENTS.contains(agent))
}

pub(super) fn is_shell(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    SHELL_PROGRAMS.contains(&name)
}

impl OpenShells {
    pub fn new(options: OpenShellsOptions) -> Arc<OpenShells> {
        Arc::new_cyclic(|this| OpenShells {
            options,
            state: Mutex::new(State::default()),
            this: this.clone(),
        })
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn publish_changed(&self, task_id: &str) {
        self.options.bus.publish(DaemonMessage::ShellsChanged {
            task_id: task_id.into(),
        });
    }

    pub fn start(&self) {
        let this = self.this.clone();
        let id = self.options.bus.subscribe(move |message| {
            let Some(this) = this.upgrade() else {
                return;
            };
            match message {
                DaemonMessage::ShellsChanged { task_id } => this.sync(task_id),
                DaemonMessage::ProgramsChanged { task_id, terminals } => {
                    this.sync_programs(task_id, terminals)
                }
                DaemonMessage::TaskDeleted { task_id } => this.forget(task_id),
                // Its shells cannot start without the folder.
                DaemonMessage::TaskUpdated { task, event, .. }
                    if event.kind == "workspace.removed" =>
                {
                    this.forget(&task.id)
                }
                _ => {}
            }
        });
        self.lock().subscription = Some(id);
    }

    /// Reads the task's file after a restart. The shells in `skip` (those of stages whose agent
    /// starts again at once) are held until `release_held`; the rest wait for a dashboard.
    pub fn load(&self, task_id: &str, skip: &[String]) {
        let shells = super::shell_files::read_open_shells(&self.options.data_dir, task_id);
        if shells.is_empty() {
            return;
        }
        // An agent with no session comes back on the newest session of the folder; a second of
        // its kind would open that one too, so it comes back plain.
        let mut newest: HashSet<String> = HashSet::new();
        let mut entries = BTreeMap::new();
        for shell in &shells {
            let by_newest =
                shell.agent.is_some() && shell.session.is_none() && !skip.contains(&shell.name);
            let second = by_newest
                && shell
                    .agent
                    .as_ref()
                    .is_some_and(|agent| newest.contains(agent));
            if let (true, Some(agent)) = (by_newest, &shell.agent) {
                newest.insert(agent.clone());
            }
            let entry = if second {
                PLAIN
            } else {
                Entry {
                    agent: shell.agent.clone(),
                    session: shell.session.clone(),
                }
            };
            entries.insert(shell.name.clone(), entry);
        }
        let closed: Vec<String> = shells
            .iter()
            .map(|shell| shell.name.clone())
            .filter(|name| !self.options.terminals.is_open(task_id, name))
            .collect();
        let (held, waiting): (Vec<String>, Vec<String>) =
            closed.into_iter().partition(|name| skip.contains(name));
        let mut state = self.lock();
        state.entries.insert(task_id.into(), entries);
        if !waiting.is_empty() {
            state.waiting.insert(task_id.into(), waiting);
        }
        if !held.is_empty() {
            state.held.insert(task_id.into(), held);
        }
    }

    /// After the agents of the stages started again: a held shell whose agent did not start
    /// waits for a dashboard like the others.
    pub fn release_held(&self) {
        let mut changed = Vec::new();
        {
            let mut state = self.lock();
            let held = std::mem::take(&mut state.held);
            for (task_id, names) in held {
                let left: Vec<String> = names
                    .into_iter()
                    .filter(|name| !self.options.terminals.is_open(&task_id, name))
                    .collect();
                if left.is_empty() {
                    continue;
                }
                let waiting = state.waiting.entry(task_id.clone()).or_default();
                for name in left {
                    if !waiting.contains(&name) {
                        waiting.push(name);
                    }
                }
                changed.push(task_id);
            }
        }
        for task_id in changed {
            self.publish_changed(&task_id);
        }
    }

    /// Whether the shell had an agent when the daemon last ran: one the person quit stays quit.
    pub fn was_running(&self, task_id: &str, terminal: &str) -> bool {
        self.lock()
            .entries
            .get(task_id)
            .and_then(|shells| shells.get(terminal))
            .is_some_and(|entry| entry.agent.is_some())
    }

    /// The last session that `agent` reported in the shell.
    pub fn session_of(&self, task_id: &str, terminal: &str, agent: &str) -> Option<String> {
        let state = self.lock();
        let entry = state.entries.get(task_id)?.get(terminal)?;
        (entry.agent.as_deref() == Some(agent))
            .then(|| entry.session.clone())
            .flatten()
    }

    /// A restore whose agent could not start: the shell waits again, for the next open or start.
    pub fn put_back(&self, task_id: &str, terminal: &str) {
        {
            let mut state = self.lock();
            let known = state
                .entries
                .get(task_id)
                .is_some_and(|shells| shells.contains_key(terminal));
            if state.stopped || self.options.terminals.is_open(task_id, terminal) || !known {
                return;
            }
            let waiting = state.waiting.entry(task_id.into()).or_default();
            if !waiting.iter().any(|name| name == terminal) {
                waiting.push(terminal.into());
            }
        }
        self.publish_changed(task_id);
    }

    pub fn waiting(&self, task_id: &str) -> Vec<String> {
        self.lock()
            .waiting
            .get(task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// The shells that are coming back, with the agent they come back with (None: a plain shell),
    /// in order: the waiting ones, then the starting ones.
    pub fn restoring(&self, task_id: &str) -> Vec<(String, Option<String>)> {
        let state = self.lock();
        let entries = state.entries.get(task_id);
        let mut restoring: Vec<(String, Option<String>)> = Vec::new();
        let mut set = |name: &str, agent: Option<String>| match restoring
            .iter_mut()
            .find(|(known, _)| known == name)
        {
            Some((_, known)) => *known = agent,
            None => restoring.push((name.to_string(), agent)),
        };
        for name in state.waiting.get(task_id).into_iter().flatten() {
            let agent = entries
                .and_then(|shells| shells.get(name))
                .and_then(resumable)
                .map(String::from);
            set(name, agent);
        }
        for (_, starting) in &state.starting {
            if starting.task_id == task_id {
                set(&starting.terminal, Some(starting.agent.clone()));
            }
        }
        restoring
    }

    /// Takes a waiting shell out of the wait, once: two dashboards that attach at once start it
    /// once. None when it does not wait, or a shell of that name runs already.
    pub fn take(&self, task_id: &str, terminal: &str) -> Option<OpenShell> {
        let mut state = self.lock();
        if state.stopped {
            return None;
        }
        let waiting = state.waiting.get_mut(task_id)?;
        let at = waiting.iter().position(|name| name == terminal)?;
        waiting.remove(at);
        if self.options.terminals.is_open(task_id, terminal) {
            return None;
        }
        let entry = state.entries.get(task_id)?.get(terminal)?;
        Some(OpenShell {
            name: terminal.into(),
            agent: resumable(entry).map(String::from),
            session: entry.session.clone(),
        })
    }

    /// An agent was started in a shell, on `session` when it went on with one.
    pub fn note_launch(&self, task_id: &str, terminal: &str, agent: &str, session: Option<&str>) {
        self.note(
            task_id,
            terminal,
            Entry {
                agent: Some(agent.into()),
                session: session.map(String::from),
            },
        );
    }

    /// A hook named the session of the agent in a shell.
    pub fn note_session(&self, task_id: &str, terminal: &str, agent: &str, session: &str) {
        self.note_launch(task_id, terminal, agent, Some(session));
    }

    /// The agent of a shell is starting again: its pane shows a spinner until it runs.
    pub fn mark_starting(&self, task_id: &str, terminal: &str, agent: &str) {
        {
            let mut state = self.lock();
            if state.stopped {
                return;
            }
            let until = self.options.scheduler.now() + RESTORE_SPINNER_MS;
            let key = key_of(task_id, terminal);
            state.starting.retain(|(known, _)| *known != key);
            state.starting.push((
                key,
                Starting {
                    task_id: task_id.into(),
                    terminal: terminal.into(),
                    agent: agent.into(),
                    until,
                },
            ));
            if state.poll.is_none() {
                state.poll = Some(self.poll_later());
            }
        }
        self.publish_changed(task_id);
    }

    pub fn forget(&self, task_id: &str) {
        let mut guard = self.lock();
        let state = &mut *guard;
        state.entries.remove(task_id);
        state.waiting.remove(task_id);
        state.held.remove(task_id);
        state
            .starting
            .retain(|(_, starting)| starting.task_id != task_id);
        let prefix = key_of(task_id, "");
        state.active.retain(|key| !key.starts_with(&prefix));
        state.probed.retain(|key, _| !key.starts_with(&prefix));
        let scheduler = &self.options.scheduler;
        for timers in [&mut state.exit_timers, &mut state.closing] {
            timers.retain(|key, timer| {
                let keep = !key.starts_with(&prefix);
                if !keep {
                    scheduler.cancel(*timer);
                }
                keep
            });
        }
        drop(guard);
        let _ = delete_open_shells(&self.options.data_dir, task_id);
    }

    /// Before the shutdown closes every shell. After this the list is not written again, so the
    /// shells that the shutdown ends stay in it, as after a kill.
    pub fn stop(&self) {
        let mut guard = self.lock();
        let state = &mut *guard;
        state.stopped = true;
        let scheduler = &self.options.scheduler;
        for (_, timer) in state.exit_timers.drain().chain(state.closing.drain()) {
            scheduler.cancel(timer);
        }
        scheduler.clear(&mut state.poll);
        if let Some(id) = state.subscription.take() {
            self.options.bus.unsubscribe(id);
        }
    }

    fn note(&self, task_id: &str, terminal: &str, entry: Entry) {
        if self.lock().stopped || !self.options.terminals.is_open(task_id, terminal) {
            return;
        }
        // A finished task keeps no open shells. The session of an agent reopened in it is kept.
        if (self.options.is_ended)(task_id) {
            let _ = note_ended_session(
                &self.options.data_dir,
                task_id,
                terminal,
                entry.agent.as_deref(),
                entry.session.as_deref(),
            );
            return;
        }
        self.lock().active.insert(key_of(task_id, terminal));
        self.set(task_id, terminal, entry);
        self.watch_exit(task_id, terminal);
    }

    pub(super) fn set(&self, task_id: &str, terminal: &str, entry: Entry) {
        if self.lock().stopped || !self.options.terminals.is_open(task_id, terminal) {
            return;
        }
        let mut state = self.lock();
        let shells = state.entries.entry(task_id.into()).or_default();
        if shells.get(terminal) == Some(&entry) {
            return;
        }
        shells.insert(terminal.into(), entry);
        self.write(&state, task_id);
    }

    pub(super) fn write(&self, state: &State, task_id: &str) {
        let mut shells: Vec<OpenShell> = state
            .entries
            .get(task_id)
            .into_iter()
            .flatten()
            .map(|(name, entry)| OpenShell {
                name: name.clone(),
                agent: entry.agent.clone(),
                session: entry.session.clone(),
            })
            .collect();
        shells.sort_by_key(|shell| shell.name.parse::<u32>().unwrap_or(u32::MAX));
        let data_dir = &self.options.data_dir;
        // The list is a convenience: a full disk must not break a shell.
        let _ = if shells.is_empty() {
            delete_open_shells(data_dir, task_id)
        } else if let Some(save) = &self.options.save {
            save(data_dir, task_id, &shells)
        } else {
            save_open_shells(data_dir, task_id, &shells)
        };
    }

    pub(super) fn in_foreground(&self, task_id: &str, terminal: &str) -> bool {
        self.options
            .terminals
            .foreground(task_id, terminal)
            .is_some_and(|program| !is_shell(&program))
    }
}
