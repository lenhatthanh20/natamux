//! The group 'setWorkflow': the choices of a stage (workflow, CI, skip) and their logs.

use natamux_daemon::tasks::errors::TaskError;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, StageKind};

use super::*;

struct Setup {
    fx: Fixture,
    shared: Arc<Mutex<Vec<Step>>>,
    service: TaskService,
}

impl Setup {
    fn new() -> Setup {
        let fx = Fixture::new();
        let (shared, settings_of) = live_settings(default_pipeline());
        let service = fx.make(Some(settings_of));
        Setup {
            fx,
            shared,
            service,
        }
    }

    fn task(&self) -> Task {
        create(&self.service, "Export")
    }

    fn count(&self, id: &str) -> usize {
        self.service.list_events(id).unwrap().len()
    }

    fn payloads(&self, id: &str, kind: &str) -> Vec<Map<String, Value>> {
        let events = self.service.list_events(id).unwrap();
        events
            .into_iter()
            .filter(|entry| entry.event.kind == kind)
            .map(|entry| entry.event.payload)
            .collect()
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|each| each.to_string()).collect()
}

fn choices(stage: &str, on: bool) -> Map<String, Value> {
    obj(json!({ "stage": stage, "on": on }))
}

#[test]
fn turns_the_workflow_on_and_off_for_a_stage_and_logs_each_change() {
    let s = Setup::new();
    let task = create_with(
        &s.service,
        json!({ "title": "Export", "agent": "claude-code" }),
    );
    assert_eq!(task.workflow_stages, Vec::<String>::new());

    let on = s.service.set_workflow(&task.id, "implement", true).unwrap();
    assert_eq!(on.workflow_stages, ["implement"]);
    assert_eq!(
        file_task(&s.fx, &task.id).unwrap().workflow_stages,
        ["implement"]
    );
    match last_message(&s.fx) {
        DaemonMessage::TaskUpdated { task, event, .. } => {
            assert_eq!(task.workflow_stages, ["implement"]);
            assert_eq!(
                (event.kind.as_str(), event.source.as_str()),
                ("stage.workflow-set", "human")
            );
        }
        other => panic!("not task.updated: {other:?}"),
    }

    assert_eq!(
        s.service
            .set_workflow(&task.id, "plan", true)
            .unwrap()
            .workflow_stages,
        ["implement", "plan"]
    );
    assert_eq!(
        s.service
            .set_workflow(&task.id, "implement", false)
            .unwrap()
            .workflow_stages,
        ["plan"]
    );
    assert_eq!(
        s.payloads(&task.id, "stage.workflow-set"),
        [
            choices("implement", true),
            choices("plan", true),
            choices("implement", false)
        ]
    );
}

#[test]
fn turns_the_check_on_ci_on_and_off_for_a_stage_that_offers_it_and_logs_each_change() {
    let s = Setup::new();
    let task = s.task();
    assert_eq!(task.ci_stages, None);

    let on = s.service.set_ci(&task.id, "implement", true).unwrap();
    assert_eq!(on.ci_stages, Some(strings(&["implement"])));
    assert_eq!(
        file_task(&s.fx, &task.id).unwrap().ci_stages,
        Some(strings(&["implement"]))
    );
    assert_eq!(
        s.service
            .set_ci(&task.id, "testing", true)
            .unwrap()
            .ci_stages,
        Some(strings(&["implement", "testing"]))
    );
    assert_eq!(
        s.service
            .set_ci(&task.id, "implement", false)
            .unwrap()
            .ci_stages,
        Some(strings(&["testing"]))
    );
    assert_eq!(
        s.payloads(&task.id, "stage.ci-set"),
        [
            choices("implement", true),
            choices("testing", true),
            choices("implement", false)
        ]
    );
    assert!(err_text(s.service.set_ci(&task.id, "spec", true)).contains("project settings"));
    assert!(matches!(
        s.service.set_ci(&task.id, "nope", true),
        Err(TaskError::Edit(_))
    ));
}

