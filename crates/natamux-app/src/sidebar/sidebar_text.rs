//! What the sidebar's rows, menus and dialogs say, with no gpui (`WorkspaceList.tsx`,
//! `WorkspaceRow.tsx`, `lib/workspace.ts` `rowText`, `lib/pull-request.ts`, and the type-ahead of
//! `lib/roving.ts`).

use natamux_proto::events::task_event::{StoredEvent, WaitReason, wait_reason};
use natamux_proto::tasks::task::{PullRequestStatus, Task, TaskState};

/// Letters typed within this time make one word for type-ahead (`useRovingFocus`).
pub const TYPE_AHEAD_MS: i64 = 600;

/// What a blurred project is called in labels that show as tooltips.
pub const HIDDEN_NAME: &str = "this project";

/// One item of a row or project menu. `C` names what the item does; the view runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem<C> {
    Item {
        /// Its command in the registry (`app/commands.rs`): the menu shows that command's key.
        id: &'static str,
        label: &'static str,
        /// A Lucide icon name.
        icon: &'static str,
        command: C,
        danger: bool,
        disabled: bool,
        /// A line under a disabled item that says why.
        hint: Option<&'static str>,
    },
    Separator,
}

fn item<C>(id: &'static str, label: &'static str, icon: &'static str, command: C) -> MenuItem<C> {
    MenuItem::Item {
        id,
        label,
        icon,
        command,
        danger: false,
        disabled: false,
        hint: None,
    }
}

fn danger<C>(id: &'static str, label: &'static str, icon: &'static str, command: C) -> MenuItem<C> {
    MenuItem::Item {
        id,
        label,
        icon,
        command,
        danger: true,
        disabled: false,
        hint: None,
    }
}

fn disabled<C>(menu_item: MenuItem<C>, off: bool, why: Option<&'static str>) -> MenuItem<C> {
    match menu_item {
        MenuItem::Item {
            id,
            label,
            icon,
            command,
            danger,
            ..
        } => MenuItem::Item {
            id,
            label,
            icon,
            command,
            danger,
            disabled: off,
            hint: if off { why } else { None },
        },
        separator => separator,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowCommand {
    ViewChanges,
    Rename,
    AddSubtask,
    TogglePin,
    CopyBranch,
    CopyFolder,
    RemoveWorkspace,
    DeleteTask,
}

/// The row menu (docs/ui-spec.md 4.9). "View changes" is first, for a task with a workspace.
pub fn row_menu(task: &Task, pinned: bool) -> Vec<MenuItem<RowCommand>> {
    let mut items = Vec::new();
    if task.workspace.is_some() {
        items.push(item(
            "task.view-changes",
            "View changes",
            "file-diff",
            RowCommand::ViewChanges,
        ));
    }
    items.push(item("row.rename", "Rename", "pencil", RowCommand::Rename));
    // Subtasks go one level deep.
    if task.parent_id.is_none() {
        items.push(item(
            "task.add-subtask",
            "Add subtask…",
            "list-plus",
            RowCommand::AddSubtask,
        ));
    }
    items.push(if pinned {
        item("row.unpin", "Unpin", "pin-off", RowCommand::TogglePin)
    } else {
        item("row.pin", "Pin to the top", "pin", RowCommand::TogglePin)
    });
    items.push(disabled(
        item(
            "row.copy-branch",
            "Copy branch name",
            "git-branch",
            RowCommand::CopyBranch,
        ),
        task.branch.is_none(),
        None,
    ));
    items.push(item(
        "row.copy-path",
        "Copy folder path",
        "copy",
        RowCommand::CopyFolder,
    ));
    items.push(MenuItem::Separator);
    items.push(danger(
        "row.remove",
        "Remove workspace",
        "folder-minus",
        RowCommand::RemoveWorkspace,
    ));
    items.push(danger(
        "task.delete",
        "Delete task",
        "trash",
        RowCommand::DeleteTask,
    ));
    items
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectCommand {
    StartTask,
    ToggleBlur,
    RemoveFinished,
    RemoveProject,
}

/// What the project menu needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectMenuInput {
    /// The daemon still has the project; a group of a project that is gone has only the actions
    /// on its workspaces.
    pub known: bool,
    pub blurred: bool,
    /// Finished workspaces in the group.
    pub finished: usize,
    /// A removal of finished workspaces runs.
    pub removing_finished: bool,
    /// Workspaces of the project, also the ones the filter hides: the daemon refuses to remove it.
    pub workspaces: usize,
}

/// The project menu (docs/ui-spec.md 4.9). "Settings" waits for the settings screen.
pub fn project_menu(input: ProjectMenuInput) -> Vec<MenuItem<ProjectCommand>> {
    let blur = if input.blurred {
        item(
            "project.show-names",
            "Show names",
            "eye",
            ProjectCommand::ToggleBlur,
        )
    } else {
        item(
            "project.blur",
            "Blur names",
            "eye-off",
            ProjectCommand::ToggleBlur,
        )
    };
    let mut items = Vec::new();
    if input.known {
        items.push(item(
            "workspace.new",
            "Start a task",
            "plus",
            ProjectCommand::StartTask,
        ));
    }
    items.push(blur);
    if input.finished > 0 {
        items.push(MenuItem::Separator);
        items.push(disabled(
            danger(
                "project.remove-finished",
                "Remove finished workspaces",
                "folder-minus",
                ProjectCommand::RemoveFinished,
            ),
            input.removing_finished,
            None,
        ));
    }
    if input.known {
        items.push(MenuItem::Separator);
        items.push(disabled(
            danger(
                "project.remove",
                "Remove project",
                "folder-minus",
                ProjectCommand::RemoveProject,
            ),
            input.workspaces > 0,
            Some("Remove its workspaces first"),
        ));
    }
    items
}

pub fn remove_project_title(name: &str) -> String {
    format!("Remove {name} from Natamux?")
}

pub fn remove_project_description(path: &str) -> String {
    format!(
        "Natamux forgets this project. The folder {path} and its branches stay on disk. Tasks \
         without a workspace keep their link and come back if you add the folder again."
    )
}

/// The name of a project in a label that shows as a tooltip.
pub fn shown_name(name: &str, blurred: bool) -> String {
    if blurred {
        HIDDEN_NAME.to_owned()
    } else {
        name.to_owned()
    }
}

/// "1 workspace", "3 workspaces": what the count badge of a project says to a screen reader.
pub fn workspace_count(count: usize) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} workspace{plural}")
}

