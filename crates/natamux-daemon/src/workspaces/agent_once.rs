//! `runAgentOnce` and `shellProcess` (`git/commit-drafter.ts`): runs an agent once for a short
//! answer, such as a commit message. The prompt of a draft is in `natamux_git::commit_drafter`.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use natamux_agents::launch::{Platform, one_shot_command, quote_argument};
use natamux_core::js::js_trim;
use natamux_git::commit_drafter::{AgentRun, DAEMON_VARIABLES, MAX_OUTPUT_CHARS, name_of};
use natamux_pty::Env;
use natamux_pty::shell_command::shell_command;

use crate::workspaces::service::RunAgentOnce;

const POLL: Duration = Duration::from_millis(10);
// How long the output readers get after the shell ended; a grandchild may hold the pipe open.
const PIPE_GRACE: Duration = Duration::from_millis(500);

/// The process that runs a command line in the user's login shell.
#[derive(Debug, PartialEq, Eq)]
pub struct ShellProcess {
    pub program: String,
    pub args: Vec<String>,
    pub env: Env,
    /// On Unix it leads its own process group, so a timeout can end all of it.
    pub detached: bool,
}

/// The process that runs a command line in the user's login shell, so the PATH is the one of a
/// terminal. The variables that reach the daemon are left out of its environment.
pub fn shell_process(line: &str, cwd: &str, daemon_env: &Env, windows: bool) -> ShellProcess {
    let env: Env = daemon_env
        .iter()
        .filter(|(name, _)| !DAEMON_VARIABLES.contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let (program, mut args) = shell_command(&env, windows, &|file| Path::new(file).exists());
    // A login profile may change folder, so go to the workspace after it.
    if windows {
        let folder = quote_argument(cwd, Platform::Windows);
        args = vec![
            "-Command".to_string(),
            format!("Set-Location -LiteralPath {folder}; {line}"),
        ];
    } else {
        let folder = quote_argument(cwd, Platform::Posix);
        args.extend(["-c".to_string(), format!("cd {folder} && {line}")]);
    }
    ShellProcess {
        program,
        args,
        env,
        detached: !windows,
    }
}

pub fn default_run_agent_once() -> RunAgentOnce {
    Arc::new(run_agent_once)
}

/// Runs an agent once and gives what it printed, or the reason for the user.
pub fn run_agent_once(run: AgentRun) -> Result<String, String> {
    let env: Env = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    run_with_env(run, &env)
}

// `daemon_env` is a parameter so a test can run with the variables a daemon has.
fn run_with_env(run: AgentRun, daemon_env: &Env) -> Result<String, String> {
    let windows = cfg!(windows);
    let platform = if windows {
        Platform::Windows
    } else {
        Platform::Posix
    };
    let one_shot = one_shot_command(&run.agent, &run.program, &run.prompt, platform);
    let cwd = run.cwd.to_string_lossy().into_owned();
    let shell = shell_process(&one_shot.line, &cwd, daemon_env, windows);
    let name = name_of(&run.program).to_string();

    let mut command = Command::new(&shell.program);
    command
        .args(&shell.args)
        .current_dir(&run.cwd)
        .env_clear()
        .envs(&shell.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    detach(&mut command, shell.detached);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{name} could not start: {error}"))?;

    // An agent that ends before it reads its input closes the pipe; its exit code tells the rest.
    let writer = child.stdin.take().map(|mut stdin| {
        thread::spawn(move || {
            let _ = stdin.write_all(one_shot.stdin.as_bytes());
        })
    });
    let too_much = Arc::new(AtomicBool::new(false));
    let (done, finished) = mpsc::channel();
    let mut readers = 0;
    let stdout = child.stdout.take().map(|pipe| {
        readers += 1;
        read_all(pipe, Some(Arc::clone(&too_much)), done.clone())
    });
    let stderr = child.stderr.take().map(|pipe| {
        readers += 1;
        read_all(pipe, None, done.clone())
    });
    drop(done);

    let failure = wait_for(&mut child, &run, &name, &too_much);
    wait_for_readers(&finished, readers);
    let stdout = stdout.map(text_of).unwrap_or_default();
    let stderr = stderr.map(text_of).unwrap_or_default();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    match failure {
        Failure::Ended(Some(0)) => Ok(stdout),
        Failure::Ended(code) => {
            let first = js_trim(&stderr).split('\n').next().map_or("", js_trim);
            let detail = if first.is_empty() {
                String::new()
            } else {
                format!(": {first}")
            };
            let code = code.map_or("null".to_string(), |code| code.to_string());
            Err(format!("{name} exited with code {code}{detail}"))
        }
        Failure::Stopped(reason) => Err(reason),
    }
}

enum Failure {
    /// The exit code; None when a signal ended it.
    Ended(Option<i32>),
    Stopped(String),
}

// Waits for the end of the agent; ends it and all it started on a timeout or too much output.
fn wait_for(child: &mut Child, run: &AgentRun, name: &str, too_much: &AtomicBool) -> Failure {
    let deadline = Instant::now() + Duration::from_millis(run.timeout_ms);
    let reason = loop {
        if too_much.load(Ordering::SeqCst) {
            break format!("{name} printed too much");
        }
        match child.try_wait() {
            Ok(Some(status)) => return Failure::Ended(status.code()),
            Ok(None) => {}
            Err(error) => break format!("{name} could not be waited for: {error}"),
        }
        if Instant::now() >= deadline {
            // JavaScript's Math.round: half up.
            break format!("{name} timed out after {} s", (run.timeout_ms + 500) / 1000);
        }
        thread::sleep(POLL);
    };
    kill_tree(child);
    let _ = child.wait();
    Failure::Stopped(reason)
}

/// What a pipe gave so far. The text is decoded across chunk borders; the length is in UTF-16
/// units, as Node counts it.
#[derive(Default)]
struct Collected {
    // The start of a character that the next chunk finishes.
    pending: Vec<u8>,
    text: String,
    units: usize,
}

impl Collected {
    fn push(&mut self, chunk: &[u8]) {
        self.pending.extend_from_slice(chunk);
        loop {
            let (valid, bad) = match std::str::from_utf8(&self.pending) {
                Ok(_) => (self.pending.len(), Some(0)),
                Err(error) => (error.valid_up_to(), error.error_len()),
            };
            let head: Vec<u8> = self.pending.drain(..valid).collect();
            self.append(&String::from_utf8_lossy(&head));
            match bad {
                Some(0) | None => break,
                Some(length) => {
                    self.append("\u{FFFD}");
                    self.pending.drain(..length);
                }
            }
        }
    }

    fn append(&mut self, text: &str) {
        self.units += text.chars().map(char::len_utf16).sum::<usize>();
        self.text.push_str(text);
    }

    // Keeps the last `limit` units; a character is never cut in two.
    fn keep_tail(&mut self, limit: usize) {
        let mut excess = self.units.saturating_sub(limit);
        if excess == 0 {
            return;
        }
        let mut cut = 0;
        for character in self.text.chars() {
            if excess == 0 {
                break;
            }
            excess = excess.saturating_sub(character.len_utf16());
            cut += character.len_utf8();
        }
        self.text.drain(..cut);
        self.units = self.text.chars().map(char::len_utf16).sum();
    }
}

type Shared = Arc<Mutex<Collected>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Collected> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read_all(
    mut pipe: impl Read + Send + 'static,
    cap: Option<Arc<AtomicBool>>,
    done: Sender<()>,
) -> Shared {
    let shared = Shared::default();
    let kept = Arc::clone(&shared);
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut collected = lock(&kept);
            collected.push(&chunk[..read]);
            if collected.units > MAX_OUTPUT_CHARS {
                match &cap {
                    Some(too_much) => {
                        too_much.store(true, Ordering::SeqCst);
                        break;
                    }
                    // The tail is what tells why it failed.
                    None => collected.keep_tail(MAX_OUTPUT_CHARS),
                }
            }
        }
        let _ = done.send(());
    });
    shared
}

