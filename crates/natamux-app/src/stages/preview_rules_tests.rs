use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::{BranchPath, Gate, Stage, StageGroup, StageKind, Step};

use super::*;
use crate::stages::editor_branches::set_otherwise;
use crate::stages::editor_fields::{TextField, set_enabled, set_text};
use crate::stages::editor_find::{find_branch, find_stage};
use crate::stages::editor_fixtures::{branched, stage_step};
use crate::stages::editor_labels::{pipeline_label, stage_runner};

fn agent_label(id: &str) -> String {
    match id {
        "codex" => "Codex".to_string(),
        "cursor" => "Cursor".to_string(),
        other => other.to_string(),
    }
}

fn stage<'a>(steps: &'a [Step], id: &str) -> &'a Stage {
    find_stage(steps, id).unwrap()
}

fn all_off(steps: &[Step]) -> Vec<Step> {
    let ids: Vec<String> = steps
        .iter()
        .flat_map(Step::stages)
        .map(|s| s.id.clone())
        .collect();
    ids.iter()
        .fold(steps.to_vec(), |steps, id| set_enabled(&steps, id, false))
}

fn group_of_stages(id: &str, stages: &[&str]) -> Step {
    Step::Group(StageGroup {
        id: id.into(),
        name: None,
        parallel: stages
            .iter()
            .map(|stage| Stage::new(stage, StageKind::Agent, Gate::Auto))
            .collect(),
    })
}

fn tip(steps: &[Step], id: &str) -> String {
    let stage = stage(steps, id);
    tip_of(stage, stage_runner(steps, stage).as_ref(), agent_label)
}

fn node(steps: &[Step], id: &str) -> String {
    let stage = stage(steps, id);
    node_label(stage, stage_runner(steps, stage).as_ref(), agent_label)
}

fn lane<'a>(steps: &'a [Step], id: &str) -> &'a BranchPath {
    find_branch(steps, "after-plan")
        .unwrap()
        .paths
        .iter()
        .find(|p| p.id == id)
        .unwrap()
}

#[test]
fn names_the_stages_in_order_with_the_group_side_by_side() {
    let steps = default_pipeline();
    assert_eq!(
        pipeline_label(&steps),
        "Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Automated tests, then Review, then Release"
    );
    let Step::Group(group) = &steps[3] else {
        panic!("not a group");
    };
    assert_eq!(
        group_list_label(group),
        "Test spec and Implement, side by side"
    );
    assert_eq!(node(&steps, "test-spec"), "Test spec, waits for approval");
    assert_eq!(node(&steps, "implement"), "Implement");
}

#[test]
fn marks_the_stages_that_wait_for_approval_and_the_ones_with_a_prompt_of_their_own() {
    let steps = set_text(
        &default_pipeline(),
        "plan",
        TextField::Prompt,
        "Plan {title}.",
    );
    assert!(waits_for_person(stage(&steps, "plan")));
    assert!(has_custom_prompt(stage(&steps, "plan")));
    assert_eq!(
        stage_label(stage(&steps, "plan")),
        "Plan, waits for approval, custom prompt"
    );
    assert!(!waits_for_person(stage(&steps, "implement")));
    assert!(!has_custom_prompt(stage(&steps, "implement")));
}

#[test]
fn leaves_out_the_stages_that_are_off() {
    let steps = set_enabled(&default_pipeline(), "plan", false);
    assert!(!pipeline_label(&steps).contains("Plan"));
}

#[test]
fn says_so_when_no_stage_is_on() {
    let off = all_off(&default_pipeline());
    assert!(active_steps(&off).is_empty());
    assert_eq!(NO_STAGE_ON, "No stage is on. Turn on at least one.");
}

#[test]
fn marks_a_selected_stage_and_a_wrong_one() {
    let issues = [PipelineIssue {
        stage: Some("agent-review".into()),
        message: "Something is wrong".into(),
        level: IssueLevel::Error,
    }];
    assert!(error_ids(&issues).contains("agent-review"));
    assert_eq!(
        tip(&default_pipeline(), "agent-review"),
        "Agent \u{b7} Moves on by itself \u{b7} Output: review.md"
    );
}

#[test]
fn c19_draws_one_stage_as_one_node_without_a_fork() {
    assert_eq!(edge_of(0, 1), Some(Edge::Only));
    assert_eq!(edge_of(0, 3), Some(Edge::First));
    assert_eq!(edge_of(1, 3), None);
    assert_eq!(edge_of(2, 3), Some(Edge::Last));
}

#[test]
fn c19_draws_a_group_of_four_as_four_rows_of_one_fork() {
    let group = group_of_stages("finish", &["docs", "refactor", "i18n", "changelog"]);
    let Step::Group(group) = &group else {
        panic!("not a group");
    };
    assert_eq!(
        group_list_label(group),
        "Documentation, Refactor, Translations and Release notes, side by side"
    );
}

