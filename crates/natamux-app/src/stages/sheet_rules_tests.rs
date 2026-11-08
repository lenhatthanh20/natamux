use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};

use super::*;
use crate::stages::advanced_rules::SheetGroup;

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

#[test]
fn the_stepper_walks_every_stage_in_list_order_also_in_paths() {
    let steps = crate::stages::editor_fixtures::branched();
    let all: Vec<String> = steps
        .iter()
        .flat_map(Step::stages)
        .map(|stage| stage.id.clone())
        .collect();
    let first = stage_neighbours(&steps, &all[0]).unwrap();
    assert_eq!(first.prev, None);
    assert_eq!(first.next.unwrap().id, all[1]);
    assert_eq!((first.place, first.count), (1, all.len()));
    let implement = stage_neighbours(&steps, "implement").unwrap();
    assert_eq!(implement.prev.unwrap().id, "plan");
    assert_eq!(implement.next.unwrap().id, "testing");
    let last = stage_neighbours(&steps, &all[all.len() - 1]).unwrap();
    assert_eq!(last.next, None);
    assert_eq!(last.place, all.len());
    assert_eq!(stage_neighbours(&steps, "gone"), None);
}

#[test]
fn an_agent_stage_folds_done_check_files_and_options() {
    let mut spec = stage("spec", StageKind::Agent, Gate::Manual);
    spec.output = Some("spec.md".into());
    let shows = sheet_shows(&spec, &alone(&spec));
    assert_eq!(
        groups_shown(&shows),
        [
            SheetGroup::DoneCheck,
            SheetGroup::Files,
            SheetGroup::Options
        ]
    );
}

#[test]
fn a_testing_stage_has_its_command_at_the_left_over_the_prompt() {
    let mut test = stage("testing", StageKind::Testing, Gate::Auto);
    test.command = Some("pnpm test".into());
    let shows = sheet_shows(&test, &alone(&test));
    assert!(command_at_left(&shows));
    assert!(groups_shown(&shows).contains(&SheetGroup::DoneCheck));
}

#[test]
fn a_command_an_agent_stage_does_not_run_stays_in_files() {
    let mut agent = stage("implement", StageKind::Agent, Gate::Auto);
    agent.command = Some("make".into());
    let shows = sheet_shows(&agent, &alone(&agent));
    assert!(!command_at_left(&shows));
    assert!(groups_shown(&shows).contains(&SheetGroup::Files));
}

#[test]
fn a_stage_without_a_prompt_has_its_run_fields_at_the_left_not_in_groups() {
    let qa = stage("qa", StageKind::Manual, Gate::Manual);
    assert!(groups_shown(&sheet_shows(&qa, &alone(&qa))).is_empty());
    let review = stage("review", StageKind::Review, Gate::Manual);
    let groups = groups_shown(&sheet_shows(&review, &alone(&review)));
    assert!(!groups.contains(&SheetGroup::DoneCheck));
    assert!(!groups.contains(&SheetGroup::Files));
}

#[test]
fn a_single_stage_has_no_neighbours_and_is_1_of_1() {
    let spec = stage("spec", StageKind::Agent, Gate::Manual);
    let at = stage_neighbours(&alone(&spec), "spec").unwrap();
    assert_eq!((at.prev, at.next, at.place, at.count), (None, None, 1, 1));
}

// `advanced_rules` keeps a testing stage's Command out of Files by kind; this is the same rule.
#[test]
fn only_a_testing_stage_has_its_command_at_the_left() {
    for kind in [
        StageKind::Context,
        StageKind::Agent,
        StageKind::Review,
        StageKind::Release,
        StageKind::Testing,
        StageKind::Manual,
    ] {
        let mut one = stage("perf-budget", kind, Gate::Manual);
        one.command = Some("make".into());
        let shows = sheet_shows(&one, &alone(&one));
        assert_eq!(
            command_at_left(&shows),
            kind == StageKind::Testing,
            "{kind:?}"
        );
    }
}
