//! `describe('a stage of your own with its own agent')` of `stage-runner.test.ts`.

use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::prompts::WORKFLOW_LINE;
use serde_json::json;

use crate::context::{
    act_to, any_killed, lane, launches_of, payloads_of, report, running, set_stages,
};
use crate::{Fixture, expect_submitted};

fn docs() -> Stage {
    Stage {
        agent: Some("codex".into()),
        ..Stage::new("write-docs", StageKind::Agent, Gate::Auto)
    }
}

fn auto(id: &str) -> Stage {
    Stage::new(id, StageKind::Agent, Gate::Auto)
}

fn with_stages(f: &Fixture, own: Vec<Stage>) {
    let mut stages = vec![auto("implement")];
    stages.extend(own);
    stages.push(auto("finish"));
    stages.push(Stage::new("release", StageKind::Release, Gate::Manual));
    set_stages(f, stages);
}

/// Implement ran in Shell 1 with the task's agent and is done, so the stage of your own starts.
fn after_implement(f: &Fixture) -> Task {
    let task = f.implementing("implement");
    report(f, &task.id, "1");
    f.settle();
    f.task(&task.id)
}

fn launches(f: &Fixture, id: &str) -> Vec<serde_json::Value> {
    payloads_of(f, id, "agent.launched")
}

#[test]
fn starts_its_own_agent_in_shell_7_with_its_words_and_keeps_the_task_agent_in_shell_1() {
    let f = Fixture::new();
    with_stages(
        &f,
        vec![Stage {
            args: Some("--model gpt-5".into()),
            ..docs()
        }],
    );
    let task = after_implement(&f);

    assert_eq!(
        (task.stage.as_str(), task.state),
        ("write-docs", TaskState::Running)
    );
    assert_eq!(task.agent.as_deref(), Some("claude-code"));
    assert!(!any_killed(&f));
    assert!(f.launch_script(&task.id, "1").starts_with("claude "));
    assert!(f.launch_script(&task.id, "7").starts_with("codex "));
    assert!(f.launch_script(&task.id, "7").contains("--model gpt-5"));
    let last = launches(&f, &task.id).pop().unwrap();
    assert_eq!(
        (&last["agent"], &last["stage"]),
        (&json!("codex"), &json!("write-docs"))
    );
    assert_eq!(lane(&f, &task, "7"), running("write-docs"));
    assert_eq!(lane(&f, &task, "1"), None);
}

#[test]
fn gives_the_next_stage_to_the_same_task_agent_in_shell_1_with_no_new_launch() {
    let f = Fixture::new();
    with_stages(&f, vec![docs()]);
    let task = after_implement(&f);

    report(&f, &task.id, "7");
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("finish", TaskState::Running)
    );
    assert!(!any_killed(&f));
    expect_submitted(&f.pty(0), "Task: Export");
    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 1);
}

#[test]
fn gives_its_prompt_to_its_own_agent_when_the_stage_runs_again() {
    let f = Fixture::new();
    with_stages(&f, vec![docs()]);
    let task = after_implement(&f);
    let shell = f.ptys().pop().unwrap();
    report(&f, &task.id, "7");
    f.settle();

    act_to(&f, &task.id, StageAction::Reopen, Some("write-docs"), None);

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("write-docs", TaskState::Running)
    );
    assert!(!shell.killed());
    expect_submitted(&shell, "Task: Export");
    assert_eq!(launches_of(&f, &task.id, "codex").len(), 1);
}

#[test]
fn runs_two_stages_with_other_agents_in_two_shells_and_closes_neither() {
    let f = Fixture::new();
    let check = Stage {
        agent: Some("cursor".into()),
        ..auto("check-docs")
    };
    with_stages(&f, vec![docs(), check]);
    let task = after_implement(&f);

    report(&f, &task.id, "7");
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("check-docs", TaskState::Running)
    );
    assert!(!any_killed(&f));
    assert!(f.launch_script(&task.id, "7").starts_with("codex "));
    let last = launches(&f, &task.id).pop().unwrap();
    assert_eq!(
        (&last["agent"], &last["stage"]),
        (&json!("cursor"), &json!("check-docs"))
    );
    assert_eq!(lane(&f, &now, "8"), running("check-docs"));
}

#[test]
fn starts_its_own_agent_again_in_its_shell_when_the_daemon_starts() {
    let f = Fixture::new();
    with_stages(&f, vec![docs()]);
    let task = after_implement(&f);
    f.terminals.close(&task.id);

    f.runner.run.resume_agents();
    f.settle();

    let codex = launches_of(&f, &task.id, "codex");
    assert_eq!(codex.len(), 2);
    assert_eq!(codex.last().unwrap()["stage"], "write-docs");
    assert!(f.launch_script(&task.id, "7").starts_with("codex "));
    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn keeps_the_claude_code_session_of_shell_1_across_a_stage_with_its_own_agent() {
    let f = Fixture::new();
    with_stages(&f, vec![docs()]);
    let task = f.implementing("implement");
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.waiting", "payload": { "stage": "implement", "sessionId": "s-1" } }),
    );
    report(&f, &task.id, "1");
    f.settle();
    assert_eq!(f.task(&task.id).stage, "write-docs");

    // Back to Implement: Claude Code in Shell 1 still runs and gets the prompt.
    act_to(&f, &task.id, StageAction::Reopen, Some("implement"), None);
    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 1);

    // After a restart it goes on with its session; the agent of Write docs does not count.
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.finished", "payload": { "stage": "implement" } }),
    );
    f.terminals.close(&task.id);
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "agent.resumed", "payload": {} }),
    );

    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 2);
    assert!(f.launch_script(&task.id, "1").contains("--resume 's-1'"));
}

#[test]
fn asks_no_agent_but_claude_code_for_a_workflow() {
    let f = Fixture::new();
    with_stages(
        &f,
        vec![Stage {
            workflow: Some(true),
            ..docs()
        }],
    );
    let task = f.implementing("implement");
    f.tasks.set_workflow(&task.id, "write-docs", true).unwrap();

    report(&f, &task.id, "1");
    f.settle();

    let script = f.launch_script(&task.id, "7");
    assert!(script.starts_with("codex "));
    assert!(!script.contains(WORKFLOW_LINE));
}
