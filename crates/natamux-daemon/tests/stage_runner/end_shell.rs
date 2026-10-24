//! `describe('a stage that ends its shell')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::{MAX_AGENT_SEND_BACKS, StageAction};

use crate::{Fixture, expect_submitted, expect_written};

#[path = "end_shell_more.rs"]
mod more;

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

fn agent_stage(id: &str) -> Stage {
    stage(id, StageKind::Agent, Gate::Auto)
}

/// Implement runs in Shell 1 with the task's agent; Write docs runs in Shell 7 with its own.
fn with_stages(
    f: &Fixture,
    docs: impl FnOnce(&mut Stage),
    after: Vec<Stage>,
    implement: impl FnOnce(&mut Stage),
) {
    let mut first = agent_stage("implement");
    implement(&mut first);
    let mut write_docs = agent_stage("write-docs");
    write_docs.agent = Some("codex".into());
    docs(&mut write_docs);
    let mut steps = vec![Step::Stage(first), Step::Stage(write_docs)];
    steps.extend(after.into_iter().map(Step::Stage));
    steps.push(Step::Stage(agent_stage("finish")));
    steps.push(Step::Stage(stage(
        "release",
        StageKind::Release,
        Gate::Manual,
    )));
    f.set_settings(ProjectSettings {
        stages: steps,
        ..ProjectSettings::default()
    });
}

fn ends_shell(stage: &mut Stage) {
    stage.end_shell = Some(true);
}

fn nothing(_: &mut Stage) {}

fn input(terminal: Option<&str>, to: Option<&str>, reason: Option<&str>) -> StageActionInput {
    StageActionInput {
        terminal: terminal.map(Into::into),
        to: to.map(Into::into),
        reason: reason.map(Into::into),
        ..StageActionInput::default()
    }
}

fn act(f: &Fixture, id: &str, what: StageAction, input: &StageActionInput) {
    f.runner.run.act(id, what, input).unwrap();
    f.settle();
}

fn report(f: &Fixture, id: &str, terminal: &str) {
    act(
        f,
        id,
        StageAction::Report,
        &input(Some(terminal), None, None),
    );
}

fn assert_at(f: &Fixture, id: &str, stage: &str, state: TaskState) {
    let task = f.task(id);
    assert_eq!((task.stage.as_str(), task.state), (stage, state));
}

fn writing_docs(f: &Fixture) -> Task {
    let task = f.implementing("implement");
    report(f, &task.id, "1");
    assert_at(f, &task.id, "write-docs", TaskState::Running);
    task
}

fn launches_of(f: &Fixture, id: &str, agent: &str) -> usize {
    f.events(id)
        .iter()
        .filter(|(kind, payload)| kind == "agent.launched" && payload["agent"] == agent)
        .count()
}

fn killed(f: &Fixture, index: usize) -> bool {
    f.pty(index).killed()
}

fn any_killed(f: &Fixture) -> bool {
    f.ptys().iter().any(|pty| pty.killed())
}

fn terminals(f: &Fixture, id: &str) -> Vec<String> {
    f.terminals.list(id)
}

#[test]
fn closes_its_shell_when_the_stage_passes_and_shell_1_goes_on_with_the_next_stage() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert_eq!(f.ptys().len(), 2);
    assert!(!killed(&f, 0));
    assert!(killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1"]);
    assert_eq!(launches_of(&f, &task.id, "claude-code"), 1);
    expect_submitted(&f.pty(0), "Task: Export");
}

#[test]
fn drops_the_report_of_the_closed_agent_so_a_next_agent_in_its_shell_reports_its_turn_end() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(!f.runner.run.take_report(&task.id, "7"));
}

