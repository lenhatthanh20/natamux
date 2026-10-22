//! Port of `needsCommit`, `pushButton`, `pullRequestButton`, `nextGitStep`, `latestQuestion`,
//! `reviewStages`, `createBlockedReason` and `tabToOpen` in `packages/web/src/lib/workspace.ts`.
//! `waitReason`, `bannerText` and `rowText` are in `sidebar/sidebar_text.rs` and `natamux-proto`.

use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::PushStatus;

use crate::app::view_rules::WorkspaceTab;

/// What the agent asked, from the newest `agent.asked` event that carries a message.
#[cfg_attr(not(test), allow(unused_imports))]
pub use natamux_proto::events::task_event::latest_question;

/// A git button of the workspace header.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitButton {
    pub label: String,
    /// Why the button does nothing now. None: it works.
    pub reason: Option<String>,
    /// What a click does, when it works.
    pub tip: Option<String>,
}

fn off(label: impl Into<String>, reason: impl Into<String>) -> GitButton {
    GitButton {
        label: label.into(),
        reason: Some(reason.into()),
        tip: None,
    }
}

fn on(label: impl Into<String>, tip: impl Into<String>) -> GitButton {
    GitButton {
        label: label.into(),
        reason: None,
        tip: Some(tip.into()),
    }
}

fn commits(count: u64) -> String {
    if count == 1 {
        "1 commit".into()
    } else {
        format!("{count} commits")
    }
}

// Work that is not committed yet is the real next step when no commit is ahead.
fn commit_first(uncommitted: Option<u64>) -> Option<String> {
    let count = uncommitted.filter(|count| *count > 0)?;
    let files = if count == 1 {
        "changed file".to_string()
    } else {
        format!("{count} changed files")
    };
    Some(format!("Commit your {files} first"))
}

// Commit… comes with screen 4; until then Push shows its "commit first" reason.
#[cfg_attr(not(test), allow(dead_code))]
/// Commit takes the place of Push while origin has every commit and files are not committed.
pub fn needs_commit(push: Option<&PushStatus>, uncommitted: Option<u64>) -> bool {
    let all_pushed = matches!(
        push,
        Some(PushStatus::New { unpushed: 0 } | PushStatus::Tracked { unpushed: 0, .. })
    );
    all_pushed && commit_first(uncommitted).is_some()
}

fn tracked_button(
    branch: &str,
    unpushed: u64,
    unpulled: u64,
    uncommitted: Option<u64>,
) -> GitButton {
    let remote_branch = format!("origin/{branch}");
    let commit_reason = if unpushed == 0 {
        commit_first(uncommitted)
    } else {
        None
    };
    if let Some(reason) = commit_reason {
        return off("Push", reason);
    }
    if unpulled > 0 {
        // git refuses a push that would drop commits on origin.
        let arrows = if unpushed > 0 {
            format!("↑{unpushed} ↓{unpulled}")
        } else {
            format!("↓{unpulled}")
        };
        let missing = format!(
            "{remote_branch} has {} that this branch does not have",
            commits(unpulled)
        );
        return off(
            format!("Push {arrows}"),
            format!("{missing}. Pull or rebase first."),
        );
    }
    if unpushed == 0 {
        return off(
            "Push",
            format!("Nothing to push: up to date with {remote_branch}"),
        );
    }
    on(
        format!("Push ↑{unpushed}"),
        format!("Push {} to {remote_branch}", commits(unpushed)),
    )
}

