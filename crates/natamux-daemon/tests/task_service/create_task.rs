//! The group 'createTask'.

use std::sync::{Arc, Mutex};

use natamux_core::tasks::inputs::Upload;
use natamux_daemon::tasks::errors::TaskError;
use natamux_daemon::tasks::upload::decode_attachments;
use natamux_proto::tasks::task::TaskState;
use natamux_store::attachments::list_attachments;
use natamux_store::task_files::task_file_path;

use super::*;

fn titled(service: &TaskService, title: &str) -> TaskResult<Task> {
    service.create_task(Some(&json!({ "title": title })), &[], true)
}

#[test]
fn creates_a_draft_task_with_the_next_id_and_stores_it_as_a_file() {
    let fx = Fixture::new();
    let first = create(&fx.service, "First");
    let second = create(&fx.service, "Second");

    assert_eq!(first.id, "T-0001");
    assert_eq!(first.state, TaskState::Draft);
    assert_eq!(first.priority, "normal");
    assert_eq!(second.id, "T-0002");
    assert_eq!(file_task(&fx, "T-0001"), Some(first));
}

#[test]
fn records_a_task_created_event_and_publishes_the_new_task() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    assert_eq!(event_kinds(&fx.service, &task.id), ["task.created"]);
    assert_eq!(fx.messages(), vec![DaemonMessage::TaskCreated { task }]);
}

#[test]
fn rejects_an_empty_title() {
    let fx = Fixture::new();
    assert!(titled(&fx.service, "  ").is_err());
}

#[test]
fn takes_the_id_from_the_start_of_the_title() {
    for title in ["Task-0005: Fix bug xxx", "task-0005 Fix bug xxx"] {
        let fx = Fixture::new();
        let task = create(&fx.service, title);

        assert_eq!(
            (task.id.as_str(), task.title.as_str()),
            ("TASK-0005", "Fix bug xxx")
        );
        assert_eq!(file_task(&fx, "TASK-0005"), Some(task));
    }
}

#[test]
fn rejects_an_id_that_is_already_used_and_writes_nothing() {
    let fx = Fixture::new();
    let first = create(&fx.service, "TASK-0005: First");
    let published = fx.messages().len();

    let result = titled(&fx.service, "Task-0005: Other");
    assert!(matches!(result, Err(TaskError::Edit(_))));
    assert_eq!(file_task(&fx, "TASK-0005"), Some(first));
    assert_eq!(fx.service.list_tasks(None).unwrap().len(), 1);
    assert_eq!(fx.service.list_events("TASK-0005").unwrap().len(), 1);
    assert_eq!(fx.messages().len(), published);
}

#[test]
fn counts_only_t_ids_for_the_next_default_id() {
    let fx = Fixture::new();
    create(&fx.service, "T-0003: Third");
    create(&fx.service, "TASK-0050: Custom");

    assert_eq!(create(&fx.service, "Next").id, "T-0004");
}

// A task whose file is gone from outside stays in memory, with its event log and agents.
#[test]
fn does_not_reuse_the_id_when_its_file_is_gone() {
    let cases: [(&str, &[&str], &str); 2] = [
        ("T-0002", &["First", "Second"], "T-0003"),
        ("T-0001", &["Only"], "T-0002"),
    ];
    for (removed, titles, next) in cases {
        let fx = Fixture::new();
        for title in titles {
            create(&fx.service, title);
        }
        fs::remove_file(task_file_path(fx.data(), removed)).unwrap();

        let task = create(&fx.service, "Next");

        assert_eq!(task.id, next);
        assert_eq!(file_task(&fx, next), Some(task));
        assert_eq!(event_kinds(&fx.service, next), ["task.created"]);
        assert_eq!(fx.service.list_events(removed).unwrap().len(), 1);
    }
}

// delete_task clears memory and disk, so the id counts no more; nothing of the task is left.
#[test]
fn gives_the_next_id_after_delete_task() {
    for (deleted, next) in [("T-0002", "T-0002"), ("T-0001", "T-0003")] {
        let fx = Fixture::new();
        create(&fx.service, "First");
        create(&fx.service, "Second");
        fx.service.delete_task(deleted).unwrap();

        let task = create(&fx.service, "Next");

        assert_eq!(task.id, next);
        assert_eq!(event_kinds(&fx.service, next), ["task.created"]);
    }
}

#[test]
fn rejects_an_id_the_user_chose_when_only_memory_holds_that_task() {
    let fx = Fixture::new();
    let first = create(&fx.service, "TASK-0005: First");
    fs::remove_file(task_file_path(fx.data(), &first.id)).unwrap();
    let published = fx.messages().len();

    let result = titled(&fx.service, "TASK-0005: Other");
    assert!(matches!(result, Err(TaskError::Edit(_))));
    assert_eq!(
        err_text(titled(&fx.service, "TASK-0005: Other")),
        "Task id TASK-0005 is already used"
    );
    assert_eq!(file_task(&fx, &first.id), None);
    assert_eq!(fx.service.get_task(&first.id).unwrap(), first);
    assert_eq!(fx.service.list_events(&first.id).unwrap().len(), 1);
    assert_eq!(fx.messages().len(), published);
}

