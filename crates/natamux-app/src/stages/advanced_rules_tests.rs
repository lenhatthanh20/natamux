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

#[test]
fn a_stage_of_your_own_with_nothing_set_has_no_change() {
    assert_eq!(changed_count(&custom()), 0);
}

#[test]
fn counts_each_text_and_flag_that_differs() {
    let mut stage = custom();
    stage.check = Some("make test".into());
    stage.ci = Some(true);
    stage.workflow = Some(false);
    stage.args = Some(String::new());
    assert_eq!(changed_count(&stage), 2);
}

#[test]
fn a_catalog_stage_as_the_catalog_has_it_has_no_change() {
    for id in ["spec", "plan", "implement", "unit-test"] {
        if let Some(stage) = catalog_stage(id) {
            assert_eq!(changed_count(&stage), 0, "{id}");
        }
    }
}

#[test]
fn a_flag_off_that_the_catalog_has_on_counts() {
    let Some(mut stage) = catalog_stage("implement") else {
        return;
    };
    let before = changed_count(&stage);
    stage.ci = Some(stage.ci != Some(true));
    assert_eq!(changed_count(&stage), before + 1);
}

#[test]
fn sees_a_problem_in_an_advanced_field_only() {
    let stage = custom();
    assert!(advanced_has_issue(&stage, &[issue("Command: is too long")]));
    assert!(advanced_has_issue(
        &stage,
        &[issue("Give the command to run")]
    ));
    assert!(!advanced_has_issue(&stage, &[issue("Name: is too long")]));
    let mut other = issue("Command: is too long");
    other.stage = Some("plan".into());
    assert!(!advanced_has_issue(&stage, &[other]));
}

#[test]
fn cleared_required_sections_of_a_catalog_stage_count() {
    let mut stage = catalog_stage("spec").expect("spec is in the catalog");
    assert!(stage.sections.as_ref().is_some_and(|list| !list.is_empty()));
    stage.sections = None;
    assert_eq!(changed_count(&stage), 1);
}

#[test]
fn the_own_agent_or_arguments_issue_is_advanced_when_args_is_set() {
    let message = "Only an agent or testing stage can have its own agent or arguments, and \"perf-budget\" is a review stage";
    let mut stage = Stage::new("perf-budget", StageKind::Review, Gate::Manual);
    assert!(!advanced_has_issue(&stage, &[issue(message)]));
    stage.args = Some("--model sonnet".into());
    assert!(advanced_has_issue(&stage, &[issue(message)]));
}
