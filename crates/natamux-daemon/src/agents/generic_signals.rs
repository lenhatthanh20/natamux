//! `generic-signals.ts`: what any agent in a task's first terminal tells by its output alone:
//! - output is a sign of life, so a running task does not look quiet while it prints;
//! - the terminal bell, which many command line tools ring when they wait for the user, moves a
//!   running task to needs input. Agents with hooks are left to their hooks;
//! - a shell that cannot find the agent fails the task;
//! - an agent that shows its sign-in screen moves the task to needs input: it waits for the user,
//!   and no hook runs before the login;
//! - an agent that ends with an error right after its start, before it reported, fails the task.
//!
//! `on_output` runs on the terminal's output path, so it only takes a lock on a small map, checks
//! the clock and looks for the bell. The work on the task store goes over a channel to one thread.

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, PoisonError};

use natamux_agents::custom::{agent_kind, custom_agent};
use natamux_agents::sign_in::sign_in_line;
use natamux_core::js::parse_iso_millis;
use serde_json::{Value, json};

use super::launch_text::{
    BellScan, failed_mark_at, last_line, not_found_line, quoted, tail, without_escapes,
};
use super::runner::AgentLaunch;
use crate::shared::jobs::{Clock, system_clock};
use crate::tasks::records::ActivityOptions;
use crate::tasks::service::TaskService;
use crate::terminals::limits::MAIN_TERMINAL;

/// Agents whose hooks tell when they wait for the user. Copilot's hooks may be off until its
/// folder is trusted, and Cursor's CLI has no such hook, so the bell still counts for them.
const AGENTS_WITH_HOOKS: [&str; 3] = ["claude-code", "codex", "gemini"];

/// Output comes in many small pieces. Looking at the task for each would read its file each time.
const LOOK_EVERY_MS: i64 = 10_000;
/// Some agents ring the bell as they start (GitHub Copilot does); that is not a question.
pub const START_BELL_MS: i64 = 15_000;
/// How long after a start the terminal is read for a shell that cannot find the agent, a sign-in
/// screen, or an agent that ended. As long as the launch grace of a shell: an agent installed
/// with npm can take more than 15 s to show its first screen on a cold start.
pub const LAUNCH_WATCH_MS: i64 = 30_000;
/// The last bit of text kept to join a line that came in two pieces, in UTF-16 units.
const WATCH_TEXT_UNITS: usize = 4000;

/// The part of the daemon's variables for an agent: `NATAMUX_<name>_COMMAND`.
fn variable_name(agent: &str) -> String {
    if agent == "claude-code" {
        "CLAUDE".to_string()
    } else {
        agent.to_uppercase()
    }
}

/// Where the user changes an agent's command: the variable the daemon reads, or Settings > Agents
/// for a custom agent.
fn command_hint(agent: &str) -> String {
    if custom_agent(agent).is_some() {
        return "change its command in Settings > Agents".into();
    }
    let name = variable_name(agent);
    format!("set NATAMUX_{name}_COMMAND to its command")
}

/// Where the user changes the words an agent starts with.
fn words_hint(agent: &str) -> String {
    if custom_agent(agent).is_some() {
        return "its command in Settings > Agents".into();
    }
    let name = variable_name(agent);
    format!("the words in NATAMUX_{name}_ARGS")
}

/// The words hint, after the task's model when one was passed: a wrong name can end the agent.
fn launch_hint(launch: &AgentLaunch) -> String {
    let words = words_hint(&launch.agent);
    match &launch.model {
        Some(model) => format!("the model {model} (Edit the task), {words}"),
        None => words,
    }
}

fn terminal_label(terminal: &str) -> String {
    format!("Terminal {terminal}")
}

struct LaunchWatch {
    launch: AgentLaunch,
    at: i64,
    text: String,
    /// Windows types the whole launch line, prompt and all, and the terminal echoes it. Its first
    /// line is left out once, so a prompt that names a sign-in line does not match.
    echo_left: bool,
}

#[derive(Default)]
struct Watching {
    last_look: HashMap<String, i64>,
    /// Its own clock: output just before a bell must not hide it.
    last_bell: HashMap<String, i64>,
    /// Where the output of each task's first terminal stands in an OSC sequence.
    bell_scans: HashMap<String, BellScan>,
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
    SignIn {
        task_id: String,
        launch: AgentLaunch,
        line: String,
    },
    Ended {
        task_id: String,
        launch: AgentLaunch,
        /// When the launch was, so an agent that reported since is left alone.
        at: i64,
        line: Option<String>,
    },
    /// Answered when every job before it is done.
    Barrier(Sender<()>),
}

pub struct GenericSignals {
    now: Clock,
    windows: bool,
    watching: Mutex<Watching>,
    jobs: Mutex<Sender<Job>>,
}