#[test]
fn logs_and_publishes_the_task_with_the_id_the_user_chose() {
    let fx = Fixture::new();
    let task = create(&fx.service, "  Task-0005  :   Fix bug xxx  ");

    assert_eq!(
        (task.id.as_str(), task.title.as_str()),
        ("TASK-0005", "Fix bug xxx")
    );
    assert_eq!(event_kinds(&fx.service, "TASK-0005"), ["task.created"]);
    assert_eq!(fx.messages(), vec![DaemonMessage::TaskCreated { task }]);
}

#[test]
fn keeps_the_digits_of_the_id_as_typed() {
    let fx = Fixture::new();
    let short = create(&fx.service, "Task-5: a: b");
    let long = create(&fx.service, "TASK-0005: fix T-12 parser");

    assert_eq!(
        (short.id.as_str(), short.title.as_str()),
        ("TASK-5", "a: b")
    );
    assert_eq!(
        (long.id.as_str(), long.title.as_str()),
        ("TASK-0005", "fix T-12 parser")
    );
}

#[test]
fn accepts_an_id_of_32_characters_and_rejects_a_longer_one_with_no_side_effects() {
    let fx = Fixture::new();
    let fits = format!("{}-12", "A".repeat(29));
    assert_eq!(create(&fx.service, &format!("{fits}: x")).id, fits);

    let published = fx.messages().len();
    assert!(titled(&fx.service, &format!("A{fits}: x")).is_err());
    assert_eq!(fx.service.list_tasks(None).unwrap().len(), 1);
    assert_eq!(fx.messages().len(), published);
}

#[test]
fn rejects_the_empty_title_with_no_side_effects() {
    for title in ["", "   "] {
        let fx = Fixture::new();
        assert!(titled(&fx.service, title).is_err());
        assert_eq!(fx.service.list_tasks(None).unwrap(), []);
        assert_eq!(fx.messages(), []);
    }
}

#[test]
fn rejects_an_id_that_exists_ignoring_case() {
    for (title, existing) in [
        ("task-0005: Other", "TASK-0005: First"),
        ("T-0003: Other", "T-0003: First"),
    ] {
        let fx = Fixture::new();
        let first = create(&fx.service, existing);
        let path = fx.data().join("tasks").join(format!("{}.md", first.id));
        let before = fs::read_to_string(&path).unwrap();

        assert_eq!(
            err_text(titled(&fx.service, title)),
            format!("Task id {} is already used", first.id)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }
}

#[test]
fn treats_task_5_and_task_0005_as_two_ids() {
    let fx = Fixture::new();
    create(&fx.service, "TASK-0005: First");

    assert_eq!(create(&fx.service, "TASK-5: Second").id, "TASK-5");
}

#[test]
fn gives_the_next_default_id_after_other_ids() {
    let cases: [(&[&str], &str); 6] = [
        (&["TASK-0050: x", "ABC-7: y"], "T-0001"),
        (&["T-0100: x"], "T-0101"),
        (&["T-0003: x", "TT-0200: y"], "T-0004"),
        (&["T-0003: x", "T-5: y"], "T-0006"),
        (&["T-0009: x"], "T-0010"),
        (&["T-9999: x"], "T-10000"),
    ];
    for (titles, next) in cases {
        let fx = Fixture::new();
        for title in titles {
            create(&fx.service, title);
        }

        assert_eq!(create(&fx.service, "Fix bug").id, next, "after {titles:?}");
    }
}

#[test]
fn keeps_the_whole_title_when_asked_not_to_split_it() {
    let fx = Fixture::new();
    let task = fx
        .service
        .create_task(Some(&json!({ "title": "abc-2 y" })), &[], false)
        .unwrap();

    assert_eq!(
        (task.id.as_str(), task.title.as_str()),
        ("T-0001", "abc-2 y")
    );
}

#[test]
fn keeps_the_attachments_with_the_task_and_logs_each_one_before_the_task_is_published() {
    let fx = Fixture::new();
    let bus = EventBus::new();
    let listed = Arc::new(Mutex::new(Vec::<String>::new()));
    let (seen, data) = (Arc::clone(&listed), fx.data().to_path_buf());
    bus.subscribe(move |message| {
        if let DaemonMessage::TaskCreated { task } = message {
            let names = list_attachments(&data, &task.id).unwrap();
            *seen.lock().unwrap() = names.into_iter().map(|each| each.name).collect();
        }
    });
    let with_files = TaskService::new(TaskServiceOptions {
        data_dir: fx.data().to_path_buf(),
        index: fx.index(),
        bus,
        now: None,
        settings_of: None,
        clock: None,
    });
    let files = decode_attachments(&[
        Upload {
            name: "a.png".into(),
            data: "aW1hZ2U=".into(),
        },
        Upload {
            name: "b.pdf".into(),
            data: "cGRm".into(),
        },
    ])
    .unwrap();

    let task = with_files
        .create_task(Some(&json!({ "title": "With files" })), &files, true)
        .unwrap();

    assert_eq!(*listed.lock().unwrap(), ["a.png", "b.pdf"]);
    let events: Vec<(String, Map<String, Value>)> = with_files
        .list_events(&task.id)
        .unwrap()
        .into_iter()
        .map(|stored| (stored.event.kind, stored.event.payload))
        .collect();
    assert_eq!(
        events,
        [
            ("task.created".to_string(), obj(json!({}))),
            (
                "attachment.added".to_string(),
                obj(json!({ "name": "a.png", "size": 5 }))
            ),
            (
                "attachment.added".to_string(),
                obj(json!({ "name": "b.pdf", "size": 3 }))
            ),
        ]
    );
}
