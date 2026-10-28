//! The task in the body of a `task.created`, `task.updated` or `task.changed` message. The daemon
//! writes it with the same `Task` serializer as `GET /tasks` (`DaemonMessage::to_json`), so the
//! feed passes it on and reads no list.

use std::cmp::Ordering;

use natamux_proto::tasks::task::Task;
use serde::Deserialize;

#[derive(Deserialize)]
struct WithTask {
    kind: String,
    task: Task,
}

#[derive(Deserialize)]
struct WithMoved {
    kind: String,
    #[serde(default)]
    moved: bool,
}

/// The task a message carries. None for another kind, or for a body that is not a whole task:
/// then the list is read instead.
pub fn task_of(text: &str) -> Option<Task> {
    let message: WithTask = serde_json::from_str(text).ok()?;
    match message.kind.as_str() {
        "task.created" | "task.updated" | "task.changed" => Some(message.task),
        _ => None,
    }
}

/// A `task.updated` that moved the task to another step.
pub fn has_moved(text: &str) -> bool {
    let Ok(message) = serde_json::from_str::<WithMoved>(text) else {
        return false;
    };
    message.kind == "task.updated" && message.moved
}

/// Puts one task into a list in the order of `GET /tasks`: newest `updatedAt` first, then id
/// (`Store::list_tasks`). A task older than the one held is left out, as the app's `upsert_task`
/// does: a list read since is newer.
pub fn merge_task(tasks: &mut Vec<Task>, task: Task) {
    if tasks
        .iter()
        .any(|held| held.id == task.id && held.updated_at > task.updated_at)
    {
        return;
    }
    tasks.retain(|held| held.id != task.id);
    let at = tasks
        .iter()
        .position(|held| newest_first(&task, held) == Ordering::Less)
        .unwrap_or(tasks.len());
    tasks.insert(at, task);
}

fn newest_first(a: &Task, b: &Task) -> Ordering {
    b.updated_at
        .cmp(&a.updated_at)
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::{TaskList, TaskState};
    use serde_json::{Map, json};

    use super::*;

    fn task(id: &str, updated_at: &str) -> Task {
        Task {
            id: id.into(),
            title: format!("Title of {id}"),
            updated_at: updated_at.into(),
            ..Task::default()
        }
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    #[test]
    fn the_body_decodes_to_the_task_the_list_gives() {
        let mut lanes = Map::new();
        lanes.insert("implement".into(), json!("running"));
        let sent = Task {
            state: TaskState::NeedsInput,
            labels: vec!["perf".into()],
            branch: Some("natamux/t-1".into()),
            lanes,
            without_agent: Some(true),
            created_at: "2026-10-29T09:00:00.000Z".into(),
            ..task("T-1", "2026-10-29T10:00:00.000Z")
        };
        // As the daemon writes both: the message and the list route.
        let value = serde_json::to_value(&sent).unwrap();
        let message = json!({ "kind": "task.updated", "task": value, "accepted": true });
        let list: TaskList = serde_json::from_value(json!({ "tasks": [value] })).unwrap();
        assert_eq!(task_of(&message.to_string()), Some(sent));
        assert_eq!(task_of(&message.to_string()).as_ref(), list.tasks.first());
    }

    #[test]
    fn only_a_whole_task_of_a_task_message_is_taken() {
        let value = serde_json::to_value(task("T-1", "a")).unwrap();
        for kind in ["task.created", "task.updated", "task.changed"] {
            let message = json!({ "kind": kind, "task": value });
            assert!(task_of(&message.to_string()).is_some(), "{kind}");
        }
        let other = json!({ "kind": "shells.changed", "task": value });
        assert_eq!(task_of(&other.to_string()), None);
        assert_eq!(task_of(r#"{"kind":"task.deleted","taskId":"T-1"}"#), None);
        let no_title = r#"{"kind":"task.changed","task":{"id":"T-1"}}"#;
        assert_eq!(task_of(no_title), None);
        assert_eq!(task_of("not json"), None);
    }

    #[test]
    fn only_a_task_updated_with_moved_true_has_moved() {
        assert!(has_moved(r#"{"kind":"task.updated","moved":true}"#));
        assert!(!has_moved(r#"{"kind":"task.updated","moved":false}"#));
        assert!(!has_moved(r#"{"kind":"task.updated"}"#));
        assert!(!has_moved(r#"{"kind":"task.changed","moved":true}"#));
    }

    #[test]
    fn a_merged_task_takes_the_place_of_the_list_order_and_an_older_one_is_left_out() {
        let mut tasks = vec![task("T-2", "c"), task("T-1", "a"), task("T-3", "a")];
        merge_task(&mut tasks, task("T-1", "d"));
        assert_eq!(ids(&tasks), ["T-1", "T-2", "T-3"]);
        merge_task(&mut tasks, task("T-0", "a"));
        assert_eq!(ids(&tasks), ["T-1", "T-2", "T-0", "T-3"]);
        let older = Task {
            title: "Older".into(),
            ..task("T-2", "b")
        };
        merge_task(&mut tasks, older);
        assert_eq!(tasks[1], task("T-2", "c"));
        // The same time counts as newer: an edit that kept the time still shows.
        let same_time = Task {
            title: "Edited".into(),
            ..task("T-2", "c")
        };
        merge_task(&mut tasks, same_time.clone());
        assert_eq!(tasks[1], same_time);
    }
}