#[test]
fn c19_tells_the_output_file_and_the_command_in_the_tooltip() {
    let mut spec = Stage::new("spec", StageKind::Agent, Gate::Manual);
    spec.output = Some("spec.md".into());
    let mut lint = Stage::new("lint", StageKind::Testing, Gate::Auto);
    lint.command = Some("pnpm lint".into());
    let steps = vec![Step::Stage(spec), Step::Stage(lint)];
    assert_eq!(
        tip(&steps, "spec"),
        "Agent \u{b7} Waits for your approval \u{b7} Output: spec.md"
    );
    assert_eq!(
        tip(&steps, "lint"),
        "Testing \u{b7} Moves on by itself \u{b7} Command: pnpm lint"
    );
}

#[test]
fn c5_draws_no_fork_when_every_stage_of_a_group_is_off() {
    let off = set_enabled(
        &set_enabled(&default_pipeline(), "test-spec", false),
        "implement",
        false,
    );
    assert_eq!(
        pipeline_label(&off),
        "Pipeline: Context, then Spec, then Plan, then Automated tests, then Review, then Release"
    );
    assert!(
        active_steps(&off)
            .iter()
            .all(|step| !matches!(step, Step::Group(_)))
    );
}

#[test]
fn c8_marks_review_and_manual_stages_as_waiting_for_a_person_whatever_the_gate_says() {
    let steps = vec![
        stage_step("review", StageKind::Review, Gate::Auto),
        stage_step("qa", StageKind::Manual, Gate::Auto),
    ];
    assert_eq!(node(&steps, "review"), "Manual review, waits for approval");
    assert_eq!(node(&steps, "qa"), "Manual testing, waits for approval");
}

#[test]
fn names_the_branch_and_its_lanes_for_a_screen_reader() {
    let steps = branched();
    let branch = find_branch(&steps, "after-plan").unwrap();
    assert_eq!(
        branch_button_label(branch),
        "Branch After plan, one path of 2"
    );
    assert_eq!(branch_sub_text(branch), "One path of 2, checked in order");
    assert_eq!(fork_label(branch), "After plan, 2 paths, checked in order");
    assert_eq!(path_stages_label(&branch.paths[0]), "Small change, stages");
    assert_eq!(path_stages_label(&branch.paths[1]), "Review PR, stages");
    assert_eq!(node(&steps, "implement"), "Implement");
    assert_eq!(node(&steps, "testing"), "Automated tests");
    assert_eq!(node(&steps, "review"), "Manual review, waits for approval");
}

#[test]
fn names_the_branch_for_a_screen_reader_with_its_paths_and_conditions() {
    let steps = vec![
        branched()[1].clone(),
        branched()[2].clone(),
        branched()[3].clone(),
    ];
    assert_eq!(
        pipeline_label(&steps),
        "Pipeline: Plan, then After plan, one path of 2: Small change if plan.md \"Size\" contains \"small\"; Review PR if test -f .no-review does not succeed, then Release"
    );
}

#[test]
fn labels_each_lane_with_its_name_and_condition_and_the_full_text_in_the_tooltip() {
    let steps = branched();
    assert_eq!(
        when_label(lane(&steps, "small")),
        "Small change if plan.md \u{203a} Size has \"small\""
    );
    assert_eq!(
        when_tip(lane(&steps, "small")),
        "plan.md \"Size\" contains \"small\""
    );
    assert_eq!(
        when_label(lane(&steps, "pr-review")),
        "Review PR if NOT test -f .no-review succeeds"
    );
    assert_eq!(
        when_tip(lane(&steps, "pr-review")),
        "test -f .no-review does not succeed"
    );
}

#[test]
fn draws_the_skip_line_when_no_path_is_the_otherwise_path_and_names_the_next_step() {
    let steps = branched();
    let branch = find_branch(&steps, "after-plan").unwrap();
    assert_eq!(
        skip_text(branch, steps.get(3)).as_deref(),
        Some("No path holds \u{2192} skip to Release")
    );
}

#[test]
fn says_the_end_when_the_branch_is_the_last_step() {
    let steps = branched();
    let branch = find_branch(&steps, "after-plan").unwrap();
    assert_eq!(
        skip_text(branch, None).as_deref(),
        Some("No path holds \u{2192} skip to the end")
    );
}

#[test]
fn draws_no_skip_line_with_an_otherwise_path() {
    let steps = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let branch = find_branch(&steps, "after-plan").unwrap();
    assert_eq!(skip_text(branch, steps.get(3)), None);
    assert_eq!(when_label(lane(&steps, "pr-review")), "Review PR Otherwise");
    assert_eq!(when_tip(lane(&steps, "pr-review")), "Otherwise");
}

#[test]
fn writes_the_branch_in_words_under_the_chart() {
    let steps = branched();
    let branch = find_branch(&steps, "after-plan").unwrap();
    assert_eq!(paths_list_label(branch), "Paths of After plan");
    assert_eq!(
        branch_note_head(branch),
        "After plan \u{b7} one path of 2, checked in order:"
    );
    let notes: Vec<String> = branch.paths.iter().map(path_note).collect();
    assert_eq!(
        notes,
        [
            "Small change \u{b7} plan.md \"Size\" contains \"small\" \u{2192} Implement, Automated tests",
            "Review PR \u{b7} test -f .no-review does not succeed \u{2192} Manual review",
        ]
    );
}

