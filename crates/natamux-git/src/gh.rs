//! Ports `packages/daemon/src/git/gh.ts`: the GitHub CLI call (`RunGh`, `runGh`, `ghError`) and the
//! pull request work around it.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use natamux_core::js::{is_js_space, js_trim};
use natamux_proto::tasks::task::PullRequestStatus;
use serde_json::Value;

use crate::commit_push::utf16_prefix;
use crate::exec::{MAX_OUTPUT_BYTES, note_spawn};
use crate::process::{Captured, capture};

/// A gh that hangs, for example after the machine sleeps, must not block every later PR check.
pub const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// `GitHubError`: `message` starts with "GitHub CLI failed: ". `output` is what gh printed before
/// it failed: a GraphQL answer with errors still has its data there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubError {
    pub message: String,
    pub output: String,
}

impl GitHubError {
    pub fn new(detail: &str) -> GitHubError {
        GitHubError::with_output(detail, "")
    }

    pub fn with_output(detail: &str, output: &str) -> GitHubError {
        GitHubError {
            message: format!("GitHub CLI failed: {detail}"),
            output: output.to_string(),
        }
    }
}

impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitHubError {}

pub type RunGh = Arc<dyn Fn(&Path, &[String]) -> Result<String, GitHubError> + Send + Sync>;

pub fn default_run_gh() -> RunGh {
    Arc::new(run_gh)
}

/// Runs the GitHub CLI without prompts. Its errors become `GitHubError`.
pub fn run_gh(cwd: &Path, args: &[String]) -> Result<String, GitHubError> {
    run_gh_with_timeout(cwd, args, GH_TIMEOUT)
}

pub fn run_gh_with_timeout(
    cwd: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<String, GitHubError> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1");
    note_spawn();
    let ran = capture(command, timeout, MAX_OUTPUT_BYTES);
    if ran.code == Some(0) && !ran.killed() && !ran.not_found {
        return Ok(ran.stdout);
    }
    Err(gh_error_of(&ran, args))
}

/// `ghError`: the `GitHubError` for a gh that did not finish well.
pub fn gh_error_of(ran: &Captured, args: &[String]) -> GitHubError {
    if ran.not_found {
        return GitHubError::new("gh is not installed or not on the PATH");
    }
    // The default text of a killed gh is only "Command failed".
    if ran.timed_out {
        let seconds = GH_TIMEOUT.as_secs();
        return GitHubError::new(&format!("gh did not finish in {seconds} seconds"));
    }
    if ran.overflow {
        return GitHubError::new("gh printed more than 10 MiB of output");
    }
    let stderr = js_trim(&ran.stderr);
    if !stderr.is_empty() {
        return GitHubError::with_output(stderr, &ran.stdout);
    }
    let detail = format!("Command failed: gh {}\n{}", args.join(" "), ran.stderr);
    GitHubError::with_output(&detail, &ran.stdout)
}

/// Where a pull request address that starts with `https://` ends in `rest`, as Node's regex
/// `https://\S+/pull/\d+` matches it: `\S+` takes as much as it can.
fn pull_request_end(rest: &str) -> Option<usize> {
    let run_end = rest.find(is_js_space).unwrap_or(rest.len());
    let run = &rest[..run_end];
    let mut starts: Vec<usize> = run.match_indices("/pull/").map(|(at, _)| at).collect();
    starts.reverse();
    starts.into_iter().filter(|at| *at > 0).find_map(|at| {
        let digits = run[at + 6..].bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0).then_some(at + 6 + digits)
    })
}

/// The address of a pull request in the output of `gh pr create`, or None.
pub fn pull_request_url(output: &str) -> Option<String> {
    let scheme = "https://";
    output.match_indices(scheme).find_map(|(at, _)| {
        let rest = &output[at + scheme.len()..];
        pull_request_end(rest).map(|end| output[at..at + scheme.len() + end].to_string())
    })
}

/// Opens a pull request from `branch` into `base`, or else the default branch of the repository.
/// When one is open for the branch already, gives its address instead.
pub fn create_pull_request(
    worktree: &Path,
    branch: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
    gh: &RunGh,
) -> Result<String, GitHubError> {
    let mut args = vec!["pr", "create", "--head", branch];
    if let Some(base) = base {
        args.extend(["--base", base]);
    }
    args.extend(["--title", title, "--body", body]);
    let args: Vec<String> = args.into_iter().map(str::to_string).collect();
    let output = match gh(worktree, &args) {
        Ok(output) => output,
        Err(error) => {
            if let Some(existing) = pull_request_url(&error.message)
                && error
                    .message
                    .to_ascii_lowercase()
                    .contains("already exists")
            {
                return Ok(existing);
            }
            return Err(error);
        }
    };
    pull_request_url(&output).ok_or_else(|| {
        GitHubError::new(&format!(
            "no pull request address in the answer: {}",
            js_trim(&output)
        ))
    })
}

/// One of the checks of a pull request: `statusCheckRollup` mixes check runs (`status` and
/// `conclusion`) and commit statuses (`state`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckItem {
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub state: Option<String>,
}

