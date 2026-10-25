//! `describe('agent stages')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::context::{Blocked, act_in_background, payloads_of, report, set_stages, write_working};
use crate::{Fixture, expect_submitted, wait_for};

fn at_spec(f: &Fixture) -> Task {
    f.created(json!({ "title": "Export tasks", "agent": "claude-code" }))
}

fn report_from(terminal: &str) -> StageActionInput {
    StageActionInput {
        terminal: Some(terminal.into()),
        ..StageActionInput::default()
    }
}

#[test]
fn waits_for_a_workspace_then_starts_the_agent_with_the_stage_prompt() {
    let f = Fixture::new();
    let waiting = at_spec(&f);
    assert_eq!(
        (waiting.stage.as_str(), waiting.state),
        ("spec", TaskState::Ready)
    );
    assert!(f.ptys().is_empty());

    let task = f.with_workspace(&waiting.id);
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Running)
    );
    assert_eq!(f.ptys().len(), 1);
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("spec.md"));
    assert!(script.contains("natamux done"));
    let context =
        std::path::Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task/context.md");
    assert!(
        std::fs::read_to_string(context)
            .unwrap()
            .contains("Export tasks")
    );
}

#[test]
fn gives_an_extra_shell_no_stage_so_an_agent_started_there_by_hand_leaves_the_task_alone() {
    let f = Fixture::new();
    let id = at_spec(&f).id;
    let task = f.with_workspace(&id);
    let run = &f.runner.run;
    let spec = Some(("spec".to_string(), TaskState::Running));
    assert_eq!(run.lane_for(&task, Some("1")).unwrap(), spec);
    assert_eq!(run.lane_for(&task, None).unwrap(), spec);
    assert_eq!(run.lane_for(&task, Some("2")).unwrap(), None);
}

#[test]
fn sends_failed_checks_back_to_the_agent_once_then_waits_for_you() {
    let f = Fixture::new();
    let id = at_spec(&f).id;
    let task = f.with_workspace(&id);
    let run = &f.runner.run;

    run.act(&task.id, StageAction::Report, &report_from("1"))
        .unwrap();
    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(f.pty(0).written().concat().contains("spec.md is missing"));
    expect_submitted(&f.pty(0), "The checks of this stage failed");

    run.act(&task.id, StageAction::Report, &report_from("1"))
        .unwrap();
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
}

fn only_implement(f: &Fixture, name: Option<&str>, check: &str) {
    use natamux_proto::stages::schema::{Gate, Stage, StageKind};
    let stage = Stage {
        name: name.map(Into::into),
        check: Some(check.into()),
        ..Stage::new("implement", StageKind::Agent, Gate::Manual)
    };
    set_stages(f, vec![stage]);
}

// The agent runs `natamux done` while a person clicks Mark as done.
#[test]
fn runs_the_checks_once_for_two_reports_at_the_same_time_and_keeps_the_agent_its_try() {
    let f = Fixture::new();
    only_implement(&f, None, "make test");
    let blocked = Blocked::new(&f);
    let task = f.implementing("implement");

    let first = act_in_background(&f, &task.id, StageAction::Report, report_from("1"));
    let second = act_in_background(
        &f,
        &task.id,
        StageAction::Report,
        StageActionInput::default(),
    );
    blocked.wait_for_calls(1);
    // Node sends both in one turn and the first always runs the check. Here either may: the
    // other meets the running check and returns, before the check ends.
    wait_for(|| first.is_finished() || second.is_finished());
    blocked.finish(1);
    first.join().unwrap();
    second.join().unwrap();

    let failures = payloads_of(&f, &task.id, "stage.check-failed");
    assert_eq!(
        failures
            .iter()
            .map(|payload| &payload["final"])
            .collect::<Vec<_>>(),
        [&json!(false)]
    );
    assert_eq!(f.task(&task.id).state, TaskState::Running);
    expect_submitted(&f.pty(0), "The checks of this stage failed");

    // Once the first check ended, a report checks again.
    let again = act_in_background(&f, &task.id, StageAction::Report, report_from("1"));
    blocked.wait_for_calls(2);
    blocked.finish(1);
    again.join().unwrap();
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
}

#[test]
fn shows_the_local_checks_as_a_background_job_while_they_run() {
    let f = Fixture::new();
    only_implement(&f, Some("Build"), "x");
    let blocked = Blocked::new(&f);
    let task = f.implementing("implement");
    let before = f.job_lists().len();

    let report = act_in_background(&f, &task.id, StageAction::Report, report_from("1"));
    // The job shows before the output and git checks end, so wait for the command itself.
    blocked.wait_for_calls(1);
    let lists = f.job_lists()[before..].to_vec();
    assert_eq!(lists.len(), 1);
    let [job] = lists[0].as_slice() else {
        panic!("one job expected");
    };
    assert_eq!(job.kind, "stage-check");
    assert_eq!(job.label, "Checking Build");
    assert_eq!(job.task_id.as_deref(), Some(task.id.as_str()));
    blocked.finish(0);
    report.join().unwrap();
    assert!(f.job_lists().last().unwrap().is_empty());
}

const SPEC_SECTIONS: &str =
    "# Goal\nx\n# Scope\nx\n# Out of scope\nx\n# Acceptance criteria\nx\n# Code map\nx\n";

#[test]
fn waits_for_approval_when_the_output_has_its_sections_and_drops_the_turn_end_after() {
    let f = Fixture::new();
    let id = at_spec(&f).id;
    let task = f.with_workspace(&id);
    write_working(&task, "spec.md", SPEC_SECTIONS);

    report(&f, &task.id, "1");
    let now = f.task(&task.id);
    assert_eq!((now.stage.as_str(), now.state), ("spec", TaskState::Review));
    assert!(f.runner.run.take_report(&task.id, "1"));
    assert!(!f.runner.run.take_report(&task.id, "1"));
}

#[test]
fn names_the_missing_sections() {
    let f = Fixture::new();
    let id = at_spec(&f).id;
    let task = f.with_workspace(&id);
    write_working(&task, "spec.md", "# Goal\nx\n");

    report(&f, &task.id, "1");
    let (kind, payload) = f.events(&task.id).pop().unwrap();
    assert_eq!(kind, "stage.check-failed");
    assert_eq!(
        payload["reasons"],
        json!(["spec.md has no Scope, Out of scope, Acceptance criteria, Code map section"])
    );
}
