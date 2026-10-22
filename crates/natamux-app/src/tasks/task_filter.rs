//! Which tasks the list shows, and in what order.

use natamux_proto::tasks::task::Task;

/// The tasks whose id or title has `query` in it, ignoring case, newest change first.
pub fn visible_tasks(tasks: &[Task], query: &str) -> Vec<Task> {
    let query = query.trim().to_lowercase();
    let mut shown: Vec<Task> = tasks
        .iter()
        .filter(|task| {
            query.is_empty()
                || task.id.to_lowercase().contains(&query)
                || task.title.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    // ISO 8601 times in UTC sort as text.
    shown.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    shown
}

/// The row to select after the list changed: the same task when it is still shown, else the
/// nearest row.
pub fn keep_selection(
    shown: &[Task],
    selected_id: Option<&str>,
    old_index: usize,
) -> Option<usize> {
    if shown.is_empty() {
        return None;
    }
    selected_id
        .and_then(|id| shown.iter().position(|task| task.id == id))
        .or(Some(old_index.min(shown.len() - 1)))
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::TaskState;

    use super::*;

    fn task(id: &str, title: &str, updated_at: &str) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            state: TaskState::Running,
            stage: "implement".into(),
            workspace: None,
            agent: None,
            parent_id: None,
            repo: None,
            updated_at: updated_at.into(),
            ..Task::default()
        }
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    fn sample() -> Vec<Task> {
        vec![
            task("T-1", "Fix the login page", "2026-10-20T10:00:00.000Z"),
            task("T-2", "Rust migration phase 1", "2026-10-22T09:00:00.000Z"),
            task("ABC-7", "Người dùng mới", "2026-10-21T08:00:00.000Z"),
        ]
    }

    #[test]
    fn newest_change_first() {
        assert_eq!(ids(&visible_tasks(&sample(), "")), ["T-2", "ABC-7", "T-1"]);
    }

    #[test]
    fn matches_id_or_title_ignoring_case() {
        assert_eq!(ids(&visible_tasks(&sample(), "rust")), ["T-2"]);
        assert_eq!(ids(&visible_tasks(&sample(), " abc-")), ["ABC-7"]);
        assert_eq!(ids(&visible_tasks(&sample(), "NGƯỜI")), ["ABC-7"]);
        assert!(visible_tasks(&sample(), "nothing like it").is_empty());
    }

    #[test]
    fn keeps_the_selected_task_or_the_nearest_row() {
        let shown = visible_tasks(&sample(), "");
        assert_eq!(keep_selection(&shown, Some("T-1"), 0), Some(2));
        assert_eq!(keep_selection(&shown, Some("T-9"), 7), Some(2));
        assert_eq!(keep_selection(&shown, None, 1), Some(1));
        assert_eq!(keep_selection(&[], Some("T-1"), 0), None);
    }
}
