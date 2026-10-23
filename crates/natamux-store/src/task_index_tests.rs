use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, json};

use super::*;

fn make_task(id: &str) -> Task {
    Task {
        id: id.into(),
        title: "Example".into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement"), json!("review"), json!("release")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn index() -> TaskIndex {
    TaskIndex::open(Path::new(":memory:")).unwrap()
}

fn event(kind: &str, payload: serde_json::Value) -> TaskEvent {
    TaskEvent {
        task_id: "T-0001".into(),
        source: "agent".into(),
        kind: kind.into(),
        at: "2026-10-08T09:05:00.000Z".into(),
        payload: payload.as_object().cloned().unwrap_or_else(Map::new),
    }
}

fn logged(event: TaskEvent, accepted: bool) -> LoggedEvent {
    LoggedEvent { event, accepted }
}

fn stat(mtime_ms: f64, size: u64) -> FileStat {
    FileStat { mtime_ms, size }
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|task| task.id.as_str()).collect()
}

fn accepted(index: &TaskIndex) -> Vec<bool> {
    index
        .list_events("T-0001")
        .unwrap()
        .iter()
        .map(|entry| entry.accepted)
        .collect()
}

// Makes the second insert into `events` fail, so a change breaks half way.
fn fail_on_events(index: &TaskIndex, when: &str) {
    index
        .connection()
        .execute_batch(&format!(
            "create trigger refuse before insert on events when {when} \
             begin select raise(abort, 'refused'); end;"
        ))
        .unwrap();
}

#[test]
fn lists_the_newest_updated_task_first() {
    let mut index = index();
    index.upsert_task(&make_task("T-0001")).unwrap();
    let newer = Task {
        updated_at: "2026-10-08T10:00:00.000Z".into(),
        ..make_task("T-0002")
    };
    index.upsert_task(&newer).unwrap();
    assert_eq!(ids(&index.list_tasks(None).unwrap()), ["T-0002", "T-0001"]);
}

#[test]
fn filters_by_state() {
    let mut index = index();
    for (id, state) in [
        ("T-0001", TaskState::Running),
        ("T-0002", TaskState::Review),
    ] {
        index
            .upsert_task(&Task {
                state,
                ..make_task(id)
            })
            .unwrap();
    }
    assert_eq!(ids(&index.list_tasks(Some("review")).unwrap()), ["T-0002"]);
}

#[test]
fn replaces_a_task_that_is_upserted_again() {
    let mut index = index();
    for title in ["Old", "New"] {
        index
            .upsert_task(&Task {
                title: title.into(),
                ..make_task("T-0001")
            })
            .unwrap();
    }
    let titles: Vec<String> = index
        .list_tasks(None)
        .unwrap()
        .into_iter()
        .map(|task| task.title)
        .collect();
    assert_eq!(titles, ["New"]);
}

#[test]
fn replaces_every_task_at_once() {
    let mut index = index();
    index.upsert_task(&make_task("T-0001")).unwrap();
    index
        .replace_all_tasks(&[&make_task("T-0005"), &make_task("T-0006")], &[])
        .unwrap();
    let mut listed: Vec<String> = index
        .list_tasks(None)
        .unwrap()
        .into_iter()
        .map(|task| task.id)
        .collect();
    listed.sort();
    assert_eq!(listed, ["T-0005", "T-0006"]);
}

#[test]
fn lists_events_of_a_task_in_the_order_they_were_added_with_the_accepted_flag() {
    let mut index = index();
    let asked = event("agent.asked", json!({"question": "Which branch?"}));
    index.append_event(&asked, true).unwrap();
    index
        .append_event(&event("agent.failed", json!({})), false)
        .unwrap();
    let other = TaskEvent {
        task_id: "T-0002".into(),
        ..asked.clone()
    };
    index.append_event(&other, true).unwrap();

    let stored = index.list_events("T-0001").unwrap();

    let seen: Vec<(&str, bool)> = stored
        .iter()
        .map(|entry| (entry.event.kind.as_str(), entry.accepted))
        .collect();
    assert_eq!(seen, [("agent.asked", true), ("agent.failed", false)]);
    assert_eq!(stored[0].event.payload, asked.payload);
    assert!(stored[0].seq < stored[1].seq);
}

