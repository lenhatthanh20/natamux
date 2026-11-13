use natamux_proto::stages::catalog::catalog_stage;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};

use super::*;

fn custom() -> Stage {
    Stage::new("perf-budget", StageKind::Agent, Gate::Manual)
}

fn issue(message: &str) -> PipelineIssue {
    PipelineIssue {
        stage: Some("perf-budget".into()),
        message: message.into(),
        level: IssueLevel::Error,
    }
}

fn total(stage: &Stage) -> usize {
    SheetGroup::ALL
        .iter()
        .map(|group| changed_in(stage, *group))
        .sum()
}

#[test]
fn a_stage_of_your_own_with_nothing_set_has_no_change() {
    assert_eq!(total(&custom()), 0);
}

#[test]
fn counts_each_text_and_flag_in_its_group() {
    let mut stage = custom();
    stage.check = Some("make test".into());
    stage.ci = Some(true);
    stage.workflow = Some(false);
    stage.args = Some(String::new());
    assert_eq!(changed_in(&stage, SheetGroup::DoneCheck), 1);
    assert_eq!(changed_in(&stage, SheetGroup::Options), 1);
    assert_eq!(changed_in(&stage, SheetGroup::Files), 0);
    stage.output = Some("notes.md".into());
    stage.command = Some("make".into());
    assert_eq!(changed_in(&stage, SheetGroup::Files), 2);
}

#[test]
fn a_catalog_stage_as_the_catalog_has_it_has_no_change() {
    for id in ["spec", "plan", "implement", "unit-test"] {
        if let Some(stage) = catalog_stage(id) {
            assert_eq!(total(&stage), 0, "{id}");
        }
    }
}

#[test]
fn a_flag_off_that_the_catalog_has_on_counts_in_options() {
    let Some(mut stage) = catalog_stage("implement") else {
        return;
    };
    let before = changed_in(&stage, SheetGroup::Options);
    stage.ci = Some(stage.ci != Some(true));
    assert_eq!(changed_in(&stage, SheetGroup::Options), before + 1);
}

#[test]
fn cleared_required_sections_of_a_catalog_stage_count_in_files() {
    let mut stage = catalog_stage("spec").expect("spec is in the catalog");
    assert!(stage.sections.as_ref().is_some_and(|list| !list.is_empty()));
    stage.sections = None;
    assert_eq!(changed_in(&stage, SheetGroup::Files), 1);
    assert_eq!(total(&stage), 1);
}

#[test]
fn a_folded_group_says_how_many_values_changed() {
    assert_eq!(group_summary(0), "\u{b7} all default");
    assert_eq!(group_summary(2), "\u{b7} 2 changed");
}

#[test]
fn only_done_check_starts_open() {
    let open: Vec<_> = SheetGroup::ALL
        .iter()
        .filter(|group| group.open_at_first())
        .collect();
    assert_eq!(open, [&SheetGroup::DoneCheck]);
}

#[test]
fn a_problem_opens_the_group_of_its_field_only() {
    let stage = custom();
    let check = [issue("Check command: is too long")];
    assert!(group_has_issue(&stage, &check, SheetGroup::DoneCheck));
    assert!(!group_has_issue(&stage, &check, SheetGroup::Files));
    let command = [issue("Command: is too long")];
    assert!(group_has_issue(&stage, &command, SheetGroup::Files));
    assert!(!group_has_issue(&stage, &command, SheetGroup::DoneCheck));
    let ci = [issue("It cannot be checked on CI")];
    assert!(group_has_issue(&stage, &ci, SheetGroup::Options));
    for group in SheetGroup::ALL {
        assert!(!group_has_issue(
            &stage,
            &[issue("Name: is too long")],
            group
        ));
    }
    let mut other = issue("Command: is too long");
    other.stage = Some("plan".into());
    assert!(!group_has_issue(&stage, &[other], SheetGroup::Files));
}

#[test]
fn the_own_agent_or_arguments_issue_opens_files_when_args_is_set() {
    let message = "Only an agent or testing stage can have its own agent or arguments, and \"perf-budget\" is a review stage";
    let mut stage = Stage::new("perf-budget", StageKind::Review, Gate::Manual);
    assert!(!group_has_issue(
        &stage,
        &[issue(message)],
        SheetGroup::Files
    ));
    stage.args = Some("--model sonnet".into());
    assert!(group_has_issue(
        &stage,
        &[issue(message)],
        SheetGroup::Files
    ));
}

// A testing stage draws its Command at the left, over the prompt: Files neither counts it nor
// opens for its problem.
#[test]
fn a_testing_stage_command_is_not_in_files() {
    let mut stage = Stage::new("perf-budget", StageKind::Testing, Gate::Manual);
    stage.command = Some("make bench".into());
    assert_eq!(changed_in(&stage, SheetGroup::Files), 0);
    for message in ["Command: is too long", "Give the command to run"] {
        assert!(!group_has_issue(
            &stage,
            &[issue(message)],
            SheetGroup::Files
        ));
    }
    let mut agent = custom();
    agent.command = Some("make bench".into());
    assert_eq!(changed_in(&agent, SheetGroup::Files), 1);
    assert!(group_has_issue(
        &agent,
        &[issue("Give the command to run")],
        SheetGroup::Files
    ));
}

// T-0333: one message names the agent, the arguments and the terminal of the stage after the
// context; it opens the group of each one that is set.
#[test]
fn the_after_context_issue_opens_the_groups_of_what_is_set() {
    let found = [issue(
        "\"perf-budget\" comes right after the context stage, so it runs with the task's agent in Terminal 1. Clear its own agent, arguments and terminal",
    )];
    let plain = custom();
    for group in SheetGroup::ALL {
        assert!(!group_has_issue(&plain, &found, group));
    }
    let own = Stage {
        own_shell: Some(true),
        ..custom()
    };
    assert!(group_has_issue(&own, &found, SheetGroup::Options));
    assert!(!group_has_issue(&own, &found, SheetGroup::Files));
    let args = Stage {
        args: Some("--model x".into()),
        ..custom()
    };
    assert!(group_has_issue(&args, &found, SheetGroup::Files));
    assert!(!group_has_issue(&args, &found, SheetGroup::Options));
}
