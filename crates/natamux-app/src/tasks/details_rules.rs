//! The texts and choices of the Details tab (`features/tasks/TaskDetail.tsx`, `Subtasks.tsx`,
//! `packages/core/src/terminals/ports.ts`, `packages/web/src/lib/pull-request.ts`).

// Ported with their tests (spec D14); the details panel calls them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::projects::repo::BranchList;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::{TerminalLane, TerminalList};

use super::task_rules::{AgentAction, TaskAction};
use super::task_time::{AgeStyle, format_age, format_exact};
use crate::workspaces::new_workspace_rules::start_branch;

pub const NO_NOTES: &str = "No notes. Add some with Edit.";
pub const NO_WORKSPACE: &str = "No workspace.";

/// "No sign of life for 12m. The agent may be stuck." The caller shows it when `is_quiet`.
pub fn quiet_banner(updated_at: &str, now_ms: i64, offset_minutes: i32) -> String {
    let age = format_age(updated_at, now_ms, AgeStyle::Short, offset_minutes);
    format!("No sign of life for {age}. The agent may be stuck.")
}

/// Nothing when the task waits for no one.
pub fn waits_for_text(waiting_for: &[String]) -> Option<String> {
    (!waiting_for.is_empty()).then(|| format!("Waits for {} to be done", waiting_for.join(", ")))
}

