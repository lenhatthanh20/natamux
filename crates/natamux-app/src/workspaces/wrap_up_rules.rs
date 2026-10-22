//! What a done or cancelled task still has to do with git: the rules behind the finished banner
//! and the wrap-up. Port of `isFinishedState`, `finishedBanner`, `wrapUpRows` and `finishedAt` in
//! `packages/web/src/lib/finished.ts`, and `finishedText` in `lib/notify.ts`. The remove dialog
//! reads the same rules in `finished_rules.rs`, so the three always agree.

use natamux_proto::events::task_event::{RECORD_ONLY_EVENT_TYPES, StoredEvent};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::GitGraph;

use super::finished_rules::{
    FinishedTask, branch_merged, commits_are, is_open_pull_request, pull_request_name, unpushed,
};

/// What git says about the workspace. None while it is read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitFacts {
    /// Files that differ from the last commit.
    pub uncommitted: Option<u64>,
    pub graph: Option<GitGraph>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishedAction {
    Commit,
    Push,
    CreatePullRequest,
    OpenPullRequest,
    Remove,
    /// A wrap-up row only.
    ViewChanges,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinishedBannerText {
    pub title: String,
    /// None while git is read.
    pub text: Option<String>,
    /// The next step. None while git is read; Remove workspace is then a plain button.
    pub primary: Option<FinishedAction>,
    /// Why Remove workspace does nothing now. None: it works.
    pub remove_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    Ok,
    Warn,
    Info,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapUpRowId {
    Changes,
    Push,
    PullRequest,
    Branch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrapUpRow {
    pub id: WrapUpRowId,
    pub tone: RowTone,
    pub text: String,
    pub detail: Option<String>,
    pub action: Option<FinishedAction>,
}

pub fn is_finished_state(state: TaskState) -> bool {
    matches!(state, TaskState::Done | TaskState::Cancelled)
}

fn files_are(count: u64) -> String {
    if count == 1 {
        "1 file is".into()
    } else {
        format!("{count} files are")
    }
}

fn commits(count: u64) -> String {
    if count == 1 {
        "1 commit".into()
    } else {
        format!("{count} commits")
    }
}

fn checks_text(checks: &str) -> Option<&'static str> {
    match checks {
        "passing" => Some("Checks pass"),
        "failing" => Some("Checks fail"),
        "pending" => Some("Checks running"),
        _ => None,
    }
}

fn banner(title: &str, text: impl Into<String>, primary: FinishedAction) -> FinishedBannerText {
    FinishedBannerText {
        title: title.into(),
        text: Some(text.into()),
        primary: Some(primary),
        remove_reason: None,
    }
}

pub fn finished_banner(task: &Task, facts: &GitFacts) -> FinishedBannerText {
    let title = if task.state == TaskState::Done {
        "Task done."
    } else {
        "Task cancelled."
    };
    let (Some(uncommitted), Some(graph)) = (facts.uncommitted, facts.graph.as_ref()) else {
        return FinishedBannerText {
            title: title.into(),
            text: None,
            primary: None,
            remove_reason: None,
        };
    };
    if uncommitted > 0 {
        return FinishedBannerText {
            remove_reason: Some("Commit or discard them first".into()),
            ..banner(
                title,
                format!("{} not committed.", files_are(uncommitted)),
                FinishedAction::Commit,
            )
        };
    }
    let nothing = "Nothing is left in the workspace.";
    if task.state == TaskState::Cancelled {
        let text = if graph.ahead == Some(0) {
            nothing
        } else {
            "Its commits and branch are kept."
        };
        return banner(title, text, FinishedAction::Remove);
    }
    let to_push = unpushed(graph);
    if to_push > 0 {
        let text = format!("{} not pushed.", commits_are(to_push));
        return banner(title, text, FinishedAction::Push);
    }
    let finished = FinishedTask::from_task(task);
    let Some(pr) = task.pr.as_deref() else {
        if graph.ahead.is_some_and(|ahead| ahead > 0) {
            let text = "Its branch has no pull request.";
            return banner(title, text, FinishedAction::CreatePullRequest);
        }
        return banner(title, nothing, FinishedAction::Remove);
    };
    let name = pull_request_name(pr);
    if is_open_pull_request(&finished) {
        let text = format!("{name} is still open.");
        return banner(title, text, FinishedAction::OpenPullRequest);
    }
    if status_state(&finished) == Some("closed") {
        let text = format!("{name} was closed without a merge.");
        return banner(title, text, FinishedAction::Remove);
    }
    banner(
        title,
        format!("{name} is merged. {nothing}"),
        FinishedAction::Remove,
    )
}

fn status_state(task: &FinishedTask) -> Option<&str> {
    task.pr_status.as_ref().map(|status| status.state.as_str())
}

fn row(id: WrapUpRowId, tone: RowTone, text: impl Into<String>) -> WrapUpRow {
    WrapUpRow {
        id,
        tone,
        text: text.into(),
        detail: None,
        action: None,
    }
}

fn changes_row(uncommitted: Option<u64>, paths: &[String]) -> WrapUpRow {
    let id = WrapUpRowId::Changes;
    let Some(uncommitted) = uncommitted else {
        return row(id, RowTone::Muted, "Reading the changes…");
    };
    if uncommitted == 0 {
        return row(id, RowTone::Ok, "All changes are committed");
    }
    let shown = &paths[..paths.len().min(3)];
    let more = uncommitted.saturating_sub(shown.len() as u64);
    let mut detail = shown.join(", ");
    if more > 0 {
        detail.push_str(&format!(" and {more} more"));
    }
    WrapUpRow {
        detail: (!shown.is_empty()).then_some(detail),
        action: Some(FinishedAction::ViewChanges),
        ..row(
            id,
            RowTone::Warn,
            format!("{} not committed", files_are(uncommitted)),
        )
    }
}

fn push_row(graph: Option<&GitGraph>) -> WrapUpRow {
    use natamux_proto::workspaces::workspace::PushStatus;
    let id = WrapUpRowId::Push;
    let Some(graph) = graph else {
        return row(id, RowTone::Muted, "Reading the branch…");
    };
    let with_push = |text: String| WrapUpRow {
        action: Some(FinishedAction::Push),
        ..row(id, RowTone::Warn, text)
    };
    match graph.push {
        None | Some(PushStatus::Unknown) => {
            row(id, RowTone::Muted, "The workspace is not on a branch")
        }
        Some(PushStatus::None) => row(
            id,
            RowTone::Muted,
            "This project has no remote named origin",
        ),
        Some(PushStatus::New { unpushed }) if unpushed > 0 => {
            with_push("The branch is not on origin yet".into())
        }
        Some(PushStatus::New { .. }) => row(id, RowTone::Muted, "Nothing to push"),
        Some(PushStatus::Tracked { unpushed, .. }) if unpushed > 0 => {
            with_push(format!("{} not pushed", commits_are(unpushed)))
        }
        Some(PushStatus::Tracked { .. }) => row(id, RowTone::Ok, "Pushed to origin"),
    }
}

fn pull_request_row(task: &Task, graph: Option<&GitGraph>) -> WrapUpRow {
    let id = WrapUpRowId::PullRequest;
    let Some(pr) = task.pr.as_deref() else {
        let ahead = graph.and_then(|graph| graph.ahead).unwrap_or(0);
        let can_create = task.state == TaskState::Done && ahead > 0;
        return WrapUpRow {
            action: can_create.then_some(FinishedAction::CreatePullRequest),
            ..row(id, RowTone::Muted, "No pull request")
        };
    };
    let name = pull_request_name(pr);
    let status = task.pull_request_status();
    let detail = status
        .as_ref()
        .and_then(|status| checks_text(&status.checks));
    let with = |tone, text: String| WrapUpRow {
        detail: detail.map(String::from),
        action: Some(FinishedAction::OpenPullRequest),
        ..row(id, tone, text)
    };
    match status.as_ref().map(|status| status.state.as_str()) {
        Some("merged") => match graph.and_then(|graph| graph.base.as_deref()) {
            Some(base) if !base.is_empty() => {
                with(RowTone::Ok, format!("{name} merged into {base}"))
            }
            _ => with(RowTone::Ok, format!("{name} merged")),
        },
        Some("closed") => with(RowTone::Muted, format!("{name} was closed without a merge")),
        Some("draft") => with(RowTone::Info, format!("{name} is a draft")),
        _ => with(RowTone::Info, format!("{name} is open")),
    }
}

fn branch_row(task: &FinishedTask, graph: Option<&GitGraph>) -> WrapUpRow {
    let id = WrapUpRowId::Branch;
    let Some(graph) = graph else {
        return row(id, RowTone::Muted, "Reading the branch…");
    };
    if branch_merged(task, graph) {
        let text = "The branch is merged. Removing the workspace deletes it.";
        return row(id, RowTone::Ok, text);
    }
    match graph.ahead {
        Some(ahead) if ahead > 0 => WrapUpRow {
            detail: Some("It stays after the workspace is removed".into()),
            ..row(
                id,
                RowTone::Info,
                format!("The branch has {} that are not merged", commits(ahead)),
            )
        },
        _ => row(id, RowTone::Muted, "The branch stays unless it is merged"),
    }
}

/// One row per thing to check before the workspace goes. `paths` are the changed files, for the
/// first row.
pub fn wrap_up_rows(task: &Task, facts: &GitFacts, paths: &[String]) -> Vec<WrapUpRow> {
    let graph = facts.graph.as_ref();
    let mut rows = vec![
        changes_row(facts.uncommitted, paths),
        push_row(graph),
        pull_request_row(task, graph),
    ];
    if task.branch.is_some() {
        rows.push(branch_row(&FinishedTask::from_task(task), graph));
    }
    rows
}

/// When a done or cancelled task finished: its last accepted event that moves a task. A finished
/// task takes no such event, only ones that are kept for the record.
pub fn finished_at(events: &[StoredEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|stored| {
            stored.accepted && !RECORD_ONLY_EVENT_TYPES.contains(&stored.event.kind.as_str())
        })
        .map(|stored| stored.event.at.clone())
}

/// The toast for the task on screen when it finishes.
pub fn finished_text(task: &Task) -> String {
    if task.state == TaskState::Cancelled {
        format!("{} was cancelled.", task.id)
    } else {
        format!("{} is done.", task.id)
    }
}

#[cfg(test)]
#[path = "wrap_up_rules_tests.rs"]
mod tests;
