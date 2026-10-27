//! How `OpenShells` follows the shells as they open, close and run agents (`sync` and the timers
//! of `open-shells.ts`).

use std::collections::BTreeMap;

use super::open_shells::{EXIT_CONFIRM_MS, Entry, OpenShells, PLAIN, RESTORE_POLL_MS, key_of};
use crate::shared::scheduler::TimerId;

impl OpenShells {
    /// Keeps the list in step with the open shells: a new shell is added as a plain one, a
    /// closed one goes after EXIT_CONFIRM_MS. Shells that wait to come back, or are held, stay.
    /// A paused shell that is open again is no longer paused.
    pub(super) fn sync(&self, task_id: &str) {
        if self.lock().stopped {
            return;
        }
        if (self.options.is_ended)(task_id) {
            if self.lock().entries.contains_key(task_id) {
                self.forget(task_id);
            }
            return;
        }
        let open = self.options.terminals.list(task_id);
        let replaced = self.take_replaced(task_id, &open);
        let state = self.lock();
        let old = state.entries.get(task_id).cloned().unwrap_or_default();
        let mut kept: Vec<String> = state.waiting.get(task_id).cloned().unwrap_or_default();
        kept.extend(state.held.get(task_id).cloned().unwrap_or_default());
        let gone: Vec<String> = old
            .keys()
            .filter(|name| !open.contains(name) && !kept.contains(name))
            .cloned()
            .collect();
        drop(state);
        for name in gone {
            self.close_later(task_id, &name);
        }
        let mut state = self.lock();
        let mut unpaused = false;
        for name in &open {
            let key = key_of(task_id, name);
            if !state.pausing.contains(&key) {
                unpaused |= state.paused.remove(&key);
            }
        }
        let closing: Vec<String> = old
            .keys()
            .filter(|name| state.closing.contains_key(&key_of(task_id, name)))
            .cloned()
            .collect();
        let mut next: BTreeMap<String, Entry> = BTreeMap::new();
        for name in open.iter().chain(&kept).chain(&closing) {
            let entry = if replaced.contains(name) {
                None
            } else {
                old.get(name).cloned()
            };
            next.entry(name.clone()).or_insert(entry.unwrap_or(PLAIN));
        }
        if next != old || unpaused {
            if next.is_empty() {
                state.entries.remove(task_id);
            } else {
                state.entries.insert(task_id.into(), next);
            }
            self.write(&state, task_id);
        }
        drop(state);
        for name in &open {
            self.watch_exit(task_id, name);
            self.find_agent(task_id, name);
        }
    }

    /// The program in front of these shells changed: only they are looked at again. A shell the
    /// list does not have yet, or an ended task, needs the whole `sync`.
    pub(super) fn sync_programs(&self, task_id: &str, terminals: &[String]) {
        let known = {
            let state = self.lock();
            if state.stopped {
                return;
            }
            state
                .entries
                .get(task_id)
                .is_some_and(|shells| terminals.iter().all(|name| shells.contains_key(name)))
        };
        if !known || (self.options.is_ended)(task_id) {
            self.sync(task_id);
            return;
        }
        for name in terminals {
            if self.options.terminals.is_open(task_id, name) {
                self.watch_exit(task_id, name);
                self.find_agent(task_id, name);
            }
        }
    }

    /// A stage or a person started a shell of a waiting name, or of one that just closed: it is a
    /// new shell, not the old one. The stage of a held shell started its agent again. Gives those
    /// names, out of the waiting, held and closing lists, with their quit watch stopped. A shell
    /// that is being paused is still the old one.
    fn take_replaced(&self, task_id: &str, open: &[String]) -> Vec<String> {
        let mut state = self.lock();
        let scheduler = &self.options.scheduler;
        let mut replaced = Vec::new();
        for name in open {
            let key = key_of(task_id, name);
            if state.pausing.contains(&key) {
                continue;
            }
            let closed = state
                .closing
                .remove(&key)
                .map(|timer| scheduler.cancel(timer))
                .is_some();
            if let Some(held) = state.held.get_mut(task_id) {
                held.retain(|known| known != name);
            }
            let was_waiting = state.waiting.get_mut(task_id).is_some_and(|waiting| {
                let before = waiting.len();
                waiting.retain(|known| known != name);
                waiting.len() != before
            });
            if was_waiting || closed {
                state.active.remove(&key);
                if let Some(timer) = state.exit_timers.remove(&key) {
                    scheduler.cancel(timer);
                }
                replaced.push(name.clone());
            }
        }
        replaced
    }

