//! `describe('a stage whose command ran')` and `describe('a stage that waits')` of
//! `stage-runner.test.ts`, both nested in 'when the daemon starts'.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::schema::{Gate, Stage, StageGroup, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::Fixture;
use crate::resume_restart::*;

fn resume_all(f: &Fixture) {
    f.runner.run.resume_agents();
    f.settle();
}

fn script_of(f: &Fixture, id: &str) -> std::path::PathBuf {
    f.data_dir.join(format!("stage-command-{id}.sh"))
}

fn sh_line(f: &Fixture, id: &str) -> String {
    format!("sh '{}'\r", script_of(f, id).display())
}

/// The daemon stops, which ends every shell, and starts again.
fn restart(f: &mut Fixture) {
    f.runner.stop();
    for task in f.tasks.list_tasks(None).unwrap() {
        f.terminals.close(&task.id);
    }
    f.runner = f.make_runner(None);
    f.runner.start();
}

// The test command runs in the task's workspace.
fn testing_running(f: &Fixture, agent: Value) -> Task {
    with_test_command(f);
    let task = f.created(json!({ "title": "Export tasks", "agent": agent }));
    ev(
        f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "testing" }),
    );
    f.with_workspace(&task.id);
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("testing", TaskState::Running)
    );
    now
}

// The test command ran when the daemon stopped: its shell is gone, the stage still runs.
fn testing_after_restart(f: &mut Fixture, agent: Value) -> Task {
    let task = testing_running(f, agent);
    restart(f);
    f.task(&task.id)
}

fn last_two(f: &Fixture, id: &str) -> Vec<String> {
    let types = event_types(f, id);
    types[types.len() - 2..].to_vec()
}

fn written_of_last(f: &Fixture) -> Vec<String> {
    last_pty(f).written()
}

#[test]
fn runs_the_command_again_in_a_new_shell_with_no_agent() {
    let mut f = Fixture::new();
    let task = testing_after_restart(&mut f, json!("claude-code"));
    let shells_before = f.ptys().len();

    resume_all(&f);

    let events = f.tasks.list_events(&task.id).unwrap();
    let tail = &events[events.len() - 2..];
    assert!(tail[0].accepted);
    assert_eq!(tail[0].event.source, "system");
    assert_eq!(tail[0].event.kind, "agent.stopped");
    assert_eq!(tail[0].event.payload["stage"], "testing");
    assert!(tail[1].accepted);
    assert_eq!(tail[1].event.kind, "stage.started");
    assert_eq!(tail[1].event.payload["stage"], "testing");
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("testing", TaskState::Running)
    );
    assert_eq!(f.ptys().len(), shells_before + 1);
    assert_eq!(shells(&f, &task.id), ["2"]);
    assert_eq!(written_of_last(&f), [sh_line(&f, &task.id)]);
    let script = std::fs::read_to_string(script_of(&f, &task.id)).unwrap();
    assert!(script.contains("make test"));
}

#[test]
fn runs_the_command_again_for_a_task_with_no_agent_or_one_natamux_cannot_start() {
    for agent in [Value::Null, json!("aider")] {
        let mut f = Fixture::new();
        let task = testing_after_restart(&mut f, agent.clone());

        resume_all(&f);

        assert_eq!(
            last_two(&f, &task.id),
            ["agent.stopped", "stage.started"],
            "{agent}"
        );
        assert_eq!(f.task(&task.id).state, TaskState::Running, "{agent}");
        assert_eq!(shells(&f, &task.id), ["2"], "{agent}");
        assert_eq!(written_of_last(&f), [sh_line(&f, &task.id)], "{agent}");
    }
}

#[test]
fn runs_the_command_again_for_each_task_in_its_own_shell() {
    let mut f = Fixture::new();
    let first = testing_running(&f, json!("claude-code"));
    let second = testing_running(&f, Value::Null);
    restart(&mut f);
    let shells_before = f.ptys().len();

    resume_all(&f);

    for task in [&first, &second] {
        assert_eq!(last_two(&f, &task.id), ["agent.stopped", "stage.started"]);
        assert_eq!(f.task(&task.id).state, TaskState::Running);
        assert_eq!(shells(&f, &task.id), ["2"]);
        let script = std::fs::read_to_string(script_of(&f, &task.id)).unwrap();
        assert!(script.contains("make test"));
    }
    let mut lines: Vec<String> = f.ptys()[shells_before..]
        .iter()
        .flat_map(|pty| pty.written())
        .collect();
    let mut expected: Vec<String> = [&first, &second]
        .iter()
        .map(|t| sh_line(&f, &t.id))
        .collect();
    lines.sort();
    expected.sort();
    assert_eq!(lines, expected);
}