const FAILED_CONCLUSIONS: [&str; 5] = [
    "FAILURE",
    "TIMED_OUT",
    "CANCELLED",
    "ACTION_REQUIRED",
    "STARTUP_FAILURE",
];

/// One word for all checks of a pull request: none, failing, pending or passing.
pub fn summarize_checks(items: &[CheckItem]) -> &'static str {
    if items.is_empty() {
        return "none";
    }
    let failed = items.iter().any(|item| {
        item.conclusion
            .as_deref()
            .is_some_and(|conclusion| FAILED_CONCLUSIONS.contains(&conclusion))
            || matches!(item.state.as_deref(), Some("FAILURE" | "ERROR"))
    });
    if failed {
        return "failing";
    }
    let waiting = items.iter().any(|item| match item.state.as_deref() {
        None => item.status.as_deref() != Some("COMPLETED"),
        Some(state) => state == "PENDING" || state == "EXPECTED",
    });
    if waiting { "pending" } else { "passing" }
}

fn status_of(state: Option<&str>, is_draft: bool, checks: &[CheckItem]) -> PullRequestStatus {
    let state = match state {
        Some("MERGED") => "merged",
        Some("CLOSED") => "closed",
        _ if is_draft => "draft",
        _ => "open",
    };
    PullRequestStatus {
        state: state.into(),
        checks: summarize_checks(checks).into(),
    }
}

fn text_field(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).map(str::to_string)
}

fn check_items(value: Option<&Value>) -> Result<Vec<CheckItem>, String> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items
            .iter()
            .map(|item| CheckItem {
                status: text_field(item, "status"),
                conclusion: text_field(item, "conclusion"),
                state: text_field(item, "state"),
            })
            .collect()),
        Some(_) => Err("the checks are not a list".into()),
    }
}

fn parse_json(output: &str) -> Result<Value, String> {
    serde_json::from_str(output).map_err(|error| error.to_string())
}

/// Reads the output of `gh pr view --json state,isDraft,statusCheckRollup`.
pub fn parse_pull_request_view(output: &str) -> Result<PullRequestStatus, String> {
    let data = parse_json(output)?;
    if data.is_null() {
        return Err("no pull request in the answer".into());
    }
    let checks = check_items(data.get("statusCheckRollup"))?;
    let is_draft = data.get("isDraft") == Some(&Value::Bool(true));
    Ok(status_of(
        data.get("state").and_then(Value::as_str),
        is_draft,
        &checks,
    ))
}

/// The repository of a pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    pub host: String,
    pub owner: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestRef {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub number: u64,
}

impl PullRequestRef {
    pub fn repo(&self) -> RepoRef {
        RepoRef {
            host: self.host.clone(),
            owner: self.owner.clone(),
            name: self.name.clone(),
        }
    }
}

/// The repository and number of a pull request address, as Node's regex
/// `^https://([^/\s]+)/([^/\s]+)/([^/\s]+)/pull/(\d+)` reads it.
pub fn parse_pull_request_ref(url: &str) -> Option<PullRequestRef> {
    let rest = url.strip_prefix("https://")?;
    let mut parts = rest.splitn(4, '/');
    let (host, owner, name, tail) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let word = |part: &str| !part.is_empty() && !part.contains(is_js_space);
    if !(word(host) && word(owner) && word(name)) {
        return None;
    }
    let digits: String = tail
        .strip_prefix("pull/")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Some(PullRequestRef {
        host: host.into(),
        owner: owner.into(),
        name: name.into(),
        number: digits.parse().ok()?,
    })
}

// The same as `gh pr view --json state,isDraft,statusCheckRollup` reads.
const PULL_REQUEST_FIELDS: [&str; 3] = [
    "state isDraft commits(last: 1) { nodes { commit { statusCheckRollup {",
    "contexts(first: 100) { nodes { ... on CheckRun { status conclusion }",
    "... on StatusContext { state } } } } } } }",
];

/// Reads the answer of `read_pull_request_statuses`. A number GitHub did not find is left out.
pub fn parse_pull_request_batch(
    output: &str,
    numbers: &[u64],
) -> Result<BTreeMap<u64, PullRequestStatus>, String> {
    let data = parse_json(output)?;
    let repository = data
        .pointer("/data/repository")
        .filter(|repository| !repository.is_null())
        .ok_or("no repository in the answer")?;
    let mut statuses = BTreeMap::new();
    for number in numbers {
        let Some(node) = repository
            .get(format!("pr{number}"))
            .filter(|node| !node.is_null())
        else {
            continue;
        };
        let rollup = node.pointer("/commits/nodes/0/commit/statusCheckRollup/contexts/nodes");
        let is_draft = node.get("isDraft") == Some(&Value::Bool(true));
        let state = node.get("state").and_then(Value::as_str);
        statuses.insert(*number, status_of(state, is_draft, &check_items(rollup)?));
    }
    Ok(statuses)
}