#[test]
fn marks_a_later_stage_to_skip_and_unmarks_it_and_logs_each_change() {
    let s = Setup::new();
    let task = s.task();
    assert_eq!(task.skip_stages, None);

    let on = s.service.set_skip(&task.id, "testing", true).unwrap();
    assert_eq!(on.skip_stages, Some(strings(&["testing"])));
    assert_eq!(
        file_task(&s.fx, &task.id).unwrap().skip_stages,
        Some(strings(&["testing"]))
    );
    match last_message(&s.fx) {
        DaemonMessage::TaskUpdated { task, event, .. } => {
            assert_eq!(task.skip_stages, Some(strings(&["testing"])));
            assert_eq!(
                (event.kind.as_str(), event.source.as_str()),
                ("stage.skip-set", "human")
            );
        }
        other => panic!("not task.updated: {other:?}"),
    }
    assert_eq!(
        s.service
            .set_skip(&task.id, "implement", true)
            .unwrap()
            .skip_stages,
        Some(strings(&["testing", "implement"]))
    );
    assert_eq!(
        s.service
            .set_skip(&task.id, "testing", false)
            .unwrap()
            .skip_stages,
        Some(strings(&["implement"]))
    );
    let count = s.count(&task.id);
    s.service.set_skip(&task.id, "implement", true).unwrap();
    s.service.set_skip(&task.id, "release", false).unwrap();
    assert_eq!(s.count(&task.id), count);
    assert_eq!(
        s.payloads(&task.id, "stage.skip-set"),
        [
            choices("testing", true),
            choices("implement", true),
            choices("testing", false)
        ]
    );
}

#[test]
fn marks_only_a_stage_after_the_current_one_of_a_task_that_is_not_over() {
    let s = Setup::new();
    let task = s.task();
    assert!(err_text(s.service.set_skip(&task.id, "nope", true)).contains("\"nope\""));
    assert!(matches!(
        s.service.set_skip(&task.id, "context", true),
        Err(TaskError::Edit(_))
    ));

    human(&s.service, &task.id, "task.readied");
    apply(
        &s.service,
        &task.id,
        json!({ "source": "system", "type": "stage.passed" }),
    );
    assert_eq!(s.service.get_task(&task.id).unwrap().stage, "spec");
    assert!(err_text(s.service.set_skip(&task.id, "spec", true)).contains("Skip"));
    assert!(matches!(
        s.service.set_skip(&task.id, "context", true),
        Err(TaskError::Edit(_))
    ));
    assert_eq!(
        s.service
            .set_skip(&task.id, "plan", true)
            .unwrap()
            .skip_stages,
        Some(strings(&["plan"]))
    );

    human(&s.service, &task.id, "task.cancelled");
    assert!(matches!(
        s.service.set_skip(&task.id, "testing", true),
        Err(TaskError::Edit(_))
    ));
    assert!(matches!(
        s.service.set_skip(&task.id, "plan", false),
        Err(TaskError::Edit(_))
    ));
    assert_eq!(
        s.service.get_task(&task.id).unwrap().skip_stages,
        Some(strings(&["plan"]))
    );
}

#[test]
fn refuses_a_stage_of_the_current_group_and_an_earlier_one() {
    let s = Setup::new();
    let task = s.task();
    override_to(&s.service, &task.id, "ready", Some("build"));
    let count = s.count(&task.id);

    assert!(err_text(s.service.set_skip(&task.id, "implement", true)).contains("Skip"));
    assert!(matches!(
        s.service.set_skip(&task.id, "plan", true),
        Err(TaskError::Edit(_))
    ));
    assert_eq!(s.count(&task.id), count);
    assert_eq!(
        s.service
            .set_skip(&task.id, "testing", true)
            .unwrap()
            .skip_stages,
        Some(strings(&["testing"]))
    );
}

#[test]
fn adds_no_field_and_logs_nothing_when_an_unmarked_stage_is_unmarked() {
    let s = Setup::new();
    let task = s.task();
    let count = s.count(&task.id);

    assert_eq!(
        s.service
            .set_skip(&task.id, "testing", false)
            .unwrap()
            .skip_stages,
        None
    );
    assert_eq!(s.count(&task.id), count);
}

#[test]
fn logs_nothing_when_the_choice_does_not_change() {
    let s = Setup::new();
    let task = s.task();
    s.service.set_workflow(&task.id, "implement", true).unwrap();
    let count = s.count(&task.id);

    s.service.set_workflow(&task.id, "implement", true).unwrap();
    s.service.set_workflow(&task.id, "plan", false).unwrap();

    assert_eq!(s.count(&task.id), count);
}