#[test]
fn returns_no_events_for_an_unknown_task() {
    assert!(index().list_events("T-0099").unwrap().is_empty());
}

#[test]
fn stores_the_task_its_events_and_the_stats_of_its_files_together() {
    let mut index = index();
    let task = Task {
        state: TaskState::Running,
        ..make_task("T-0001")
    };
    let started = event("agent.started", json!({}));

    index
        .record(
            "T-0001",
            &TaskChange {
                task: Some(&task),
                events: &[logged(started, true)],
                files: TaskFiles {
                    task: Some(stat(1.5, 10)),
                    log: Some(stat(2.0, 20)),
                },
            },
        )
        .unwrap();

    assert_eq!(index.list_tasks(None).unwrap(), [task]);
    assert_eq!(accepted(&index), [true]);
    assert_eq!(
        index.task_files().unwrap().get("T-0001"),
        Some(&TaskFiles {
            task: Some(stat(1.5, 10)),
            log: Some(stat(2.0, 20))
        })
    );
}

#[test]
fn keeps_the_stat_of_a_file_that_a_change_did_not_write() {
    let mut index = index();
    let task = make_task("T-0001");
    let files = TaskFiles {
        task: Some(stat(1.0, 10)),
        log: None,
    };
    index
        .record(
            "T-0001",
            &TaskChange {
                task: Some(&task),
                events: &[],
                files,
            },
        )
        .unwrap();
    let files = TaskFiles {
        task: None,
        log: Some(stat(2.0, 20)),
    };
    let events = [logged(event("agent.started", json!({})), false)];
    index
        .record(
            "T-0001",
            &TaskChange {
                task: None,
                events: &events,
                files,
            },
        )
        .unwrap();

    assert_eq!(accepted(&index), [false]);
    assert_eq!(
        index.task_files().unwrap().get("T-0001"),
        Some(&TaskFiles {
            task: Some(stat(1.0, 10)),
            log: Some(stat(2.0, 20))
        })
    );
}

#[test]
fn stores_nothing_of_a_change_that_fails_half_way() {
    // Node makes the event fail with a BigInt, which JSON cannot hold. A payload cannot fail in
    // Rust, so a trigger refuses the event row, after the task row was written.
    let mut index = index();
    fail_on_events(&index, "1");
    let task = make_task("T-0001");
    let events = [logged(event("agent.started", json!({"size": 1})), true)];
    let files = TaskFiles {
        task: Some(stat(1.0, 10)),
        log: None,
    };

    assert!(
        index
            .record(
                "T-0001",
                &TaskChange {
                    task: Some(&task),
                    events: &events,
                    files
                }
            )
            .is_err()
    );

    assert!(index.list_tasks(None).unwrap().is_empty());
    assert!(index.list_events("T-0001").unwrap().is_empty());
    assert!(index.task_files().unwrap().is_empty());
}

#[test]
fn forgets_the_files_of_a_deleted_task() {
    let mut index = index();
    let task = make_task("T-0001");
    let files = TaskFiles {
        task: Some(stat(1.0, 10)),
        log: None,
    };
    index
        .record(
            "T-0001",
            &TaskChange {
                task: Some(&task),
                events: &[],
                files,
            },
        )
        .unwrap();
    index.delete_task("T-0001").unwrap();
    assert!(index.task_files().unwrap().is_empty());
}

