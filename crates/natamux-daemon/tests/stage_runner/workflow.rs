//! `describe('a stage that runs as a workflow')` of `stage-runner.test.ts`.

use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use natamux_stages::prompts::WORKFLOW_LINE;
use serde_json::{Value, json};

use crate::context::{act_to, change_stage, last_start, report, set_stages};
use crate::support::SIZE;
use crate::{Fixture, expect_submitted, wait_for};

/// Plan waits for approval; the next step is Test spec and Implement.
fn at_plan_review(f: &Fixture, agent: &str) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": agent }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "plan" } }),
    );
    f.with_workspace(&task.id)
}

fn approve(f: &Fixture, id: &str) {
    act_to(f, id, StageAction::Approve, None, None);
}

fn count_line(text: &str) -> usize {
    text.matches(WORKFLOW_LINE).count()
}

fn workflow_of(f: &Fixture, id: &str, stage: &str) -> Option<Value> {
    last_start(f, id, stage).get("workflow").cloned()
}

#[test]
fn asks_the_agent_for_a_workflow_when_implement_starts_after_plan_is_approved() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();

    approve(&f, &task.id);

    let lanes = Value::Object(f.task(&task.id).lanes);
    assert_eq!(
        lanes,
        json!({ "test-spec": "running", "implement": "running" })
    );
    assert_eq!(count_line(&f.launch_script(&task.id, "1")), 1);
    assert_eq!(count_line(&f.launch_script(&task.id, "2")), 0);
    assert_eq!(workflow_of(&f, &task.id, "implement"), Some(json!(true)));
    assert_eq!(workflow_of(&f, &task.id, "test-spec"), None);
}

#[test]
fn sends_the_same_prompt_without_the_workflow_when_it_is_off() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");

    approve(&f, &task.id);

    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("Write the tests first"));
    assert!(!script.contains(WORKFLOW_LINE));
    assert_eq!(workflow_of(&f, &task.id, "implement"), None);
}

#[test]
fn asks_for_it_in_the_prompt_pasted_to_an_agent_that_runs_already() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    f.terminals.run(&task.id, "claude", "1", SIZE).unwrap();
    let pty = f.ptys().pop().unwrap();
    pty.set_program(Some("node"));

    approve(&f, &task.id);

    let pasted = pty.written().concat();
    assert!(pasted.contains("\x1b[200~Task: Export tasks"));
    assert_eq!(count_line(&pasted), 1);
}

#[test]
fn asks_for_it_again_when_the_stage_is_sent_back_but_not_with_failed_checks() {
    let f = Fixture::new();
    let implement = Stage {
        check: Some("make test".into()),
        workflow: Some(true),
        ..Stage::new("implement", StageKind::Agent, Gate::Auto)
    };
    set_stages(
        &f,
        vec![
            implement,
            Stage::new("review", StageKind::Review, Gate::Manual),
        ],
    );
    f.set_run_command(|_, _| Ran {
        code: 1,
        output: "tests failed".into(),
    });
    let made = f.created(json!({ "title": "Export", "agent": "claude-code" }));
    f.tasks.set_workflow(&made.id, "implement", true).unwrap();
    f.with_workspace(&made.id);
    let task = f
        .runner
        .run
        .act(&made.id, StageAction::Start, &Default::default())
        .unwrap();
    let pty = f.pty(0);
    pty.set_program(Some("node"));

    report(&f, &task.id, "1");
    expect_submitted(&pty, "The checks of this stage failed");
    let failed = pty
        .written()
        .into_iter()
        .find(|data| data.contains("The checks of this stage"))
        .unwrap();
    assert!(!failed.contains(WORKFLOW_LINE));

    act_to(
        &f,
        &task.id,
        StageAction::Reopen,
        Some("implement"),
        Some("Fix the dates"),
    );
    wait_for(|| {
        pty.written()
            .iter()
            .any(|data| data.contains("What to change: Fix the dates"))
    });
    expect_submitted(&pty, "Task: Export");
    let again = pty
        .written()
        .into_iter()
        .rfind(|data| data.starts_with("\x1b[200~Task: Export"))
        .unwrap();
    assert!(again.contains(WORKFLOW_LINE));
    assert!(again.contains("What to change: Fix the dates"));
}