/// The first 200 UTF-16 units of an answer, for the error texts.
fn start_of(output: &str) -> &str {
    utf16_prefix(output, 200).0
}

/// The pull requests of one repository with one gh call, instead of one `gh pr view` each.
pub fn read_pull_request_statuses(
    cwd: &Path,
    repo: &RepoRef,
    numbers: &[u64],
    gh: &RunGh,
) -> Result<BTreeMap<u64, PullRequestStatus>, GitHubError> {
    let fields = PULL_REQUEST_FIELDS.join(" ");
    let mut query = vec![
        "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {"
            .to_string(),
    ];
    for number in numbers {
        query.push(format!(
            "pr{number}: pullRequest(number: {number}) {{ {fields} }}"
        ));
    }
    query.push("} }".into());
    let mut args: Vec<String> = vec!["api".into(), "graphql".into()];
    if repo.host != "github.com" {
        args.extend(["--hostname".to_string(), repo.host.clone()]);
    }
    args.extend([
        "-f".to_string(),
        format!("query={}", query.join(" ")),
        "-f".to_string(),
        format!("owner={}", repo.owner),
        "-f".to_string(),
        format!("name={}", repo.name),
    ]);
    let output = match gh(cwd, &args) {
        Ok(output) => output,
        Err(error) => {
            // gh fails when one number is not found, and still prints the others.
            if error.output.is_empty() {
                return Err(error);
            }
            return parse_pull_request_batch(&error.output, numbers).map_err(|_| error);
        }
    };
    parse_pull_request_batch(&output, numbers).map_err(|_| {
        GitHubError::new(&format!(
            "could not read the pull requests: {}",
            start_of(&output)
        ))
    })
}

pub fn read_pull_request_status(
    cwd: &Path,
    url: &str,
    gh: &RunGh,
) -> Result<PullRequestStatus, GitHubError> {
    let args = [
        "pr",
        "view",
        url,
        "--json",
        "state,isDraft,statusCheckRollup",
    ]
    .map(str::to_string);
    let output = gh(cwd, &args)?;
    parse_pull_request_view(&output).map_err(|_| {
        GitHubError::new(&format!(
            "could not read the pull request: {}",
            start_of(&output)
        ))
    })
}

fn list_error(output: &str) -> GitHubError {
    GitHubError::new(&format!(
        "could not read the pull requests: {}",
        start_of(output)
    ))
}

/// The address of the newest pull request from `branch`, when it is open or merged. A closed one
/// is left out: the work it had was dropped.
pub fn find_pull_request(
    cwd: &Path,
    branch: &str,
    gh: &RunGh,
) -> Result<Option<String>, GitHubError> {
    let args = [
        "pr",
        "list",
        "--head",
        branch,
        "--state",
        "all",
        "--json",
        "url,state",
        "--limit",
        "1",
    ]
    .map(str::to_string);
    let output = gh(cwd, &args)?;
    let found = parse_json(&output).map_err(|_| list_error(&output))?;
    let newest = found.get(0);
    let closed = newest
        .and_then(|pr| pr.get("state"))
        .and_then(Value::as_str)
        == Some("CLOSED");
    let url = newest.and_then(|pr| pr.get("url")).and_then(Value::as_str);
    Ok(url.filter(|_| !closed).map(str::to_string))
}

/// How many of the newest pull requests `find_pull_requests` reads. shortcut: a branch whose pull
/// request is older is not found here; `find_pull_request` (on an agent's stop) still finds it.
const LISTED_PULL_REQUESTS: usize = 100;

/// The address of the newest pull request of each branch, by branch, from one gh call for the
/// whole repository. Left out as in `find_pull_request`: a branch whose newest one is closed.
pub fn find_pull_requests(cwd: &Path, gh: &RunGh) -> Result<HashMap<String, String>, GitHubError> {
    let limit = LISTED_PULL_REQUESTS.to_string();
    let args = [
        "pr",
        "list",
        "--state",
        "all",
        "--json",
        "url,state,headRefName",
        "--limit",
        limit.as_str(),
    ]
    .map(str::to_string);
    let output = gh(cwd, &args)?;
    let found = parse_json(&output).map_err(|_| list_error(&output))?;
    let Some(list) = found.as_array() else {
        return Err(list_error(&output));
    };
    let mut by_branch = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    // Newest first, so the first one of a branch is its newest.
    for pr in list {
        let Some(branch) = pr.get("headRefName").and_then(Value::as_str) else {
            continue;
        };
        if !seen.insert(branch.to_string()) {
            continue;
        }
        let url = pr.get("url").and_then(Value::as_str);
        let closed = pr.get("state").and_then(Value::as_str) == Some("CLOSED");
        if let (Some(url), false) = (url, closed) {
            by_branch.insert(branch.to_string(), url.to_string());
        }
    }
    Ok(by_branch)
}

#[cfg(test)]
#[path = "gh_batch_tests.rs"]
mod batch_tests;
#[cfg(test)]
#[path = "gh_tests.rs"]
mod tests;
