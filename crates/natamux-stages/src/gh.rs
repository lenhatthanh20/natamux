//! Ports `RunGh`, `runGh` and `ghError` of `packages/daemon/src/git/gh.ts`.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use crate::git::MAX_OUTPUT_BYTES;
use crate::process::capture;

/// A gh that hangs, for example after the machine sleeps, must not block every later PR check.
pub const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// The error text is the message of `GitHubError`: "GitHub CLI failed: ...".
pub type RunGh = Arc<dyn Fn(&Path, &[String]) -> Result<String, String> + Send + Sync>;

pub fn default_run_gh() -> RunGh {
    Arc::new(run_gh)
}

/// Runs the GitHub CLI without prompts.
pub fn run_gh(cwd: &Path, args: &[String]) -> Result<String, String> {
    run_gh_with_timeout(cwd, args, GH_TIMEOUT)
}

pub(crate) fn run_gh_with_timeout(
    cwd: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1");
    let ran = capture(command, timeout, MAX_OUTPUT_BYTES);
    let fail = |detail: &str| format!("GitHub CLI failed: {detail}");
    if ran.not_found {
        return Err(fail("gh is not installed or not on the PATH"));
    }
    // The default text of a killed gh is only "Command failed".
    if ran.killed() {
        return Err(fail(&format!(
            "gh did not finish in {} seconds",
            GH_TIMEOUT.as_secs()
        )));
    }
    if ran.code == Some(0) {
        return Ok(ran.stdout);
    }
    let stderr = natamux_core::js::js_trim(&ran.stderr);
    if !stderr.is_empty() {
        return Err(fail(stderr));
    }
    Err(fail(&format!(
        "Command failed: gh {}\n{}",
        args.join(" "),
        ran.stderr
    )))
}

#[cfg(test)]
#[path = "gh_tests.rs"]
mod tests;
