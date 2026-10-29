//! Ports `packages/daemon/src/git/exec.ts`: the `git` helper, `GitError`, `withGitConfig`,
//! and the error text of a git that is missing.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use natamux_core::js::js_trim;

use crate::process::{CapturedBytes, capture_bytes};

/// `MAX_OUTPUT_BYTES` of `git/exec.ts`.
pub const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

/// `GITHUB_CREDENTIALS`: when git asks for a password for github.com, it asks the GitHub CLI.
pub const GITHUB_CREDENTIALS: [(&str, &str); 1] = [(
    "credential.https://github.com.helper",
    "!gh auth git-credential",
)];

/// `GitError`: `git <command> failed: <detail>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitError(pub String);

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

/// A branch or ref name that is safe as a git argument: git reads a name that starts with `-`
/// as an option (`--upload-pack=...` runs a command), so it is refused, as is an empty name.
pub fn checked_ref(name: &str) -> Result<&str, GitError> {
    if name.is_empty() || name.starts_with('-') {
        return Err(GitError(format!("git ref refused: {name:?}")));
    }
    Ok(name)
}

/// `withGitConfig`: the `GIT_CONFIG_*` variables that add `entries` after the ones the
/// environment has (`current_count` is its `GIT_CONFIG_COUNT`).
pub fn with_git_config(
    current_count: Option<&str>,
    entries: &[(&str, &str)],
) -> Vec<(String, String)> {
    let start: usize = current_count
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0);
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_string(),
        (start + entries.len()).to_string(),
    )];
    for (offset, (key, value)) in entries.iter().enumerate() {
        env.push((
            format!("GIT_CONFIG_KEY_{}", start + offset),
            key.to_string(),
        ));
        env.push((
            format!("GIT_CONFIG_VALUE_{}", start + offset),
            value.to_string(),
        ));
    }
    env
}

static SPAWNS: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    // Tests run in parallel in one process, so a test reads the spawns of its own thread.
    static THREAD_SPAWNS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many git and gh processes this daemon started, for the benchmark. Not shown to users.
pub fn spawn_count() -> u64 {
    SPAWNS.load(Ordering::Relaxed)
}

/// Counts one spawn. Every git and gh call makes it before it starts the process, so a process
/// that cannot start is counted too.
pub fn note_spawn() {
    SPAWNS.fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    THREAD_SPAWNS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn thread_spawn_count() -> u64 {
    THREAD_SPAWNS.with(std::cell::Cell::get)
}

/// `OutsideWorkspaceError`: a link that leaves the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutsideWorkspaceError(pub String);

impl std::fmt::Display for OutsideWorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} points outside the workspace, so it is not shown",
            self.0
        )
    }
}

impl std::error::Error for OutsideWorkspaceError {}

fn run(
    cwd: &Path,
    args: &[&str],
    timeout: Option<Duration>,
    env: &[(String, String)],
) -> Result<Vec<u8>, GitError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(env.iter().map(|(key, value)| (key, value)));
    note_spawn();
    let ran = capture_bytes(
        command,
        timeout.unwrap_or(Duration::from_secs(60 * 60 * 24 * 365)),
        MAX_OUTPUT_BYTES,
    );
    if ran.code == Some(0) && !ran.overflow {
        return Ok(ran.stdout);
    }
    Err(GitError(format!(
        "git {} failed: {}",
        args.first().unwrap_or(&""),
        error_text(&ran, args)
    )))
}

/// `errorText`: why a git call failed, as Node words it.
fn error_text(ran: &CapturedBytes, args: &[&str]) -> String {
    if ran.not_found {
        return "git is not installed or not on the PATH".into();
    }
    let stderr = js_trim(&ran.stderr);
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    if ran.overflow {
        return "stdout maxBuffer length exceeded".into();
    }
    // The message of Node's `execFile` error: the command, a line break, and the stderr.
    format!("Command failed: git {}\n{}", args.join(" "), ran.stderr)
}

