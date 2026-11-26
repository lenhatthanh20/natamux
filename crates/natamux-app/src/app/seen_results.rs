//! Which background results the user has seen (T-0390). A turn that background work started, not
//! the user, leaves `backgroundResultAt` on its task; the task shows as unread until it is opened.
//! Kept like the seen states of `view_rules.rs`: a task seen for the first time is the baseline.

use std::borrow::Borrow;

use natamux_proto::tasks::task::Task;

use crate::app::view_rules::Entries;
use crate::tasks::task_changes::task_of;

// As many as the seen states.
const MAX_REMEMBERED: usize = 1000;

/// The task's last background result; empty when it has none.
fn result_of(task: &Task) -> &str {
    task.background_result_at.as_deref().unwrap_or_default()
}

/// Unread: the task has another result than the one recorded. A task never recorded counts as
/// read, so the first look marks nothing.
pub fn is_result_unseen(seen: &[(String, String)], task: &Task) -> bool {
    seen.iter()
        .find(|(id, _)| *id == task.id)
        .is_some_and(|(_, at)| at != result_of(task))
}

/// Records what the user saw of the task. A task recorded already keeps its entry unless
/// `overwrite` is set (the user looks at it now). The newest entry is the last.
pub fn note_result(seen: &mut Entries<String>, task: &Task, overwrite: bool) {
    let now = result_of(task);
    match seen.iter().position(|(id, _)| *id == task.id) {
        Some(index) => {
            let same = seen.get(index).is_some_and(|(_, at)| at == now);
            if overwrite && !same {
                seen.remove(index);
                seen.push((task.id.clone(), now.to_owned()));
            }
        }
        None => {
            seen.push((task.id.clone(), now.to_owned()));
            let extra = seen.len().saturating_sub(MAX_REMEMBERED);
            seen.drain(..extra);
        }
    }
}

/// The seen results after a new task list: each task with a workspace is recorded once, and the
/// open task is being looked at, so its result is seen whatever it is.
pub fn seen_results_after_list<T: Borrow<Task>>(
    current: &[(String, String)],
    tasks: &[T],
    open: Option<&str>,
) -> Entries<String> {
    let mut next = current.to_vec();
    for task in tasks.iter().map(task_of) {
        if task.workspace.is_some() {
            note_result(&mut next, task, open == Some(task.id.as_str()));
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, result: Option<&str>) -> Task {
        Task {
            id: id.into(),
            workspace: Some(format!("/work/{id}")),
            background_result_at: result.map(str::to_owned),
            ..Task::default()
        }
    }

    const AT: &str = "2026-11-26T09:00:00.000Z";
    const LATER: &str = "2026-11-26T10:00:00.000Z";

    #[test]
    fn the_first_look_is_the_baseline() {
        let tasks = [task("T-1", Some(AT)), task("T-2", None)];
        let seen = seen_results_after_list(&[], &tasks, None);
        assert!(tasks.iter().all(|task| !is_result_unseen(&seen, task)));
    }

    #[test]
    fn a_new_result_is_unread_until_the_task_is_open() {
        let seen = seen_results_after_list(&[], &[task("T-1", None)], None);
        let news = [task("T-1", Some(AT))];

        let kept = seen_results_after_list(&seen, &news, None);
        assert!(is_result_unseen(&kept, &news[0]));

        let open = seen_results_after_list(&kept, &news, Some("T-1"));
        assert!(!is_result_unseen(&open, &news[0]));
        let newer = task("T-1", Some(LATER));
        assert!(is_result_unseen(&open, &newer));
    }

    #[test]
    fn a_task_without_a_workspace_is_not_recorded() {
        let mut backlog = task("T-1", None);
        backlog.workspace = None;
        assert!(seen_results_after_list(&[], &[backlog], None).is_empty());
    }

    #[test]
    fn keeps_the_newest_entries() {
        let mut seen: Entries<String> = Vec::new();
        for index in 0..=MAX_REMEMBERED {
            note_result(&mut seen, &task(&format!("T-{index}"), None), false);
        }
        assert_eq!(seen.len(), MAX_REMEMBERED);
        assert_eq!(seen.first().map(|(id, _)| id.as_str()), Some("T-1"));
    }
}
