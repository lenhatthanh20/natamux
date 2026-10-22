//! Live changes to the task list (`sortByRecent`, `upsertTask`, `applyTaskChanges` in `packages/
//! web/src/lib/tasks.ts`).

// Ported with their tests (spec D11); the feed replaces the whole list until events carry tasks.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::tasks::task::Task;

fn by_recent(a: &Task, b: &Task) -> std::cmp::Ordering {
    b.updated_at
        .cmp(&a.updated_at)
        .then_with(|| a.id.cmp(&b.id))
}

pub fn sort_by_recent(tasks: &[Task]) -> Vec<Task> {
    let mut sorted = tasks.to_vec();
    sorted.sort_by(by_recent);
    sorted
}

/// A late answer must not replace what the live feed already brought: keep the task that was
/// updated last. The same time counts as newer, so the answer of an edit still shows. The list
/// is newest first, so the task goes into its place without a full sort.
pub fn upsert_task(tasks: &[Task], task: Task) -> Vec<Task> {
    if tasks
        .iter()
        .any(|known| known.id == task.id && known.updated_at > task.updated_at)
    {
        return tasks.to_vec();
    }
    let mut next: Vec<Task> = tasks.iter().filter(|t| t.id != task.id).cloned().collect();
    let at = next
        .iter()
        .position(|existing| by_recent(&task, existing).is_lt())
        .unwrap_or(next.len());
    next.insert(at, task);
    next
}

/// A change from the live feed: a task that changed, or the id of one that was deleted.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskChange {
    Changed(Box<Task>),
    Deleted(String),
}

/// The changes in the order they came.
pub fn apply_task_changes(tasks: &[Task], changes: Vec<TaskChange>) -> Vec<Task> {
    changes
        .into_iter()
        .fold(tasks.to_vec(), |list, change| match change {
            TaskChange::Deleted(id) => list.into_iter().filter(|task| task.id != id).collect(),
            TaskChange::Changed(task) => upsert_task(&list, *task),
        })
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::TaskState;

    use super::*;

    fn make(overrides: impl FnOnce(&mut Task)) -> Task {
        let mut task = Task {
            id: "T-0001".into(),
            title: "Example".into(),
            state: TaskState::Draft,
            stage: "implement".into(),
            created_at: "2026-10-08T09:00:00.000Z".into(),
            updated_at: "2026-10-08T09:00:00.000Z".into(),
            ..Task::default()
        };
        overrides(&mut task);
        task
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    fn with(id: &str, state: TaskState, updated_at: &str) -> Task {
        make(|task| {
            task.id = id.into();
            task.state = state;
            task.updated_at = updated_at.into();
        })
    }

    #[test]
    fn puts_the_most_recently_updated_task_first() {
        let older = with("T-0001", TaskState::Draft, "2026-10-08T09:00:00.000Z");
        let newer = with("T-0002", TaskState::Draft, "2026-10-08T10:00:00.000Z");
        assert_eq!(ids(&sort_by_recent(&[older, newer])), ["T-0002", "T-0001"]);
    }

    #[test]
    fn replaces_a_task_that_is_already_in_the_list() {
        let before = make(|t| t.title = "Old".into());
        let result = upsert_task(&[before], make(|t| t.title = "New".into()));
        assert_eq!(
            result.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["New"]
        );
    }

    #[test]
    fn keeps_the_task_that_was_updated_last() {
        let live = make(|t| {
            t.title = "Live".into();
            t.updated_at = "2026-10-08T10:00:00.000Z".into();
        });
        let late = make(|t| {
            t.title = "Late answer".into();
            t.updated_at = "2026-10-08T09:00:00.000Z".into();
        });
        let titles = |list: Vec<Task>| list.into_iter().map(|t| t.title).collect::<Vec<_>>();
        assert_eq!(
            titles(upsert_task(std::slice::from_ref(&live), late.clone())),
            ["Live"]
        );
        assert_eq!(titles(upsert_task(&[late], live)), ["Live"]);
    }

    #[test]
    fn adds_a_task_that_is_new() {
        let result = upsert_task(&[make(|_| {})], make(|t| t.id = "T-0002".into()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn puts_a_changed_task_where_a_full_sort_would_and_keeps_the_other_tasks_as_they_are() {
        let at = |minute: u32| format!("2026-10-08T09:0{minute}:00.000Z");
        let task = |id: &str, minute: u32| with(id, TaskState::Draft, &at(minute));
        let tasks = sort_by_recent(&[
            task("T-0001", 1),
            task("T-0002", 2),
            task("T-0003", 3),
            task("T-0004", 3),
        ]);
        let mut same_time = task("T-0004", 3);
        same_time.title = "Same time".into();
        for change in [
            task("T-0001", 9),
            same_time,
            task("T-0005", 2),
            task("T-0000", 0),
        ] {
            let result = upsert_task(&tasks, change.clone());
            let mut rest: Vec<Task> = tasks
                .iter()
                .filter(|t| t.id != change.id)
                .cloned()
                .collect();
            rest.push(change);
            assert_eq!(result, sort_by_recent(&rest));
            assert_eq!(result.iter().find(|t| t.id == "T-0002"), tasks.get(2));
        }
    }

    #[test]
    fn applies_live_changes_in_the_order_they_came_deletions_too() {
        let first = with("T-0001", TaskState::Draft, "2026-10-08T09:00:00.000Z");
        let changed = Task {
            title: "Changed".into(),
            updated_at: "2026-10-08T10:00:00.000Z".into(),
            ..first.clone()
        };
        let added = with("T-0002", TaskState::Draft, "2026-10-08T11:00:00.000Z");
        let result = apply_task_changes(
            std::slice::from_ref(&first),
            vec![
                TaskChange::Changed(Box::new(changed.clone())),
                TaskChange::Changed(Box::new(added)),
                TaskChange::Deleted("T-0001".into()),
            ],
        );
        assert_eq!(ids(&result), ["T-0002"]);
        let readded = apply_task_changes(
            &[first],
            vec![
                TaskChange::Deleted("T-0001".into()),
                TaskChange::Changed(Box::new(changed)),
            ],
        );
        assert_eq!(readded[0].title, "Changed");
    }
}