/// What a waiting task's banner says (`bannerText`).
pub fn banner_text(reason: &WaitReason) -> String {
    match reason {
        WaitReason::Asked { question: None } => {
            "The agent is waiting for your answer in its terminal.".to_owned()
        }
        WaitReason::Asked {
            question: Some(question),
        } => format!("The agent asks: {question}"),
        WaitReason::CheckFailed { reasons } if reasons.is_empty() => {
            "The checks of this stage failed.".to_owned()
        }
        WaitReason::CheckFailed { reasons } => {
            format!("The checks failed: {}", reasons.join("; "))
        }
        WaitReason::Failed { reason } => reason
            .clone()
            .unwrap_or_else(|| "The agent failed.".to_owned()),
        WaitReason::Stopped => {
            "The agent ended its turn. Reply in the terminal, or resume it.".to_owned()
        }
    }
}

/// The line under a task in the sidebar (`rowText`).
pub fn row_text(reason: &WaitReason) -> String {
    match reason {
        WaitReason::Asked { question: None } => "Waiting for your answer".to_owned(),
        other => banner_text(other),
    }
}

/// What an attention card says under its title: the question or why the task failed. A blocked
/// task, or one whose events are not read yet, says nothing, except that a task that needs input
/// waits for the answer.
pub fn card_line(task: &Task, events: Option<&[StoredEvent]>) -> Option<String> {
    let asks =
        matches!(task.state, TaskState::NeedsInput | TaskState::Failed) && task.workspace.is_some();
    match events {
        Some(events) if asks => Some(row_text(&wait_reason(Some(events)))),
        _ if task.state == TaskState::NeedsInput => Some("Waiting for your answer".to_owned()),
        _ => None,
    }
}

/// "3/5" for a task with 2 steps or more.
pub fn step_text(at: usize, of: usize) -> Option<String> {
    (of > 1).then(|| format!("{at}/{of}"))
}

/// The new title after a rename, or None when nothing changes.
pub fn renamed_title(draft: &str, title: &str) -> Option<String> {
    let draft = draft.trim();
    (!draft.is_empty() && draft != title).then(|| draft.to_owned())
}

/// "https://github.com/o/r/pull/12" becomes "#12" (`pullRequestNumber`).
pub fn pull_request_number(url: &str) -> String {
    url.split("/pull/")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .filter(|digits| !digits.is_empty())
        .map_or_else(|| "Pull request".to_owned(), |digits| format!("#{digits}"))
}

/// "open, checks fail", or None when GitHub was not asked yet (`pullRequestSummary`).
pub fn pull_request_summary(status: Option<&PullRequestStatus>) -> Option<String> {
    let status = status?;
    let checks = match status.checks.as_str() {
        "passing" => Some("checks pass"),
        "failing" => Some("checks fail"),
        "pending" => Some("checks running"),
        _ => None,
    };
    let parts: Vec<&str> = std::iter::once(status.state.as_str())
        .chain(checks)
        .collect();
    Some(parts.join(", "))
}

/// The next label that starts with what was typed. A single letter pressed again goes to the
/// next label with that letter (`typeAheadTarget`).
pub fn type_ahead_target(labels: &[String], typed: &str, from: usize) -> Option<usize> {
    let text = typed.to_lowercase();
    if text.is_empty() || labels.is_empty() {
        return None;
    }
    let count = labels.len();
    let start = if text.chars().count() == 1 {
        from + 1
    } else {
        from
    };
    (0..count)
        .map(|step| (start + step) % count)
        .find(|&index| labels[index].to_lowercase().starts_with(&text))
}

/// The word typed so far: a pause longer than `TYPE_AHEAD_MS` starts a new one.
pub fn typed_word(word: &str, last_at_ms: i64, now_ms: i64, key: &str) -> String {
    if now_ms - last_at_ms > TYPE_AHEAD_MS {
        key.to_owned()
    } else {
        format!("{word}{key}")
    }
}

#[cfg(test)]
#[path = "sidebar_text_tests.rs"]
mod tests;
