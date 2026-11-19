use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::Task;
use serde_json::json;

use super::*;
use crate::event_log::LoggedEvent;
use crate::task_index::TaskIndex;

fn make_task(id: &str) -> Task {
    Task {
        id: id.into(),
        title: "Example".into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn sync() -> TaskSync {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let folder: PathBuf = std::env::temp_dir().join(format!(
        "natamux-reads-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let index = TaskIndex::open(Path::new(":memory:")).unwrap();
    TaskSync::new(&folder, index, Arc::new(|| 1_000), Box::new(|_| {}))
}

fn noted(id: &str, text: &str) -> LoggedEvent {
    LoggedEvent {
        event: TaskEvent {
            task_id: id.into(),
            source: "system".into(),
            kind: "task.created".into(),
            at: "2026-10-08T09:00:00.000Z".into(),
            payload: json!({ "text": text }).as_object().cloned().unwrap(),
        },
        accepted: true,
    }
}

fn texts(events: &[StoredEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|stored| stored.event.payload.get("text")?.as_str())
        .collect()
}

#[test]
fn the_last_events_are_the_end_of_the_event_list_of_the_task() {
    let mut sync = sync();
    let own: Vec<LoggedEvent> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(|text| noted("T-0001", text))
        .collect();
    sync.store(make_task("T-0001"), &own[..3], true).unwrap();
    sync.store(make_task("T-0002"), &[noted("T-0002", "x")], true)
        .unwrap();
    sync.store(make_task("T-0001"), &own[3..], true).unwrap();
    let all = sync.list_events("T-0001").unwrap();
    assert_eq!(texts(&all), ["a", "b", "c", "d", "e"]);

    let read = sync.last_events_later("T-0001");

    assert_eq!(read.read(2).unwrap(), all[3..]);
    assert_eq!(read.read(4).unwrap(), all[1..]);
    // A limit past the start gives the whole list, which tells the reader it has all of it.
    assert_eq!(read.read(64).unwrap(), all);
    assert!(read.read(0).unwrap().is_empty());
    assert!(sync.last_events_later("T-0099").read(8).unwrap().is_empty());
}

#[test]
fn a_read_of_the_last_events_waits_after_the_lock_for_a_write_decided_before_it() {
    let sync = Arc::new(std::sync::Mutex::new(sync()));
    let (writes, read) = {
        let mut sync = sync.lock().unwrap();
        sync.defer_writes();
        sync.store(make_task("T-0001"), &[noted("T-0001", "a")], true)
            .unwrap();
        let writes = sync.take_writes().unwrap();
        (writes, sync.last_events_later("T-0001"))
    };
    let reader = std::thread::spawn(move || read.read(8).unwrap());

    std::thread::sleep(Duration::from_millis(50));
    assert!(!reader.is_finished());
    assert!(sync.try_lock().is_ok());
    assert!(writes.run().result.is_ok());

    assert_eq!(texts(&reader.join().unwrap()), ["a"]);
}

#[test]
fn a_read_does_not_wait_for_a_write_decided_after_it_was_made() {
    let mut sync = sync();
    sync.store(make_task("T-0001"), &[noted("T-0001", "a")], true)
        .unwrap();
    let last = sync.last_events_later("T-0001");
    let starts = sync.stage_starts_later("T-0001");
    // A later change of the task, whose write does not run before the reads.
    sync.defer_writes();
    sync.store(make_task("T-0001"), &[noted("T-0001", "b")], true)
        .unwrap();
    let writes = sync.take_writes().unwrap();

    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let read = (last.read(8).unwrap(), starts.read().unwrap());
        done.send(read).ok();
    });
    let read = result.recv_timeout(Duration::from_secs(10));

    let (last, starts) = read.expect("the reads waited for a later write");
    assert_eq!(texts(&last), ["a"]);
    assert!(starts.is_empty());
    assert!(writes.run().result.is_ok());
}