    /// A closed shell leaves the list once it stayed closed for EXIT_CONFIRM_MS.
    fn close_later(&self, task_id: &str, terminal: &str) {
        let key = key_of(task_id, terminal);
        let mut state = self.lock();
        if state.closing.contains_key(&key) {
            return;
        }
        if let Some(timer) = state.exit_timers.remove(&key) {
            self.options.scheduler.cancel(timer);
        }
        let this = self.this.clone();
        let (task, name, timer_key) = (task_id.to_string(), terminal.to_string(), key.clone());
        let timer = self.options.scheduler.after(EXIT_CONFIRM_MS, move || {
            let Some(this) = this.upgrade() else {
                return;
            };
            let mut state = this.lock();
            state.closing.remove(&timer_key);
            state.active.remove(&timer_key);
            state.paused.remove(&timer_key);
            state.probed.remove(&timer_key);
            if state.stopped {
                return;
            }
            let Some(shells) = state.entries.get_mut(&task) else {
                return;
            };
            if shells.remove(&name).is_none() {
                return;
            }
            if shells.is_empty() {
                state.entries.remove(&task);
            }
            this.write(&state, &task);
        });
        state.closing.insert(key, timer);
    }

    /// An agent that left the foreground counts as quit once it stayed gone for EXIT_CONFIRM_MS.
    /// Just after a launch, and on Windows (no foreground), `agent_runs` decides.
    pub(super) fn watch_exit(&self, task_id: &str, terminal: &str) {
        let key = key_of(task_id, terminal);
        let in_foreground = self.in_foreground(task_id, terminal);
        let mut state = self.lock();
        let has_agent = state
            .entries
            .get(task_id)
            .and_then(|shells| shells.get(terminal))
            .is_some_and(|entry| entry.agent.is_some());
        if self.options.agent_runs.is_none()
            || !state.active.contains(&key)
            || !has_agent
            || in_foreground
        {
            if let Some(timer) = state.exit_timers.remove(&key) {
                self.options.scheduler.cancel(timer);
            }
            return;
        }
        if state.exit_timers.contains_key(&key) {
            return;
        }
        let this = self.this.clone();
        let (task, name, timer_key) = (task_id.to_string(), terminal.to_string(), key.clone());
        let timer = self.options.scheduler.after(EXIT_CONFIRM_MS, move || {
            let Some(this) = this.upgrade() else {
                return;
            };
            {
                let mut state = this.lock();
                state.exit_timers.remove(&timer_key);
                if state.stopped || !this.options.terminals.is_open(&task, &name) {
                    return;
                }
            }
            let quit = this
                .options
                .agent_runs
                .as_ref()
                .is_some_and(|runs| !runs(&task, &name));
            if quit {
                this.lock().active.remove(&timer_key);
                this.set(&task, &name, PLAIN);
            } else {
                this.watch_exit(&task, &name);
            }
        });
        state.exit_timers.insert(key, timer);
    }

    /// An agent typed by hand reports no launch, and some agents no hook either. Its process
    /// names it; with no session it comes back on the newest session of the folder.
    fn find_agent(&self, task_id: &str, terminal: &str) {
        let key = key_of(task_id, terminal);
        let program = self.options.terminals.foreground(task_id, terminal);
        let in_foreground = self.in_foreground(task_id, terminal);
        let mut state = self.lock();
        let plain = state
            .entries
            .get(task_id)
            .and_then(|shells| shells.get(terminal))
            .is_some_and(|entry| entry.agent.is_none());
        let (Some(agent_in), Some(program), true, true) =
            (self.options.agent_in.clone(), program, plain, in_foreground)
        else {
            state.probed.remove(&key);
            return;
        };
        if state.probed.get(&key) == Some(&program) {
            return;
        }
        state.probed.insert(key, program);
        let this = self.this.clone();
        let (task, name) = (task_id.to_string(), terminal.to_string());
        // `ps` may take a moment: not in the caller's turn, and not on the timer thread.
        self.options.scheduler.off_thread(move || {
            let Some(agent) = agent_in(&task, &name) else {
                return;
            };
            let Some(this) = this.upgrade() else {
                return;
            };
            let still_plain = this
                .lock()
                .entries
                .get(&task)
                .and_then(|shells| shells.get(&name))
                .is_some_and(|entry| entry.agent.is_none());
            if still_plain {
                this.note_launch(&task, &name, &agent, None);
            }
        });
    }

    pub(super) fn poll_later(&self) -> TimerId {
        let this = self.this.clone();
        self.options.scheduler.after(RESTORE_POLL_MS, move || {
            if let Some(this) = this.upgrade() {
                this.check_starting();
            }
        })
    }

    fn check_starting(&self) {
        let now = self.options.scheduler.now();
        let mut changed: Vec<String> = Vec::new();
        let starting: Vec<(String, String, String, u64)> = self
            .lock()
            .starting
            .iter()
            .map(|(key, s)| (key.clone(), s.task_id.clone(), s.terminal.clone(), s.until))
            .collect();
        let mut done = Vec::new();
        for (key, task_id, terminal, until) in starting {
            let runs = self.in_foreground(&task_id, &terminal);
            if runs || !self.options.terminals.is_open(&task_id, &terminal) || now >= until {
                done.push(key);
                if !changed.contains(&task_id) {
                    changed.push(task_id);
                }
            }
        }
        {
            let mut state = self.lock();
            state.starting.retain(|(key, _)| !done.contains(key));
            state.poll = None;
            if !state.starting.is_empty() && !state.stopped {
                state.poll = Some(self.poll_later());
            }
        }
        for task_id in changed {
            self.publish_changed(&task_id);
        }
    }
}
