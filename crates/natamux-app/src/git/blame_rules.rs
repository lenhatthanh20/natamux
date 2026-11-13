//! The text of the blame column (Annotate with Git Blame) and of its right-click menu. Dates are
//! in UTC, as every other time of the app.

use natamux_core::js::iso_from_millis;
use natamux_proto::workspaces::changes::{BlameAnswer, BlameCommit};

const AUTHOR_CHARS: usize = 14;
/// `YYYY-MM-DD` and a space before the author.
const DATE_CHARS: usize = 11;
const TITLE_CHARS: usize = 80;
pub const NOT_TRACKED: &str = "Not committed yet: nothing to annotate.";

/// What the menu of an editor offers. `Annotate` is off while the file has unsaved edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlameItem {
    Annotate { blocked: bool },
    Title(String),
    CopyHash(String),
    Close,
}

/// Off: the one item that turns it on. On: the clicked line's commit, then Close.
pub fn menu_items(on: bool, blocked: bool, commit: Option<&BlameCommit>) -> Vec<BlameItem> {
    if !on {
        return vec![BlameItem::Annotate { blocked }];
    }
    let mut items = Vec::new();
    if let Some(commit) = commit.filter(|commit| commit.committed()) {
        items.push(BlameItem::Title(menu_title(commit)));
        items.push(BlameItem::CopyHash(commit.sha.clone()));
    }
    items.push(BlameItem::Close);
    items
}

/// `<short sha> · <author> · <date> — <summary>`, cut to 80 characters.
pub fn menu_title(commit: &BlameCommit) -> String {
    let short: String = commit.sha.chars().take(7).collect();
    let date = date_of(commit.time);
    let title = format!("{short} · {} · {date} — {}", commit.author, commit.summary);
    if title.chars().count() <= TITLE_CHARS {
        return title;
    }
    let cut: String = title.chars().take(TITLE_CHARS - 1).collect();
    format!("{cut}…")
}

/// `YYYY-MM-DD` of a unix time in seconds.
pub fn date_of(time: i64) -> String {
    iso_from_millis(time.saturating_mul(1000))
        .chars()
        .take(10)
        .collect()
}

/// The label of each line (0-based) below `line_count`, all of the same width.
pub fn line_labels(answer: &BlameAnswer, line_count: u32) -> Vec<(u32, String)> {
    let mut labels = Vec::new();
    for run in &answer.runs {
        let Some(commit) = answer.commits.get(run.commit) else {
            continue;
        };
        let label = label_of(commit);
        let end = run.start.saturating_add(run.count).min(line_count);
        labels.extend((run.start..end).map(|line| (line, label.clone())));
    }
    labels
}

/// The commit that last changed `line` (0-based).
pub fn commit_at(answer: &BlameAnswer, line: u32) -> Option<&BlameCommit> {
    let run = answer
        .runs
        .iter()
        .find(|run| run.start <= line && line - run.start < run.count)?;
    answer.commits.get(run.commit)
}

fn label_of(commit: &BlameCommit) -> String {
    let text = if commit.committed() {
        format!(
            "{} {}",
            date_of(commit.time),
            fit(&commit.author, AUTHOR_CHARS)
        )
    } else {
        fit("Not committed", DATE_CHARS + AUTHOR_CHARS)
    };
    format!("{text}  ")
}

/// `text` cut or padded with spaces to `width` characters.
fn fit(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width).collect();
    let short = width - out.chars().count();
    out.extend(std::iter::repeat_n(' ', short));
    out
}

#[cfg(test)]
#[path = "blame_rules_tests.rs"]
mod tests;
