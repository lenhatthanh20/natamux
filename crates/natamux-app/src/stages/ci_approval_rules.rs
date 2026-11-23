//! The CI approval of Strict `approved-snapshot`: the CI push a person still has to approve, and
//! the words of its dialog. It names the exact tree, so the daemon pushes only what was shown.

use natamux_proto::events::task_event::TaskEvent;
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

fn ask_of(event: &TaskEvent, stage: &str) -> CiAsk {
    CiAsk {
        stage: stage.to_string(),
        tree: text(event, "tree"),
        branch: text(event, "branch"),
        remote: text(event, "remote"),
        files: list(event, "files"),
        not_sent: list(event, "notSent"),
        not_sent_count: event
            .payload
            .get("notSentCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
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
    text
}

/// The dialog's rows: the files CI gets, at most `max`, then one row for the rest.
pub fn ask_rows(ask: &CiAsk, max: usize) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = ask
        .files
        .iter()
        .take(max)
        .map(|file| (format!("ci-file-{file}"), file.clone()))
        .collect();
    if ask.files.len() > max {
        let rest = ask.files.len() - max;
        rows.push(("ci-file-more".into(), format!("and {rest} more files")));
    }
    rows
}

#[cfg(test)]
#[path = "ci_approval_rules_tests.rs"]
mod tests;
