//! `generic-signals.ts`: what any agent in a task's first terminal tells by its output alone:
//! - output is a sign of life, so a running task does not look quiet while it prints;
//! - the terminal bell, which many command line tools ring when they wait for the user, moves a
//!   running task to needs input. Agents with hooks are left to their hooks;
//! - a shell that cannot find the agent fails the task.
//!
//! `on_output` runs on the terminal's output path, so it only takes a lock on a small map, checks
//! the clock and looks for the bell. The work on the task store goes over a channel to one thread.

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, PoisonError};

use natamux_core::js::{js_trim, parse_iso_millis, utf16_len};
use serde_json::{Value, json};

use super::runner::AgentLaunch;
use crate::shared::jobs::{Clock, system_clock};
use crate::tasks::records::ActivityOptions;
use crate::tasks::service::TaskService;
use crate::terminals::limits::MAIN_TERMINAL;

/// Agents whose hooks tell when they wait for the user. Copilot's hooks may be off until its
/// folder is trusted, and Cursor's CLI has no such hook, so the bell still counts for them.
const AGENTS_WITH_HOOKS: [&str; 3] = ["claude-code", "codex", "gemini"];

const BELL: u8 = 0x07;

/// Output comes in many small pieces. Looking at the task for each would read its file each time.
const LOOK_EVERY_MS: i64 = 10_000;
/// Some agents ring the bell as they start (GitHub Copilot does); that is not a question.
pub const START_BELL_MS: i64 = 15_000;
/// How long after a start the terminal is read for a shell that cannot find the agent.
pub const LAUNCH_WATCH_MS: i64 = 15_000;
/// The last bit of text kept to join a line that came in two pieces, in UTF-16 units.
const WATCH_TEXT_UNITS: usize = 4000;

/// What shells say when they do not know a command: bash, zsh, sh and dash, fish, PowerShell, cmd.
const NOT_FOUND: [&str; 4] = [
    "command not found",
    ": not found",
    "unknown command",
    "is not recognized",
];

/// The variable that sets an agent's command, as the daemon reads it.
fn command_variable(agent: &str) -> String {
    let name = if agent == "claude-code" {
        "CLAUDE".to_string()
    } else {
        agent.to_uppercase()
    };
    format!("NATAMUX_{name}_COMMAND")
}

/// The text without colors and cursor moves, which would split the words of a line:
/// `ESC [ params letter`, and `ESC ] ... (BEL | ESC \)`.
fn without_escapes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\x1b'
            && let Some(end) = escape_end(&chars, at)
        {
            at = end;
            continue;
        }
        out.push(chars[at]);
        at += 1;
    }
    out
}

/// Where the escape sequence that starts at `start` ends, if one does.
fn escape_end(chars: &[char], start: usize) -> Option<usize> {
    match *chars.get(start + 1)? {
        '[' => {
            let mut at = start + 2;
            while chars
                .get(at)
                .is_some_and(|c| c.is_ascii_digit() || matches!(*c, ';' | '?'))
            {
                at += 1;
            }
            chars
                .get(at)
                .filter(|c| c.is_ascii_alphabetic())
                .map(|_| at + 1)
        }
        ']' => {
            let mut at = start + 2;
            while let Some(c) = chars.get(at) {
                match *c {
                    '\x07' => return Some(at + 1),
                    '\x1b' => {
                        return (chars.get(at + 1) == Some(&'\\')).then_some(at + 2);
                    }
                    _ => at += 1,
                }
            }
            None
        }
        _ => None,
    }
}

/// The shell's line that says it could not find the program, if the text holds one.
pub fn not_found_line(text: &str, program: &str) -> Option<String> {
    let name = program.rsplit(['\\', '/']).next().unwrap_or(program);
    without_escapes(text)
        .split(['\r', '\n'])
        .map(js_trim)
        .find(|line| {
            let lower = line.to_lowercase();
            NOT_FOUND.iter().any(|words| lower.contains(words)) && line.contains(name)
        })
        .map(String::from)
}

/// The last `units` UTF-16 units of the text.
fn tail(text: String, units: usize) -> String {
    if utf16_len(&text) <= units {
        return text;
    }
    let mut used = 0;
    let mut start = text.len();
    for (index, c) in text.char_indices().rev() {
        used += c.len_utf16();
        if used > units {
            break;
        }
        start = index;
    }
    text[start..].to_string()
}

