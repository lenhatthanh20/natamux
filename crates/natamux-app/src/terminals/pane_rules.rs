//! Port of `packages/web/src/lib/panes.ts`: which tasks the agents grid shows, its columns, and the
//! text of a pane. `terminalState` is `sidebar/shell_rows.rs::terminal_state`.

use std::collections::BTreeMap;

use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::TerminalLane;

use crate::sidebar::shell_rows::terminal_state;
use crate::sidebar::sidebar_tree::state_label;
use crate::tasks::task_sort::task_number;

/// More panes than this make each terminal too small to read.
pub const MAX_PANES: usize = 6;

fn has_agent(task: &Task) -> bool {
    task.workspace.is_some() && matches!(task.state, TaskState::Running | TaskState::NeedsInput)
}

/// The tasks in the agents grid. Until the user picks (`chosen` is None), the ones whose agent
/// works or waits. The task list comes newest first, so each update would move a pane: the default
/// grid sorts by task id, which does not change.
pub fn agent_pane_tasks<'a>(chosen: Option<&[String]>, tasks: &'a [Task]) -> Vec<&'a Task> {
    let Some(chosen) = chosen else {
        let mut found: Vec<&Task> = tasks.iter().filter(|task| has_agent(task)).collect();
        found.sort_by(|a, b| {
            task_number(&a.id)
                .cmp(&task_number(&b.id))
                .then_with(|| a.id.cmp(&b.id))
        });
        found.truncate(MAX_PANES);
        return found;
    };
    chosen
        .iter()
        .filter_map(|id| tasks.iter().find(|task| task.id == *id))
        .filter(|task| task.workspace.is_some())
        .collect()
}

/// How many columns the pane grid has. Auto fits as many as the width allows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneLayout {
    One,
    Two,
    Four,
    #[default]
    Auto,
}

pub const PANE_LAYOUTS: [PaneLayout; 4] = [
    PaneLayout::One,
    PaneLayout::Two,
    PaneLayout::Four,
    PaneLayout::Auto,
];

impl PaneLayout {
    /// The text kept in `view.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Four => "4",
            Self::Auto => "auto",
        }
    }

    fn picked_columns(self) -> Option<usize> {
        match self {
            Self::One => Some(1),
            Self::Two => Some(2),
            Self::Four => Some(4),
            Self::Auto => None,
        }
    }
}

pub fn parse_pane_layout(raw: Option<&str>) -> PaneLayout {
    PANE_LAYOUTS
        .into_iter()
        .find(|layout| Some(layout.as_str()) == raw)
        .unwrap_or_default()
}

const MIN_PANE_WIDTH: f32 = 320.0;
const PANE_GAP: f32 = 8.0;
// Before the grid is measured (and in tests), three columns are assumed.
const UNMEASURED_COLUMNS: usize = 3;

/// The most columns that keep every pane at its minimum width.
pub fn max_columns(width: Option<f32>) -> usize {
    match width {
        Some(width) if width > 0.0 => {
            (((width + PANE_GAP) / (MIN_PANE_WIDTH + PANE_GAP)).floor() as usize).max(1)
        }
        _ => UNMEASURED_COLUMNS,
    }
}

/// A layout that needs more room than there is gets the most that fits.
pub fn columns_for(layout: PaneLayout, count: usize, width: Option<f32>) -> usize {
    let fits = max_columns(width);
    let count = count.max(1);
    match layout.picked_columns() {
        // Balanced rows: five panes in a grid of four columns become 3 + 2, not 4 + 1.
        None => count.div_ceil(count.div_ceil(fits)).max(1),
        Some(picked) => picked.min(fits).min(count).max(1),
    }
}

/// The last pane takes the rest of its row, so the grid has no hole.
pub fn pane_span(index: usize, count: usize, columns: usize) -> usize {
    if index + 1 != count {
        return 1;
    }
    columns - (count - 1) % columns
}

/// The line under a pane's title: the state, and what the agent does now.
pub fn pane_state_text(state: TaskState, activity: Option<&str>) -> String {
    let label = state_label(state);
    match activity {
        Some(activity) if state == TaskState::Running && !activity.is_empty() => {
            format!("{label} · {activity}")
        }
        _ => label.to_string(),
    }
}

/// A line in a pane header about another terminal. Only one that needs the user is amber.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneHint {
    pub text: String,
    pub needs_you: bool,
}

/// A pane shows only Terminal 1 of a task. This names another agent terminal that needs the user
/// or, while Terminal 1 has no agent (an ownShell stage runs in Terminal 5), the one that works.
pub fn other_terminal_hint(
    task: &Task,
    lanes: &BTreeMap<String, TerminalLane>,
) -> Option<PaneHint> {
    // By terminal number: the map's text order must not pick the terminal.
    let mut states: Vec<(&str, Option<TaskState>)> = lanes
        .iter()
        .filter(|(name, _)| name.as_str() != "1")
        .map(|(name, lane)| (name.as_str(), terminal_state(task, lane)))
        .collect();
    states.sort_by_key(|(name, _)| (name.parse::<u64>().unwrap_or(u64::MAX), *name));
    let first = |wanted| {
        states
            .iter()
            .find(|(_, state)| *state == Some(wanted))
            .map(|(name, _)| *name)
    };
    if let Some(name) = first(TaskState::NeedsInput) {
        return Some(PaneHint {
            text: format!("Terminal {name} needs input"),
            needs_you: true,
        });
    }
    if let Some(name) = first(TaskState::Failed) {
        return Some(PaneHint {
            text: format!("Terminal {name} failed"),
            needs_you: true,
        });
    }
    if lanes.contains_key("1") {
        return None;
    }
    first(TaskState::Running).map(|name| PaneHint {
        text: format!("Terminal {name} is working"),
        needs_you: false,
    })
}

/// A pane header's New terminal button: the workspace's panes have it (its tooltip, and whether
/// it can be used); the Agents grid's do not.
pub fn new_terminal_button(in_grid: bool, has_agent: bool, full: bool) -> Option<(String, bool)> {
    (!in_grid).then(|| (new_terminal_tip(has_agent, full), !full))
}

/// The tooltip of a pane header's New terminal button.
pub fn new_terminal_tip(has_agent: bool, full: bool) -> String {
    if full {
        return format!("New terminal: at most {MAX_PANES} terminals");
    }
    let mut tip = "New terminal (Ctrl+Shift+`)".to_owned();
    if has_agent {
        tip.push_str(". Terminal 1 runs the agent.");
    }
    tip
}

#[cfg(test)]
#[path = "pane_rules_tests.rs"]
mod tests;