#[test]
fn draws_a_branch_next_to_a_group() {
    let mut steps = branched()[..2].to_vec();
    steps.push(group_of_stages("finish", &["docs", "i18n"]));
    let Step::Group(group) = &steps[2] else {
        panic!("not a group");
    };
    assert_eq!(
        group_list_label(group),
        "Documentation and Translations, side by side"
    );
}

#[test]
fn draws_four_paths_as_four_lanes() {
    let mut paths = Vec::new();
    for (at, id) in ["a", "b", "c", "d"].iter().enumerate() {
        paths.push(BranchPath {
            id: id.to_string(),
            name: None,
            when: Some(natamux_proto::stages::schema::Condition::Command {
                run: format!("test {at}"),
            }),
            not: None,
            stages: vec![Stage::new(
                &format!("stage-{id}"),
                StageKind::Agent,
                Gate::Auto,
            )],
        });
    }
    let branch = natamux_proto::stages::schema::StageBranch {
        id: "four".into(),
        name: None,
        paths,
    };
    assert_eq!(branch_button_label(&branch), "Branch Four, one path of 4");
    assert_eq!(branch.paths.iter().map(path_stages_label).count(), 4);
}

#[test]
fn marks_the_branch_and_a_stage_of_a_path_that_have_an_error() {
    let issue = |stage: &str| PipelineIssue {
        stage: Some(stage.into()),
        message: "A path has at least one stage".into(),
        level: IssueLevel::Error,
    };
    let wrong = error_ids(&[issue("after-plan"), issue("review")]);
    assert!(wrong.contains("after-plan"));
    assert!(wrong.contains("review"));
    assert!(!wrong.contains("implement"));
    let warning = PipelineIssue {
        level: IssueLevel::Warning,
        ..issue("plan")
    };
    assert!(error_ids(&[warning]).is_empty());
}

#[test]
fn leaves_out_the_stages_that_are_off_and_a_path_with_none_on() {
    let steps = set_enabled(&branched(), "review", false);
    assert!(pipeline_label(&steps).contains("one path of 1"));
}

#[test]
fn draws_the_stages_of_the_one_path_left_as_plain_stages_when_it_has_no_condition() {
    let otherwise = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let steps = set_enabled(
        &set_enabled(&otherwise, "implement", false),
        "testing",
        false,
    );
    assert!(
        active_steps(&steps)
            .iter()
            .all(|step| !matches!(step, Step::Branch(_)))
    );
    assert!(pipeline_label(&steps).contains("Manual review"));
}

fn own_steps() -> Vec<Step> {
    let mut opinion = Stage::new("second-opinion", StageKind::Agent, Gate::Auto);
    opinion.agent = Some("codex".into());
    opinion.args = Some("--model gpt-5-codex".into());
    opinion.output = Some("opinion.md".into());
    let mut polish = Stage::new("polish", StageKind::Agent, Gate::Auto);
    polish.agent = Some("cursor".into());
    let mut notes = Stage::new("notes", StageKind::Agent, Gate::Auto);
    notes.args = Some("--verbose".into());
    vec![
        stage_step("plan", StageKind::Agent, Gate::Manual),
        Step::Stage(opinion),
        Step::Stage(polish),
        Step::Stage(notes),
    ]
}

fn line(steps: &[Step], id: &str) -> Option<String> {
    let runner = stage_runner(steps, stage(steps, id))?;
    Some(agent_line_text(&runner, agent_label))
}

#[test]
fn shows_the_model_or_else_the_agent_under_the_name() {
    let steps = own_steps();
    assert_eq!(
        line(&steps, "second-opinion").as_deref(),
        Some("gpt-5-codex")
    );
    assert_eq!(line(&steps, "polish").as_deref(), Some("Cursor"));
    assert_eq!(line(&steps, "notes").as_deref(), Some("Task's agent"));
    assert_eq!(line(&steps, "plan"), None);
    assert!(shows_agents(&steps));
}

#[test]
fn tells_the_agent_and_model_in_the_tooltip_and_to_a_screen_reader() {
    let steps = own_steps();
    assert_eq!(
        tip(&steps, "second-opinion"),
        "Agent \u{b7} Moves on by itself \u{b7} Agent: Codex \u{b7} Model: gpt-5-codex \u{b7} Output: opinion.md"
    );
    assert_eq!(
        node(&steps, "second-opinion"),
        "Second opinion, runs Codex, model gpt-5-codex"
    );
    assert_eq!(
        tip(&steps, "notes"),
        "Agent \u{b7} Moves on by itself \u{b7} Agent: task's agent"
    );
    assert_eq!(node(&steps, "notes"), "Notes, runs the task's agent");
}

#[test]
fn shows_no_agent_line_for_a_stage_whose_agent_is_not_used_nor_in_the_default_pipeline() {
    let mut implement = Stage::new("implement", StageKind::Agent, Gate::Auto);
    implement.agent = Some("codex".into());
    let steps = vec![Step::Stage(implement)];
    assert_eq!(line(&steps, "implement"), None);
    assert!(!shows_agents(&steps));
    assert!(!shows_agents(&default_pipeline()));
}