struct LaunchWatch {
    launch: AgentLaunch,
    at: i64,
    text: String,
}

#[derive(Default)]
struct Watching {
    last_look: HashMap<String, i64>,
    /// Its own clock: output just before a bell must not hide it.
    last_bell: HashMap<String, i64>,
    launches: HashMap<String, LaunchWatch>,
}

impl Watching {
    /// Whether a chunk of the first terminal sends a look job, and if it looks for a bell. A
    /// program that rings on every line would send a job, and a read of the task, per chunk.
    fn look_for(&mut self, task_id: &str, bell: bool, now: i64) -> Option<bool> {
        let due = |looks: &HashMap<String, i64>| {
            looks
                .get(task_id)
                .is_none_or(|last| now - last >= LOOK_EVERY_MS)
        };
        let bell = bell && due(&self.last_bell);
        if !bell && !due(&self.last_look) {
            return None;
        }
        if bell {
            self.last_bell.insert(task_id.to_string(), now);
        }
        self.last_look.insert(task_id.to_string(), now);
        Some(bell)
    }
}

/// Work for the signals thread.
enum Job {
    Look {
        task_id: String,
        terminal: String,
        bell: bool,
        now: i64,
    },
    Failed {
        task_id: String,
        launch: AgentLaunch,
        line: String,
    },
    /// Answered when every job before it is done.
    Barrier(Sender<()>),
}

pub struct GenericSignals {
    now: Clock,
    watching: Mutex<Watching>,
    jobs: Mutex<Sender<Job>>,
}

impl GenericSignals {
    /// `now`: milliseconds; the system clock by default.
    pub fn new(tasks: TaskService, now: Option<Clock>) -> GenericSignals {
        let (sender, receiver) = channel::<Job>();
        let work = Worker { tasks };
        // Ends when the service is dropped: the channel closes. If no thread can be made, jobs
        // are dropped and the signals are lost, which only costs the sign of life.
        let _ = std::thread::Builder::new()
            .name("generic-signals".into())
            .spawn(move || {
                for job in receiver {
                    work.run(job);
                }
            });
        GenericSignals {
            now: now.unwrap_or_else(system_clock),
            watching: Mutex::new(Watching::default()),
            jobs: Mutex::new(sender),
        }
    }

    fn send(&self, job: Job) {
        let jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = jobs.send(job);
    }

    /// Waits until the jobs sent so far are done. For tests.
    pub fn settle(&self) {
        let (done, wait) = channel();
        self.send(Job::Barrier(done));
        let _ = wait.recv();
    }

    /// Reads the terminal of an agent that just started, for a shell that does not know it.
    pub fn watch_launch(&self, launch: &AgentLaunch) {
        let key = format!("{}/{}", launch.task_id, launch.terminal);
        let watch = LaunchWatch {
            launch: launch.clone(),
            at: (self.now)(),
            text: String::new(),
        };
        self.watching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .launches
            .insert(key, watch);
    }

    pub fn on_output(&self, task_id: &str, terminal: &str, data: &[u8]) {
        let now = (self.now)();
        if let Some(job) = self.check_launch(task_id, terminal, data, now) {
            self.send(job);
        }
        if terminal != MAIN_TERMINAL {
            return;
        }
        let rang = data.contains(&BELL);
        let look = self
            .watching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .look_for(task_id, rang, now);
        let Some(bell) = look else {
            return;
        };
        self.send(Job::Look {
            task_id: task_id.to_string(),
            terminal: terminal.to_string(),
            bell,
            now,
        });
    }

    /// A shell that cannot find the agent prints so at once. The stage would wait for an agent
    /// that never came, so the task fails with the shell's words and what to do.
    fn check_launch(&self, task_id: &str, terminal: &str, data: &[u8], now: i64) -> Option<Job> {
        let mut watching = self.watching.lock().unwrap_or_else(PoisonError::into_inner);
        if watching.launches.is_empty() {
            return None;
        }
        let key = format!("{task_id}/{terminal}");
        let watch = watching.launches.get_mut(&key)?;
        if now - watch.at > LAUNCH_WATCH_MS {
            watching.launches.remove(&key);
            return None;
        }
        // A line can come in two pieces; the last bit of text is enough to join them. Decoded
        // only here, while a launch is watched: the rest needs no text.
        let joined = format!("{}{}", watch.text, String::from_utf8_lossy(data));
        watch.text = tail(joined, WATCH_TEXT_UNITS);
        let line = not_found_line(&watch.text, &watch.launch.program)?;
        let launch = watch.launch.clone();
        watching.launches.remove(&key);
        Some(Job::Failed {
            task_id: task_id.to_string(),
            launch,
            line,
        })
    }
}

