//! The second half of `describe('a stage that ends its shell')`: groups, commands and CI.

use natamux_daemon::stages::act::StageActionInput;
use natamux_daemon::terminals::shell_files::read_ended_shells;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::ci::CiResult;
use serde_json::{Value, json};

use super::{act, agent_stage, any_killed, assert_at, input, killed, report, stage, terminals};
use crate::{Fixture, wait_for};

/// The build group: Implement keeps Shell 1, Test spec runs in Shell 2.
fn building(f: &Fixture, end_shell: &str) -> Task {
    let steps = default_pipeline()
        .into_iter()
        .map(|step| match step {
            Step::Group(mut group) => {
                for stage in &mut group.parallel {
                    if stage.id == end_shell {
                        stage.end_shell = Some(true);
                    }
                }
                Step::Group(group)
            }
            other => other,
        })
        .collect();
    f.set_settings(ProjectSettings {
        stages: steps,
        ..ProjectSettings::default()
    });
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    f.with_workspace(&task.id);
    assert_eq!(f.ptys().len(), 2);
    f.task(&task.id)
}

fn lanes(f: &Fixture, id: &str) -> Value {
    Value::Object(f.task(id).lanes)
}

#[test]
fn closes_only_the_shell_of_the_lane_that_is_done_in_a_group() {
    let f = Fixture::new();
    let task = building(&f, "test-spec");

    // Test spec waits for approval after a report; a skip makes its lane done at once.
    let skip = StageActionInput {
        stage: Some("test-spec".into()),
        ..StageActionInput::default()
    };
    act(&f, &task.id, StageAction::Skip, &skip);

    assert_eq!(
        lanes(&f, &task.id),
        json!({ "test-spec": "done", "implement": "running" })
    );
    assert_eq!(f.ptys().iter().filter(|pty| pty.killed()).count(), 1);
    assert_eq!(terminals(&f, &task.id), ["1"]);
}

#[test]
fn keeps_shell_1_when_the_lane_there_is_done_in_a_group() {
    let f = Fixture::new();
    let task = building(&f, "implement");

    report(&f, &task.id, "1");

    assert_eq!(
        lanes(&f, &task.id),
        json!({ "test-spec": "running", "implement": "done" })
    );
    assert!(!any_killed(&f));
    assert_eq!(terminals(&f, &task.id), ["1", "2"]);
}

fn with_testing_command(f: &Fixture) {
    let mut testing = stage("testing", StageKind::Testing, Gate::Auto);
    testing.command = Some("make test".into());
    testing.end_shell = Some(true);
    f.set_settings(ProjectSettings {
        stages: vec![
            Step::Stage(agent_stage("implement")),
            Step::Stage(testing),
            Step::Stage(agent_stage("finish")),
            Step::Stage(stage("release", StageKind::Release, Gate::Manual)),
        ],
        ..ProjectSettings::default()
    });
}

#[test]
fn closes_the_shell_of_a_command_when_the_command_passes() {
    let f = Fixture::new();
    with_testing_command(&f);
    let task = f.implementing("implement");
    report(&f, &task.id, "1");
    assert_eq!(f.task(&task.id).stage, "testing");
    assert_eq!(terminals(&f, &task.id), ["1", "2"]);

    let pass = StageActionInput {
        stage: Some("testing".into()),
        ..StageActionInput::default()
    };
    act(&f, &task.id, StageAction::Pass, &pass);

    assert_eq!(f.task(&task.id).stage, "finish");
    assert!(killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1"]);
}

#[test]
fn closes_the_shell_of_a_command_when_the_task_is_sent_back_from_its_stage() {
    let f = Fixture::new();
    with_testing_command(&f);
    let task = f.implementing("implement");
    report(&f, &task.id, "1");
    assert_eq!(terminals(&f, &task.id), ["1", "2"]);

    let back = input(None, Some("implement"), Some("Fix the dates"));
    act(&f, &task.id, StageAction::Reopen, &back);

    assert_at(&f, &task.id, "implement", TaskState::Running);
    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
    assert_eq!(terminals(&f, &task.id), ["1"]);
}

fn last_event(f: &Fixture, id: &str, kind: &str) -> Option<Value> {
    f.events(id)
        .into_iter()
        .rfind(|(each, _)| each == kind)
        .map(|(_, payload)| payload)
}

/// A testing stage of your own with its own agent in Shell 7 fails on CI, so the task goes back
/// to Implement.
fn failed_on_ci(f: &Fixture, end_shell: bool) -> Task {
    let mut smoke = stage("smoke", StageKind::Testing, Gate::Auto);
    smoke.agent = Some("codex".into());
    smoke.ci = Some(true);
    smoke.end_shell = Some(end_shell);
    f.set_settings(ProjectSettings {
        stages: vec![
            Step::Stage(agent_stage("implement")),
            Step::Stage(smoke),
            Step::Stage(stage("release", StageKind::Release, Gate::Manual)),
        ],
        ..ProjectSettings::default()
    });
    let task = f.implementing("implement");
    f.tasks.set_ci(&task.id, "smoke", true).unwrap();
    report(f, &task.id, "1");
    assert_at(f, &task.id, "smoke", TaskState::Running);
    assert_eq!(terminals(f, &task.id), ["1", "7"]);

    f.runner
        .run
        .act(&task.id, StageAction::Report, &input(Some("7"), None, None))
        .unwrap();
    f.ci.finish(CiResult {
        passed: false,
        reasons: vec!["CI E2E failure: u".into()],
        passes: vec![],
        final_: false,
        logs: None,
    });
    f.settle();
    let payload = last_event(f, &task.id, "stage.reopened").unwrap();
    assert_eq!(
        (&payload["from"], &payload["ci"]),
        (&json!("smoke"), &json!(true))
    );
    assert_at(f, &task.id, "implement", TaskState::Running);
    task
}

#[test]
fn closes_its_shell_when_ci_sends_the_task_back_from_it() {
    let f = Fixture::new();
    let task = failed_on_ci(&f, true);

    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
    assert_eq!(terminals(&f, &task.id), ["1"]);
}

#[test]
fn keeps_the_shell_of_a_stage_without_it_when_ci_sends_the_task_back() {
    let f = Fixture::new();
    let task = failed_on_ci(&f, false);

    assert!(!any_killed(&f));
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);
}

#[test]
fn leaves_the_last_stage_to_the_finish_of_the_task_which_keeps_the_screens() {
    let f = Fixture::new();
    let mut only = agent_stage("implement");
    only.end_shell = Some(true);
    f.set_settings(ProjectSettings {
        stages: vec![Step::Stage(only)],
        ..ProjectSettings::default()
    });
    let task = f.implementing("implement");
    f.pty(0).emit("last words of the agent\r\n");
    f.terminals.settled(&task.id, "1");

    report(&f, &task.id, "1");

    assert_eq!(f.task(&task.id).state, TaskState::Done);
    wait_for(|| killed(&f, 0));
    let shells = read_ended_shells(&f.data_dir, &task.id);
    assert_eq!(shells[0].name, "1");
    assert_eq!(shells[0].stage.as_deref(), Some("implement"));
    assert!(shells[0].text.contains("last words of the agent"));
}