/// The branch a new workspace starts from: the parent's branch when it is in the list, else the
/// default branch, else the checkout's.
pub fn details_start_branch(
    parent_branch: Option<&str>,
    list: Option<&BranchList>,
) -> Option<String> {
    if let (Some(parent), Some(list)) = (parent_branch, list)
        && list.branches.iter().any(|name| name == parent)
    {
        return Some(parent.to_string());
    }
    start_branch(list)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentButton {
    pub label: &'static str,
    pub icon: &'static str,
    pub tip: String,
    /// The button waits while the task waits for other tasks.
    pub disabled: bool,
}

pub fn agent_button(action: AgentAction, waiting_for: &[String], busy: bool) -> AgentButton {
    let restart = action == AgentAction::Restart;
    let tip = match waits_for_text(waiting_for) {
        Some(text) => text,
        None if restart => "Starts Claude Code again in the terminal of the workspace".into(),
        None => "Creates the workspace when there is none, then starts Claude Code in its terminal"
            .into(),
    };
    AgentButton {
        label: if restart {
            "Restart agent"
        } else {
            "Start agent"
        },
        icon: if restart { "rotate-ccw" } else { "play" },
        tip,
        disabled: busy || !waiting_for.is_empty(),
    }
}

/// Copies of a task that works now would run beside its own agent.
pub fn can_fan_out(state: TaskState, agent_button: Option<AgentAction>) -> bool {
    !state.is_finished() && agent_button.is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Edit,
    SaveTemplate,
    RunMoreTries,
    Cancel,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoreItem {
    Item {
        label: String,
        icon: &'static str,
        danger: bool,
        action: MenuAction,
    },
    Separator,
}

fn item(label: &str, icon: &'static str, danger: bool, action: MenuAction) -> MoreItem {
    MoreItem::Item {
        label: label.into(),
        icon,
        danger,
        action,
    }
}

/// The "..." menu: edit and a line, save and run more tries, a line, then cancel and delete.
pub fn more_menu(
    can_edit: bool,
    can_save_template: bool,
    can_fan_out: bool,
    cancel: Option<&TaskAction>,
    can_delete: bool,
) -> Vec<MoreItem> {
    let mut leading = Vec::new();
    if can_save_template {
        leading.push(item(
            "Save as template",
            "save",
            false,
            MenuAction::SaveTemplate,
        ));
    }
    if can_fan_out {
        leading.push(item(
            "Run more tries...",
            "copy-plus",
            false,
            MenuAction::RunMoreTries,
        ));
    }
    let mut danger = Vec::new();
    if let Some(cancel) = cancel {
        danger.push(item(cancel.label, "ban", true, MenuAction::Cancel));
    }
    if can_delete {
        danger.push(item("Delete task", "trash", true, MenuAction::Delete));
    }
    let mut menu = Vec::new();
    if can_edit {
        menu.push(item("Edit task", "pencil", false, MenuAction::Edit));
    }
    for group in [leading, danger] {
        if group.is_empty() {
            continue;
        }
        if !menu.is_empty() {
            menu.push(MoreItem::Separator);
        }
        menu.extend(group);
    }
    menu
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailField {
    pub label: &'static str,
    pub text: String,
    /// The exact time, for a relative one.
    pub tip: Option<String>,
}

fn field(label: &'static str, text: impl Into<String>, tip: Option<String>) -> DetailField {
    DetailField {
        label,
        text: text.into(),
        tip,
    }
}

/// The fields of the Details section, in order.
pub fn details_fields(task: &Task, now_ms: i64, offset_minutes: i32) -> Vec<DetailField> {
    let mut fields = Vec::new();
    if let Some(repo) = &task.repo {
        fields.push(field("Project", repo.as_str(), None));
    }
    let depends = if task.depends_on.is_empty() {
        "-".to_string()
    } else {
        task.depends_on.join(", ")
    };
    fields.push(field("Depends on", depends, None));
    if let Some(parent) = &task.parent_id {
        fields.push(field("Part of", parent.as_str(), None));
    }
    for (label, time) in [("Created", &task.created_at), ("Updated", &task.updated_at)] {
        let age = format_age(time, now_ms, AgeStyle::Long, offset_minutes);
        fields.push(field(label, age, Some(format_exact(time, offset_minutes))));
    }
    fields
}

const PORT_BASE: u32 = 20_000;
const PORTS_PER_TASK: u32 = 10;
const BLOCKS: u32 = 1000;

/// Each task gets its own block of dev ports: T-0001 gets 20010 to 20019. The block comes from
/// the number at the end of the id and wraps after 1000 tasks.
pub fn port_range(task_id: &str) -> (u32, u32) {
    let digits = task_id
        .rsplit_once('-')
        .map(|(_, digits)| digits)
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .unwrap_or("0");
    // Only the last three digits matter for a number modulo 1000.
    let tail = &digits[digits.len().saturating_sub(3)..];
    let number: u32 = tail.parse().unwrap_or(0);
    let first = PORT_BASE + (number % BLOCKS) * PORTS_PER_TASK;
    (first, first + PORTS_PER_TASK - 1)
}

/// "20420–20429".
pub fn ports_text(task_id: &str) -> String {
    let (first, last) = port_range(task_id);
    format!("{first}–{last}")
}

pub const PORTS_HINT: &str = " (first is $PORT)";

/// "open, checks fail", or None when GitHub was not asked yet.
pub fn pull_request_summary(task: &Task) -> Option<String> {
    let status = task.pull_request_status()?;
    let checks = match status.checks.as_str() {
        "passing" => Some("checks pass"),
        "failing" => Some("checks fail"),
        "pending" => Some("checks running"),
        _ => None,
    };
    let parts: Vec<&str> = [Some(status.state.as_str()), checks]
        .into_iter()
        .flatten()
        .collect();
    Some(parts.join(", "))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksTone {
    Add,
    Remove,
    Signal,
    Muted,
}

/// The colour of the summary: green passing, red failing, amber running, grey none.
pub fn checks_tone(checks: &str) -> ChecksTone {
    match checks {
        "passing" => ChecksTone::Add,
        "failing" => ChecksTone::Remove,
        "pending" => ChecksTone::Signal,
        _ => ChecksTone::Muted,
    }
}

/// The address without `https://github.com/`, as the Pull request section shows it.
pub fn pull_request_label(url: &str) -> &str {
    url.strip_prefix("https://github.com/").unwrap_or(url)
}

/// "2 of 5 done", beside the Subtasks title; None without subtasks.
pub fn subtasks_aside(subtasks: &[Task]) -> Option<String> {
    let done = subtasks
        .iter()
        .filter(|task| task.state == TaskState::Done)
        .count();
    (!subtasks.is_empty()).then(|| format!("{done} of {} done", subtasks.len()))
}

/// Answer in terminal: the terminal whose agent asks, else the one of the task's stage, else
/// Terminal 1.
pub fn asking_terminal(list: Option<&TerminalList>, task: &Task) -> String {
    let lanes = list.map(|list| &list.lanes);
    let find = |keep: &dyn Fn(&TerminalLane) -> bool| {
        lanes.and_then(|lanes| {
            lanes
                .iter()
                .find(|(_, lane)| keep(lane))
                .map(|(name, _)| name.clone())
        })
    };
    find(&|lane| lane.state == Some(TaskState::NeedsInput))
        .or_else(|| find(&|lane| lane.stage.as_deref() == Some(task.stage.as_str())))
        .unwrap_or_else(|| "1".into())
}

#[cfg(test)]
#[path = "details_rules_tests.rs"]
mod tests;
