use serde_json::{Value, json};

use super::tests::steps;
use super::*;
use crate::stages::catalog::default_pipeline;

fn issues(value: Value) -> Vec<PipelineIssue> {
    pipeline_issues(&steps(value))
}

fn error(stage: &str, message: &str) -> PipelineIssue {
    PipelineIssue {
        stage: Some(stage.into()),
        message: message.into(),
        level: IssueLevel::Error,
    }
}

fn stages_of(found: &[PipelineIssue]) -> Vec<Option<&str>> {
    found.iter().map(|issue| issue.stage.as_deref()).collect()
}

fn manual(id: &str) -> Value {
    json!({"id": id, "kind": "manual", "gate": "manual"})
}

#[test]
fn finds_nothing_wrong_with_the_default_pipeline() {
    assert!(pipeline_issues(&default_pipeline()).is_empty());
}

#[test]
fn names_the_stage_of_a_rule_that_fails() {
    let found = issues(json!([{"id": "ship", "kind": "release", "gate": "manual"}, manual("qa")]));
    assert_eq!(found, [error("ship", "The release stage must come last")]);
}

#[test]
fn names_the_stage_that_cannot_have_its_own_agent() {
    let found = issues(json!([{"id": "build", "parallel": [
        {"id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md"},
        {"id": "implement", "kind": "agent", "gate": "auto", "agent": "codex"}]}]));
    assert_eq!(
        found,
        [error(
            "implement",
            "Only a stage outside Terminal 1, or a custom stage that an agent works on, can have its own agent or arguments, and \"implement\" cannot"
        )]
    );
}

#[test]
fn names_the_field_and_the_stage_of_a_wrong_value() {
    let found =
        issues(json!([{"id": "spec", "kind": "agent", "gate": "manual", "output": "spec.txt"}]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].stage.as_deref(), Some("spec"));
    assert_eq!(found[0].level, IssueLevel::Error);
    assert!(found[0].message.starts_with("Output file: "));
}

#[test]
fn counts_a_command_of_spaces_as_none_and_leaves_out_stages_that_are_off() {
    let lint = json!({"id": "lint", "kind": "testing", "gate": "auto"});
    let spaces = json!({"id": "lint", "kind": "testing", "gate": "auto", "command": "   "});
    assert_eq!(stages_of(&issues(json!([spaces]))), [Some("lint")]);
    let off = json!({"id": "lint", "kind": "testing", "gate": "auto", "enabled": false});
    assert!(issues(json!([off, manual("qa")])).is_empty());
    assert!(!issues(json!([lint])).is_empty());
}

#[test]
fn needs_no_command_on_a_testing_stage_of_your_own_or_on_testing() {
    assert!(issues(json!([{"id": "load-test", "kind": "testing", "gate": "auto"}])).is_empty());
    assert!(issues(json!([{"id": "testing", "kind": "testing", "gate": "auto"}])).is_empty());
    // A stage of your own with a catalog id but another kind needs no command either.
    assert!(issues(json!([manual("lint")])).is_empty());
}

#[test]
fn names_a_group_that_is_too_big_or_holds_a_kind_that_cannot_run_side_by_side() {
    let agent =
        |id: &str| json!({"id": id, "kind": "agent", "gate": "auto", "output": format!("{id}.md")});
    let five: Vec<Value> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|id| agent(id))
        .collect();
    assert_eq!(
        stages_of(&issues(json!([{"id": "big", "parallel": five}]))),
        [Some("big")]
    );
    let mixed = issues(json!([{"id": "mixed", "parallel": [agent("a"),
        {"id": "t", "kind": "testing", "gate": "auto"}]}]));
    assert!(!mixed.is_empty());
    assert!(mixed.iter().all(|issue| issue.level == IssueLevel::Error));
}

#[test]
fn counts_two_problems_as_two() {
    let found = issues(json!([
        {"id": "ship", "kind": "release", "gate": "manual"},
        manual("qa"),
        {"id": "context", "kind": "context", "gate": "manual"}]));
    let texts: Vec<&str> = found.iter().map(|issue| issue.message.as_str()).collect();
    assert_eq!(
        texts,
        [
            "The context stage must come first",
            "The release stage must come last"
        ]
    );
}

#[test]
fn names_the_second_stage_of_an_id_used_twice() {
    let found = issues(json!([manual("qa"), manual("qa")]));
    assert_eq!(found, [error("qa", "The stage id \"qa\" is used twice")]);
}

#[test]
fn asks_for_a_command_where_the_catalog_needs_one() {
    let lint = json!({"id": "lint", "kind": "testing", "gate": "auto"});
    assert_eq!(
        issues(json!([lint])),
        [error("lint", "Give the command to run")]
    );
    let with = json!({"id": "lint", "kind": "testing", "gate": "auto", "command": "pnpm lint"});
    assert!(issues(json!([with])).is_empty());
}

#[test]
fn warns_about_two_code_changing_stages_side_by_side_without_stopping_a_save() {
    let found = issues(json!([{"id": "build", "parallel": [
        {"id": "implement", "kind": "agent", "gate": "auto"},
        {"id": "docs", "kind": "agent", "gate": "auto"}]}]));
    assert_eq!(
        found,
        [PipelineIssue {
            stage: Some("build".into()),
            message: "2 stages without an output file change code in one worktree at the same time"
                .into(),
            level: IssueLevel::Warning,
        }]
    );
}

mod of_a_branch {
    use super::*;

    fn branch(first: Value, second: Value) -> Value {
        json!([{"id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md"},
            {"id": "after-plan", "paths": [first, second]}])
    }

    fn other() -> Value {
        json!({"id": "other", "stages": [manual("qa")]})
    }

    fn when() -> Value {
        json!({"type": "command", "run": "true"})
    }

    #[test]
    fn names_the_stage_in_a_path_that_breaks_a_rule() {
        let ship = json!({"id": "ship", "kind": "release", "gate": "manual"});
        let found = issues(branch(
            json!({"id": "small", "when": when(), "stages": [ship]}),
            other(),
        ));
        assert_eq!(
            found,
            [error(
                "ship",
                "\"ship\" is a release stage, which cannot be in a path"
            )]
        );
    }

    #[test]
    fn names_the_branch_of_a_rule_about_its_paths_or_conditions() {
        let first = json!({"id": "small", "stages": [{"id": "implement", "kind": "agent", "gate": "auto"}]});
        let second = json!({"id": "other", "when": when(), "stages": [manual("qa")]});
        assert_eq!(
            issues(branch(first.clone(), second)),
            [error(
                "after-plan",
                "Only the last path can be the Otherwise path"
            )]
        );
        let mut empty = first;
        empty["when"] = json!({"type": "command", "run": " "});
        assert_eq!(
            issues(branch(empty, other())),
            [error("after-plan", "Give the command to run")]
        );
    }
}
