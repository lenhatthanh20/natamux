//! A stage that ends its shell, when its own agent ended the stage: the shell closes at the
//! agent's turn end, so its last command (`natamux done`, a send back) is not cut off.

use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::schema::{Gate, StageKind, Step};
use natamux_proto::tasks::task::TaskState;
use natamux_stages::actions::{MAX_AGENT_SEND_BACKS, StageAction};
use serde_json::json;

use super::{
    act, agent_stage, assert_at, ends_shell, input, killed, launches_of, nothing, report, stage,
    terminals, turn_end, with_stages, writing_docs,
};
use crate::{Fixture, wait_for};

fn closing(f: &Fixture) -> usize {
    f.runner.run.state().closing.len()
}

#[test]
fn keeps_its_shell_after_natamux_done_until_its_agent_ends_the_turn() {
    let f = Fixture::new();
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(!killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);
    assert_eq!(closing(&f), 1);

    assert!(turn_end(&f, &task.id, "7"));

    assert!(killed(&f, 1));
    assert!(!killed(&f, 0));
    assert_eq!(terminals(&f, &task.id), ["1"]);
    assert_eq!(closing(&f), 0);
}

#[test]
fn a_refused_natamux_done_leaves_no_report_and_keeps_its_shell() {
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
    assert!(turn_end(&f, &task.id, "7"));

    // The stage waits for approval, so it is not running and takes no report.
    let refused = f
        .runner
        .run
        .act(&task.id, StageAction::Report, &input(Some("7"), None, None));
    f.settle();

    // Refused by the report itself, after it noted the report.
    let error = refused.unwrap_err().to_string();
    assert!(
        error.contains("the write-docs stage is not running"),
        "{error}"
    );
    assert_at(&f, &task.id, "write-docs", TaskState::Review);
    assert!(!f.runner.run.take_report(&task.id, "7"));
    assert_eq!(closing(&f), 0);
    assert!(!killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);
}

#[test]
fn closes_its_shell_after_the_grace_time_when_the_turn_end_never_comes() {
    let mut f = Fixture::new();
    f.with_end_shell_grace(100);
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");

    wait_for(|| killed(&f, 1));
    wait_for(|| terminals(&f, &task.id) == ["1"]);
    assert!(!killed(&f, 0));
    assert_eq!(closing(&f), 0);
    // The killed agent never ends its turn: its report must not swallow a next one.
    assert!(!f.runner.run.take_report(&task.id, "7"));
}

#[test]
fn a_turn_end_closes_nothing_when_no_shell_waits_for_it() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], nothing);
    let task = writing_docs(&f);

    report(&f, &task.id, "7");
    assert!(turn_end(&f, &task.id, "7"));

    assert_at(&f, &task.id, "finish", TaskState::Running);
    assert!(!killed(&f, 1));
    assert_eq!(terminals(&f, &task.id), ["1", "7"]);
}

#[test]
fn a_next_stage_in_the_same_shell_closes_it_first_and_starts_a_new_agent() {
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
            report_stage("check-docs", true),
            report_stage("check-names", false),
            Step::Stage(agent_stage("finish")),
            Step::Stage(stage("release", StageKind::Release, Gate::Manual)),
        ],
        ..ProjectSettings::default()
    });
    let task = f.implementing("implement");
    report(&f, &task.id, "1");
    assert_eq!(f.task(&task.id).stage, "check-docs");
    assert_eq!(terminals(&f, &task.id), ["1", "5"]);
    let dir = std::path::Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("check-docs.md"), "# Findings\nNone\n").unwrap();
    let launches = launches_of(&f, &task.id, "claude-code");
    let pastes = || {
        f.pty(1)
            .written()
            .iter()
            .filter(|data| data.contains("\x1b[200~"))
            .count()
    };
    let pasted = pastes();

    report(&f, &task.id, "5");

    assert_eq!(f.task(&task.id).stage, "check-names");
    assert!(killed(&f, 1));
    assert_eq!(f.ptys().len(), 3);
    assert!(!killed(&f, 2));
    assert_eq!(terminals(&f, &task.id), ["1", "5"]);
    assert_eq!(launches_of(&f, &task.id, "claude-code"), launches + 1);
    // The old agent never got the prompt of the next stage.
    assert_eq!(pastes(), pasted);
    assert_eq!(closing(&f), 0);
}

#[test]
fn a_task_that_ends_while_a_shell_waits_leaves_no_timer() {
    let mut f = Fixture::new();
    f.with_end_shell_grace(300);
    with_stages(&f, ends_shell, vec![], nothing);
    let task = writing_docs(&f);
    report(&f, &task.id, "7");
    assert_eq!(closing(&f), 1);

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );

    assert_eq!(closing(&f), 0);
    wait_for(|| f.terminals.list(&task.id).is_empty());
}

#[test]
fn a_send_back_refused_after_the_most_times_leaves_its_shell_open() {
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
        turn_end(&f, &task.id, "7");
        report(&f, &task.id, "1");
    }
    let shell = f.ptys().len() - 1;

    let refused = f.runner.run.act(
        &task.id,
        StageAction::Reopen,
        &input(Some("7"), Some("implement"), None),
    );
    f.settle();

    assert!(refused.is_err());
    assert_at(&f, &task.id, "write-docs", TaskState::NeedsInput);
    assert!(!killed(&f, shell));
    assert_eq!(closing(&f), 0);
    assert!(!f.runner.run.take_report(&task.id, "7"));
}

#[test]
fn gives_the_agent_its_send_backs_again_after_a_person_sends_the_task_back() {
    let f = Fixture::new();
    with_stages(&f, nothing, vec![], nothing);
    let task = writing_docs(&f);
    let agent_back = || {
        f.runner.run.act(
            &task.id,
            StageAction::Reopen,
            &input(Some("7"), Some("implement"), None),
        )
    };
    let rounds = || {
        for _ in 0..MAX_AGENT_SEND_BACKS {
            agent_back().unwrap();
            f.settle();
            assert_at(&f, &task.id, "implement", TaskState::Running);
            report(&f, &task.id, "1");
        }
        let refused = agent_back();
        f.settle();
        assert!(refused.is_err());
        assert_at(&f, &task.id, "write-docs", TaskState::NeedsInput);
    };
    rounds();

    let person = input(None, Some("implement"), Some("Name the dates"));
    act(&f, &task.id, StageAction::Reopen, &person);
    assert_at(&f, &task.id, "implement", TaskState::Running);
    report(&f, &task.id, "1");
    assert_at(&f, &task.id, "write-docs", TaskState::Running);

    rounds();
    let asked = f
        .events(&task.id)
        .into_iter()
        .rfind(|(kind, _)| kind == "agent.asked")
        .and_then(|(_, payload)| payload["message"].as_str().map(str::to_string))
        .unwrap_or_default();
    let times = format!("sent the task back {MAX_AGENT_SEND_BACKS} times.");
    assert!(asked.contains(&times), "{asked}");
}
