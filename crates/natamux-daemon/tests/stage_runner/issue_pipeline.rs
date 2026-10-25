//! `describe('the pipeline of the issue: the task agent in Shell 1, other agents beside it')` of
//! `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::context::{
    any_killed, change_stage, from_terminal, lane, launches_of, payloads_of, report as report_from,
    running, set_stages, write_working,
};
use crate::{Fixture, expect_submitted, expect_written};

fn stage(id: &str, agent: Option<&str>, output: Option<&str>, own_shell: bool) -> Stage {
    Stage {
        agent: agent.map(Into::into),
        output: output.map(Into::into),
        own_shell: own_shell.then_some(true),
        ..Stage::new(id, StageKind::Agent, Gate::Auto)
    }
}

/// The pipeline of the issue, the `beforeEach` of the group.
fn fixture() -> Fixture {
    let f = Fixture::new();
    set_stages(
        &f,
        vec![
            stage("spec", None, None, false),
            stage("test-spec", Some("copilot"), Some("test-spec.md"), true),
            stage("implement", None, None, false),
            stage("agent-review", Some("cursor"), Some("review.md"), true),
            stage("ci-watch", Some("copilot"), None, false),
            stage("merge-pr", Some("copilot"), None, false),
            Stage::new("release", StageKind::Release, Gate::Manual),
        ],
    );
    f
}

fn report(f: &Fixture, task: &Task, terminal: &str) -> Task {
    report_from(f, &task.id, terminal);
    f.settle();
    f.task(&task.id)
}

fn reopen(f: &Fixture, id: &str, terminal: &str, to: Option<&str>, reason: &str) {
    let input = StageActionInput {
        terminal: Some(terminal.into()),
        to: to.map(Into::into),
        reason: Some(reason.into()),
        ..StageActionInput::default()
    };
    f.runner.run.act(id, StageAction::Reopen, &input).unwrap();
    f.settle();
}

/// Spec and Test spec are done; Implement runs in Shell 1, Review is next.
fn at_implement(f: &Fixture) -> Task {
    let task = f.implementing("spec");
    report(f, &task, "1");
    write_working(&task, "test-spec.md", "# Cases\nx\n");
    report(f, &task, "7")
}

fn at_review(f: &Fixture) -> Task {
    let at = at_implement(f);
    let task = report(f, &at, "1");
    write_working(&task, "review.md", "# Findings\nNone\n");
    task
}

fn state_of(task: &Task) -> (&str, TaskState) {
    (task.stage.as_str(), task.state)
}