#[test]
fn runs_the_command_again_when_another_task_is_at_a_stage_the_settings_lost() {
    let mut f = Fixture::new();
    // Node mocks `listTasks` to show a stage that is gone; here the settings lose the stage.
    let lost = f.created(json!({ "title": "Old task", "agent": "claude-code" }));
    let task = testing_after_restart(&mut f, json!("claude-code"));
    let mut settings = f.settings.lock().unwrap().clone();
    settings.stages.retain(|step| step.id() != "spec");
    f.set_settings(settings);
    assert_eq!(f.task(&lost.id).stage, "spec");

    f.runner.run.resume_agents();
    f.settle();

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert_eq!(shells(&f, &task.id), ["2"]);
    assert_eq!(written_of_last(&f), [sh_line(&f, &task.id)]);
}

#[test]
fn leaves_alone_a_stage_that_waits_for_ci() {
    let mut f = Fixture::new();
    let task = testing_after_restart(&mut f, json!("claude-code"));
    let payload = json!({ "stage": "testing", "branch": "ci" });
    f.tasks
        .record_event(&task.id, "stage.ci-started", payload, "system")
        .unwrap();
    let before = event_count(&f, &task.id);

    resume_all(&f);

    assert_eq!(event_count(&f, &task.id), before);
    assert!(shells(&f, &task.id).is_empty());
    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn runs_the_command_again_once_its_ci_check_has_a_result() {
    let mut f = Fixture::new();
    let task = testing_after_restart(&mut f, json!("claude-code"));
    let started = json!({ "stage": "testing", "branch": "ci" });
    f.tasks
        .record_event(&task.id, "stage.ci-started", started, "system")
        .unwrap();
    let failed = json!({ "stage": "testing", "reasons": ["CI failed"] });
    f.tasks
        .record_event(&task.id, "stage.check-failed", failed, "system")
        .unwrap();

    resume_all(&f);

    assert_eq!(last_two(&f, &task.id), ["agent.stopped", "stage.started"]);
    assert_eq!(shells(&f, &task.id), ["2"]);
}

// The settings schema keeps a testing stage out of a group, but the runner reads lanes the same
// way for every kind.
#[test]
fn runs_the_command_of_a_lane_again_and_resumes_the_agent_of_the_other_lane() {
    let mut f = Fixture::new();
    let implement = Stage::new("implement", StageKind::Agent, Gate::Auto);
    let mut testing = Stage::new("testing", StageKind::Testing, Gate::Auto);
    testing.command = Some("make test".into());
    f.set_settings(ProjectSettings {
        stages: vec![
            Step::Group(StageGroup {
                id: "build".into(),
                name: Some("Build".into()),
                parallel: vec![implement, testing],
            }),
            Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
        ],
        ..ProjectSettings::default()
    });
    let made = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.with_workspace(&made.id);
    let task = f
        .runner
        .run
        .act(&made.id, StageAction::Start, &StageActionInput::default())
        .unwrap();
    assert_eq!(
        Value::Object(task.lanes),
        json!({ "implement": "running", "testing": "running" })
    );
    let payload = json!({ "stage": "implement", "sessionId": "s-1" });
    ev(&f, &made.id, "agent", "agent.waiting", payload);
    restart(&mut f);

    resume_all(&f);

    let payloads_of = |kind: &str| -> Vec<Value> {
        let events = f.tasks.list_events(&made.id).unwrap();
        let of_kind = events
            .into_iter()
            .filter(|stored| stored.event.kind == kind);
        of_kind
            .map(|stored| Value::Object(stored.event.payload))
            .collect()
    };
    assert_eq!(
        payloads_of("agent.stopped"),
        [json!({ "stage": "testing" })]
    );
    let starts = payloads_of("stage.started");
    assert_eq!(
        starts
            .iter()
            .filter(|payload| payload["stage"] == "testing")
            .count(),
        2
    );
    let both = json!({ "implement": "running", "testing": "running" });
    assert_eq!(lanes_of(&f, &made.id), both);
    assert!(script_has(&f, &made.id, "1", "--resume 's-1'"));
    assert_eq!(shells(&f, &made.id), ["1", "2"]);
    assert_eq!(written_of_last(&f), [sh_line(&f, &made.id)]);
}

#[test]
fn leaves_alone_a_ready_needs_input_review_or_failed_stage() {
    for to in ["ready", "needs-input", "review", "failed"] {
        let mut f = Fixture::new();
        let task = testing_after_restart(&mut f, json!("claude-code"));
        ev(
            &f,
            &task.id,
            "human",
            "state.overridden",
            json!({ "to": to }),
        );
        let before = event_count(&f, &task.id);

        resume_all(&f);

        assert_eq!(event_count(&f, &task.id), before, "{to}");
        assert!(shells(&f, &task.id).is_empty(), "{to}");
        assert_eq!(f.task(&task.id).state, state_of(to), "{to}");
    }
}

#[test]
fn runs_the_command_once_the_task_gets_its_workspace() {
    let mut f = Fixture::new();
    with_test_command(&f);
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "testing" }),
    );
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "running" }),
    );
    f.runner.stop();
    f.runner = f.make_runner(None);
    f.runner.start();

    resume_all(&f);

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("testing", TaskState::Ready)
    );
    assert!(shells(&f, &task.id).is_empty());

    f.with_workspace(&task.id);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert_eq!(shells(&f, &task.id), ["2"]);
}

