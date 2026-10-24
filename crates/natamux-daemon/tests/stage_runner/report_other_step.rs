//! `describe('a report from the shell of a stage of another step')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::{Fixture, term_input};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([
        {
            "id": "build",
            "parallel": [
                {
                    "id": "test-spec", "kind": "agent", "gate": "auto",
                    "output": "test-spec.md", "sections": ["Cases"],
                },
                { "id": "implement", "kind": "agent", "gate": "auto" },
            ],
        },
        {
            "id": "agent-review", "kind": "agent", "gate": "auto", "output": "review.md",
            "sections": ["Findings"], "ownShell": true, "agent": "cursor",
        },
        { "id": "release", "kind": "release", "gate": "manual" },
    ]));
    f
}

// Test spec ran in Shell 2 and Implement in Shell 1; the reviewer runs in Shell 8.
fn at_review(f: &Fixture) -> Task {
    let made = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &made.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    let task = f.with_workspace(&made.id);
    let dir = std::path::Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test-spec.md"), "# Cases\n- x\n").unwrap();
    let run = &f.runner.run;
    run.act(&task.id, StageAction::Report, &term_input("2"))
        .unwrap();
    run.act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    let task = f.task(&task.id);
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("agent-review", TaskState::Running)
    );
    task
}

fn report_count(f: &Fixture, id: &str) -> usize {
    f.task_events(id)
        .iter()
        .filter(|event| event.kind == "stage.reported")
        .count()
}

#[test]
fn keeps_implement_running_when_the_reviewer_reports_after_its_send_back() {
    let f = fixture();
    let task = at_review(&f);
    let run = &f.runner.run;
    let input = StageActionInput {
        to: Some("implement".into()),
        reason: Some("Fix it".into()),
        ..term_input("8")
    };
    run.act(&task.id, StageAction::Reopen, &input).unwrap();
    f.settle();
    let lanes: Value = Value::Object(f.task(&task.id).lanes);
    assert_eq!(
        lanes,
        json!({ "test-spec": "done", "implement": "running" })
    );
    let before = report_count(&f, &task.id);

    let error = run
        .act(&task.id, StageAction::Report, &term_input("8"))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("agent-review stage of Terminal 8 does not run now"),
        "{error}"
    );

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("build", TaskState::Running)
    );
    assert_eq!(
        Value::Object(now.lanes),
        json!({ "test-spec": "done", "implement": "running" })
    );
    assert_eq!(report_count(&f, &task.id), before);

    run.act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("agent-review", TaskState::Running)
    );
}