/// The Push button. `push` is where the branch stands against origin, None when unknown.
/// `uncommitted` is the number of files that differ from the last commit, None when unknown.
pub fn push_button(
    branch: Option<&str>,
    push: Option<&PushStatus>,
    uncommitted: Option<u64>,
) -> GitButton {
    let Some(branch) = branch else {
        return off("Push", "This task has no branch to push");
    };
    match push {
        None | Some(PushStatus::Unknown) => on("Push", format!("Push {branch} to origin")),
        Some(PushStatus::None) => off("Push", "This project has no remote named origin"),
        // Like VS Code and GitHub Desktop: the first push of a branch is a publish.
        Some(PushStatus::New { unpushed }) if *unpushed > 0 => on(
            "Publish branch",
            format!("Publish {branch} to origin with {}", commits(*unpushed)),
        ),
        Some(PushStatus::New { .. }) => off(
            "Publish branch",
            commit_first(uncommitted)
                .unwrap_or_else(|| "Nothing to publish: every commit is on origin already".into()),
        ),
        Some(PushStatus::Tracked { unpushed, unpulled }) => {
            tracked_button(branch, *unpushed, *unpulled, uncommitted)
        }
    }
}

/// The button that opens a pull request.
pub fn pull_request_button(
    branch: Option<&str>,
    ahead: Option<u64>,
    base: Option<&str>,
    uncommitted: Option<u64>,
) -> GitButton {
    let label = "Create PR";
    if branch.is_none() {
        return off(label, "This task has no branch yet");
    }
    if ahead == Some(0) {
        let reason = commit_first(uncommitted).unwrap_or_else(|| {
            let of_base = base.map_or_else(String::new, |base| format!(" of {base}"));
            format!("Push first: 0 commits ahead{of_base}")
        });
        return off(label, reason);
    }
    GitButton {
        label: label.into(),
        ..GitButton::default()
    }
}

/// The git actions of a task's workspace. Only one runs at a time. Rollback and its undo join
/// with screen 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStep {
    // Commit… comes with screen 4; `next_git_step` already knows it.
    #[cfg_attr(not(test), allow(dead_code))]
    Commit,
    Push,
    PullRequest,
}

/// The git step to offer after one ends: a commit is pushed next, and a push opens the pull
/// request unless the task has one.
pub fn next_git_step(done: GitStep, has_pull_request: bool) -> Option<GitStep> {
    match done {
        GitStep::Commit => Some(GitStep::Push),
        GitStep::Push if !has_pull_request => Some(GitStep::PullRequest),
        _ => None,
    }
}

// The review banner comes with screen 3.
#[cfg_attr(not(test), allow(dead_code))]
/// The stages that wait for the user's review. In a group of stages that run side by side, only
/// the ones in review; else the task's own stage.
pub fn review_stages(task: &Task) -> Vec<String> {
    let lanes: Vec<String> = task
        .lanes
        .iter()
        .filter(|(_, state)| state.as_str() == Some("review"))
        .map(|(stage, _)| stage.clone())
        .collect();
    if lanes.is_empty() {
        vec![task.stage.clone()]
    } else {
        lanes
    }
}

/// Why Start task is off in the Start a task dialog. A source other than manual needs its
/// reference, or the task would start with no context. `skill` is the skill picked in the dialog,
/// None for a source from the project settings. `agent` is None where the dialog has no agent pick.
pub fn create_blocked_reason(
    title: &str,
    source: &str,
    reference: &str,
    skill: Option<&str>,
    agent: Option<&str>,
) -> Option<&'static str> {
    if title.trim().is_empty() {
        return Some("Enter a title");
    }
    if skill == Some("") {
        return Some("Pick a skill");
    }
    if source != "manual" && reference.trim().is_empty() {
        return Some("Enter the reference of the context");
    }
    // A plain shell cannot run a skill.
    (skill.is_some() && agent == Some("")).then_some("Pick an agent to run the skill")
}

#[cfg_attr(not(test), allow(dead_code))]
/// A task whose agent works or waits opens on its terminal. Another task opens on the tab that
/// the user used last in it, and a new one opens on its terminal.
pub fn tab_to_open(state: TaskState, saved: Option<WorkspaceTab>) -> WorkspaceTab {
    if matches!(state, TaskState::Running | TaskState::NeedsInput) {
        return WorkspaceTab::Terminal;
    }
    saved.unwrap_or(WorkspaceTab::Terminal)
}

#[cfg(test)]
#[path = "workspace_rules_tests.rs"]
mod tests;
