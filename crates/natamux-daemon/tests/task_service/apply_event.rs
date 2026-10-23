//! The group 'applyEvent', with its group 'the wait for background work'.

use natamux_daemon::tasks::errors::TaskError;
use natamux_proto::tasks::task::{PullRequestStatus, TaskState};

use super::*;

#[test]
fn moves_the_task_when_the_event_fits_and_stores_indexes_and_publishes_the_change() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let result = human(&fx.service, &task.id, "task.readied");

    assert!(result.accepted);
    assert_eq!(result.task.state, TaskState::Ready);
    assert_eq!(file_task(&fx, &task.id).unwrap().state, TaskState::Ready);
    assert_eq!(
        ids(&fx.service.list_tasks(Some(TaskState::Ready)).unwrap()),
        [task.id]
    );
    assert!(matches!(
        last_message(&fx),
        DaemonMessage::TaskUpdated { accepted: true, .. }
    ));
}

#[test]
fn keeps_the_task_when_the_event_does_not_fit_but_still_records_the_event() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let result = apply(
        &fx.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.finished" }),
    );

    assert!(!result.accepted);
    assert_eq!(result.task, task);
    assert!(!last_event(&fx.service, &task.id).accepted);
    assert!(matches!(
        last_message(&fx),
        DaemonMessage::TaskUpdated {
            accepted: false,
            ..
        }
    ));
}

#[test]
fn lets_a_human_override_jump_to_another_state() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let result = override_to(&fx.service, &task.id, "review", None);

    assert_eq!(result.task.state, TaskState::Review);
}

mod background_wait {
    use super::*;

    fn waiting_task(fx: &Fixture) -> Task {
        let task = create(&fx.service, "First");
        human(&fx.service, &task.id, "task.readied");
        apply(
            &fx.service,
            &task.id,
            json!({ "source": "agent", "type": "agent.started" }),
        );
        apply(
            &fx.service,
            &task.id,
            json!({ "source": "agent", "type": "agent.waiting" }),
        )
        .task
    }

    #[test]
    fn starts_with_agent_waiting_which_keeps_the_state() {
        let fx = Fixture::new();
        let task = waiting_task(&fx);

        assert_eq!(task.state, TaskState::Running);
        assert_eq!(task.background_since, Some(task.updated_at.clone()));
        assert_eq!(
            file_task(&fx, &task.id).unwrap().background_since,
            task.background_since
        );
    }

    #[test]
    fn goes_on_through_events_of_the_daemon_such_as_checks_that_pass() {
        let fx = Fixture::new();
        let task = waiting_task(&fx);

        let status = PullRequestStatus {
            state: "open".into(),
            checks: "passing".into(),
        };
        fx.service
            .record_pull_request_status(&task.id, status)
            .unwrap();

        assert_eq!(last_event(&fx.service, &task.id).event.kind, "ci.passed");
        assert_eq!(
            fx.service.get_task(&task.id).unwrap().background_since,
            task.background_since
        );
    }

    #[test]
    fn ends_with_the_next_event_of_the_agent() {
        let fx = Fixture::new();
        let task = waiting_task(&fx);

        let result = apply(
            &fx.service,
            &task.id,
            json!({ "source": "agent", "type": "agent.finished" }),
        );

        assert_eq!(result.task.state, TaskState::NeedsInput);
        assert_eq!(result.task.background_since, None);
    }

    #[test]
    fn ends_when_a_person_moves_the_task() {
        let fx = Fixture::new();
        let task = waiting_task(&fx);

        let result = override_to(&fx.service, &task.id, "review", None);

        assert_eq!(result.task.background_since, None);
    }
}

#[test]
fn does_not_let_a_client_send_task_created() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    let input = json!({ "source": "human", "type": "task.created" });
    assert!(fx.service.apply_event(&task.id, Some(&input)).is_err());
}

#[test]
fn fails_with_not_found_for_an_unknown_task() {
    let fx = Fixture::new();
    let input = json!({ "source": "human", "type": "task.readied" });
    assert!(matches!(
        fx.service.apply_event("T-0099", Some(&input)),
        Err(TaskError::NotFound(_))
    ));
}
