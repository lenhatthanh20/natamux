//! `usage-service.ts`: reads the token usage of agent sessions after their turns, and keeps it per
//! task and stage in `<tasks folder>/<TASK>.usage.json`.
//!
//! It also keeps the plan limits the agents tell, in `<data>/limits.json`: Codex's from its
//! session file at each read, Claude Code's from its status line (`set_limits`).
//!
//! `seen` returns at once. The session file is read on a thread of its own, one read per file at
//! a time; a hook that comes during a read makes the file be read again after it.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

use natamux_agents::usage::limits::read_codex_limits;
use natamux_agents::usage::{session_file, usage_reader};
use natamux_core::js::{iso_now, parse_iso_millis};
use natamux_core::json::to_js_string;
use natamux_proto::agents::limits::{AgentLimits, LimitWindow};
use natamux_store::task_files::tasks_dir;
use serde_json::{Map, Value};

use super::usage_file::{
    Seen, Stored, apply_read, stored_from, stored_value, task_id_of_file, task_usage,
};
use crate::shared::event_bus::{DaemonMessage, EventBus};

pub fn usage_path(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}.usage.json"))
}

pub fn limits_path(data_dir: &Path) -> PathBuf {
    data_dir.join("limits.json")
}

/// The windows that have not reset yet, at `now_ms`. A window with no reset time lasts its length
/// from when it was told.
pub fn current_limits(limits: &[AgentLimits], now_ms: i64) -> Vec<AgentLimits> {
    limits
        .iter()
        .map(|each| {
            let seen = parse_iso_millis(&each.seen_at).unwrap_or(0);
            AgentLimits {
                windows: each
                    .windows
                    .iter()
                    .filter(|window| {
                        let end = window
                            .resets_at
                            .as_deref()
                            .and_then(parse_iso_millis)
                            .unwrap_or(seen + i64::from(window.minutes) * 60_000);
                        end > now_ms
                    })
                    .cloned()
                    .collect(),
                ..each.clone()
            }
        })
        .filter(|each| !each.windows.is_empty())
        .collect()
}

/// The windows an agent told now over the ones known: two sessions read at once may tell them in
/// either order, so a window that resets later than the told one stays, and of the same window
/// the higher share wins, as a share only grows until the reset.
pub fn merge_windows(known: &[LimitWindow], told: Vec<LimitWindow>) -> Vec<LimitWindow> {
    let reset = |window: &LimitWindow| window.resets_at.as_deref().and_then(parse_iso_millis);
    told.into_iter()
        .map(|new| {
            let Some(old) = known.iter().find(|old| old.minutes == new.minutes) else {
                return new;
            };
            match (reset(old), reset(&new)) {
                (Some(old_at), Some(new_at)) if old_at > new_at => old.clone(),
                (Some(old_at), Some(new_at)) if old_at == new_at => LimitWindow {
                    used_percent: new.used_percent.max(old.used_percent),
                    ..new
                },
                _ => new,
            }
        })
        .collect()
}

pub struct UsageOptions {
    pub data_dir: PathBuf,
    pub bus: Option<Arc<EventBus>>,
    /// The folder that session files must be in. The user's home folder in the daemon.
    pub home: PathBuf,
}

#[derive(Default)]
struct State {
    usage: Vec<(String, Stored)>,
    /// The files read now, with the last hook that came during the read: it is read again after.
    reading: HashMap<String, Option<Seen>>,
    /// Tasks deleted while a read of theirs ran: that read must not bring their usage back.
    deleted: HashSet<String>,
    /// How many threads read now.
    running: usize,
    /// By agent, in the order they first told them.
    limits: Vec<AgentLimits>,
}

struct Inner {
    options: UsageOptions,
    state: Mutex<State>,
    idle: Condvar,
}

/// Reads the token usage of agent sessions after their turns, and keeps it per task and stage.
pub struct UsageService {
    inner: Arc<Inner>,
}

