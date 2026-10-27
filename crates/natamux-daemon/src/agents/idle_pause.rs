//! Pauses an idle agent that no one looks at, to free memory: once its turn ended N minutes ago
//! (the project's `agents.pauseIdleAfterMinutes`), the daemon ends its shell. The shell waits
//! with the agent's session (`open_shells_pause`), and the agent comes back on that session when
//! someone opens the terminal again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use natamux_agents::hooks::Signal;
use natamux_proto::tasks::task::Task;
use serde_json::{Value, json};

use crate::agents::runner::AgentLaunch;
use crate::shared::scheduler::{Scheduler, TimerId};
use crate::terminals::open_shells::OpenShells;

/// What the pause needs of the terminals: `TerminalService`, or a fake in tests.
pub trait PausableShells: Send + Sync {
    /// Whether a client shows the terminal now.
    fn watched(&self, task_id: &str, terminal: &str) -> bool;
    /// Ends the shell and tells its clients it paused. False when it was not open.
    fn pause_one(&self, task_id: &str, terminal: &str, minutes: u64) -> bool;
}

pub type TaskOf = dyn Fn(&str) -> Option<Task> + Send + Sync;
pub type MinutesOf = dyn Fn(&Task) -> Option<u64> + Send + Sync;
pub type AgentRunsIn = dyn Fn(&str, &str) -> bool + Send + Sync;
pub type OnPaused = dyn Fn(&str, &str, Value) + Send + Sync;

pub struct IdlePauseOptions {
    pub scheduler: Arc<Scheduler>,
    /// The length of one minute of the setting, in ms. Tests make it short.
    pub minute_ms: u64,
    pub shells: Arc<dyn PausableShells>,
    pub open_shells: Arc<OpenShells>,
    /// The task, None when it is gone, done or cancelled.
    pub task_of: Arc<TaskOf>,
    /// The idle minutes of the task's project. None: its agents never pause.
    pub minutes_of: Arc<MinutesOf>,
    pub agent_runs: Arc<AgentRunsIn>,
    /// After a pause, with the payload of its `agent.paused` event.
    pub on_paused: Arc<OnPaused>,
}

struct Seen {
    task_id: String,
    terminal: String,
    signal: Signal,
    at: u64,
}

#[derive(Default)]
struct State {
    /// The last hook signal of each shell, by "<task>/<terminal>".
    seen: HashMap<String, Seen>,
    timer: Option<TimerId>,
    stopped: bool,
}

enum Verdict {
    /// Not yet, or not now: ask again at the next check.
    Wait,
    /// It cannot pause until the agent reports again.
    Drop,
    Pause {
        minutes: u64,
        agent: String,
        session: String,
    },
}

pub struct IdlePause {
    options: IdlePauseOptions,
    state: Mutex<State>,
    this: Weak<IdlePause>,
}

fn key_of(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

impl IdlePause {
    pub fn new(options: IdlePauseOptions) -> Arc<IdlePause> {
        Arc::new_cyclic(|this| IdlePause {
            options,
            state: Mutex::new(State::default()),
            this: this.clone(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Checks once a minute from now on.
    pub fn start(&self) {
        self.lock().stopped = false;
        self.arm();
    }

    pub fn stop(&self) {
        let mut state = self.lock();
        state.stopped = true;
        self.options.scheduler.clear(&mut state.timer);
    }

    /// The last hook signal of the agent in a shell, at the time it came.
    pub fn note_signal(&self, task_id: &str, terminal: &str, signal: Signal) {
        let seen = Seen {
            task_id: task_id.into(),
            terminal: terminal.into(),
            signal,
            at: self.options.scheduler.now(),
        };
        self.lock().seen.insert(key_of(task_id, terminal), seen);
    }

    /// A restored agent waits at its prompt as after a turn, so it may pause again. One started
    /// with a prompt works first, and its hooks tell when its turn ends.
    pub fn note_launch(&self, launch: &AgentLaunch) {
        if launch.restored {
            self.note_signal(&launch.task_id, &launch.terminal, Signal::TurnEnded);
        }
    }

    fn arm(&self) {
        let mut state = self.lock();
        if state.stopped {
            return;
        }
        let this = self.this.clone();
        let timer = self
            .options
            .scheduler
            .after(self.options.minute_ms, move || {
                if let Some(this) = this.upgrade() {
                    this.tick();
                }
            });
        state.timer = Some(timer);
    }

    fn tick(&self) {
        let this = self.this.clone();
        // A pause writes files and an event, so it must not hold the timers.
        self.options.scheduler.off_thread(move || {
            if let Some(this) = this.upgrade() {
                this.check();
            }
        });
        self.arm();
    }

    /// Pauses each agent that is idle long enough and may pause now.
    pub fn check(&self) {
        let now = self.options.scheduler.now();
        let ended: Vec<(String, String, u64)> = self
            .lock()
            .seen
            .values()
            .filter(|seen| seen.signal == Signal::TurnEnded)
            .map(|seen| (seen.task_id.clone(), seen.terminal.clone(), seen.at))
            .collect();
        for (task_id, terminal, at) in ended {
            let idle_ms = now.saturating_sub(at);
            match self.verdict(&task_id, &terminal, idle_ms) {
                Verdict::Wait => {}
                Verdict::Drop => {
                    self.forget_if_same(&task_id, &terminal, at);
                }
                Verdict::Pause {
                    minutes,
                    agent,
                    session,
                } => {
                    // The agent may have reported again since the list was read.
                    if self.forget_if_same(&task_id, &terminal, at) {
                        self.pause(&task_id, &terminal, minutes, &agent, &session);
                    }
                }
            }
        }
    }

    fn verdict(&self, task_id: &str, terminal: &str, idle_ms: u64) -> Verdict {
        let Some(task) = (self.options.task_of)(task_id) else {
            return Verdict::Drop;
        };
        let Some(minutes) = (self.options.minutes_of)(&task) else {
            return Verdict::Wait;
        };
        let due = idle_ms >= minutes.saturating_mul(self.options.minute_ms);
        if !due || task.background_since.is_some() {
            return Verdict::Wait;
        }
        if self.options.shells.watched(task_id, terminal) {
            return Verdict::Wait;
        }
        if !(self.options.agent_runs)(task_id, terminal) {
            return Verdict::Drop;
        }
        let open = &self.options.open_shells;
        let Some(agent) = open.agent_of(task_id, terminal) else {
            return Verdict::Drop;
        };
        match open.session_of(task_id, terminal, &agent) {
            Some(session) => Verdict::Pause {
                minutes,
                agent,
                session,
            },
            None => Verdict::Drop,
        }
    }

    /// Forgets the last signal of a shell when it is still the one that came at `at`.
    fn forget_if_same(&self, task_id: &str, terminal: &str, at: u64) -> bool {
        let mut state = self.lock();
        let key = key_of(task_id, terminal);
        let same = state.seen.get(&key).is_some_and(|seen| seen.at == at);
        if same {
            state.seen.remove(&key);
        }
        same
    }

    fn pause(&self, task_id: &str, terminal: &str, minutes: u64, agent: &str, session: &str) {
        let open = &self.options.open_shells;
        if !open.pause(task_id, terminal) {
            return;
        }
        if !self.options.shells.pause_one(task_id, terminal, minutes) {
            open.unpause(task_id, terminal);
            return;
        }
        open.pause_done(task_id, terminal);
        let payload = json!({
            "terminal": terminal,
            "agent": agent,
            "session": session,
            "idleMinutes": minutes,
        });
        (self.options.on_paused)(task_id, terminal, payload);
    }
}
