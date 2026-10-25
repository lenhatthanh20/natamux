//! What a palette row does when it is picked (`pick` of `CommandPalette.tsx` and the handlers
//! of `useDashboardCommands.ts`). The layer closes first, so focus is back on the opener.

use gpui::Entity;
use ui::prelude::*;

use super::palette_rows::{PaletteRow, add_to_agents};
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::action_for;
use crate::components::toast_rules::ToastTone;
use crate::tasks::add_task_dialog;
use crate::terminals::pane_rules::agent_pane_tasks;
use crate::workspaces::new_workspace_dialog;

pub fn run(
    dashboard: &Entity<Dashboard>,
    row: PaletteRow,
    shift: bool,
    window: &mut Window,
    cx: &mut App,
) {
    match row {
        PaletteRow::Command { command, .. } => {
            if let Some(action) = action_for(command.id) {
                window.dispatch_action(action, cx);
            }
        }
        PaletteRow::Task { task, .. } => {
            if shift && task.workspace.is_some() {
                join_agents(dashboard, &task.id, cx);
            } else {
                dashboard.update(cx, |dashboard, cx| dashboard.open(&task.id, cx));
            }
        }
        PaletteRow::Shell { task, name } => {
            if shift {
                join_agents(dashboard, &task.id, cx);
            } else {
                dashboard.update(cx, |dashboard, cx| {
                    dashboard.open_shell(&task.id, Some(name), cx)
                });
            }
        }
        PaletteRow::Repo { repo, .. } => {
            new_workspace_dialog::open(dashboard, Some(repo.id), window, cx);
        }
        PaletteRow::CreateTask { title } => {
            add_task_dialog::open(dashboard, Some(title.trim().to_owned()), window, cx);
        }
        PaletteRow::NewWorkspace { title } => {
            let title = title.trim().to_owned();
            new_workspace_dialog::open_with(dashboard, None, Some(title), window, cx);
        }
    }
}

/// `addToAgents`: the task joins the grid, which then shows; a full grid says so instead.
fn join_agents(dashboard: &Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        let shown: Vec<String> = agent_pane_tasks(
            dashboard.view().agent_panes.as_deref(),
            &dashboard.data.tasks,
        )
        .iter()
        .map(|task| task.id.clone())
        .collect();
        match add_to_agents(&shown, id) {
            Ok(list) => {
                dashboard.set_agent_panes(Some(list), cx);
                dashboard.open_agents(cx);
            }
            Err(text) => dashboard.toast(&text, ToastTone::Error, None, cx),
        }
    });
}

/// A key the palette takes before its input sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Enter {
        shift: bool,
    },
    /// Alt+1 to Alt+9.
    Digit(usize),
    Tab,
    Backspace,
    Escape,
}

/// The palette key of a press. The key name, not the typed character, tells the digit: on macOS
/// Alt+digit types a symbol.
pub fn classify(keystroke: &gpui::Keystroke) -> Option<PaletteKey> {
    let held = &keystroke.modifiers;
    if held.platform || held.control || held.function {
        return None;
    }
    if held.alt {
        let digit = keystroke
            .key
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=9).contains(n));
        return digit.map(PaletteKey::Digit);
    }
    Some(match keystroke.key.as_str() {
        "up" => PaletteKey::Up,
        "down" => PaletteKey::Down,
        "pageup" => PaletteKey::PageUp,
        "pagedown" => PaletteKey::PageDown,
        "enter" => PaletteKey::Enter { shift: held.shift },
        "tab" => PaletteKey::Tab,
        "backspace" => PaletteKey::Backspace,
        "escape" => PaletteKey::Escape,
        _ => return None,
    })
}
