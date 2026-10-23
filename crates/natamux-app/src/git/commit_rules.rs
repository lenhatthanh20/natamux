//! The commit box and the toasts of the git actions (`ChangesPanel.tsx`, and `gitAction` in
//! `ChangesView.tsx`). Pure: what turns Commit all off, how a draft lands, and the texts.

// The commit box uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

pub const DRAFTING_REASON: &str = "Drafting the message…";
pub const BLANK_REASON: &str = "Write a commit message first";
pub const BUSY_REASON: &str = "Another git action is running";

pub const COMMIT_FAILED: &str = "Could not commit.";
pub const PUSH_FAILED: &str = "Could not push.";
pub const PULL_REQUEST_FAILED: &str = "Could not open the pull request.";
pub const ROLLBACK_FAILED: &str = "Could not roll back.";
pub const UNDO_FAILED: &str = "Could not undo the rollback.";
pub const ROLLBACK_UNDONE: &str = "Rollback undone.";
pub const PULL_REQUEST_OPENED: &str = "Pull request opened.";
pub const DRAFT_FALLBACK: &str =
    "The agent could not draft the message. A simple draft is in the box.";
pub const DRAFT_FAILED: &str = "Could not draft the commit message.";

/// "1 file" or "N files".
pub fn files_text(count: u64) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// Why Commit all is off, or None when it can run. `committing`: this box started the running
/// action, so it shows busy instead of off.
pub fn commit_reason(
    message: &str,
    drafting: bool,
    git_busy: bool,
    committing: bool,
) -> Option<&'static str> {
    if drafting {
        Some(DRAFTING_REASON)
    } else if message.trim().is_empty() {
        Some(BLANK_REASON)
    } else if git_busy && !committing {
        Some(BUSY_REASON)
    } else {
        None
    }
}

/// Whether Mod+Enter or the button may send the commit now. A second press while the commit
/// runs does nothing.
pub fn can_submit(message: &str, drafting: bool, git_busy: bool, committing: bool) -> bool {
    !(message.trim().is_empty() || committing || git_busy || drafting)
}

/// The text after a draft came back: the draft, unless the user typed meanwhile.
pub fn draft_result(before: &str, now: &str, drafted: &str) -> String {
    (if now == before { drafted } else { now }).to_string()
}

pub fn committed_text(count: u64) -> String {
    format!("Committed {}.", files_text(count))
}

pub fn rolled_back_text(count: u64) -> String {
    format!("Rolled back {}.", files_text(count))
}

pub fn pushed_text(branch: Option<&str>) -> String {
    format!("Pushed {} to origin.", branch.unwrap_or("the branch"))
}

/// The tooltip of Generate.
pub fn draft_tip(agent: &str) -> String {
    format!("Draft a message with {agent}")
}

/// The label of the Changes tab: the number of files that wait for a commit.
pub fn tab_count_label(count: u64) -> String {
    format!("{} to commit", files_text(count))
}

#[cfg(test)]
#[path = "commit_rules_tests.rs"]
mod tests;
