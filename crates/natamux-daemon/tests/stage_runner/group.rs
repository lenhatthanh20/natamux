//! `describe('a group')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_daemon::stages::errors::StageError;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::{Fixture, run_git, term_input};

fn at_build(f: &Fixture) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    f.with_workspace(&task.id)
}

fn running(stage: &str) -> Option<(String, TaskState)> {
    Some((stage.to_string(), TaskState::Running))
}

#[test]
fn starts_each_stage_in_its_own_terminal_implement_in_the_first_test_spec_in_the_next() {
    let f = Fixture::new();
    let task = at_build(&f);

    assert_eq!(
        Value::Object(task.lanes.clone()),
        json!({ "test-spec": "running", "implement": "running" })
    );
    assert!(
        f.launch_script(&task.id, "1")
            .contains("Write the tests first")
    );
    assert!(f.launch_script(&task.id, "1").contains("Do not commit"));
    assert!(f.launch_script(&task.id, "2").contains("test-spec.md"));
    let run = &f.runner.run;
    assert_eq!(
        run.lane_for(&task, Some("2")).unwrap(),
        running("test-spec")
    );
    assert_eq!(
        run.lane_for(&task, Some("1")).unwrap(),
        running("implement")
    );
    // An agent the user started in another shell works on no stage of the group.
    assert_eq!(run.lane_for(&task, Some("3")).unwrap(), None);
}

#[test]
fn starts_the_agent_and_the_words_of_a_stage_in_a_shell_of_its_own_and_keeps_the_task_agent() {
    let f = Fixture::new();
    // Test spec starts Codex with its own words, beside Claude Code in Shell 1.
    let mut settings = natamux_proto::projects::repo::ProjectSettings {
        stages: natamux_proto::stages::catalog::default_pipeline(),
        ..Default::default()
    };
    for step in &mut settings.stages {
        if let natamux_proto::stages::schema::Step::Group(group) = step {
            for stage in &mut group.parallel {
                if stage.id == "test-spec" {
                    stage.agent = Some("codex".into());
                    stage.args = Some("--model gpt-5".into());
                }
            }
        }
    }
    f.set_settings(settings);
    let task = at_build(&f);

    assert!(f.launch_script(&task.id, "1").starts_with("claude "));
    assert!(f.launch_script(&task.id, "9").starts_with("codex "));
    assert!(f.launch_script(&task.id, "9").contains("--model gpt-5"));
    assert_eq!(
        f.runner.run.lane_for(&task, Some("9")).unwrap(),
        running("test-spec")
    );
    assert_eq!(f.task(&task.id).agent.as_deref(), Some("claude-code"));
    let launched = f.task_events(&task.id).into_iter().any(|event| {
        event.kind == "agent.launched"
            && event.payload.get("agent") == Some(&json!("codex"))
            && event.payload.get("stage") == Some(&json!("test-spec"))
    });
    assert!(launched);
}

#[test]
fn finishes_implement_with_changes_that_are_not_committed_and_leaves_them_so() {
    let f = Fixture::new();
    let task = at_build(&f);
    let workspace = task.workspace.clone().unwrap();
    std::fs::write(
        std::path::Path::new(&workspace).join("export.ts"),
        "export {}\n",
    )
    .unwrap();

    f.runner
        .run
        .act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    assert_eq!(
        Value::Object(f.task(&task.id).lanes),
        json!({ "test-spec": "running", "implement": "done" })
    );
    let last = f.task_events(&task.id).pop().unwrap();
    assert_eq!(last.payload["passes"], json!([]));
    assert!(
        run_git(std::path::Path::new(&workspace), &["status", "--porcelain"]).contains("export.ts")
    );
}

#[test]
fn asks_for_the_stage_name_when_two_stages_could_be_meant() {
    let f = Fixture::new();
    let task = at_build(&f);
    let error = f
        .runner
        .run
        .act(&task.id, StageAction::Fail, &StageActionInput::default())
        .unwrap_err();
    assert!(matches!(error, StageError::Action(_)), "{error}");
    let input = StageActionInput {
        stage: Some("test-spec".into()),
        reason: Some("Stuck".into()),
        ..Default::default()
    };
    let failed = f
        .runner
        .run
        .act(&task.id, StageAction::Fail, &input)
        .unwrap();
    assert_eq!(failed.lanes["test-spec"], "failed");
}