// 'a stage that waits': the daemon stops, which ends every shell, and starts again.
fn restart_and_resume(f: &mut Fixture) {
    restart(f);
    resume_all(f);
}

#[test]
fn starts_once_the_tasks_it_depends_on_are_done() {
    let mut f = Fixture::new();
    let first = f.created(json!({ "title": "Schema" }));
    let id = f
        .created(
            json!({ "title": "Export tasks", "agent": "claude-code", "dependsOn": [first.id] }),
        )
        .id;
    let task = f.with_workspace(&id);
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Ready)
    );
    restart_and_resume(&mut f);

    ev(
        &f,
        &first.id,
        "human",
        "state.overridden",
        json!({ "to": "done" }),
    );

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn starts_once_the_task_gets_its_workspace() {
    let mut f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Ready)
    );
    restart_and_resume(&mut f);

    f.with_workspace(&task.id);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn starts_no_agent_in_a_task_left_to_a_person() {
    let mut f = Fixture::new();
    let task = f.created(json!({ "title": "By hand" }));
    f.runner.run.leave_to_person(&task.id).unwrap();
    f.with_workspace(&task.id);
    restart_and_resume(&mut f);

    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "spec" }),
    );

    assert_eq!(f.task(&task.id).state, TaskState::Ready);
    assert!(shells(&f, &task.id).is_empty());
}

#[test]
fn starts_no_stages_own_agent_in_a_task_left_to_a_person() {
    let mut f = Fixture::new();
    with_own_agent(&f);
    let task = f.created(json!({ "title": "By hand" }));
    f.runner.run.leave_to_person(&task.id).unwrap();
    f.with_workspace(&task.id);
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "build" }),
    );
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "running" }),
    );
    let both = json!({ "implement": "running", "test-spec": "running" });
    assert_eq!(lanes_of(&f, &task.id), both);

    restart_and_resume(&mut f);

    assert!(shells(&f, &task.id).is_empty());
}

#[test]
fn is_no_longer_left_to_a_person_after_start_or_start_agent() {
    for by_start in [true, false] {
        let f = Fixture::new();
        let task = f.created(json!({ "title": "By hand", "agent": "claude-code" }));
        f.runner.run.leave_to_person(&task.id).unwrap();
        assert_eq!(f.task(&task.id).without_agent, Some(true));
        f.with_workspace(&task.id);
        assert!(shells(&f, &task.id).is_empty());

        if by_start {
            let input = StageActionInput::default();
            f.runner
                .run
                .act(&task.id, StageAction::Start, &input)
                .unwrap();
        } else {
            f.runner
                .run
                .start_by_hand(&task.id, "claude-code", None, None)
                .unwrap();
        }
        f.settle();

        assert_ne!(f.task(&task.id).without_agent, Some(true), "{by_start}");
        assert_eq!(f.task(&task.id).state, TaskState::Running, "{by_start}");
    }
}
