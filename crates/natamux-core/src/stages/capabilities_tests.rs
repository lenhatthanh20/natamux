//! Ports the cases of `packages/core/src/stages/__tests__/capabilities.test.ts` for the functions
//! that are in this crate. The others are tested in `natamux-proto`.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};

use super::*;
use crate::events::test_fixture::{branch, paths, plan};

fn code_id(steps: &[Step], id: &str, taken: Option<&Paths>) -> Option<String> {
    code_stage_before(steps, id, taken).map(|stage| stage.id)
}

#[test]
fn finds_the_last_stage_that_changes_code_before_a_stage() {
    let steps = default_pipeline();
    assert_eq!(
        code_id(&steps, "testing", None).as_deref(),
        Some("implement")
    );
    assert_eq!(code_id(&steps, "spec", None), None);
    assert_eq!(
        code_id(&steps, "agent-review", None).as_deref(),
        Some("implement")
    );
}

#[test]
fn finds_the_stage_that_changes_code_only_on_the_way_the_task_went() {
    let load_test = Stage {
        ci: Some(true),
        ..Stage::new("load-test", StageKind::Testing, Gate::Auto)
    };
    let all = vec![
        Step::Stage(plan()),
        Step::Branch(branch()),
        Step::Stage(load_test),
    ];
    assert_eq!(code_id(&all, "testing", None).as_deref(), Some("implement"));
    assert_eq!(code_id(&all, "implement", None), None);
    assert_eq!(
        code_id(&all, "load-test", None).as_deref(),
        Some("implement")
    );
    let small = paths(&[("after-plan", "small")]);
    assert_eq!(
        code_id(&all, "load-test", Some(&small)).as_deref(),
        Some("implement")
    );
    let review = paths(&[("after-plan", "pr-review")]);
    assert_eq!(code_id(&all, "load-test", Some(&review)), None);
    let skipped = paths(&[("after-plan", "")]);
    assert_eq!(code_id(&all, "load-test", Some(&skipped)), None);
}

#[test]
fn runs_a_workflow_when_the_stage_offers_it_the_task_chose_it_and_the_agent_can() {
    let steps = default_pipeline();
    let implement = steps
        .iter()
        .flat_map(|step| step.stages())
        .find(|stage| stage.id == "implement")
        .cloned()
        .unwrap();
    let chosen = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    assert!(runs_workflow(
        &implement,
        &chosen(&["implement"]),
        Some("claude-code")
    ));
    assert!(!runs_workflow(
        &implement,
        &chosen(&[]),
        Some("claude-code")
    ));
    assert!(!runs_workflow(
        &implement,
        &chosen(&["plan"]),
        Some("claude-code")
    ));
    assert!(!runs_workflow(
        &implement,
        &chosen(&["implement"]),
        Some("codex")
    ));
    let off = Stage {
        workflow: Some(false),
        ..implement
    };
    assert!(!runs_workflow(
        &off,
        &chosen(&["implement"]),
        Some("claude-code")
    ));
}
