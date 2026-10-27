//! The group 'event log files'.

use natamux_daemon::tasks::errors::TaskError;
use natamux_daemon::terminals::shell_files::{
    OpenShell, ended_shells_path, open_shells_path, save_ended_shells, save_open_shells,
};
use natamux_proto::tasks::task::IssueLink;
use natamux_proto::terminals::ended::EndedShell;
use natamux_store::attachments::AttachmentFile;
use natamux_store::event_log::{event_log_path, read_event_log};
use natamux_store::task_files::{archive_dir, tasks_dir};

use natamux_daemon::tasks::records::WorkspaceChange;

use super::*;

#[test]
fn rebuilds_the_events_from_the_log_files_when_the_index_is_new() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");
    human(&fx.service, &task.id, "task.readied");

    let fresh = fx.started_on(fx.memory_index(), None);

    assert_eq!(
        event_kinds(&fresh, &task.id),
        ["task.created", "task.readied"]
    );
}

#[test]
fn writes_a_log_file_for_a_task_that_only_has_events_in_the_index() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");
    fs::remove_file(event_log_path(fx.data(), &task.id)).unwrap();

    fx.started(None);

    let log = read_event_log(fx.data(), &task.id).unwrap().unwrap();
    assert_eq!(
        log.iter()
            .map(|each| each.event.kind.as_str())
            .collect::<Vec<_>>(),
        ["task.created"]
    );
}

#[test]
fn deletes_a_task_and_the_files_stored_with_it() {
    let fx = Fixture::new();
    let hello = AttachmentFile {
        name: "a.txt".into(),
        bytes: b"hello".to_vec(),
    };
    let task = fx
        .service
        .create_task(Some(&json!({ "title": "Fix it" })), &[hello], true)
        .unwrap();
    let data = fx.data();
    let launch = |name: String| data.join(name);
    fs::write(launch(format!("agent-launch-{}.sh", task.id)), "echo hi\n").unwrap();
    fs::write(
        launch(format!("agent-launch-{}-2.sh", task.id)),
        "echo hi\n",
    )
    .unwrap();
    fs::write(launch("agent-launch-T-0099.sh".into()), "keep\n").unwrap();
    let ended = EndedShell {
        name: "1".into(),
        agent: None,
        stage: None,
        session: None,
        ended_at: "2026-10-15T00:07:00.000Z".into(),
        text: "done".into(),
    };
    save_ended_shells(data, &task.id, &[ended]).unwrap();
    let open = OpenShell {
        name: "6".into(),
        agent: Some("copilot".into()),
        session: Some("c-1".into()),
        paused: false,
    };
    save_open_shells(data, &task.id, &[open]).unwrap();
    let usage = tasks_dir(data).join(format!("{}.usage.json", task.id));
    fs::write(&usage, r#"{"files":{},"stages":{}}"#).unwrap();

    assert_eq!(fx.service.delete_task(&task.id).unwrap(), []);

    assert_eq!(file_task(&fx, &task.id), None);
    assert!(!event_log_path(data, &task.id).exists());
    assert!(!archive_dir(data, &task.id).exists());
    assert!(!launch(format!("agent-launch-{}.sh", task.id)).exists());
    assert!(!launch(format!("agent-launch-{}-2.sh", task.id)).exists());
    assert!(!ended_shells_path(data, &task.id).exists());
    assert!(!open_shells_path(data, &task.id).exists());
    assert!(!usage.exists());
    assert_eq!(
        fs::read_to_string(launch("agent-launch-T-0099.sh".into())).unwrap(),
        "keep\n"
    );
    assert_eq!(fx.service.list_tasks(None).unwrap(), []);
    assert_eq!(
        last_message(&fx),
        DaemonMessage::TaskDeleted { task_id: task.id }
    );
}

#[test]
fn refuses_a_task_that_still_has_a_workspace_and_an_unknown_task() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");
    let change = WorkspaceChange {
        workspace: Some("/work/T-0001".into()),
        branch: Some("natamux/t-0001".into()),
    };
    fx.service.record_workspace(&task.id, change).unwrap();

    assert!(err_text(fx.service.delete_task(&task.id)).contains("workspace"));
    assert_eq!(
        file_task(&fx, &task.id).unwrap().workspace.as_deref(),
        Some("/work/T-0001")
    );
    assert!(matches!(
        fx.service.delete_task("T-0099"),
        Err(TaskError::NotFound(_))
    ));
}

#[test]
fn drops_the_parent_and_the_dependency_from_the_tasks_that_remain() {
    let fx = Fixture::new();
    let parent = create(&fx.service, "Release");
    let child = create_with(
        &fx.service,
        json!({ "title": "Notes", "parentId": parent.id }),
    );
    let waiting = create(&fx.service, "After");
    fx.service
        .update_task(&waiting.id, Some(&json!({ "dependsOn": [parent.id] })))
        .unwrap();

    let related = fx.service.delete_task(&parent.id).unwrap();

    let mut related_ids = ids(&related);
    related_ids.sort();
    assert_eq!(related_ids, [child.id.clone(), waiting.id.clone()]);
    assert_eq!(fx.service.get_task(&child.id).unwrap().parent_id, None);
    assert_eq!(
        fx.service.get_task(&waiting.id).unwrap().depends_on,
        Vec::<String>::new()
    );
    assert_eq!(
        last_message(&fx),
        DaemonMessage::TaskDeleted { task_id: parent.id }
    );
}

#[test]
fn links_an_issue_and_logs_it() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");
    let link =
        json!({ "provider": "jira", "key": "ABC-1", "url": "https://jira.example.com/ABC-1" });

    let linked = fx.service.link_issue(&task.id, &link, "human").unwrap();

    let expected = IssueLink {
        provider: "jira".into(),
        key: "ABC-1".into(),
        url: "https://jira.example.com/ABC-1".into(),
    };
    assert_eq!(linked.issue, Some(expected));
    let event = last_event(&fx.service, &task.id).event;
    assert_eq!(event.kind, "issue.linked");
    assert_eq!(Value::Object(event.payload), link);
}