fn stages_of(f: &Fixture, id: &str, agent: &str) -> Vec<String> {
    launches_of(f, id, agent)
        .iter()
        .map(|payload| payload["stage"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn runs_spec_and_implement_in_shell_1_the_others_in_shells_7_to_11_and_closes_no_agent() {
    let f = fixture();
    let task = f.implementing("spec");
    assert!(f.launch_script(&task.id, "1").starts_with("claude "));

    let mut current = report(&f, &task, "1");
    assert_eq!(state_of(&current), ("test-spec", TaskState::Running));
    assert!(f.launch_script(&task.id, "7").starts_with("copilot "));
    write_working(&task, "test-spec.md", "# Cases\nx\n");

    current = report(&f, &task, "7");
    assert_eq!(state_of(&current), ("implement", TaskState::Running));
    expect_submitted(&f.pty(0), "Task: Export");

    current = report(&f, &task, "1");
    assert_eq!(state_of(&current), ("agent-review", TaskState::Running));
    let cursor = launches_of(&f, &task.id, "cursor");
    assert_eq!(cursor.last().unwrap()["stage"], "agent-review");
    assert_eq!(lane(&f, &current, "9"), running("agent-review"));
    write_working(&task, "review.md", "# Findings\nNone\n");

    current = report(&f, &task, "9");
    assert_eq!(state_of(&current), ("ci-watch", TaskState::Running));
    assert!(f.launch_script(&task.id, "10").starts_with("copilot "));

    current = report(&f, &task, "10");
    assert_eq!(state_of(&current), ("merge-pr", TaskState::Running));
    assert!(f.launch_script(&task.id, "11").starts_with("copilot "));
    assert_eq!(lane(&f, &current, "11"), running("merge-pr"));

    assert!(!any_killed(&f));
    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 1);
    assert_eq!(
        stages_of(&f, &task.id, "copilot"),
        ["test-spec", "ci-watch", "merge-pr"]
    );
}

#[test]
fn gives_a_send_back_from_review_to_claude_code_in_shell_1_then_review_to_its_own_agent() {
    let f = fixture();
    let task = at_review(&f);
    let review = f.ptys().pop().unwrap();

    reopen(&f, &task.id, "9", None, "Fix the dates");
    assert_eq!(
        state_of(&f.task(&task.id)),
        ("implement", TaskState::Running)
    );
    expect_written(&f.pty(0), "What to change: Fix the dates");
    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 1);

    report(&f, &task, "1");
    assert_eq!(
        state_of(&f.task(&task.id)),
        ("agent-review", TaskState::Running)
    );
    expect_submitted(&review, "Task: Export");
    assert_eq!(launches_of(&f, &task.id, "cursor").len(), 1);
    assert!(!any_killed(&f));
}

#[test]
fn gives_a_send_back_from_ci_cd_watch_to_claude_code_then_ci_cd_watch_to_its_own_agent() {
    let f = fixture();
    let at = at_review(&f);
    let task = report(&f, &at, "9");
    assert_eq!(task.stage, "ci-watch");
    let watch = f.ptys().pop().unwrap();

    // Without `to` a send back goes to the step before, which is Review here.
    reopen(&f, &task.id, "10", Some("implement"), "CI failed");
    assert_eq!(
        state_of(&f.task(&task.id)),
        ("implement", TaskState::Running)
    );
    expect_written(&f.pty(0), "What to change: CI failed");

    // Review runs again in between; its agent in Shell 9 gets the prompt.
    report(&f, &task, "1");
    write_working(&task, "review.md", "# Findings\nNone\n");
    report(&f, &task, "9");
    assert_eq!(
        state_of(&f.task(&task.id)),
        ("ci-watch", TaskState::Running)
    );
    expect_submitted(&watch, "Task: Export");
    assert_eq!(launches_of(&f, &task.id, "claude-code").len(), 1);
    assert_eq!(
        stages_of(&f, &task.id, "copilot"),
        ["test-spec", "ci-watch"]
    );
    assert!(!any_killed(&f));
}

#[test]
fn keeps_shell_5_for_a_stage_in_its_own_shell_with_no_agent_of_its_own() {
    let f = fixture();
    {
        let mut settings = f.settings.lock().unwrap();
        let security = stage("security-review", None, Some("security.md"), true);
        let at = settings
            .stages
            .iter()
            .position(|step| step.id() == "agent-review")
            .unwrap();
        settings.stages.insert(at, Step::Stage(security));
    }
    let at = at_implement(&f);
    let task = report(&f, &at, "1");

    assert_eq!(state_of(&task), ("security-review", TaskState::Running));
    assert!(f.launch_script(&task.id, "5").starts_with("claude "));
    write_working(&task, "security.md", "# Findings\nNone\n");

    let next = report(&f, &task, "5");
    assert_eq!(state_of(&next), ("agent-review", TaskState::Running));
    assert_eq!(
        launches_of(&f, &task.id, "cursor").last().unwrap()["stage"],
        "agent-review"
    );
    assert_eq!(lane(&f, &next, "10"), running("agent-review"));
    assert!(!any_killed(&f));
}

fn test_spec_running(f: &Fixture) -> Task {
    let at = f.implementing("spec");
    let task = report(f, &at, "1");
    assert_eq!(state_of(&task), ("test-spec", TaskState::Running));
    task
}

// T-0136: Test spec runs; the settings then give Spec its own agent and take Test spec's away.
#[test]
fn keeps_the_stage_of_a_running_agent_when_the_settings_change() {
    let f = fixture();
    let task = test_spec_running(&f);
    let started = payloads_of(&f, &task.id, "stage.started").pop().unwrap();
    assert_eq!(
        (&started["stage"], &started["terminal"]),
        (&json!("test-spec"), &json!("7"))
    );
    change_stage(&f, "spec", |stage| {
        stage.output = Some("spec.md".into());
        stage.own_shell = Some(true);
        stage.agent = Some("codex".into());
    });
    change_stage(&f, "test-spec", |stage| {
        stage.agent = None;
        stage.end_shell = Some(true);
    });

    assert_eq!(lane(&f, &task, "7"), running("test-spec"));
    let shell = f.runner.run.lane_of_terminal(&task, "7").unwrap();
    assert_eq!(shell.stage.as_deref(), Some("test-spec"));
    assert_eq!(shell.state, Some(TaskState::Running));
    assert_eq!(lane(&f, &task, "5"), None);
    write_working(&task, "test-spec.md", "# Cases\nx\n");

    let next = report(&f, &task, "7");

    assert_eq!(state_of(&next), ("implement", TaskState::Running));
    // Its endShell closes the shell it ran in.
    assert_eq!(f.terminals.list(&task.id), ["1"]);
}

#[test]
fn sends_failed_checks_to_the_agent_of_the_pinned_shell_after_the_settings_change() {
    let f = fixture();
    let task = test_spec_running(&f);
    let shell = f.ptys().pop().unwrap();
    change_stage(&f, "test-spec", |stage| stage.agent = None);

    report(&f, &task, "7");

    assert_eq!(
        state_of(&f.task(&task.id)),
        ("test-spec", TaskState::Running)
    );
    expect_submitted(&shell, "The checks of this stage failed");
}

#[test]
fn starts_the_agent_of_the_pinned_shell_again_when_the_daemon_starts() {
    let f = fixture();
    let task = test_spec_running(&f);
    change_stage(&f, "test-spec", |stage| stage.agent = None);
    f.terminals.close(&task.id);

    f.runner.run.resume_agents();
    f.settle();

    let open = f.terminals.list(&task.id);
    assert!(open.contains(&"7".to_string()));
    assert!(!open.contains(&"5".to_string()));
}

#[test]
fn runs_a_stage_that_lost_its_own_agent_in_shell_5_at_its_next_start() {
    let f = fixture();
    let task = test_spec_running(&f);
    write_working(&task, "test-spec.md", "# Cases\nx\n");
    report(&f, &task, "7");
    change_stage(&f, "test-spec", |stage| stage.agent = None);

    reopen_to(&f, &task.id, "test-spec", "More cases");

    let now = f.task(&task.id);
    assert_eq!(state_of(&now), ("test-spec", TaskState::Running));
    assert!(f.launch_script(&task.id, "5").starts_with("claude "));
    assert_eq!(lane(&f, &now, "5"), running("test-spec"));
    assert_eq!(lane(&f, &now, "7"), None);
}

fn reopen_to(f: &Fixture, id: &str, to: &str, reason: &str) {
    let input = StageActionInput {
        to: Some(to.into()),
        reason: Some(reason.into()),
        ..StageActionInput::default()
    };
    f.runner.run.act(id, StageAction::Reopen, &input).unwrap();
    f.settle();
}

#[test]
fn starts_a_stage_that_gained_its_own_agent_in_a_shell_of_its_own_at_its_next_start() {
    let f = fixture();
    let task = test_spec_running(&f);
    let copilot_shell = f.ptys().pop().unwrap();
    change_stage(&f, "spec", |stage| {
        stage.output = Some("spec.md".into());
        stage.own_shell = Some(true);
        stage.agent = Some("codex".into());
    });

    reopen_to(&f, &task.id, "spec", "Name the edge cases");

    let now = f.task(&task.id);
    assert_eq!(state_of(&now), ("spec", TaskState::Running));
    assert!(f.launch_script(&task.id, "6").starts_with("codex "));
    assert_eq!(lane(&f, &now, "6"), running("spec"));
    assert!(
        !copilot_shell
            .written()
            .concat()
            .contains("Name the edge cases")
    );
    assert!(!any_killed(&f));
}

#[test]
fn refuses_a_report_from_the_pinned_shell_of_a_stage_that_does_not_run_now() {
    let f = fixture();
    let task = test_spec_running(&f);
    write_working(&task, "test-spec.md", "# Cases\nx\n");
    report(&f, &task, "7");
    change_stage(&f, "test-spec", |stage| stage.agent = None);

    let error = f
        .runner
        .run
        .act(&task.id, StageAction::Report, &from_terminal("7"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("test-spec stage of Terminal 7 does not run now"),
        "{error}"
    );
}
