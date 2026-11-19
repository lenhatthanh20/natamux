//! The search of the task list (`filterTasks` in `packages/web/src/lib/tasks.ts`). A child of
//! `task_attention.rs`.

use std::borrow::Borrow;

use natamux_proto::tasks::task::{Task, TaskState};

use crate::tasks::task_changes::task_of;

/// The tasks in this state (all when None) whose id, title, agent, branch or labels have the
/// search text in them, ignoring case. Only the tests still want the clones.
#[cfg(test)]
pub fn filter_tasks(tasks: &[Task], state_filter: Option<TaskState>, search: &str) -> Vec<Task> {
    filter_task_refs(tasks, state_filter, search)
        .into_iter()
        .cloned()
        .collect()
}

/// `filter_tasks` without cloning: the same tasks in the same order, borrowed. The list holds
/// its tasks by value or shared, and what comes back is what it holds.
pub fn filter_task_refs<'a, T: Borrow<Task>>(
    tasks: &'a [T],
    state_filter: Option<TaskState>,
    search: &str,
) -> Vec<&'a T> {
    let needle = search.trim().to_lowercase();
    tasks
        .iter()
        .filter(|held| {
            let task = task_of(*held);
            if state_filter.is_some_and(|state| task.state != state) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            let mut parts = vec![task.id.as_str(), task.title.as_str()];
            parts.extend(task.agent.as_deref());
            parts.extend(task.branch.as_deref());
            parts.extend(task.labels.iter().map(String::as_str));
            parts.join(" ").to_lowercase().contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::*;
    use crate::tasks::task_sort::sort_task_refs;

    #[test]
    fn the_search_and_the_sort_of_shared_tasks_give_back_the_tasks_held_not_copies() {
        let task = |id: &str, state| {
            Arc::new(Task {
                id: id.into(),
                title: format!("Fix {id}"),
                state,
                ..Task::default()
            })
        };
        let held = [
            task("T-0001", TaskState::Done),
            task("T-0002", TaskState::Running),
            task("T-0013", TaskState::Review),
        ];
        let found = filter_task_refs(&held, None, "t-000");
        let sorted = sort_task_refs(found, None, &BTreeMap::new(), &BTreeMap::new());
        let ids: Vec<&str> = sorted.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, ["T-0002", "T-0001"]);
        assert!(Arc::ptr_eq(sorted[0], &held[1]));
        assert!(Arc::ptr_eq(sorted[1], &held[0]));
    }
}
