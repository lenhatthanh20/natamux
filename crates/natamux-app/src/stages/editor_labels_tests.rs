use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};

use super::*;
use crate::stages::editor_branches::set_otherwise;
use crate::stages::editor_fields::set_enabled;
use crate::stages::editor_find::find_branch;
use crate::stages::editor_fixtures::{branched, stage_step};

#[test]
fn names_the_stages_that_are_on_in_order_with_groups_side_by_side() {
    assert_eq!(
        pipeline_label(&default_pipeline()),
        "Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Automated tests, then Review, then Release"
    );
    assert!(!pipeline_label(&set_enabled(&default_pipeline(), "plan", false)).contains("Plan"));
}

#[test]
fn says_each_branch_with_its_paths_for_a_screen_reader() {
    assert_eq!(
        pipeline_label(&branched()),
        "Pipeline: Context, then Plan, then After plan, one path of 2: Small change if plan.md \"Size\" contains \"small\"; Review PR if test -f .no-review does not succeed, then Release"
    );
    let otherwise = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let branch = find_branch(&otherwise, "after-plan").unwrap();
    assert_eq!(
        branch_label(branch),
        "After plan, one path of 2: Small change if plan.md \"Size\" contains \"small\"; Review PR, otherwise"
    );
}

#[test]
fn reads_the_model_from_the_words_of_a_command_line() {
    assert_eq!(model_of(Some("--model sonnet")).as_deref(), Some("sonnet"));
    assert_eq!(model_of(Some("--model=opus")).as_deref(), Some("opus"));
    assert_eq!(model_of(Some("-m gpt-5")).as_deref(), Some("gpt-5"));
    assert_eq!(
        model_of(Some("--verbose  --model gpt-5-codex --yolo")).as_deref(),
        Some("gpt-5-codex")
    );
}

#[test]
fn finds_no_model_when_the_args_name_none() {
    for args in [
        None,
        Some(""),
        Some("--verbose"),
        Some("--verbose --model"),
        Some("--model --verbose"),
        Some("--model="),
    ] {
        assert_eq!(model_of(args), None, "{args:?}");
    }
}

fn own(change: impl Fn(&mut Stage)) -> Vec<Step> {
    let mut mine = Stage::new("my-check", StageKind::Agent, Gate::Auto);
    change(&mut mine);
    vec![
        stage_step("plan", StageKind::Agent, Gate::Manual),
        Step::Stage(mine),
    ]
}

fn runner(steps: &[Step], id: &str) -> Option<StageRunner> {
    let stage = steps
        .iter()
        .flat_map(Step::stages)
        .find(|stage| stage.id == id)
        .unwrap();
    stage_runner(steps, stage)
}

fn runner_of(agent: Option<&str>, model: Option<&str>) -> Option<StageRunner> {
    Some(StageRunner {
        agent: agent.map(str::to_string),
        model: model.map(str::to_string),
    })
}

#[test]
fn gives_the_agent_and_model_of_a_stage_that_starts_its_own_agent() {
    let steps = own(|stage| {
        stage.agent = Some("codex".into());
        stage.args = Some("--model gpt-5-codex".into());
    });
    assert_eq!(
        runner(&steps, "my-check"),
        runner_of(Some("codex"), Some("gpt-5-codex"))
    );
    let cursor = own(|stage| stage.agent = Some("cursor".into()));
    assert_eq!(runner(&cursor, "my-check"), runner_of(Some("cursor"), None));
    let opus = own(|stage| stage.args = Some("--model=opus".into()));
    assert_eq!(runner(&opus, "my-check"), runner_of(None, Some("opus")));
    let verbose = own(|stage| stage.args = Some("--verbose".into()));
    assert_eq!(runner(&verbose, "my-check"), runner_of(None, None));
}

#[test]
fn gives_nothing_for_a_stage_that_keeps_the_task_agent_or_whose_agent_is_not_used() {
    assert_eq!(runner(&own(|_| {}), "my-check"), None);
    let mut implement = Stage::new("implement", StageKind::Agent, Gate::Auto);
    implement.agent = Some("codex".into());
    assert_eq!(runner(&[Step::Stage(implement)], "implement"), None);
}

#[test]
fn joins_names_with_commas_and_and() {
    let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    assert_eq!(join_names(&names(&[])), "");
    assert_eq!(join_names(&names(&["A"])), "A");
    assert_eq!(join_names(&names(&["A", "B"])), "A and B");
    assert_eq!(join_names(&names(&["A", "B", "C"])), "A, B and C");
}

#[test]
fn labels_the_kind_and_the_approval_of_a_stage() {
    assert_eq!(kind_label(StageKind::Review), "Code review");
    assert_eq!(kind_label(StageKind::Manual), "Check by a person");
    let release = |gate| Stage::new("release", StageKind::Release, gate);
    assert_eq!(
        approval_label(&release(Gate::Manual)),
        "Opens the pull request when you click"
    );
    assert_eq!(
        approval_label(&release(Gate::Auto)),
        "Opens the pull request by itself"
    );
    assert_eq!(
        approval_label(&Stage::new("qa", StageKind::Manual, Gate::Auto)),
        "Always waits for you"
    );
    assert_eq!(
        approval_label(&Stage::new("x", StageKind::Agent, Gate::Manual)),
        "Waits for your approval"
    );
    assert_eq!(
        approval_label(&Stage::new("x", StageKind::Agent, Gate::Auto)),
        "Moves on by itself"
    );
}

#[test]
fn says_no_stage_is_on_for_a_pipeline_with_nothing_on() {
    let off = set_enabled(
        &[stage_step("plan", StageKind::Agent, Gate::Manual)],
        "plan",
        false,
    );
    assert_eq!(pipeline_label(&off), "Pipeline: no stage is on");
}
