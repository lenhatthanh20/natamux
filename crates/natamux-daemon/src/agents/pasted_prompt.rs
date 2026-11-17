//! The first prompt of an agent that takes none on its command line. The agent starts without
//! it, and the prompt is pasted into its terminal once its first screen settled: the agent is in
//! front of the shell, it printed, then printed nothing for `SETTLE_MS`. With no output, it is
//! pasted after `LAUNCH_WATCH_MS` anyway, so the prompt is not lost. Each prompt is pasted once.
//!
//! The prompt is never typed into the shell itself: the shell would run it as a command, and it
//! can hold the text of an imported issue. So the shell's echo of the launch line does not count
//! as output, and the prompt is dropped when the agent ended (the shell is in front again, or the
//! launch line's title of a failed or ended agent came). Where the platform does not tell the
//! program in front (Windows), only those titles tell.
//!
//! `on_output` runs on the terminal's output path: one lock on a small map, no other work.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use natamux_proto::terminals::list::SHELL_PROGRAMS;

use super::generic_signals::LAUNCH_WATCH_MS;
use super::runner::{AGENT_EXITED_TITLE, AGENT_FAILED_TITLE};
use crate::shared::jobs::Clock;

/// Quiet time after the agent's first output that means its screen is ready for input.
pub const SETTLE_MS: i64 = 1_500;
/// How often the held prompts are checked.
const TICK: Duration = Duration::from_millis(250);
/// Enough of the last output to find a title that comes in two pieces.
const TAIL_CHARS: usize = 64;

/// Pastes a text into a terminal: task id, terminal, text.
pub type Paste = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;
/// The program in front of a terminal's shell: task id, terminal. None where the platform does
/// not tell.
pub type Front = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

struct Held {
    text: String,
    at: i64,
    last_output: Option<i64>,
    /// When the agent was first seen in front of the shell.
    agent_since: Option<i64>,
    /// The launch line's title of a failed or ended agent came.
    ended: bool,
    tail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Wait,
    Paste,
    Drop,
}

pub struct PastedPrompts {
    now: Clock,
    paste: Paste,
    front: Front,
    held: Mutex<HashMap<String, Held>>,
    ticking: AtomicBool,
}

fn key(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

fn is_shell(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    SHELL_PROGRAMS.contains(&name)
}

/// Whether the text holds the title the launch line sets when the agent failed or ended. The
/// typed line on Windows holds the title's words too, but not the escape before them.
fn ends_agent(text: &str) -> bool {
    [AGENT_FAILED_TITLE, AGENT_EXITED_TITLE]
        .iter()
        .any(|title| {
            ["0", "2"]
                .iter()
                .any(|code| text.contains(&format!("\x1b]{code};{title}")))
        })
}

fn keep_tail(text: &str) -> String {
    let count = text.chars().count();
    text.chars()
        .skip(count.saturating_sub(TAIL_CHARS))
        .collect()
}

impl Held {
    fn verdict(&mut self, front: Option<&str>, now: i64) -> Verdict {
        if self.ended {
            return Verdict::Drop;
        }
        let timed_out = now - self.at >= LAUNCH_WATCH_MS;
        match front {
            // The agent ended, or never came up: the prompt must not reach the shell.
            Some(program) if is_shell(program) => {
                if self.agent_since.is_some() || timed_out {
                    Verdict::Drop
                } else {
                    Verdict::Wait
                }
            }
            Some(_) => {
                let since = *self.agent_since.get_or_insert(now);
                // Output before the agent came is the shell's echo.
                let settled = self
                    .last_output
                    .is_some_and(|last| last >= since && now - last >= SETTLE_MS);
                if settled || timed_out {
                    Verdict::Paste
                } else {
                    Verdict::Wait
                }
            }
            None => {
                let settled = self.last_output.is_some_and(|last| now - last >= SETTLE_MS);
                if settled || timed_out {
                    Verdict::Paste
                } else {
                    Verdict::Wait
                }
            }
        }
    }
}

impl PastedPrompts {
    pub fn new(now: Clock, paste: Paste, front: Front) -> Arc<PastedPrompts> {
        Arc::new(PastedPrompts {
            now,
            paste,
            front,
            held: Mutex::new(HashMap::new()),
            ticking: AtomicBool::new(false),
        })
    }

    /// Holds the prompt of an agent that just started in this terminal. A newer start in the
    /// same terminal replaces an older prompt that was not pasted yet.
    pub fn hold(self: &Arc<Self>, task_id: &str, terminal: &str, text: &str) {
        let held = Held {
            text: text.to_string(),
            at: (self.now)(),
            last_output: None,
            agent_since: None,
            ended: false,
            tail: String::new(),
        };
        self.lock().insert(key(task_id, terminal), held);
        self.start_ticking();
    }

    pub fn on_output(&self, task_id: &str, terminal: &str, data: &[u8]) {
        let mut held = self.lock();
        if held.is_empty() {
            return;
        }
        if let Some(prompt) = held.get_mut(&key(task_id, terminal)) {
            prompt.last_output = Some((self.now)());
            let text = format!("{}{}", prompt.tail, String::from_utf8_lossy(data));
            prompt.ended = prompt.ended || ends_agent(&text);
            prompt.tail = keep_tail(&text);
        }
    }

    /// Whether a prompt waits for this terminal. For tests.
    pub fn is_held(&self, task_id: &str, terminal: &str) -> bool {
        self.lock().contains_key(&key(task_id, terminal))
    }

    /// Pastes each prompt that is due, and forgets it and each one that can no longer be.
    pub fn tick(&self) {
        let keys: Vec<String> = self.lock().keys().cloned().collect();
        // Asked with no lock held: the output path takes this lock under the terminal's own.
        let fronts: Vec<(String, Option<String>)> = keys
            .into_iter()
            .map(|key| {
                let front = key
                    .split_once('/')
                    .and_then(|(task_id, terminal)| (self.front)(task_id, terminal));
                (key, front)
            })
            .collect();
        let now = (self.now)();
        let due: Vec<(String, String)> = {
            let mut held = self.lock();
            let mut due = Vec::new();
            for (key, front) in fronts {
                let Some(prompt) = held.get_mut(&key) else {
                    continue;
                };
                match prompt.verdict(front.as_deref(), now) {
                    Verdict::Wait => {}
                    Verdict::Drop => {
                        held.remove(&key);
                    }
                    Verdict::Paste => {
                        if let Some(prompt) = held.remove(&key) {
                            due.push((key, prompt.text));
                        }
                    }
                }
            }
            due
        };
        // Pasted with no lock held: the paste writes to the terminal.
        for (key, text) in due {
            if let Some((task_id, terminal)) = key.split_once('/') {
                (self.paste)(task_id, terminal, &text);
            }
        }
    }

    /// One thread for all held prompts, started at the first one. It ends with the service.
    fn start_ticking(self: &Arc<Self>) {
        if self.ticking.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak: Weak<PastedPrompts> = Arc::downgrade(self);
        let started = std::thread::Builder::new()
            .name("pasted-prompts".into())
            .spawn(move || {
                while let Some(prompts) = weak.upgrade() {
                    prompts.tick();
                    drop(prompts);
                    std::thread::sleep(TICK);
                }
            });
        if started.is_err() {
            // No thread: the next hold tries again.
            self.ticking.store(false, Ordering::SeqCst);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Held>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "pasted_prompt_tests.rs"]
mod tests;
