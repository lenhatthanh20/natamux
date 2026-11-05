use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};

use super::*;

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

fn alone(stage: &Stage) -> Vec<Step> {
    vec![Step::Stage(stage.clone())]
}

#[test]
fn the_first_line_names_the_kind_and_the_catalog_sentence() {
    let spec = stage("spec", StageKind::Agent, Gate::Manual);
    let sentence = catalog_entry("spec")
        .map(|entry| entry.description)
        .unwrap();
    assert_eq!(sheet_intro(&spec), format!("Agent \u{b7} {sentence}"));
    let release = stage("release", StageKind::Release, Gate::Manual);
    assert!(sheet_intro(&release).starts_with("Agent \u{b7} "));
    let own = stage("perf-budget", StageKind::Testing, Gate::Manual);
    assert_eq!(sheet_intro(&own), "A stage of your own");
}

#[test]
fn the_advanced_segment_counts_the_changed_values() {
    assert_eq!(advanced_label(0), "Advanced \u{b7} all default");
    assert_eq!(advanced_label(3), "Advanced \u{b7} 3 changed");
}

#[test]
fn the_release_stage_and_a_manual_stage_have_their_own_approval_words() {
    let (label, help) = approval_text(&stage("release", StageKind::Release, Gate::Manual));
    assert_eq!(label, "Open the pull request when I click");
    assert!(help.is_some());
    let (label, help) = approval_text(&stage("qa", StageKind::Manual, Gate::Manual));
    assert_eq!(label, "Wait for my approval");
    assert_eq!(help, Some("Always on for this kind of stage."));
    assert_eq!(
        approval_text(&stage("spec", StageKind::Agent, Gate::Manual)).1,
        None
    );
}

#[test]
fn an_agent_stage_shows_the_prompt_output_and_workflow_but_not_the_command() {
    let spec = stage("spec", StageKind::Agent, Gate::Manual);
    let shows = sheet_shows(&spec, &alone(&spec));
    assert!(shows.prompt && shows.output && shows.workflow);
    assert!(!shows.command);
    assert_eq!(shows.sections, spec.output.is_some());
}

#[test]
fn a_testing_stage_shows_its_command_and_no_output_or_workflow() {
    let mut test = stage("testing", StageKind::Testing, Gate::Auto);
    test.command = Some("pnpm test".into());
    let shows = sheet_shows(&test, &alone(&test));
    assert!(shows.command && !shows.command_unused);
    assert!(!shows.output && !shows.sections && !shows.workflow);
    assert!(shows.ci);
}

#[test]
fn a_command_on_an_agent_stage_still_shows_as_not_used() {
    let mut agent = stage("implement", StageKind::Agent, Gate::Auto);
    agent.command = Some("make".into());
    let shows = sheet_shows(&agent, &alone(&agent));
    assert!(shows.command && shows.command_unused);
}

#[test]
fn a_manual_stage_has_no_agent_prompt_or_end_switch() {
    let qa = stage("qa", StageKind::Manual, Gate::Manual);
    let shows = sheet_shows(&qa, &alone(&qa));
    assert!(!shows.agent && !shows.prompt && !shows.workflow);
    assert_eq!(shows.end_agent, EndAgent::Hidden);
}

#[test]
fn a_stage_on_the_task_s_agent_runs_in_terminal_one() {
    let spec = stage("spec", StageKind::Agent, Gate::Manual);
    assert_eq!(
        sheet_shows(&spec, &alone(&spec)).end_agent,
        EndAgent::TerminalOne
    );
    assert!(agent_help(&alone(&spec), "spec").starts_with("\"Task's agent\""));
}