#[test]
fn starts_a_new_agent_in_a_new_shell_with_the_send_back_prompt_and_closes_it_again() {
    let f = Fixture::new();
    let qa = stage("qa", StageKind::Manual, Gate::Manual);
    with_stages(&f, ends_shell, vec![qa], nothing);
    let task = writing_docs(&f);
    report(&f, &task.id, "7");
    assert_at(&f, &task.id, "qa", TaskState::Review);
    assert_eq!(terminals(&f, &task.id), ["1"]);

    let back = input(None, Some("write-docs"), Some("Fix the dates"));
    act(&f, &task.id, StageAction::Reopen, &back);

    assert_at(&f, &task.id, "write-docs", TaskState::Running);
    assert_eq!(f.ptys().len(), 3);
    assert!(!killed(&f, 2));
    assert_eq!(launches_of(&f, &task.id, "codex"), 2);
    assert!(
        f.launch_script(&task.id, "7")
            .contains("What to change: Fix the dates")
    );

    report(&f, &task.id, "7");
    assert_eq!(f.task(&task.id).stage, "qa");
    assert!(killed(&f, 2));
    assert!(!killed(&f, 0));
}

#[test]
fn keeps_the_shell_of_a_stage_without_it_as_before() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(!any_killed(&f));
    assert_eq!(launches_of(&f, &task.id, "codex"), 1);
}

#[test]
fn closes_its_shell_when_its_agent_sends_the_task_back_and_starts_a_new_agent_next_round() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    let back = input(Some("7"), Some("implement"), Some("Fix the dates"));
    act(&f, &task.id, StageAction::Reopen, &back);

    assert_at(&f, &task.id, "implement", TaskState::Running);
    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
    assert_eq!(terminals(&f, &task.id), ["1"]);
    // The killed agent never ends its turn, so nothing waits for it.
    assert!(!f.runner.run.take_report(&task.id, "7"));
    expect_written(&f.pty(0), "What to change: Fix the dates");

    report(&f, &task.id, "1");

    assert_at(&f, &task.id, "write-docs", TaskState::Running);
    assert_eq!(f.ptys().len(), 3);
    assert!(!killed(&f, 2));
    assert_eq!(launches_of(&f, &task.id, "codex"), 2);
    assert!(f.launch_script(&task.id, "7").starts_with("codex "));
}

#[test]
fn closes_its_shell_when_a_person_sends_the_task_back() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    act(
        &f,
        &task.id,
        StageAction::Reopen,
        &input(None, Some("implement"), None),
    );

    assert_at(&f, &task.id, "implement", TaskState::Running);
    assert!(killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1"]);
}

#[test]
fn keeps_its_shell_when_its_agent_sends_the_task_back_the_most_times() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);
    for _ in 0..MAX_AGENT_SEND_BACKS {
        act(
            &f,
            &task.id,
            StageAction::Reopen,
            &input(Some("7"), Some("implement"), None),
        );
        report(&f, &task.id, "1");
    }
    let shell = f.ptys().len() - 1;

    let error = f
        .runner
        .run
        .act(
            &task.id,
            StageAction::Reopen,
            &input(Some("7"), Some("implement"), None),
        )
        .unwrap_err();
    f.settle();

    assert!(error.to_string().contains("waits for the user"), "{error}");
    assert_at(&f, &task.id, "write-docs", TaskState::NeedsInput);
    assert!(!killed(&f, shell));
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);
}

#[test]
fn keeps_its_shell_when_the_task_goes_back_to_the_same_stage() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    let back = input(None, Some("write-docs"), Some("Fix the dates"));
    act(&f, &task.id, StageAction::Reopen, &back);

    assert_at(&f, &task.id, "write-docs", TaskState::Running);
    assert!(!killed(&f, 1));
    assert_eq!(launches_of(&f, &task.id, "codex"), 1);
}

#[test]
fn keeps_the_shell_of_a_stage_without_it_on_a_send_back_and_gives_the_agent_there_the_prompt() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], nothing);
    let task = writing_docs(&f);
    let shell = f.pty(1);

    act(
        &f,
        &task.id,
        StageAction::Reopen,
        &input(Some("7"), Some("implement"), None),
    );
    assert!(!shell.killed());
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);

    report(&f, &task.id, "1");

    assert_at(&f, &task.id, "write-docs", TaskState::Running);
    assert!(!any_killed(&f));
    assert_eq!(launches_of(&f, &task.id, "codex"), 1);
    expect_submitted(&shell, "Task: Export");
}

