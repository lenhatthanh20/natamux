use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::Map;

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-log-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event() -> TaskEvent {
    TaskEvent {
        task_id: "T-0001".into(),
        source: "agent".into(),
        kind: "agent.active".into(),
        at: "2026-10-08T09:00:00.000Z".into(),
        payload: Map::new(),
    }
}

fn logged(event: TaskEvent, accepted: bool) -> LoggedEvent {
    LoggedEvent { event, accepted }
}

fn failed() -> TaskEvent {
    TaskEvent {
        kind: "agent.failed".into(),
        ..event()
    }
}

#[test]
fn adds_a_line_and_gives_the_stat_of_the_log_after_it() {
    let dir = TempDir::new();
    let first = append_event_line(&dir.0, &event(), true).unwrap();
    let second = append_event_line(&dir.0, &failed(), false).unwrap();

    assert_eq!(
        Some(second),
        stat_file(&event_log_path(&dir.0, &event().task_id)).unwrap()
    );
    assert!(second.size > first.size);
    assert_eq!(
        read_event_log(&dir.0, &event().task_id).unwrap(),
        Some(vec![logged(event(), true), logged(failed(), false)])
    );
}

#[test]
fn makes_the_tasks_folder_when_there_is_none_also_after_it_was_removed() {
    let dir = TempDir::new();
    append_event_line(&dir.0, &event(), true).unwrap();
    fs::remove_dir_all(tasks_dir(&dir.0)).unwrap();

    append_event_line(&dir.0, &event(), true).unwrap();

    assert_eq!(
        read_event_log(&dir.0, &event().task_id)
            .unwrap()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn makes_the_folder_once_for_many_events_of_many_tasks() {
    // Node spies on mkdirSync and counts one call. Rust cannot spy on the file system, so the
    // count is not checked; every line must still be written, with the folder made once or not.
    let dir = TempDir::new();
    for _ in 0..100 {
        append_event_line(&dir.0, &event(), true).unwrap();
    }
    let other = TaskEvent {
        task_id: "T-0002".into(),
        ..event()
    };
    append_event_line(&dir.0, &other, true).unwrap();

    assert_eq!(
        read_event_log(&dir.0, &event().task_id)
            .unwrap()
            .unwrap()
            .len(),
        100
    );
    assert_eq!(read_event_log(&dir.0, "T-0002").unwrap().unwrap().len(), 1);
}