/// The signals thread: the only one that writes the task store for output.
struct Worker {
    tasks: TaskService,
}

impl Worker {
    fn run(&self, job: Job) {
        match job {
            Job::Look {
                task_id,
                terminal,
                bell,
                now,
            } => self.look(&task_id, &terminal, bell, now),
            Job::Failed {
                task_id,
                launch,
                line,
            } => {
                let reason = format!(
                    "The agent did not start: \"{line}\". Install {}, or set {} to its command, \
                     then start the agent again.",
                    launch.program,
                    command_variable(&launch.agent)
                );
                self.apply(
                    &task_id,
                    json!({
                        "source": "system",
                        "type": "agent.failed",
                        "payload": { "reason": reason, "terminal": launch.terminal },
                    }),
                );
            }
            Job::Barrier(done) => {
                let _ = done.send(());
            }
        }
    }

    fn apply(&self, task_id: &str, input: Value) {
        if let Err(error) = self.tasks.apply_event(task_id, Some(&input)) {
            eprintln!("Could not apply a signal of {task_id}: {error}");
        }
    }

    fn look(&self, task_id: &str, terminal: &str, bell: bool, now: i64) {
        let Ok(task) = self.tasks.get_task(task_id) else {
            return;
        };
        if task.state != natamux_proto::tasks::task::TaskState::Running {
            return;
        }
        let has_hooks = task
            .agent
            .as_deref()
            .is_some_and(|agent| AGENTS_WITH_HOOKS.contains(&agent));
        if !has_hooks && bell {
            let launched = self.tasks.list_events(task_id).ok().and_then(|events| {
                events
                    .into_iter()
                    .rfind(|entry| entry.event.kind == "agent.launched")
            });
            let just_started = launched.is_some_and(|entry| {
                parse_iso_millis(&entry.event.at).is_some_and(|at| now - at < START_BELL_MS)
            });
            if just_started {
                return;
            }
            self.apply(
                task_id,
                json!({
                    "source": "agent",
                    "type": "agent.asked",
                    // Inside a group, the terminal tells which stage asks.
                    "payload": { "reason": "terminal bell", "terminal": terminal },
                }),
            );
            return;
        }
        // Output also comes from a redraw or the user's typing, so only a hook ends a background
        // wait.
        let options = ActivityOptions {
            ends_wait: false,
            ..ActivityOptions::default()
        };
        if let Err(error) = self.tasks.record_activity(task_id, options) {
            eprintln!("Could not record the activity of {task_id}: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    #[test]
    fn many_bells_of_a_task_send_one_look_per_look_time() {
        let mut watching = Watching::default();
        assert_eq!(watching.look_for("T-1", true, NOW), Some(true));
        for later in [1, 500, LOOK_EVERY_MS - 1] {
            assert_eq!(watching.look_for("T-1", true, NOW + later), None);
        }
        assert_eq!(
            watching.look_for("T-1", true, NOW + LOOK_EVERY_MS),
            Some(true)
        );
    }

    #[test]
    fn bells_of_two_tasks_send_a_look_each() {
        let mut watching = Watching::default();
        assert_eq!(watching.look_for("T-1", true, NOW), Some(true));
        assert_eq!(watching.look_for("T-2", true, NOW), Some(true));
    }

    #[test]
    fn output_just_before_a_bell_does_not_hide_it() {
        let mut watching = Watching::default();
        assert_eq!(watching.look_for("T-1", false, NOW), Some(false));
        assert_eq!(watching.look_for("T-1", false, NOW + 1), None);
        assert_eq!(watching.look_for("T-1", true, NOW + 2), Some(true));
    }

    #[test]
    fn a_held_back_bell_still_counts_as_output_when_a_look_is_due() {
        let mut watching = Watching::default();
        watching.last_bell.insert("T-1".to_string(), NOW);
        assert_eq!(watching.look_for("T-1", true, NOW + 1), Some(false));
    }
}
