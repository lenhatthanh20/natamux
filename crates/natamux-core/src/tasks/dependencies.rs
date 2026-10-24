//! `waitingOn`, `openSubtasks`, `waitsFor` and `makesCycle` (`packages/core/src/tasks/dependencies.ts`).

use std::collections::{HashMap, HashSet};

use natamux_proto::tasks::task::{Task, TaskState};

/// The tasks this task waits for: those it depends on that are not done. A task that does not
/// exist (any more) is not waited for.
pub fn waiting_on(task: &Task, all: &[Task]) -> Vec<String> {
    task.depends_on
        .iter()
        .filter(|id| {
            // The last task with that id wins, as in a JS Map.
            let state = all.iter().rfind(|other| &other.id == *id).map(|t| t.state);
            state.is_some_and(|state| state != TaskState::Done)
        })
        .cloned()
        .collect()
}

/// The subtasks of this task that are not finished. Its next stage waits for them, as their work
/// is merged into its branch.
pub fn open_subtasks(task: &Task, all: &[Task]) -> Vec<String> {
    all.iter()
        .filter(|other| {
            other.parent_id.as_deref() == Some(task.id.as_str())
                && other.state != TaskState::Done
                && other.state != TaskState::Cancelled
        })
        .map(|other| other.id.clone())
        .collect()
}

/// What people see the task wait for: its dependencies, and its open subtasks while a stage has
/// not started. A stage that already runs is not stopped by a new subtask.
pub fn waits_for(task: &Task, all: &[Task]) -> Vec<String> {
    let not_started = matches!(task.state, TaskState::Ready | TaskState::Draft);
    let mut waits = waiting_on(task, all);
    if not_started {
        waits.extend(open_subtasks(task, all));
    }
    waits
}

/// True when `task_id` depending on `depends_on` would make a loop: one of them already depends,
/// directly or through others, on `task_id`. `tasks`: each task's id and dependencies.
pub fn makes_cycle(task_id: &str, depends_on: &[String], tasks: &[(String, Vec<String>)]) -> bool {
    let edges: HashMap<&str, &Vec<String>> =
        tasks.iter().map(|(id, deps)| (id.as_str(), deps)).collect();
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = depends_on.iter().map(String::as_str).collect();
    while let Some(id) = stack.pop() {
        if id == task_id {
            return true;
        }
        if !seen.insert(id) {
            continue;
        }
        if let Some(next) = edges.get(id) {
            stack.extend(next.iter().map(String::as_str));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: TaskState, parent: Option<&str>, depends_on: &[&str]) -> Task {
        Task {
            id: id.into(),
            state,
            parent_id: parent.map(Into::into),
            depends_on: depends_on.iter().map(|d| d.to_string()).collect(),
            ..Task::default()
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn lists_the_dependencies_that_are_not_done() {
        let all = [
            task("T-0001", TaskState::Done, None, &[]),
            task("T-0002", TaskState::Running, None, &[]),
            task("T-0003", TaskState::Cancelled, None, &[]),
        ];
        let waiting = task(
            "T-0010",
            TaskState::Ready,
            None,
            &["T-0001", "T-0002", "T-0003", "T-0099"],
        );
        assert_eq!(waiting_on(&waiting, &all), ids(&["T-0002", "T-0003"]));
        assert!(waiting_on(&task("T-0011", TaskState::Ready, None, &[]), &all).is_empty());
    }

    fn family() -> Vec<Task> {
        vec![
            task("T-0001", TaskState::Ready, None, &[]),
            task("T-0002", TaskState::Running, Some("T-0001"), &[]),
            task("T-0003", TaskState::Done, Some("T-0001"), &[]),
            task("T-0004", TaskState::Cancelled, Some("T-0001"), &[]),
            task("T-0005", TaskState::Draft, Some("T-0001"), &[]),
            task("T-0006", TaskState::Draft, Some("T-0009"), &[]),
        ]
    }

    #[test]
    fn lists_the_subtasks_that_are_not_done_or_cancelled() {
        let all = family();
        assert_eq!(open_subtasks(&all[0], &all), ids(&["T-0002", "T-0005"]));
        assert!(open_subtasks(&all[1], &all).is_empty());
    }

    #[test]
    fn adds_open_subtasks_to_the_dependencies_only_while_a_stage_has_not_started() {
        let mut all = family();
        all.push(task("T-0007", TaskState::Running, None, &[]));
        let ready = task("T-0001", TaskState::Ready, None, &["T-0007"]);
        assert_eq!(
            waits_for(&ready, &all),
            ids(&["T-0007", "T-0002", "T-0005"])
        );
        let running = Task {
            state: TaskState::Running,
            ..ready
        };
        assert_eq!(waits_for(&running, &all), ids(&["T-0007"]));
    }

    fn tasks(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        list.iter()
            .map(|(id, deps)| (id.to_string(), deps.iter().map(|d| d.to_string()).collect()))
            .collect()
    }

    #[test]
    fn finds_a_loop_in_a_chain_of_three() {
        let all = tasks(&[("T-1", &[]), ("T-2", &["T-1"]), ("T-3", &["T-2"])]);
        assert!(makes_cycle("T-1", &["T-3".into()], &all));
        assert!(!makes_cycle("T-3", &["T-1".into()], &all));
        assert!(makes_cycle("T-1", &["T-1".into()], &all));
    }

    fn chain() -> Vec<(String, Vec<String>)> {
        tasks(&[
            ("T-0001", &["T-0002"]),
            ("T-0002", &["T-0003"]),
            ("T-0003", &[]),
            ("T-0004", &[]),
        ])
    }

    #[test]
    fn finds_a_loop_through_other_tasks() {
        assert!(makes_cycle("T-0003", &["T-0001".into()], &chain()));
        assert!(makes_cycle("T-0001", &["T-0001".into()], &chain()));
    }

    #[test]
    fn allows_a_chain_without_a_loop() {
        assert!(!makes_cycle(
            "T-0004",
            &["T-0001".into(), "T-0003".into()],
            &chain()
        ));
        assert!(!makes_cycle("T-0003", &[], &chain()));
    }
}
