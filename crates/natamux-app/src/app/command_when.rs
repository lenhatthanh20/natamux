//! When a command applies (`CommandState` and the `when` of `packages/web/src/lib/commands`).

use natamux_proto::tasks::task::TaskState;

use crate::tasks::task_rules::actions_for;

/// What the view knows when a key comes or the palette opens (`CommandState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandState {
    /// A task fills the main area.
    pub task_open: bool,
    /// The Agents grid fills the main area.
    pub agents_open: bool,
    pub has_selection: bool,
    pub selected_state: Option<TaskState>,
    /// The open task has a workspace (its tabs, files and diff).
    pub workspace_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// The task list is on screen, not an open task nor the Agents grid.
    ListShown,
    HasSelection,
    /// The selected task's state offers this event without asking first.
    TaskAction(&'static str),
    /// A workspace is open and its task has a stage at work or waiting (`stage.*`).
    StageOpen,
    /// A workspace is open (`inWorkspace` of the web).
    InWorkspace,
}

impl When {
    pub fn holds(self, state: &CommandState) -> bool {
        match self {
            When::Always => true,
            When::ListShown => !state.task_open && !state.agents_open,
            When::HasSelection => state.has_selection,
            When::TaskAction(event) => state.selected_state.is_some_and(|selected| {
                actions_for(selected)
                    .iter()
                    .any(|action| action.event_type == event && !action.confirm)
            }),
            When::InWorkspace => state.workspace_open,
            When::StageOpen => {
                state.task_open
                    && matches!(
                        state.selected_state,
                        Some(
                            TaskState::Ready
                                | TaskState::Running
                                | TaskState::NeedsInput
                                | TaskState::Review
                        )
                    )
            }
        }
    }
}
