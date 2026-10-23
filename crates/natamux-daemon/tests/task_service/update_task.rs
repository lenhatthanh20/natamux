//! The group 'updateTask'.

use natamux_proto::tasks::task::TaskState;

use super::*;

fn update(service: &TaskService, id: &str, input: Value) -> TaskResult<Task> {
    service.update_task(id, Some(&input))
}

#[test]
fn keeps_the_id_when_the_new_title_starts_with_an_id() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let updated = update(&fx.service, &task.id, json!({ "title": "Task-9: x" })).unwrap();

    assert_eq!(
        (updated.id.as_str(), updated.title.as_str()),
        ("T-0001", "Task-9: x")
    );
}

#[test]
fn changes_the_details_and_logs_which_fields_changed() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let updated = update(
        &fx.service,
        &task.id,
        json!({ "title": "Better title", "description": "Notes", "priority": "high", "labels": ["ui"] }),
    )
    .unwrap();

    assert_eq!(updated.title, "Better title");
    assert_eq!(updated.description, "Notes");
    assert_eq!(updated.priority, "high");
    assert_eq!(updated.labels, ["ui"]);
    assert_eq!(updated.state, TaskState::Draft);
    assert_eq!(file_task(&fx, &task.id), Some(updated));
    let event = last_event(&fx.service, &task.id).event;
    assert_eq!(
        (event.kind.as_str(), event.source.as_str()),
        ("task.edited", "human")
    );
    assert_eq!(
        event.payload["fields"],
        json!(["title", "description", "priority", "labels"])
    );
    match last_message(&fx) {
        DaemonMessage::TaskUpdated { task, .. } => assert_eq!(task.title, "Better title"),
        other => panic!("not task.updated: {other:?}"),
    }
}

#[test]
fn writes_nothing_when_nothing_changed() {
    let fx = Fixture::new();
    let task = create_with(&fx.service, json!({ "title": "First", "labels": ["a"] }));
    let count = fx.service.list_events(&task.id).unwrap().len();

    let same = update(
        &fx.service,
        &task.id,
        json!({ "title": "First", "labels": ["a"] }),
    )
    .unwrap();

    assert_eq!(same, task);
    assert_eq!(fx.service.list_events(&task.id).unwrap().len(), count);
}

#[test]
fn refuses_what_a_person_may_not_change() {
    let inputs = [
        json!({ "title": "  " }),
        json!({ "priority": "someday" }),
        json!({ "state": "done" }),
        json!({ "workspace": "/tmp" }),
    ];
    for input in inputs {
        let fx = Fixture::new();
        let task = create(&fx.service, "First");
        assert!(
            update(&fx.service, &task.id, input.clone()).is_err(),
            "{input}"
        );
    }
}

#[test]
fn takes_dependencies_on_tasks_that_exist_and_refuses_unknown_ones_and_loops() {
    let fx = Fixture::new();
    let first = create(&fx.service, "First");
    let second = create(&fx.service, "Second");

    let updated = update(&fx.service, &second.id, json!({ "dependsOn": [first.id] })).unwrap();
    assert_eq!(updated.depends_on, std::slice::from_ref(&first.id));
    assert!(
        err_text(update(
            &fx.service,
            &second.id,
            json!({ "dependsOn": ["T-0099"] })
        ))
        .contains("T-0099")
    );
    assert!(
        err_text(update(
            &fx.service,
            &first.id,
            json!({ "dependsOn": [second.id] })
        ))
        .contains("loop")
    );
}

#[test]
fn refuses_a_task_that_depends_on_itself() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    assert!(
        err_text(update(
            &fx.service,
            &task.id,
            json!({ "dependsOn": [task.id] })
        ))
        .contains("itself")
    );
}
