//! `isTerminalState` (`packages/core/src/tasks/task-state.ts`).

use natamux_proto::tasks::task::TaskState;

/// Done and cancelled tasks move no more.
pub fn is_terminal_state(state: TaskState) -> bool {
    matches!(state, TaskState::Done | TaskState::Cancelled)
}
