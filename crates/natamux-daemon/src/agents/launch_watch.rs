//! The watch of an agent that just started: the last text of its terminal, read for a shell that
//! cannot find the agent, a sign-in screen, or the mark of an agent that ended with an error.
//!
//! A watch is taken out of the map while a chunk is read, so the text work holds no lock that
//! the output of the other terminals waits for.

use std::collections::HashMap;

use natamux_agents::custom::agent_kind;
use natamux_agents::sign_in::sign_in_line;

use super::Job;
use crate::agents::launch_text::{
    failed_mark_at, last_line, not_found_in, quoted, tail, without_escapes,
};
use crate::agents::runner::AgentLaunch;

/// The last bit of text kept to join a line that came in two pieces, in UTF-16 units.
const WATCH_TEXT_UNITS: usize = 4000;

pub(super) struct LaunchWatch {
    launch: AgentLaunch,
    pub(super) at: i64,
    text: String,
    /// Windows types the whole launch line, prompt and all, and the terminal echoes it. Its first
    /// line is left out once, so a prompt that names a sign-in line does not match.
    echo_left: bool,
}

impl LaunchWatch {
    pub(super) fn new(launch: AgentLaunch, at: i64, echo_left: bool) -> LaunchWatch {
        LaunchWatch {
            launch,
            at,
            text: String::new(),
            echo_left,
        }
    }

    /// Reads a chunk of the terminal. None: nothing was seen, the watch goes on. Else the watch
    /// ends, with the job for what was seen, if it needs one.
    pub(super) fn read(&mut self, data: &[u8]) -> Option<Option<Job>> {
        // A line can come in two pieces; the last bit of text is enough to join them. Decoded
        // only here, while a launch is watched: the rest needs no text.
        let decoded = String::from_utf8_lossy(data);
        let mut chunk: &str = &decoded;
        if self.echo_left {
            match chunk.split_once('\n') {
                Some((_, rest)) => {
                    self.echo_left = false;
                    chunk = rest;
                }
                None => chunk = "",
            }
        }
        let mut text = std::mem::take(&mut self.text);
        text.push_str(chunk);
        self.text = tail(text, WATCH_TEXT_UNITS);
        let launch = &self.launch;
        let task_id = || launch.task_id.clone();
        let plain = without_escapes(&self.text);
        if let Some(line) = not_found_in(&plain, &launch.program) {
            return Some(Some(Job::Failed {
                task_id: task_id(),
                launch: launch.clone(),
                line,
            }));
        }
        // Sign-in before the failed mark: the same output gives the same state, however the
        // terminal splits it into chunks.
        let kind = agent_kind(&launch.agent);
        if let Some(line) = sign_in_line(&kind, &plain) {
            return Some(Some(Job::SignIn {
                task_id: task_id(),
                launch: launch.clone(),
                line: quoted(&line),
            }));
        }
        let mark = failed_mark_at(&self.text)?;
        let line = last_line(&self.text[..mark]);
        // A restored agent goes on with an old session; one it cannot find is no failure.
        Some((!launch.restored).then(|| Job::Ended {
            task_id: task_id(),
            launch: launch.clone(),
            at: self.at,
            line,
        }))
    }
}

/// The watches by task, then terminal: a chunk finds its own with no new key, and this runs for
/// every chunk of every terminal while any launch is watched.
#[derive(Default)]
pub(super) struct Launches(HashMap<String, HashMap<String, LaunchWatch>>);

impl Launches {
    /// Watches a start. An older watch of the terminal ends.
    pub(super) fn insert(&mut self, watch: LaunchWatch) {
        let (task_id, terminal) = (&watch.launch.task_id, &watch.launch.terminal);
        let terminals = self.0.entry(task_id.clone()).or_default();
        terminals.insert(terminal.clone(), watch);
    }

    /// Takes the watch of a terminal out, to read a chunk. The chunks of one terminal come one
    /// after the other, so none misses the watch meanwhile.
    pub(super) fn take(&mut self, task_id: &str, terminal: &str) -> Option<LaunchWatch> {
        let terminals = self.0.get_mut(task_id)?;
        let watch = terminals.remove(terminal)?;
        if terminals.is_empty() {
            self.0.remove(task_id);
        }
        Some(watch)
    }

    /// Puts a watch back after a chunk. An agent started in the terminal meanwhile has a watch
    /// of its own, which stays: it would have replaced this one.
    pub(super) fn put_back(&mut self, watch: LaunchWatch) {
        let (task_id, terminal) = (&watch.launch.task_id, &watch.launch.terminal);
        let terminals = self.0.entry(task_id.clone()).or_default();
        terminals.entry(terminal.clone()).or_insert(watch);
    }
}

#[cfg(test)]
#[path = "launch_watch_tests.rs"]
mod tests;
