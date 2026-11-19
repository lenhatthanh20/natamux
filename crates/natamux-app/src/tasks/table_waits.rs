//! What a task waits for, from lookups made once per task list: a row of the table or a card of
//! the board then asks with a few lookups, where `waits_for` reads the whole list for each row.
//! A child of `table_rules.rs`.

use std::borrow::Borrow;
use std::collections::HashMap;

use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_changes::task_of;

/// The open subtasks of each parent, by parent id, in the order of the list.
pub type OpenSubtasks = HashMap<String, Vec<String>>;

/// One pass over the list. A parent with no open subtask has no entry.
pub fn open_subtasks<T: Borrow<Task>>(tasks: &[T]) -> OpenSubtasks {
    let mut open = OpenSubtasks::new();
    for task in tasks.iter().map(task_of) {
        let Some(parent) = task.parent_id.as_deref() else {
            continue;
        };
        if task.state.is_finished() {
            continue;
        }
        match open.get_mut(parent) {
            Some(children) => children.push(task.id.clone()),
            None => {
                open.insert(parent.to_owned(), vec![task.id.clone()]);
            }
        }
    }
    open
}

/// `waits_for` of a list whose lookups are made already: `state_of` gives the state of the first
/// task with an id, `open` is `open_subtasks` of the list.
pub fn waits_for_in(
    task: &Task,
    state_of: impl Fn(&str) -> Option<TaskState>,
    open: &OpenSubtasks,
) -> Vec<String> {
    let mut waits: Vec<String> = task
        .depends_on
        .iter()
        .filter(|id| state_of(id).is_some_and(|state| state != TaskState::Done))
        .cloned()
        .collect();
    if matches!(task.state, TaskState::Ready | TaskState::Draft) {
        waits.extend(open.get(&task.id).into_iter().flatten().cloned());
    }
    waits
}

#[cfg(test)]
mod tests {
    use super::super::waits_for;
    use super::*;

    fn task(id: &str, state: TaskState, parent: Option<&str>, depends_on: &[&str]) -> Task {
        Task {
            id: id.into(),
            state,
            parent_id: parent.map(str::to_owned),
            depends_on: depends_on.iter().map(|id| (*id).to_owned()).collect(),
            ..Task::default()
        }
    }

    #[test]
    fn the_lookups_give_what_a_read_of_the_whole_list_gives_for_every_task() {
        let tasks = vec![
            task("T-1", TaskState::Ready, None, &["T-4", "T-9", "T-5"]),
            task("T-2", TaskState::Running, Some("T-1"), &[]),
            task("T-3", TaskState::Cancelled, Some("T-1"), &[]),
            task("T-4", TaskState::Done, None, &[]),
            task("T-5", TaskState::Review, None, &["T-1"]),
            task("T-6", TaskState::Draft, Some("T-1"), &["T-2"]),
            // A parent that runs waits for no subtask, and a parent that is not in the list.
            task("T-7", TaskState::Running, None, &[]),
            task("T-8", TaskState::Failed, Some("T-7"), &[]),
            task("T-10", TaskState::Ready, Some("T-0"), &[]),
            // The same id twice: the first one counts for a dependency.
            task("T-4", TaskState::Running, None, &[]),
        ];
        let open = open_subtasks(&tasks);
        let state_of = |id: &str| tasks.iter().find(|task| task.id == id).map(|t| t.state);
        for task in &tasks {
            let quick = waits_for_in(task, state_of, &open);
            assert_eq!(quick, waits_for(task, &tasks), "{}", task.id);
        }
        assert_eq!(
            waits_for_in(&tasks[0], state_of, &open),
            ["T-5", "T-2", "T-6"]
        );
        assert_eq!(open.get("T-1").map(Vec::len), Some(2));
        assert!(!open.contains_key("T-4"));
    }
}
