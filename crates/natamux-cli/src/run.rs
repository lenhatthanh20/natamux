//! The command's entry (`packages/cli/src/run.ts`): exit codes, the token and the task of the
//! terminal.

use std::collections::HashMap;

use crate::command::{Command, parse_command};
use crate::daemon::{DaemonClient, Transport};
use crate::execute::{execute, print_json};
use crate::format::{DEFAULT_URL, usage};
use crate::hook::run_hook_command;
use crate::hooks::claude_code_hook_settings;

pub const EXIT_OK: u8 = 0;
pub const EXIT_FAILED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;

/// Where the command prints. Tests keep the text.
pub trait Io {
    fn stdout(&mut self, text: &str);
    fn stderr(&mut self, text: &str);
}

#[derive(Clone, Debug, Default)]
pub struct Env(HashMap<String, String>);

impl Env {
    pub fn from_process() -> Env {
        Env(std::env::vars_os()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
            .collect())
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// The value without spaces around it, None when that is empty.
    pub fn trimmed(&self, name: &str) -> Option<&str> {
        self.get(name)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Env {
    fn from(pairs: [(&str, &str); N]) -> Env {
        Env(pairs
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect())
    }
}

/// The address of the daemon, with no "/" at the end.
pub fn daemon_url(env: &Env) -> String {
    env.trimmed("NATAMUX_URL")
        .unwrap_or(DEFAULT_URL)
        .trim_end_matches('/')
        .to_string()
}

/// `ask`, `done` and `task show` run inside a task's terminal, which knows its task: without an
/// id they take NATAMUX_TASK_ID. Err is what to say when it is not set.
fn task_of_terminal(command: &mut Command, env: &Env) -> Result<(), &'static str> {
    let (id, missing) = match command {
        Command::AgentAsk { id, .. } | Command::AgentDone { id, .. } => (
            id,
            "Name the task with --task T-42, or set NATAMUX_TASK_ID.\n",
        ),
        Command::TaskShow { id, .. } => (
            id,
            "Name the task, for example natamux task show T-42, or set NATAMUX_TASK_ID.\n",
        ),
        _ => return Ok(()),
    };
    if id.is_none() {
        *id = Some(env.trimmed("NATAMUX_TASK_ID").ok_or(missing)?.to_string());
    }
    Ok(())
}

/// Returns the exit code: 0 done, 1 the request failed, 2 the command line is wrong.
/// `connect(base_url, token)` makes the transport to the daemon.
pub async fn run_cli<T: Transport>(
    argv: &[String],
    env: &Env,
    io: &mut dyn Io,
    read_stdin: &dyn Fn() -> Vec<u8>,
    connect: &dyn Fn(&str, &str) -> T,
) -> u8 {
    if argv.first().map(String::as_str) == Some("hook") {
        return run_hook_command(
            argv.get(1..).unwrap_or_default(),
            env,
            io,
            read_stdin,
            connect,
        )
        .await;
    }

    let mut command = match parse_command(argv) {
        Ok(command) => command,
        Err(error) => {
            io.stderr(&format!("{}\nRun \"natamux help\" for usage.\n", error.0));
            return EXIT_USAGE;
        }
    };

    match &command {
        Command::Help => {
            io.stdout(&usage());
            return EXIT_OK;
        }
        // It only prints settings, so it needs no token and does not call the daemon.
        Command::HooksClaudeCode => {
            print_json(io, &claude_code_hook_settings(&daemon_url(env)));
            return EXIT_OK;
        }
        _ => {}
    }

    if let Err(message) = task_of_terminal(&mut command, env) {
        io.stderr(message);
        return EXIT_USAGE;
    }

    let Some(token) = env.trimmed("NATAMUX_TOKEN") else {
        io.stderr("NATAMUX_TOKEN is not set. Use the token the daemon printed when it started.\n");
        return EXIT_USAGE;
    };
    let base_url = daemon_url(env);
    // shortcut: the HTTP client speaks plain HTTP only (no TLS without a new crate); the daemon
    // listens on plain HTTP on this machine.
    if !base_url.starts_with("http://") {
        io.stderr(&format!(
            "Error: NATAMUX_URL must start with http:// (got {base_url:?}).\n"
        ));
        return EXIT_FAILED;
    }
    let client = DaemonClient::new(&base_url, connect(&base_url, token));
    match execute(&command, &client, io, env).await {
        Ok(()) => EXIT_OK,
        Err(message) => {
            io.stderr(&format!("Error: {message}\n"));
            EXIT_FAILED
        }
    }
}