#[test]
fn replaces_the_stats_of_every_file_along_with_the_tasks() {
    let mut index = index();
    let task = make_task("T-0001");
    let files = TaskFiles {
        task: Some(stat(1.0, 10)),
        log: None,
    };
    index
        .record(
            "T-0001",
            &TaskChange {
                task: Some(&task),
                events: &[],
                files,
            },
        )
        .unwrap();

    let five = TaskFiles {
        task: Some(stat(5.0, 50)),
        log: None,
    };
    index
        .replace_all_tasks(&[&make_task("T-0005")], &[("T-0005".into(), five)])
        .unwrap();

    let stats = index.task_files().unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats.get("T-0005"), Some(&five));
}

#[test]
fn writes_ahead_to_a_log_and_syncs_to_disk_only_at_checkpoints() {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
        "natamux-index-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&folder).unwrap();
    {
        let on_disk = TaskIndex::open(&folder.join("index.sqlite")).unwrap();
        let db = on_disk.connection();
        let journal: String = db
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        // 1 is NORMAL. WAL alone would keep FULL (2).
        let synchronous: i64 = db
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1);
    }
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn prepares_its_statements_once_not_on_every_call() {
    // Node spies on `prepare` and counts no new call. Rust cannot spy on rusqlite, and the
    // statements come from its statement cache. So this runs every statement many times and checks
    // that the answers stay right, which a broken cache entry would not.
    let mut index = index();
    let task = make_task("T-0001");
    let started = event("agent.active", json!({}));
    for count in 0..10u32 {
        let files = TaskFiles {
            task: Some(stat(f64::from(count), 10)),
            log: Some(stat(f64::from(count), 20)),
        };
        let events = [logged(started.clone(), true)];
        index
            .record(
                "T-0001",
                &TaskChange {
                    task: Some(&task),
                    events: &events,
                    files,
                },
            )
            .unwrap();
        assert_eq!(index.list_tasks(None).unwrap().len(), 1);
        assert_eq!(index.list_tasks(Some("draft")).unwrap().len(), 1);
        assert_eq!(
            index.list_events("T-0001").unwrap().len(),
            count as usize + 1
        );
        assert_eq!(index.count_events("T-0001").unwrap(), u64::from(count) + 1);
        assert_eq!(index.task_files().unwrap().len(), 1);
    }
    index
        .replace_events("T-0001", &[logged(started, false)])
        .unwrap();
    let files = [(
        "T-0001".to_string(),
        TaskFiles {
            task: Some(stat(1.0, 1)),
            log: None,
        },
    )];
    index.replace_all_tasks(&[&task], &files).unwrap();
    index.delete_task("T-0001").unwrap();
    assert!(index.list_tasks(None).unwrap().is_empty());
    assert_eq!(index.count_events("T-0001").unwrap(), 0);
}

#[test]
fn records_a_change_in_exactly_one_transaction() {
    // Node spies on `exec` and sees one begin and one commit. Rust cannot spy, so the closest
    // check is that the change is all or nothing: the second event is refused, and the task and
    // the first event go with it.
    let mut index = index();
    fail_on_events(&index, "new.accepted = 0");
    let task = make_task("T-0001");
    let events = [
        logged(event("agent.active", json!({})), true),
        logged(event("agent.active", json!({})), false),
    ];
    let files = TaskFiles {
        task: Some(stat(1.0, 10)),
        log: Some(stat(2.0, 20)),
    };

    assert!(
        index
            .record(
                "T-0001",
                &TaskChange {
                    task: Some(&task),
                    events: &events,
                    files
                }
            )
            .is_err()
    );

    assert!(index.list_tasks(None).unwrap().is_empty());
    assert_eq!(index.count_events("T-0001").unwrap(), 0);
    assert!(index.task_files().unwrap().is_empty());
    // The connection took the next change after the rollback.
    index
        .connection()
        .execute_batch("drop trigger refuse")
        .unwrap();
    index
        .record(
            "T-0001",
            &TaskChange {
                task: Some(&task),
                events: &events,
                files,
            },
        )
        .unwrap();
    assert_eq!(index.count_events("T-0001").unwrap(), 2);
}