impl GenericSignals {
    /// `now`: milliseconds; the system clock by default.
    pub fn new(tasks: TaskService, now: Option<Clock>) -> GenericSignals {
        GenericSignals::with_platform(tasks, now, cfg!(windows))
    }

    /// As `new`, for the launch lines of Windows or not.
    pub fn with_platform(tasks: TaskService, now: Option<Clock>, windows: bool) -> GenericSignals {
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
            windows,
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
            echo_left: self.windows,
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
        let look = {
            let mut watching = self.watching.lock().unwrap_or_else(PoisonError::into_inner);
            // No new key per chunk: this runs on the output path.
            let rang = match watching.bell_scans.get_mut(task_id) {
                Some(scan) => scan.rings(data),
                None => {
                    let mut scan = BellScan::default();
                    let rang = scan.rings(data);
                    watching.bell_scans.insert(task_id.to_string(), scan);
                    rang
                }
            };
            watching.look_for(task_id, rang, now)
        };
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

    /// A shell that cannot find the agent prints so at once, and an agent with no login shows
    /// its sign-in screen or ends. The stage would wait for an agent that never came, so the
    /// task fails, or waits for the user, with the words seen and what to do.
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
        let decoded = String::from_utf8_lossy(data);
        let mut chunk: &str = &decoded;
        if watch.echo_left {
            match chunk.split_once('\n') {
                Some((_, rest)) => {
                    watch.echo_left = false;
                    chunk = rest;
                }
                None => chunk = "",
            }
        }
        let joined = format!("{}{chunk}", watch.text);
        watch.text = tail(joined, WATCH_TEXT_UNITS);
        let task_id = task_id.to_string();
        let launch = watch.launch.clone();
        if let Some(line) = not_found_line(&watch.text, &launch.program) {
            watching.launches.remove(&key);
            return Some(Job::Failed {
                task_id,
                launch,
                line,
            });
        }
        // Sign-in before the failed mark: the same output gives the same state, however the
        // terminal splits it into chunks.
        let kind = agent_kind(&launch.agent);
        if let Some(line) = sign_in_line(&kind, &without_escapes(&watch.text)) {
            watching.launches.remove(&key);
            return Some(Job::SignIn {
                task_id,
                launch,
                line: quoted(&line),
            });
        }
        let mark = failed_mark_at(&watch.text)?;
        let line = last_line(&watch.text[..mark]);
        let at = watch.at;
        watching.launches.remove(&key);
        // A restored agent goes on with an old session; one it cannot find is no failure.
        (!launch.restored).then_some(Job::Ended {
            task_id,
            launch,
            at,
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
                    "The agent did not start: \"{line}\". Install {}, or {}, then start the agent \
                     again.",
                    launch.program,
                    command_hint(&launch.agent)
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
            Job::SignIn {
                task_id,
                launch,
                line,
            } => {
                let message = format!(
                    "{} asks you to sign in: \"{line}\". Sign in in {}. If the agent then lost \
                     the prompt, send it again.",
                    launch.program,
                    terminal_label(&launch.terminal)
                );
                self.apply(
                    &task_id,
                    json!({
                        "source": "system",
                        "type": "agent.asked",
                        "payload": {
                            "reason": "sign in",
                            "message": message,
                            "terminal": launch.terminal,
                        },
                    }),
                );
            }
            Job::Ended {
                task_id,
                launch,
                at,
                line,
            } => self.ended(&task_id, &launch, at, line.as_deref()),
            Job::Barrier(done) => {
                let _ = done.send(());
            }
        }
    }

    /// Fails the task of an agent that ended with an error right after its start. One that
    /// reported since its launch was up, so a person or the agent ended it later: left alone.
    fn ended(&self, task_id: &str, launch: &AgentLaunch, at: i64, line: Option<&str>) {
        let Ok(events) = self.tasks.list_events(task_id) else {
            return;
        };
        let reported = events.iter().any(|entry| {
            entry.event.source == "agent"
                && parse_iso_millis(&entry.event.at).is_some_and(|when| when >= at)
        });
        if reported {
            return;
        }
        let said = line.map_or(String::new(), |line| format!(": \"{line}\""));
        let reason = format!(
            "The agent ended right after it started{said}. Read {} for the cause (sign-in, \
             version, {}), fix it, then start the agent again.",
            terminal_label(&launch.terminal),
            launch_hint(launch)
        );
        self.apply(
            task_id,
            json!({
                "source": "system",
                "type": "agent.failed",
                "payload": { "reason": reason, "terminal": launch.terminal },
            }),
        );
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
            .is_some_and(|agent| AGENTS_WITH_HOOKS.contains(&agent_kind(agent).as_str()));
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