impl UsageService {
    pub fn new(options: UsageOptions) -> UsageService {
        let bus = options.bus.clone();
        let inner = Arc::new(Inner {
            options,
            state: Mutex::new(State::default()),
            idle: Condvar::new(),
        });
        inner.load();
        if let Some(bus) = bus {
            let weak: Weak<Inner> = Arc::downgrade(&inner);
            bus.subscribe(move |message| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_message(message);
                }
            });
        }
        UsageService { inner }
    }

    /// Called after a turn of an agent in a task. Returns at once; the file is read in the
    /// background.
    pub fn seen(&self, task_id: &str, stage: Option<&str>, agent: &str, transcript: &str) {
        if usage_reader(agent).is_none() {
            return;
        }
        let next = Seen {
            task_id: task_id.to_string(),
            stage: stage.unwrap_or("").to_string(),
            agent: agent.to_string(),
            transcript: transcript.to_string(),
        };
        {
            let mut state = self.inner.state();
            if let Some(waiting) = state.reading.get_mut(transcript) {
                *waiting = Some(next);
                return;
            }
            state.reading.insert(transcript.to_string(), None);
            state.running += 1;
        }
        let inner = Arc::clone(&self.inner);
        let spawned = std::thread::Builder::new()
            .name("usage-read".into())
            .spawn(move || inner.read_until_done(next));
        if let Err(error) = spawned {
            eprintln!("Could not read the usage of {task_id}: {error}");
            let mut state = self.inner.state();
            state.reading.remove(transcript);
            state.running -= 1;
            self.inner.idle.notify_all();
        }
    }

    pub fn usage_of(&self, task_id: &str) -> Option<Value> {
        let state = self.inner.state();
        let (_, stored) = state.usage.iter().find(|(known, _)| known == task_id)?;
        Some(task_usage(stored))
    }

    /// `{ taskId: usage }`, in the order the tasks got their usage.
    pub fn all(&self) -> Map<String, Value> {
        let state = self.inner.state();
        state
            .usage
            .iter()
            .map(|(task_id, stored)| (task_id.clone(), task_usage(stored)))
            .collect()
    }

    /// The windows an agent told now. No windows change nothing.
    pub fn set_limits(&self, agent: &str, windows: Vec<LimitWindow>) {
        self.inner.set_limits(agent, windows);
    }

    /// Every agent's windows that have not reset yet.
    pub fn limits(&self) -> Vec<AgentLimits> {
        let now = parse_iso_millis(&iso_now()).unwrap_or(0);
        current_limits(&self.inner.state().limits, now)
    }

    /// Waits for the reads that run now. For tests.
    pub fn settle(&self) {
        let mut state = self.inner.state();
        while state.running > 0 {
            state = self
                .inner
                .idle
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn on_message(&self, message: &DaemonMessage) {
        let mut state = self.state();
        match message {
            DaemonMessage::TaskDeleted { task_id } => {
                state.usage.retain(|(known, _)| known != task_id);
                state.deleted.insert(task_id.clone());
            }
            DaemonMessage::TaskCreated { task } => {
                state.deleted.remove(&task.id);
            }
            _ => {}
        }
    }

    fn read_until_done(&self, first: Seen) {
        let transcript = first.transcript.clone();
        let mut current = first;
        loop {
            let read = catch_unwind(AssertUnwindSafe(|| self.read(&current)));
            if read.is_err() {
                eprintln!("Could not read the usage of {}", current.task_id);
            }
            let mut state = self.state();
            let next = state.reading.get_mut(&transcript).and_then(Option::take);
            match next {
                Some(next) => current = next,
                None => {
                    state.reading.remove(&transcript);
                    state.running -= 1;
                    self.idle.notify_all();
                    return;
                }
            }
        }
    }

    fn read(&self, seen: &Seen) {
        let Some(reader) = usage_reader(&seen.agent) else {
            return;
        };
        let Some(path) = session_file(&seen.transcript, &self.options.home) else {
            return;
        };
        if seen.agent == "codex"
            && let Some(windows) = read_codex_limits(&path)
        {
            self.set_limits(&seen.agent, windows);
        }
        let now = reader(&path);
        let usage = {
            let mut state = self.state();
            if state.deleted.contains(&seen.task_id) {
                return;
            }
            let at = state
                .usage
                .iter()
                .position(|(known, _)| *known == seen.task_id);
            let mut stored = at.map_or_else(Stored::default, |at| state.usage[at].1.clone());
            if !apply_read(&mut stored, seen, now) {
                return;
            }
            match at {
                Some(at) => state.usage[at].1 = stored.clone(),
                None => state.usage.push((seen.task_id.clone(), stored.clone())),
            }
            if let Err(error) = self.save(&seen.task_id, &stored) {
                eprintln!("Could not read the usage of {}: {error}", seen.task_id);
                return;
            }
            task_usage(&stored)
        };
        if let Some(bus) = &self.options.bus {
            bus.publish(DaemonMessage::UsageChanged {
                task_id: seen.task_id.clone(),
                usage,
            });
        }
    }

    fn save(&self, task_id: &str, stored: &Stored) -> std::io::Result<()> {
        std::fs::create_dir_all(tasks_dir(&self.options.data_dir))?;
        let path = usage_path(&self.options.data_dir, task_id);
        let temporary = path.with_file_name(format!("{task_id}.usage.json.tmp"));
        std::fs::write(&temporary, to_js_string(&stored_value(stored)))?;
        std::fs::rename(&temporary, &path)
    }

    fn set_limits(&self, agent: &str, windows: Vec<LimitWindow>) {
        if windows.is_empty() {
            return;
        }
        let list = {
            let mut state = self.state();
            let at = state.limits.iter().position(|each| each.agent == agent);
            let known = at.map_or(&[][..], |at| &state.limits[at].windows[..]);
            let windows = merge_windows(known, windows);
            if known == windows.as_slice() {
                return;
            }
            let told = AgentLimits {
                agent: agent.to_string(),
                windows,
                seen_at: iso_now(),
            };
            match at {
                Some(at) => state.limits[at] = told,
                None => state.limits.push(told),
            }
            let value = serde_json::to_value(&state.limits).unwrap_or(Value::Null);
            if let Err(error) = save_atomic(&limits_path(&self.options.data_dir), &value) {
                eprintln!("Could not keep the plan limits: {error}");
            }
            value
        };
        if let Some(bus) = &self.options.bus {
            bus.publish(DaemonMessage::LimitsChanged { limits: list });
        }
    }

    /// A missing or broken file reads as no usage.
    fn load(&self) {
        self.state().limits = std::fs::read_to_string(limits_path(&self.options.data_dir))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let dir = tasks_dir(&self.options.data_dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        // As Node lists a folder: by name.
        names.sort();
        let mut state = self.state();
        for name in names {
            let Some(task_id) = task_id_of_file(&name) else {
                continue;
            };
            let stored = std::fs::read_to_string(dir.join(&name))
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|value| stored_from(&value));
            if let Some(stored) = stored {
                state.usage.push((task_id.to_string(), stored));
            }
        }
    }
}

fn save_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, to_js_string(value))?;
    std::fs::rename(&temporary, path)
}