#[test]
fn refuses_a_stage_that_does_not_offer_a_workflow_or_that_the_task_does_not_have() {
    let s = Setup::new();
    let task = s.task();
    assert!(matches!(
        s.service.set_workflow(&task.id, "review", true),
        Err(TaskError::Edit(_))
    ));
    assert!(err_text(s.service.set_workflow(&task.id, "spec", true)).contains("project settings"));
    assert!(err_text(s.service.set_workflow(&task.id, "nope", true)).contains("\"nope\""));
}

#[test]
fn refuses_a_workflow_for_an_agent_other_than_claude_code_but_turns_it_off() {
    let s = Setup::new();
    let task = create_with(
        &s.service,
        json!({ "title": "Export", "agent": "claude-code" }),
    );
    s.service.set_workflow(&task.id, "implement", true).unwrap();
    s.service
        .record_agent_launch(&task.id, "codex", None, None, None)
        .unwrap();

    assert!(err_text(s.service.set_workflow(&task.id, "plan", true)).contains("Only Claude Code"));
    assert_eq!(
        s.service
            .set_workflow(&task.id, "implement", false)
            .unwrap()
            .workflow_stages,
        Vec::<String>::new()
    );
}

#[test]
fn turns_off_a_stage_that_no_longer_offers_a_workflow() {
    let s = Setup::new();
    let task = s.task();
    s.service.set_workflow(&task.id, "implement", true).unwrap();
    *s.shared.lock().unwrap() = vec![
        Step::Stage(Stage::new("context", StageKind::Context, Gate::Manual)),
        Step::Stage(Stage::new("implement", StageKind::Agent, Gate::Auto)),
    ];

    assert!(s.service.set_workflow(&task.id, "plan", true).is_ok());
    assert_eq!(
        s.service
            .set_workflow(&task.id, "implement", false)
            .unwrap()
            .workflow_stages,
        ["plan"]
    );
    assert!(
        err_text(s.service.set_workflow(&task.id, "implement", true)).contains("project settings")
    );
}

#[test]
fn writes_one_event_per_change_when_the_choice_goes_on_off_and_on() {
    let s = Setup::new();
    let task = s.task();
    for on in [true, false, true] {
        s.service.set_workflow(&task.id, "implement", on).unwrap();
    }

    assert_eq!(s.payloads(&task.id, "stage.workflow-set").len(), 3);
    assert_eq!(
        s.service.get_task(&task.id).unwrap().workflow_stages,
        ["implement"]
    );
}

// The choice only matters for a later start, such as a send back, so a finished stage takes it.
#[test]
fn takes_the_choice_for_a_stage_that_is_done_already() {
    let s = Setup::new();
    let task = s.task();
    override_to(&s.service, &task.id, "review", Some("agent-review"));

    assert_eq!(
        s.service
            .set_workflow(&task.id, "plan", true)
            .unwrap()
            .workflow_stages,
        ["plan"]
    );
}

#[test]
fn rebuilds_the_index_when_the_log_has_a_choice_for_a_stage_the_task_does_not_have() {
    let s = Setup::new();
    let task = s.task();
    s.service
        .record_event(
            &task.id,
            "stage.workflow-set",
            json!({ "stage": "nope", "on": true }),
            "human",
        )
        .unwrap();

    let fresh =
        s.fx.started_on(s.fx.memory_index(), Some(settings(default_pipeline())));

    assert_eq!(
        fresh.get_task(&task.id).unwrap().workflow_stages,
        Vec::<String>::new()
    );
    assert_eq!(last_payload(&fresh, &task.id), choices("nope", true));
}

#[test]
fn keeps_the_choice_when_the_index_is_built_again_from_the_files() {
    let s = Setup::new();
    let task = s.task();
    s.service.set_workflow(&task.id, "implement", true).unwrap();

    let fresh =
        s.fx.started_on(s.fx.memory_index(), Some(settings(default_pipeline())));

    assert_eq!(
        fresh.get_task(&task.id).unwrap().workflow_stages,
        ["implement"]
    );
    assert_eq!(
        fresh.list_tasks(None).unwrap()[0].workflow_stages,
        ["implement"]
    );
    assert_eq!(
        last_event(&fresh, &task.id).event.kind,
        "stage.workflow-set"
    );
}
