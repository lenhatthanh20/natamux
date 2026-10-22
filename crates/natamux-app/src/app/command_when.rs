//! When a command applies (`CommandState` and the `when` of `packages/web/src/lib/commands`).

use natamux_proto::tasks::task::TaskState;

use crate::tasks::task_rules::actions_for;

// `when` gates the command palette of a later screen.
#[cfg_attr(not(test), allow(dead_code))]
/// What the view knows when a key comes (`CommandState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandState {
    /// A task fills the main area (the terminal pane until screen 2).
    pub task_open: bool,
    pub has_selection: bool,
    pub selected_state: Option<TaskState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// The task list is on screen, not an open task.
    ListShown,
    HasSelection,
    /// The selected task's state offers this event without asking first.
    TaskAction(&'static str),
    /// A workspace is open and its task has a stage at work or waiting (`stage.*`).
    StageOpen,
}

impl When {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn holds(self, state: &CommandState) -> bool {
        match self {
            When::Always => true,
            When::ListShown => !state.task_open,
            When::HasSelection => state.has_selection,
            When::TaskAction(event) => state.selected_state.is_some_and(|selected| {
                actions_for(selected)
                    .iter()
                    .any(|action| action.event_type == event && !action.confirm)
            }),
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