// Waits until `count` readers are done, or the grace passed; what was read so far is kept.
fn wait_for_readers(finished: &Receiver<()>, count: usize) {
    let until = Instant::now() + PIPE_GRACE;
    for _ in 0..count {
        let left = until.saturating_duration_since(Instant::now());
        if finished.recv_timeout(left).is_err() {
            break;
        }
    }
}

fn text_of(shared: Shared) -> String {
    let mut collected = lock(&shared);
    let rest = std::mem::take(&mut collected.pending);
    let mut text = std::mem::take(&mut collected.text);
    text.push_str(&String::from_utf8_lossy(&rest));
    text
}

#[cfg(unix)]
fn detach(command: &mut Command, detached: bool) {
    use std::os::unix::process::CommandExt;
    if detached {
        command.process_group(0);
    }
}

#[cfg(windows)]
fn detach(command: &mut Command, _detached: bool) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW
    command.creation_flags(0x0800_0000);
}

// Ends the agent and every process it started. On Unix the agent leads its own process group.
fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    let ended = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid])
            .output()
    } else {
        Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{pid}")])
            .output()
    };
    // The group is gone already, or the tool is missing: end the child itself.
    if ended.is_err() {
        let _ = child.kill();
    }
}

#[cfg(test)]
#[path = "agent_once_tests.rs"]
mod tests;
