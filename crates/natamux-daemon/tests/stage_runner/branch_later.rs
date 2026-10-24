//! The last cases of `describe('a branch')` of `stage-runner.test.ts`: send backs, restart, races.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::stages::errors::StageError;
use natamux_daemon::stages::run::EventInput;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_proto::tasks::task::TaskState;
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

use super::*;

#[test]
fn refuses_a_send_back_to_a_path_not_taken_and_checks_again_after_one_to_plan() {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");
    act(&f, &task.id, StageAction::Approve);
    let count = f.task_events(&task.id).len();
    let reopen = |to: &str| {
        f.runner
            .run
            .act(&task.id, StageAction::Reopen, &reopen_to(to))
    };

    let error = reopen("review").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("review is on a path this task did not take"),
        "{error}"
    );
    assert!(matches!(
        reopen("review").unwrap_err(),
        StageError::Input(_)
    ));
    assert!(matches!(reopen("docs").unwrap_err(), StageError::Input(_)));
    assert_eq!(f.task_events(&task.id).len(), count);

    let back = reopen("plan").unwrap();
    assert_eq!(back.stage, "plan");
    assert!(back.paths.is_empty());
    f.settle();
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "plan" } }),
    );
    act(&f, &task.id, StageAction::Approve);

    let approvals: Vec<Value> = f
        .task_events(&task.id)
        .into_iter()
        .filter(|event| event.kind == "stage.approved")
        .map(|event| event.payload["path"].clone())
        .collect();
    assert_eq!(approvals, vec![json!("small"), json!("small")]);
    assert_eq!(f.task(&task.id).stage, "implement");
}

#[test]
fn takes_a_send_back_to_a_stage_of_the_taken_path() {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");
    act(&f, &task.id, StageAction::Approve);
    act(&f, &task.id, StageAction::Pass);

    let back = act_with(&f, &task.id, StageAction::Reopen, reopen_to("implement"));

    assert_eq!(back.stage, "implement");
    assert_eq!(paths(&back), small());
}

#[test]
fn keeps_the_path_over_a_restart_and_starts_the_stage_of_the_path_again() {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");
    act(&f, &task.id, StageAction::Approve);
    f.settle();
    f.terminals.close(&task.id);

    let stages = Arc::clone(&f.settings);
    let reloaded = TaskService::new(TaskServiceOptions {
        data_dir: f.data_dir.clone(),
        index: TaskIndex::open(&f.dir.0.join("reloaded.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: Some(Arc::new(move |_| stages.lock().unwrap().stages.clone())),
        clock: None,
    });
    reloaded.init().unwrap();
    reloaded.close();
    let kept = reloaded.get_task(&task.id).unwrap();
    assert_eq!(
        (kept.stage.as_str(), kept.state),
        ("implement", TaskState::Running)
    );
    assert_eq!(paths(&kept), small());

    let launches = f.ptys().len();
    f.runner.run.resume_agents();
    f.settle();
    assert!(f.ptys().len() > launches);
    let lane = f.runner.run.lane_for(&f.task(&task.id), Some("1")).unwrap();
    assert_eq!(lane, Some(("implement".to_string(), TaskState::Running)));
}

#[test]
fn drops_the_checks_of_a_round_that_started_again_while_the_conditions_ran() {
    let f = fixture();
    pipeline(
        &f,
        vec![json!({ "id": "plan", "kind": "agent", "gate": "auto" })],
    );
    let (asked_send, asked) = channel::<()>();
    let (finish_send, finish) = channel::<i32>();
    let asked_send = Mutex::new(asked_send);
    let finish = Mutex::new(finish);
    f.set_run_command(move |command, _| {
        if command != NO_REVIEW {
            return Ran {
                code: 0,
                output: String::new(),
            };
        }
        let _ = asked_send.lock().unwrap().send(());
        let code = finish
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or(0);
        Ran {
            code,
            output: String::new(),
        }
    });
    let task = f.implementing("plan");

    let run = Arc::clone(&f.runner.run);
    let id = task.id.clone();
    let report =
        std::thread::spawn(move || run.act(&id, StageAction::Report, &crate::term_input("1")));
    asked.recv_timeout(Duration::from_secs(10)).unwrap();
    act_with(&f, &task.id, StageAction::Reopen, reopen_to("plan"));
    finish_send.send(1).unwrap();
    report.join().unwrap().unwrap();
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("plan", TaskState::Running)
    );
    assert!(now.paths.is_empty());
    let checked = f
        .task_events(&task.id)
        .into_iter()
        .filter(|event| event.kind == "stage.checked" && event.payload["stage"] == "plan")
        .count();
    assert_eq!(checked, 0);
}

#[test]
fn takes_no_path_or_reason_from_the_sender_and_keeps_the_stage_of_the_event() {
    let f = fixture();
    f.set_run_command(|_, _| Ran {
        code: 0,
        output: String::new(),
    });
    let task = at_plan_review(&f, "Export");
    let run = &f.runner.run;

    let input = run
        .with_path(
            &task.id,
            EventInput::new(
                "human",
                "stage.approved",
                json!({ "path": "pr-review", "because": "I say so" }),
            ),
        )
        .unwrap();
    assert_eq!(
        Value::Object(input.payload),
        json!({ "stage": "plan", "because": "No condition held" })
    );
    let asked = run
        .with_path(
            &task.id,
            EventInput::new(
                "agent",
                "agent.asked",
                json!({ "stage": "plan", "path": "small", "because": "I say so" }),
            ),
        )
        .unwrap();
    assert_eq!(Value::Object(asked.payload), json!({ "stage": "plan" }));
}

#[test]
fn checks_a_path_the_task_has_though_its_stages_are_off_now() {
    let f = fixture();
    // Small change first, Review PR, then an Otherwise path.
    let with_small = |enabled: bool| {
        f.set_stages(json!([
            { "id": "context", "kind": "context", "gate": "manual", "output": "context.md" },
            plan(),
            after_plan(enabled, true),
            { "id": "release", "kind": "release", "gate": "manual" },
        ]));
    };
    with_small(true);
    let task = at_plan_review(&f, "Export [small]");
    with_small(false);

    let moved = act(&f, &task.id, StageAction::Approve);

    assert_eq!(moved.stage, "implement");
    assert_eq!(paths(&moved), small());
}
