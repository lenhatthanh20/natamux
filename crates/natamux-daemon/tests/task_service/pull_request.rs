//! The group 'recordPullRequestStatus'.

use natamux_proto::tasks::task::{PullRequestStatus, TaskState};

use super::*;

fn status(state: &str, checks: &str) -> PullRequestStatus {
    PullRequestStatus {
        state: state.into(),
        checks: checks.into(),
    }
}

fn task_in_review_with_pr(fx: &Fixture) -> Task {
    let task = create(&fx.service, "Fix it");
    human(&fx.service, &task.id, "task.readied");
    apply(
        &fx.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.started" }),
    );
    apply(
        &fx.service,
        &task.id,
        json!({ "source": "system", "type": "stage.checked" }),
    );
    fx.service
        .record_git(
            &task.id,
            "pr.opened",
            json!({ "url": "https://github.com/o/r/pull/1" }),
        )
        .unwrap()
}

#[test]
fn keeps_the_status_and_logs_only_when_checks_pass_or_fail() {
    let fx = Fixture::new();
    let task = task_in_review_with_pr(&fx);
    let count = event_kinds(&fx.service, &task.id).len();

    fx.service
        .record_pull_request_status(&task.id, status("open", "pending"))
        .unwrap();
    assert_eq!(event_kinds(&fx.service, &task.id).len(), count);
    assert_eq!(
        fx.service.get_task(&task.id).unwrap().pr_status,
        Some(status("open", "pending"))
    );
    assert!(matches!(
        last_message(&fx),
        DaemonMessage::TaskChanged { .. }
    ));

    for checks in ["failing", "failing", "passing"] {
        fx.service
            .record_pull_request_status(&task.id, status("open", checks))
            .unwrap();
    }
    assert_eq!(
        event_kinds(&fx.service, &task.id)[count..],
        ["ci.failed", "ci.passed"]
    );
}

#[test]
fn finishes_a_task_in_review_when_its_pull_request_is_merged() {
    let fx = Fixture::new();
    let task = task_in_review_with_pr(&fx);

    let updated = fx
        .service
        .record_pull_request_status(&task.id, status("merged", "passing"))
        .unwrap();

    assert_eq!(updated.state, TaskState::Done);
    let kinds = event_kinds(&fx.service, &task.id);
    assert_eq!(kinds[kinds.len() - 2..], ["ci.passed", "pr.merged"]);
}

#[test]
fn logs_a_merge_that_does_not_fit_the_state_as_not_accepted() {
    let fx = Fixture::new();
    let task = create(&fx.service, "Fix it");

    let updated = fx
        .service
        .record_pull_request_status(&task.id, status("merged", "none"))
        .unwrap();

    assert_eq!(updated.state, TaskState::Draft);
    let last = last_event(&fx.service, &task.id);
    assert!(!last.accepted);
    assert_eq!(last.event.kind, "pr.merged");
}
