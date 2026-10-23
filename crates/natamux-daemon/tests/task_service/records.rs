//! The groups 'recordWorkspaceFailure' and 'recordGit'.

use natamux_proto::tasks::task::TaskState;

use super::*;

#[test]
fn logs_the_workspace_failure_without_changing_the_state() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");

    let updated = fx
        .service
        .record_workspace_failure(&task.id, "git worktree failed: index.lock")
        .unwrap();

    assert_eq!(updated.state, TaskState::Draft);
    assert_eq!(updated.workspace, None);
    let event = last_event(&fx.service, &task.id).event;
    assert_eq!(
        (event.kind.as_str(), event.source.as_str()),
        ("workspace.failed", "system")
    );
    assert_eq!(event.payload["message"], "git worktree failed: index.lock");
}

#[test]
fn logs_a_commit_and_a_push_without_changing_the_state_or_the_pull_request() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");

    fx.service
        .record_git(
            &task.id,
            "git.committed",
            json!({ "hash": "abc", "subject": "Fix it" }),
        )
        .unwrap();
    let pushed = fx
        .service
        .record_git(
            &task.id,
            "git.pushed",
            json!({ "branch": "natamux/t-0001" }),
        )
        .unwrap();

    assert_eq!(pushed.state, TaskState::Draft);
    assert_eq!(pushed.pr, None);
    let events: Vec<TaskEvent> = fx
        .service
        .list_events(&task.id)
        .unwrap()
        .into_iter()
        .map(|entry| entry.event)
        .collect();
    assert_eq!(events[0].kind, "task.created");
    assert_eq!(
        (events[1].kind.as_str(), events[1].source.as_str()),
        ("git.committed", "git")
    );
    assert_eq!(
        events[1].payload,
        obj(json!({ "hash": "abc", "subject": "Fix it" }))
    );
    assert_eq!(
        (events[2].kind.as_str(), events[2].source.as_str()),
        ("git.pushed", "git")
    );
    assert_eq!(
        events[2].payload,
        obj(json!({ "branch": "natamux/t-0001" }))
    );
    assert_eq!(events.len(), 3);
}

#[test]
fn keeps_the_address_of_a_new_pull_request_on_the_task_and_in_its_file() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");
    let url = "https://github.com/owner/repo/pull/7";

    let updated = fx
        .service
        .record_git(&task.id, "pr.opened", json!({ "url": url }))
        .unwrap();

    assert_eq!(updated.pr.as_deref(), Some(url));
    assert_eq!(file_task(&fx, &task.id).unwrap().pr.as_deref(), Some(url));
    match last_message(&fx) {
        DaemonMessage::TaskUpdated { task, .. } => assert_eq!(task.pr.as_deref(), Some(url)),
        other => panic!("not task.updated: {other:?}"),
    }
}
