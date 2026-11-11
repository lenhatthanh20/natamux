use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::Task;
use serde_json::json;

use super::*;
use crate::event_log::LoggedEvent;
use crate::task_index::{TaskChange, TaskFiles};

fn rows(id: &str) -> ChangeRows {
    let task = Task {
        id: id.into(),
        title: "Example".into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    };
    let events = [LoggedEvent {
        event: TaskEvent {
            task_id: id.into(),
            source: "system".into(),
            kind: "task.created".into(),
            at: "2026-10-08T09:00:00.000Z".into(),
            payload: serde_json::Map::new(),
        },
        accepted: true,
    }];
    let change = TaskChange {
        task: Some(&task),
        events: &events,
        files: TaskFiles::default(),
    };
    ChangeRows::new(id, &change)
}

fn memory_index() -> Mutex<TaskIndex> {
    Mutex::new(TaskIndex::open(Path::new(":memory:")).unwrap())
}

fn three() -> Vec<(u64, ChangeRows)> {
    vec![
        (0, rows("T-0001")),
        (1, rows("T-0002")),
        (2, rows("T-0003")),
    ]
}

fn indexed_ids(index: &TaskIndex) -> Vec<String> {
    let tasks = index.list_tasks(None).unwrap();
    tasks.into_iter().map(|task| task.id).collect()
}

#[test]
fn one_transaction_commits_the_rows_and_events_of_every_task_in_a_batch() {
    let index = memory_index();

    let results = commit_batch(&index, three());

    assert!(results.iter().all(|(_, result)| result.is_ok()));
    let index = index.lock().unwrap();
    assert_eq!(indexed_ids(&index), ["T-0001", "T-0002", "T-0003"]);
    for id in ["T-0001", "T-0002", "T-0003"] {
        assert_eq!(index.count_events(id).unwrap(), 1);
    }
}

#[test]
fn a_bad_change_in_a_batch_fails_only_its_own_writer() {
    let index = memory_index();
    let refuse = "create trigger refuse before insert on events when new.task_id = 'T-0002'
        begin select raise(abort, 'refused'); end;";
    index
        .lock()
        .unwrap()
        .connection()
        .execute_batch(refuse)
        .unwrap();

    let results = commit_batch(&index, three());

    let failed: Vec<u64> = results
        .iter()
        .filter(|(_, result)| result.is_err())
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(failed, [1]);
    let index = index.lock().unwrap();
    // The row of the refused change is not kept without its events.
    assert_eq!(indexed_ids(&index), ["T-0001", "T-0003"]);
    assert_eq!(index.count_events("T-0002").unwrap(), 0);
}

#[test]
fn every_writer_returns_once_its_own_rows_are_committed() {
    let index = Arc::new(memory_index());
    let commits = Arc::new(Commits::default());

    let writers: Vec<_> = (1..=16)
        .map(|number| {
            let (index, commits) = (Arc::clone(&index), Arc::clone(&commits));
            std::thread::spawn(move || {
                let id = format!("T-{number:04}");
                commits.commit(&index, rows(&id)).unwrap();
                assert_eq!(index.lock().unwrap().count_events(&id).unwrap(), 1);
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }

    assert_eq!(indexed_ids(&index.lock().unwrap()).len(), 16);
}

fn wait_until(done: impl Fn() -> bool) {
    while !done() {
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn writers_that_come_while_a_commit_runs_commit_together() {
    let index = Arc::new(memory_index());
    let commits = Arc::new(Commits::default());
    let held = index.lock().unwrap();
    let spawn = |number: u32| {
        let (index, commits) = (Arc::clone(&index), Arc::clone(&commits));
        std::thread::spawn(move || commits.commit(&index, rows(&format!("T-{number:04}"))))
    };

    // The first writer commits alone and waits for the index; the next two queue behind it.
    let first = spawn(1);
    wait_until(|| commits.state().running);
    let (second, third) = (spawn(2), spawn(3));
    wait_until(|| commits.state().queue.len() == 2);
    drop(held);

    for writer in [first, second, third] {
        assert!(writer.join().unwrap().is_ok());
    }
    let index = index.lock().unwrap();
    assert_eq!(indexed_ids(&index), ["T-0001", "T-0002", "T-0003"]);
}

#[test]
fn a_commit_that_stops_still_answers_its_writers_and_lets_the_next_start() {
    let commits = Commits::default();
    commits.state().running = true;
    let batch = vec![(7, rows("T-0001"))];

    // What a panic in the commit drops.
    drop(Leading::new(&commits, &batch));

    let mut state = commits.state();
    assert!(!state.running);
    assert!(
        state
            .results
            .remove(&7)
            .is_some_and(|result| result.is_err())
    );
}