#[test]
fn never_closes_shell_1_on_a_send_back() {
    let f = Fixture::new();
    let mut polish = agent_stage("polish");
    polish.end_shell = Some(true);
    with_stages(&f, nothing, vec![polish], nothing);
    let task = writing_docs(&f);
    report(&f, &task.id, "7");
    assert_eq!(f.task(&task.id).stage, "polish");

    act(
        &f,
        &task.id,
        StageAction::Reopen,
        &input(Some("1"), Some("implement"), None),
    );

    assert_at(&f, &task.id, "implement", TaskState::Running);
    assert!(!killed(&f, 0));
    assert_eq!(launches_of(&f, &task.id, "claude-code"), 1);
}

#[test]
fn keeps_the_shell_while_the_stage_waits_for_approval_and_closes_it_on_approval() {
    let f = Fixture::new();
    with_stages(
        &f,
        |stage| {
            stage.gate = Gate::Manual;
            stage.end_shell = Some(true);
        },
        vec![],
        nothing,
    );
    let task = writing_docs(&f);

    report(&f, &task.id, "7");
    assert_at(&f, &task.id, "write-docs", TaskState::Review);
    assert!(!killed(&f, 1));

    act(
        &f,
        &task.id,
        StageAction::Approve,
        &StageActionInput::default(),
    );
    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
}

#[test]
fn closes_the_shell_of_a_skipped_stage() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    act(
        &f,
        &task.id,
        StageAction::Skip,
        &StageActionInput::default(),
    );

    assert_eq!(f.task(&task.id).stage, "finish");
    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
}

#[test]
fn keeps_the_shell_of_the_stage_before_when_a_stage_marked_to_skip_is_skipped() {
    // Both run in Shell 5, so a planned skip of the second must not end the agent of the first.
    let f = Fixture::new();
    let report_stage = |id: &str, end: bool| {
        let mut stage = agent_stage(id);
        stage.output = Some(format!("{id}.md"));
        stage.own_shell = Some(true);
        stage.end_shell = end.then_some(true);
        Step::Stage(stage)
    };
    f.set_settings(ProjectSettings {
        stages: vec![
            Step::Stage(agent_stage("implement")),
            report_stage("check-docs", false),
            report_stage("check-names", true),
            Step::Stage(agent_stage("finish")),
            Step::Stage(stage("release", StageKind::Release, Gate::Manual)),
        ],
        ..ProjectSettings::default()
    });
    let task = f.implementing("implement");
    f.tasks.set_skip(&task.id, "check-names", true).unwrap();
    report(&f, &task.id, "1");
    assert_eq!(f.task(&task.id).stage, "check-docs");
    assert_eq!(terminals(&f, &task.id), ["1", "5"]);
    let dir = std::path::Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("check-docs.md"), "# Findings\nNone\n").unwrap();

    report(&f, &task.id, "5");

    assert_eq!(f.task(&task.id).stage, "finish");
    assert!(!any_killed(&f));
    assert_eq!(terminals(&f, &task.id), ["1", "5"]);
}

#[test]
fn never_closes_shell_1_so_its_agent_goes_on_with_the_next_stage() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], ends_shell);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(!any_killed(&f));
    assert_eq!(launches_of(&f, &task.id, "claude-code"), 1);
    expect_submitted(&f.pty(0), "Task: Export");
}

#[test]
fn never_closes_shell_1_when_its_stage_is_skipped() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], ends_shell);
    let task = f.implementing("implement");

    act(
        &f,
        &task.id,
        StageAction::Skip,
        &StageActionInput::default(),
    );

    assert_eq!(f.task(&task.id).stage, "write-docs");
    assert!(!killed(&f, 0));
}
