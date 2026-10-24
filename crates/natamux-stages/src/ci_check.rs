//! Ports `checkOnCi` of `packages/daemon/src/stages/ci-check.ts`. The types and names are in `ci.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use natamux_core::js::{js_trim, utf16_len};
use natamux_git::gh::{RunGh, default_run_gh};
use natamux_proto::terminals::text::plain_text;

use crate::ci::{CI_BRANCH_PREFIX, CiCheck, CiResult, ci_branch_of, ci_error};

const POLL_MS: u64 = 15_000;
/// A workflow that has not started a run by then does not run on the CI branch at all.
const START_WAIT_MS: u64 = 3 * 60 * 1000;
const CI_TIMEOUT_MS: u64 = 30 * 60 * 1000;
/// The end of a failed log holds the error; the rest would only cost the agent tokens.
const LOG_TAIL_CHARS: usize = 3000;
const PASSING: [&str; 3] = ["success", "skipped", "neutral"];
const FIELDS: &str = "databaseId,workflowName,status,conclusion,url";

pub type SnapshotFn = Arc<dyn Fn(&Path, &str) -> Result<String, String> + Send + Sync>;
pub type PushFn = Arc<dyn Fn(&Path, &str, &str) -> Result<(), String> + Send + Sync>;

/// The calls `GhCi` makes, so tests can stub them. Times are in ms.
#[derive(Clone)]
pub struct CiDeps {
    pub snapshot: SnapshotFn,
    pub push: PushFn,
    pub gh: RunGh,
    pub sleep: Arc<dyn Fn(u64) + Send + Sync>,
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl Default for CiDeps {
    fn default() -> Self {
        Self {
            snapshot: Arc::new(|worktree, message| {
                natamux_git::commit_push::snapshot_commit(worktree, message)
                    .map_err(|error| error.to_string())
            }),
            push: Arc::new(|worktree, sha, ref_name| {
                natamux_git::commit_push::push_ref(worktree, sha, ref_name)
                    .map_err(|error| error.to_string())
            }),
            gh: default_run_gh(),
            sleep: Arc::new(|ms| std::thread::sleep(std::time::Duration::from_millis(ms))),
            now: Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis() as u64)
            }),
        }
    }
}

#[derive(Default)]
pub struct GhCi {
    pub deps: CiDeps,
}

impl CiCheck for GhCi {
    fn check(&self, worktree: &Path, branch: &str) -> CiResult {
        check_on_ci(worktree, branch, &self.deps)
    }
}

/// One run of `gh run list`. Fields are kept as the text Node's template strings print: an
/// unfinished run has a null `conclusion`, which prints as `null` and is not a pass.
struct CiRun {
    database_id: String,
    workflow_name: String,
    status: String,
    conclusion: String,
    url: String,
}

fn js_text(value: Option<&serde_json::Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

fn parse_runs(text: &str) -> Result<Vec<CiRun>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let list = value
        .as_array()
        .ok_or_else(|| format!("could not read the CI runs: {}", text.trim()))?;
    Ok(list
        .iter()
        .map(|run| CiRun {
            database_id: js_text(run.get("databaseId")),
            workflow_name: js_text(run.get("workflowName")),
            status: js_text(run.get("status")),
            conclusion: js_text(run.get("conclusion")),
            url: js_text(run.get("url")),
        })
        .collect())
}

/// What gh said so far: the runs of the last good read, and the last error.
struct Reader<'a> {
    deps: &'a CiDeps,
    worktree: &'a Path,
    list: Vec<String>,
    runs: Vec<CiRun>,
    last_error: String,
}

impl Reader<'_> {
    /// A reason to stop when gh cannot work at all.
    fn read(&mut self) -> Option<String> {
        let parsed = (self.deps.gh)(self.worktree, &self.list)
            .map_err(|error| error.to_string())
            .and_then(|text| parse_runs(&text));
        match parsed {
            Ok(runs) => self.runs = runs,
            Err(error) => {
                self.last_error = error;
                // A missing gh, or one that is not signed in, cannot get better; a network error may.
                if is_fatal(&self.last_error) {
                    return Some(format!("Could not read the CI runs: {}", self.last_error));
                }
            }
        }
        None
    }
}

