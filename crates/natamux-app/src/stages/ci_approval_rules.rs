//! The CI approval of Strict `approved-snapshot`: the CI push a person still has to approve, and
//! the words of its dialog. It names the exact tree, so the daemon pushes only what was shown.

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::TaskState;
use serde_json::Value;

/// One CI push that waits for a person (`stage.ci-approval-needed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiAsk {
    pub stage: String,
    pub tree: String,
    pub branch: String,
    pub remote: String,
    pub files: Vec<String>,
    pub not_sent: Vec<String>,
    pub not_sent_count: u64,
    /// The commits that no remote has yet; None when the daemon could not read them.
    pub new_commits: Option<u64>,
    /// The paths those commits change (cut by the daemon), and how many there are.
    pub commit_files: Vec<String>,
    pub commit_files_count: u64,
    /// A path under `.github/workflows/` changes; None when unknown.
    pub workflows_changed: Option<bool>,
}

fn text(event: &TaskEvent, name: &str) -> String {
    event
        .payload
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn list(event: &TaskEvent, name: &str) -> Vec<String> {
    let Some(Value::Array(items)) = event.payload.get(name) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn number(event: &TaskEvent, name: &str) -> Option<u64> {
    event.payload.get(name).and_then(Value::as_u64)
}

fn ask_of(event: &TaskEvent, stage: &str) -> CiAsk {
    let commit_files = list(event, "commitFiles");
    CiAsk {
        stage: stage.to_string(),
        tree: text(event, "tree"),
        branch: text(event, "branch"),
        remote: text(event, "remote"),
        files: list(event, "files"),
        not_sent: list(event, "notSent"),
        not_sent_count: number(event, "notSentCount").unwrap_or(0),
        new_commits: number(event, "newCommits"),
        commit_files_count: number(event, "commitFilesCount").unwrap_or(commit_files.len() as u64),
        commit_files,
        workflows_changed: event
            .payload
            .get("workflowsChanged")
            .and_then(Value::as_bool),
    }
}

/// The CI pushes that wait for a person: the latest `stage.ci-approval-needed` of each stage with
/// nothing after it that answered or ended it. The same rule as the daemon's `pending_approval`.
pub fn pending_ci_asks(events: &[TaskEvent]) -> Vec<CiAsk> {
    let mut asks: Vec<CiAsk> = Vec::new();
    for event in events {
        let Some(stage) = event.payload.get("stage").and_then(Value::as_str) else {
            continue;
        };
        match event.kind.as_str() {
            "stage.ci-approval-needed" => {
                asks.retain(|ask| ask.stage != stage);
                asks.push(ask_of(event, stage));
            }
            "stage.ci-approved" | "stage.started" | "stage.checked" | "stage.check-failed" => {
                asks.retain(|ask| ask.stage != stage);
            }
            _ => {}
        }
    }
    asks
}

/// The asks a person can still answer: none once the task is done or cancelled, since the daemon
/// refuses an approval then.
pub fn open_ci_asks(state: TaskState, events: &[TaskEvent]) -> Vec<CiAsk> {
    if state.is_finished() {
        return Vec::new();
    }
    pending_ci_asks(events)
}

/// The first 7 characters of an object id, as git shows it.
fn short(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

/// The dialog's description: where it goes, and what CI does not get.
pub fn ask_description(ask: &CiAsk) -> String {
    let remote = if ask.remote.is_empty() {
        "origin"
    } else {
        ask.remote.as_str()
    };
    let mut text = format!(
        "Natamux pushes tree {} to {} on {remote}. CI gets exactly the {} files below; later \
         edits are not sent.",
        short(&ask.tree),
        ask.branch,
        ask.files.len()
    );
    if ask.not_sent_count > 0 {
        let more = ask.not_sent_count.saturating_sub(ask.not_sent.len() as u64);
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        text.push_str(&format!(" Not sent: {}{more}.", ask.not_sent.join(", ")));
    }
    text.push_str(&commits_line(ask));
    text
}

const UNKNOWN_COMMITS: &str =
    " Natamux could not read which commits no remote has yet; they are sent too.";

/// The commits go too: the pushed commit holds HEAD's history.
fn commits_line(ask: &CiAsk) -> String {
    let commits = match ask.new_commits {
        None => return UNKNOWN_COMMITS.to_string(),
        Some(0) => return String::new(),
        Some(1) => "1 commit".to_string(),
        Some(n) => format!("{n} commits"),
    };
    let files = match ask.commit_files_count {
        1 => "1 file".to_string(),
        n => format!("{n} files"),
    };
    format!(" It also sends {commits} that no remote has ({files}).")
}

/// The warning row of a push that changes a GitHub workflow, which CI then runs; also when
/// Natamux could not tell.
fn warning_row(ask: &CiAsk) -> Option<(String, String)> {
    let text = match ask.workflows_changed {
        Some(false) => return None,
        Some(true) => "Warning: this push changes .github/workflows/, and CI runs that code.",
        None => "Warning: Natamux could not check this push for workflow changes.",
    };
    Some(("ci-warning".into(), text.into()))
}

/// At most `max` files, then one row for the `count - shown` others.
fn push_files(
    rows: &mut Vec<(String, String)>,
    kind: &str,
    files: &[String],
    count: u64,
    max: usize,
) {
    let shown = files.iter().take(max);
    rows.extend(shown.map(|file| (format!("{kind}-{file}"), file.clone())));
    let rest = count.saturating_sub(files.len().min(max) as u64);
    if rest > 0 {
        rows.push((format!("{kind}-more"), format!("and {rest} more files")));
    }
}

/// The dialog's rows: the workflow warning, the files CI gets, then the files the new commits
/// change, each list at most `max` rows and one row for the rest.
pub fn ask_rows(ask: &CiAsk, max: usize) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = warning_row(ask).into_iter().collect();
    let sent = ask.files.len() as u64;
    push_files(&mut rows, "ci-file", &ask.files, sent, max);
    let count = ask.commit_files_count;
    push_files(&mut rows, "commit-file", &ask.commit_files, count, max);
    rows
}

#[cfg(test)]
#[path = "ci_approval_rules_tests.rs"]
mod tests;
