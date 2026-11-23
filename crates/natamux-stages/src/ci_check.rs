//! Ports `checkOnCi` of `packages/daemon/src/stages/ci-check.ts`. The types and names are in `ci.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use natamux_core::js::{js_trim, utf16_len};
use natamux_core::security::{CI_FORBID_REASON, CiMode, ci_mode};
use natamux_git::ci_snapshot::{ApprovedPush, push_approved, verify_approved};
use natamux_git::gh::{RunGh, default_run_gh};
use natamux_proto::terminals::text::plain_text;

use crate::ci::{CI_BRANCH_PREFIX, CiCheck, CiResult, CiTarget, ci_branch_of, ci_error};

const POLL_MS: u64 = 15_000;
/// A workflow that has not started a run by then does not run on the CI branch at all.
const START_WAIT_MS: u64 = 3 * 60 * 1000;
const CI_TIMEOUT_MS: u64 = 30 * 60 * 1000;
/// The end of a failed log holds the error; the rest would only cost the agent tokens.
const LOG_TAIL_CHARS: usize = 3000;
const PASSING: [&str; 3] = ["success", "skipped", "neutral"];
const FIELDS: &str = "databaseId,workflowName,status,conclusion,url";

pub type PushFn = Arc<dyn Fn(&Path, &CiTarget) -> Result<(), String> + Send + Sync>;

/// The calls `GhCi` makes, so tests can stub them. Times are in ms. `verify` checks the approval
/// with no network call; `push` checks it again and pushes the approved commit.
#[derive(Clone)]
pub struct CiDeps {
    pub verify: PushFn,
    pub push: PushFn,
    pub gh: RunGh,
    pub sleep: Arc<dyn Fn(u64) + Send + Sync>,
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

fn approved<'a>(target: &'a CiTarget, ref_name: &'a str) -> ApprovedPush<'a> {
    ApprovedPush {
        mode: target.mode,
        commit: &target.commit,
        tree: &target.tree,
        remote: &target.remote,
        ref_name,
    }
}

impl Default for CiDeps {
    fn default() -> Self {
        Self {
            verify: Arc::new(|worktree, target| {
                let ref_name = ci_branch_of(&target.branch);
                verify_approved(worktree, &approved(target, &ref_name))
                    .map_err(|error| error.to_string())
            }),
            push: Arc::new(|worktree, target| {
                let ref_name = ci_branch_of(&target.branch);
                push_approved(worktree, &approved(target, &ref_name))
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
    fn check(&self, worktree: &Path, target: &CiTarget) -> CiResult {
        check_on_ci(worktree, target, &self.deps)
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

/// The text of Node's `JSON.parse` error (V8), so `is_fatal` sees the word `token` as Node does.
fn json_error_text(text: &str) -> String {
    let Some(first) = text.chars().next().filter(|_| !js_trim(text).is_empty()) else {
        return "Unexpected end of JSON input".to_string();
    };
    let shown: String = text.chars().take(10).collect();
    let dots = if text.chars().count() > 10 { "..." } else { "" };
    format!("Unexpected token '{first}', \"{shown}\"{dots} is not valid JSON")
}

fn parse_runs(text: &str) -> Result<Vec<CiRun>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| json_error_text(text))?;
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

/// Why the approved push may not go now: the mode forbids it or changed, or `verify` refused it.
/// Under `ciPush: forbid` no dependency is called at all.
fn refusal(worktree: &Path, target: &CiTarget, deps: &CiDeps) -> Option<String> {
    let now = ci_mode();
    if now == CiMode::Forbid || target.mode == CiMode::Forbid {
        return Some(CI_FORBID_REASON.to_string());
    }
    if target.mode != now {
        return Some(format!(
            "CI push refused: the CI push mode changed from {} to {} since the approval.",
            target.mode.as_str(),
            now.as_str()
        ));
    }
    (deps.verify)(worktree, target).err()
}

/// Checks the approved commit of `target` on CI. The approval is checked before any network
/// call, `gh run list` included.
pub fn check_on_ci(worktree: &Path, target: &CiTarget, deps: &CiDeps) -> CiResult {
    if let Some(reason) = refusal(worktree, target, deps) {
        return stop(&reason);
    }
    let ref_name = ci_branch_of(&target.branch);
    let push_failed = |error: String| {
        stop(&format!(
            "Could not push the work to {ref_name} for CI: {error}"
        ))
    };
    let sha = target.commit.as_str();
    let mut reader = Reader {
        deps,
        worktree,
        list: [
            "run", "list", "--commit", sha, "--json", FIELDS, "--limit", "50",
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
        && let Err(error) = (deps.push)(worktree, target)
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