/// Node's `/not installed|auth|log ?in|token/i`.
fn is_fatal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["not installed", "auth", "login", "log in", "token"]
        .iter()
        .any(|word| lower.contains(word))
}

fn stop(reason: &str) -> CiResult {
    ci_error(reason)
}

pub fn check_on_ci(worktree: &Path, branch: &str, deps: &CiDeps) -> CiResult {
    let ref_name = ci_branch_of(branch);
    let push_failed = |error: String| {
        stop(&format!(
            "Could not push the work to {ref_name} for CI: {error}"
        ))
    };
    let sha = match (deps.snapshot)(worktree, &format!("Natamux CI check of {branch}")) {
        Ok(sha) => sha,
        Err(error) => return push_failed(error),
    };
    let mut reader = Reader {
        deps,
        worktree,
        list: [
            "run", "list", "--commit", &sha, "--json", FIELDS, "--limit", "50",
        ]
        .map(String::from)
        .to_vec(),
        runs: Vec::new(),
        last_error: String::new(),
    };
    if let Some(reason) = reader.read() {
        return stop(&reason);
    }
    if reader.runs.is_empty()
        && let Err(error) = (deps.push)(worktree, &sha, &ref_name)
    {
        return push_failed(error);
    }

    let start = (deps.now)();
    // How many runs were all done at the last read; None when they were not.
    let mut settled: Option<usize> = None;
    loop {
        if let Some(reason) = reader.read() {
            return stop(&reason);
        }
        let waited = (deps.now)().saturating_sub(start);
        let runs = &reader.runs;
        // A workflow may list its run after another one ended, so all are done on two reads in a row.
        let done = !runs.is_empty() && runs.iter().all(|each| each.status == "completed");
        if done && settled == Some(runs.len()) {
            break;
        }
        settled = done.then_some(runs.len());
        if runs.is_empty() && waited >= START_WAIT_MS {
            let cause = if reader.last_error.is_empty() {
                String::new()
            } else {
                format!(" The last gh error: {}", reader.last_error)
            };
            return stop(&format!(
                "No CI run started for {ref_name}. Make the CI workflows run on a push to \"{CI_BRANCH_PREFIX}**\".{cause}"
            ));
        }
        if waited >= CI_TIMEOUT_MS {
            let urls: Vec<&str> = runs.iter().map(|each| each.url.as_str()).collect();
            return stop(&format!(
                "CI did not finish within 30 minutes: {}",
                urls.join(" ")
            ));
        }
        (deps.sleep)(POLL_MS);
    }
    result_of(&reader.runs, worktree, &deps.gh)
}

fn result_of(runs: &[CiRun], worktree: &Path, gh: &RunGh) -> CiResult {
    let failed: Vec<&CiRun> = runs
        .iter()
        .filter(|each| !PASSING.contains(&each.conclusion.as_str()))
        .collect();
    if failed.is_empty() {
        let passes = runs
            .iter()
            .map(|each| format!("CI passed: {} {}", each.workflow_name, each.url))
            .collect();
        return CiResult {
            passed: true,
            passes,
            ..CiResult::default()
        };
    }
    let mut reasons = Vec::new();
    let mut logs = Vec::new();
    for each in failed {
        let view = ["run", "view", each.database_id.as_str(), "--log-failed"].map(String::from);
        let log = plain_text(&gh(worktree, &view).unwrap_or_default());
        let tail = if js_trim(&log).is_empty() {
            String::new()
        } else {
            format!("\n{}", tail_chars(&log, LOG_TAIL_CHARS))
        };
        let reason = format!(
            "CI {} {}: {}",
            each.workflow_name, each.conclusion, each.url
        );
        logs.push(format!("{reason}{tail}"));
        reasons.push(reason);
    }
    CiResult {
        reasons,
        logs: Some(logs),
        ..CiResult::default()
    }
}

/// The last `limit` UTF-16 units, as JS `slice(-limit)`; a split pair drops its half.
fn tail_chars(text: &str, limit: usize) -> &str {
    let mut units = utf16_len(text);
    let mut rest = text;
    while units > limit {
        let Some(c) = rest.chars().next() else { break };
        units -= c.len_utf16();
        rest = &rest[c.len_utf8()..];
    }
    rest
}

#[cfg(test)]
#[path = "ci_check_tests.rs"]
mod tests;