#[test]
fn never_asks_an_agent_other_than_claude_code_for_a_workflow() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    f.tasks
        .record_agent_launch(&task.id, "codex", None, None, None)
        .unwrap();

    approve(&f, &task.id);

    assert!(!f.launch_script(&task.id, "1").contains(WORKFLOW_LINE));
    assert_eq!(workflow_of(&f, &task.id, "implement"), None);
}

#[test]
fn asks_for_it_in_plan_when_only_plan_is_ticked_and_not_in_implement() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "spec" } }),
    );
    f.tasks.set_workflow(&task.id, "plan", true).unwrap();
    f.with_workspace(&task.id);

    approve(&f, &task.id);

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("plan", TaskState::Running)
    );
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("plan.md"));
    assert_eq!(count_line(&script), 1);
    assert_eq!(workflow_of(&f, &task.id, "plan"), Some(json!(true)));
}

#[test]
fn starts_without_the_workflow_when_it_was_ticked_and_then_unticked() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    f.tasks.set_workflow(&task.id, "implement", false).unwrap();

    approve(&f, &task.id);

    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("Write the tests first"));
    assert!(!script.contains(WORKFLOW_LINE));
}

#[test]
fn sends_nothing_to_a_running_stage_and_uses_the_choice_at_its_next_start() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    approve(&f, &task.id);
    let written = |f: &Fixture| -> Vec<String> {
        f.ptys().iter().map(|pty| pty.written().concat()).collect()
    };
    let before = written(&f);

    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    f.settle();

    assert_eq!(written(&f), before);
    assert_eq!(f.task(&task.id).lanes["implement"], "running");

    // Each send back pastes the prompt again, with the line once, not once per round.
    for reason in ["Fix the dates", "Fix the times"] {
        act_to(
            &f,
            &task.id,
            StageAction::Reopen,
            Some("implement"),
            Some(reason),
        );
        let sent = || -> Vec<String> {
            f.ptys()
                .iter()
                .flat_map(|pty| pty.written())
                .filter(|data| data.contains(&format!("change: {reason}")))
                .collect()
        };
        wait_for(|| sent().len() == 1);
        assert_eq!(count_line(&sent()[0]), 1);
    }
}

#[test]
fn asks_for_it_again_once_the_task_is_back_on_claude_code() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    f.tasks
        .record_agent_launch(&task.id, "codex", None, None, None)
        .unwrap();
    f.tasks
        .record_agent_launch(&task.id, "claude-code", None, None, None)
        .unwrap();

    approve(&f, &task.id);

    assert_eq!(count_line(&f.launch_script(&task.id, "1")), 1);
}

#[test]
fn drops_the_workflow_when_the_settings_no_longer_offer_it_for_the_stage() {
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-code");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();
    change_stage(&f, "implement", |stage| stage.workflow = Some(false));

    approve(&f, &task.id);

    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("Write the tests first"));
    assert!(!script.contains(WORKFLOW_LINE));
}

// The only test of this binary that sets the custom agents (they are the process's).
#[test]
fn asks_a_claude_code_variant_for_a_workflow_as_claude_code() {
    let qwen = natamux_proto::agents::custom::CustomAgent {
        id: "claude-qwen".into(),
        base: "claude-code".into(),
        command: "cq".into(),
    };
    natamux_agents::custom::set_custom_agents(vec![qwen]);
    let f = Fixture::new();
    let task = at_plan_review(&f, "claude-qwen");
    f.tasks.set_workflow(&task.id, "implement", true).unwrap();

    approve(&f, &task.id);

    let script = f.launch_script(&task.id, "1");
    assert_eq!(count_line(&script), 1, "{script}");
    assert_eq!(workflow_of(&f, &task.id, "implement"), Some(json!(true)));
}