/// Runs git with an argument list, never a shell. `timeout` None waits for the end.
pub fn git(
    cwd: &Path,
    args: &[&str],
    timeout: Option<Duration>,
    env: &[(String, String)],
) -> Result<String, GitError> {
    run(cwd, args, timeout, env).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// `gitBuffer`: the stdout of git as bytes, for a file that may not be text.
pub fn git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    run(cwd, args, None, &[])
}

/// An object of git read with its size known first.
#[derive(Debug, PartialEq, Eq)]
pub enum BlobRead {
    Content(Vec<u8>),
    /// Bigger than the limit: the content was not read.
    TooLarge,
}

/// The object `spec` (for example `HEAD:src/a.rs`) with one `git cat-file --batch` process: it
/// reads the header first and kills git when the size is over `max`, so a large file is never
/// piped. A missing object fails as `cat-file -s` did.
pub fn git_blob_limited(worktree: &Path, spec: &str, max: u64) -> Result<BlobRead, GitError> {
    let fail = |detail: String| GitError(format!("git cat-file failed: {detail}"));
    // The batch input is one spec per line.
    if spec.contains('\n') {
        return Err(fail(format!("Not a valid object name {spec}")));
    }
    let mut command = Command::new("git");
    command
        .args(["cat-file", "--batch"])
        .current_dir(worktree)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    note_spawn();
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GitError("git cat-file failed: git is not installed or not on the PATH".into())
        } else {
            fail(error.to_string())
        }
    })?;
    let read = read_blob(&mut child, spec, max).map_err(|error| fail(error.to_string()));
    // The child may still run when the blob is too large or on an error.
    let _ = child.kill();
    let _ = child.wait();
    match read? {
        Some(read) => Ok(read),
        None => Err(fail(format!("Not a valid object name {spec}"))),
    }
}

// None: git says the object is missing.
fn read_blob(child: &mut Child, spec: &str, max: u64) -> std::io::Result<Option<BlobRead>> {
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(std::io::Error::other("no pipe to git"));
    };
    stdin.write_all(spec.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    let mut stdout = BufReader::new(stdout);
    let mut header = String::new();
    stdout.read_line(&mut header)?;
    let header = header.trim_end();
    if header.is_empty() || header.ends_with(" missing") || header.ends_with(" ambiguous") {
        return Ok(None);
    }
    // `<oid> <type> <size>`; a size that is not a number is never "too large", as before.
    let size: u64 = header
        .rsplit(' ')
        .next()
        .and_then(|size| size.parse().ok())
        .unwrap_or(0);
    if size > max {
        return Ok(Some(BlobRead::TooLarge));
    }
    let mut content = Vec::new();
    stdout.take(size).read_to_end(&mut content)?;
    Ok(Some(BlobRead::Content(content)))
}

/// The branch a checkout is on, or None when HEAD is detached.
pub fn current_branch(cwd: &Path) -> Option<String> {
    // Diff stats ask this on every read: at the top of a plain repository, `.git/HEAD` says it
    // without a git process. Anything else (a worktree, a subfolder) asks git.
    let git_dir = cwd.join(".git");
    if git_dir.is_dir()
        && let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
    {
        let head = head.trim_end();
        if let Some(name) = head.strip_prefix("ref: refs/heads/") {
            return (!name.is_empty()).then(|| name.to_string());
        }
        if !head.starts_with("ref: ") {
            return None;
        }
    }
    let name = git(
        cwd,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        None,
        &[],
    )
    .ok()?;
    let name = js_trim(&name);
    (!name.is_empty()).then(|| name.to_string())
}

/// The top folder of the repository that holds `path`. Fails with a GitError when it is not in one.
pub fn repository_root(path: &Path) -> Result<String, GitError> {
    let output = git(path, &["rev-parse", "--show-toplevel"], None, &[])?;
    Ok(js_trim(&output).to_string())
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
