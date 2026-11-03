//! The row menu of the task table (`menuFor` in `TaskTable.tsx`): Open workspace, Add subtask…,
//! the state actions, Start or Restart agent, the copy items, Cancel task and Delete task. Which
//! items show is `table_rules::row_menu`; this file gives them icons and handlers. Titles and keys
//! come from the command registry (`app/commands.rs`).

use gpui::{ClipboardItem, Entity, FocusHandle};
use natamux_proto::tasks::task::Task;
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::add_subtask_dialog;
use super::table_rules::{RowItem, row_menu};
use super::task_confirms::{confirm_cancel_task, confirm_delete_task};
use super::task_rules::TaskAction;
use crate::agents::agent_choices::{DEFAULT_AGENT, is_startable};
use crate::app::commands::{shortcut_of, title_of};
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::action_for;
use crate::components::icons::icon_path;
use crate::components::toast_rules::ToastTone;

/// The icon of a state action (`ACTION_ICON`).
fn action_icon(event: &str) -> &'static str {
    match event {
        "task.blocked" => "circle-pause",
        "agent.resumed" => "play",
        "review.changes-requested" => "undo-2",
        "task.unblocked" => "circle-play",
        "task.retried" => "rotate-ccw",
        _ => "check",
    }
}

/// The item of a registry command, with the command's key when it has one.
fn command_entry(id: &'static str, label: &'static str, icon: &str) -> ContextMenuEntry {
    let entry = ContextMenuEntry::new(label).custom_icon_path(icon_path(icon));
    match shortcut_of(id).and_then(|_| action_for(id)) {
        Some(action) => entry.action(action),
        None => entry,
    }
}

fn copy(text: String, what: &'static str, dashboard: &Entity<Dashboard>, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    dashboard.update(cx, |dashboard, cx| {
        dashboard.toast(&format!("Copied {what}."), ToastTone::Info, None, cx)
    });
}

fn state_action(
    task: &Task,
    action: TaskAction,
    dashboard: &Entity<Dashboard>,
) -> ContextMenuEntry {
    let id = task.id.clone();
    let dashboard = dashboard.clone();
    let command = format!("task.{}", action.event_type);
    let entry = ContextMenuEntry::new(action.label)
        .custom_icon_path(icon_path(action_icon(action.event_type)))
        .handler(move |_, cx| {
            dashboard.update(cx, |dashboard, cx| {
                dashboard.send_event(&id, action.event_type, cx).detach()
            })
        });
    match shortcut_of(&command).and_then(|_| action_for(&command)) {
        Some(registry_action) => entry.action(registry_action),
        None => entry,
    }
}

/// Starts the task's agent (or the default one), then opens the task, as the web does.
fn start_agent(task: &Task, dashboard: &Entity<Dashboard>, cx: &mut App) {
    let id = task.id.clone();
    let agent = match task.agent.as_deref() {
        Some(agent) if is_startable(Some(agent)) => agent.to_string(),
        _ => DEFAULT_AGENT.to_string(),
    };
    let written = dashboard.update(cx, |dashboard, cx| dashboard.start_agent(&id, &agent, cx));
    let dashboard = dashboard.clone();
    cx.spawn(async move |cx| {
        if written.await.is_ok() {
            dashboard.update(cx, |dashboard, cx| dashboard.open(&id, cx));
        }
    })
    .detach();
}

fn entry(task: &Task, item: RowItem, dashboard: &Entity<Dashboard>) -> Option<ContextMenuEntry> {
    let id = task.id.clone();
    let dashboard = dashboard.clone();
    let entry = match item {
        RowItem::Separator => return None,
        RowItem::Open => command_entry("task.open", "Open workspace", "file-diff")
            .handler(move |_, cx| dashboard.update(cx, |dashboard, cx| dashboard.open(&id, cx))),
        RowItem::Details => command_entry(
            "task.show-details",
            title_of("task.show-details"),
            "panel-right",
        )
        .handler(move |_, cx| dashboard.update(cx, |dashboard, cx| dashboard.choose(&id, cx))),
        RowItem::AddSubtask => {
            let task = task.clone();
            let id = "task.add-subtask";
            command_entry(id, title_of(id), "list-plus")
                .handler(move |window, cx| add_subtask_dialog::open(task.clone(), window, cx))
        }
        RowItem::Action(action) => state_action(task, action, &dashboard),
        RowItem::Agent { restart, waits } => {
            let (command, icon) = if restart {
                ("agent.restart", "rotate-ccw")
            } else {
                ("agent.start", "play")
            };
            let task = task.clone();
            command_entry(command, title_of(command), icon)
                .disabled(!waits.is_empty())
                .handler(move |_, cx| start_agent(&task, &dashboard, cx))
        }
        RowItem::CopyId => command_entry("task.copy-id", title_of("task.copy-id"), "copy")
            .handler(move |_, cx| copy(id.clone(), "the task id", &dashboard, cx)),
        RowItem::CopyBranch { enabled } => {
            let branch = task.branch.clone().unwrap_or_default();
            command_entry("row.copy-branch", title_of("row.copy-branch"), "copy")
                .disabled(!enabled)
                .handler(move |_, cx| copy(branch.clone(), "the branch name", &dashboard, cx))
        }
        RowItem::Cancel(action) => {
            let task = task.clone();
            command_entry("task.cancel", action.label, "ban")
                .icon_color(Color::Error)
                .handler(move |window, cx| confirm_cancel_task(&dashboard, &task, window, cx))
        }
        RowItem::Delete => {
            let task = task.clone();
            command_entry("task.delete", title_of("task.delete"), "trash")
                .icon_color(Color::Error)
                .handler(move |window, cx| confirm_delete_task(&dashboard, &task, window, cx))
        }
    };
    Some(entry)
}

/// The menu of `task`. `list` is the table's focus, so the menu's keys show the list's bindings
/// and focus goes back to the list.
pub fn build_row_menu(
    task: &Task,
    tasks: &[Task],
    dashboard: &Entity<Dashboard>,
    list: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    let items = row_menu(task, tasks);
    let task = task.clone();
    let dashboard = dashboard.clone();
    ContextMenu::build(window, cx, move |menu, _, _| {
        items.into_iter().fold(menu.context(list), |menu, item| {
            if item == RowItem::Separator {
                return menu.separator();
            }
            match entry(&task, item, &dashboard) {
                Some(entry) => menu.item(entry),
                None => menu,
            }
        })
    })
}
