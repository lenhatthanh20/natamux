use serde_json::{Value, json};

use super::tests::{agent, messages, stage, steps, valid};
use crate::stages::pipeline_rules::pipeline_problems;
use crate::stages::schema::Step;

fn plan() -> Value {
    json!({"id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md"})
}

fn review() -> Value {
    stage("review", "review")
}

fn release() -> Value {
    stage("release", "release")
}

fn off(stage: Value) -> Value {
    let mut stage = stage;
    stage["enabled"] = json!(false);
    stage
}

fn with_paths(paths: Vec<Value>) -> Value {
    json!([plan(), {"id": "after-plan", "paths": paths}, release()])
}

fn path(id: &str, stages: Vec<Value>) -> Value {
    json!({"id": id, "when": {"type": "command", "run": "true"}, "stages": stages})
}

fn otherwise(id: &str, stages: Vec<Value>) -> Value {
    json!({"id": id, "stages": stages})
}

fn small() -> Value {
    json!({"id": "small", "name": "Small change",
        "when": {"type": "output", "file": "plan.md", "section": "Size", "contains": "small"},
        "stages": [agent("implement"), {"id": "testing", "kind": "testing", "gate": "auto"}]})
}

fn pr_review() -> Value {
    json!({"id": "pr-review", "name": "Review PR",
        "when": {"type": "command", "run": "test -f .no-review"}, "not": true, "stages": [review()]})
}

fn branch() -> Value {
    json!({"id": "after-plan", "name": "After plan", "paths": [small(), pr_review()]})
}

#[test]
fn reads_a_branch_and_gives_it_back_the_same_as_neither_a_stage_nor_a_group() {
    let value = json!([plan(), branch(), release()]);
    let parsed = steps(value.clone());
    assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    assert!(parsed[1].is_branch() && !parsed[1].is_group() && !parsed[1].is_stage());
    assert!(!parsed[0].is_branch() && parsed[0].is_stage());
    assert!(pipeline_problems(&parsed).is_empty());
    // No names, no section, and an Otherwise path last.
    let plain = with_paths(vec![
        json!({"id": "a", "when": {"type": "output", "file": "plan.md", "contains": "small"}, "stages": [agent("implement")]}),
        otherwise("b", vec![review()]),
    ]);
    assert_eq!(serde_json::to_value(steps(plain.clone())).unwrap(), plain);
}

#[test]
fn holds_2_to_4_paths() {
    let paths: Vec<Value> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|id| path(id, vec![agent(&format!("stage-{id}"))]))
        .collect();
    let size = "A branch holds 2 to 4 paths";
    assert_eq!(messages(with_paths(paths[..1].to_vec())), [size]);
    assert!(valid(with_paths(paths[..2].to_vec())));
    assert!(valid(with_paths(paths[..4].to_vec())));
    assert_eq!(messages(with_paths(paths)), [size]);
}

#[test]
fn needs_ids_that_are_unique_across_stages_paths_and_branches() {
    let implement = || agent("implement");
    let twice = |id: &str| format!("The path id \"{id}\" is used twice");
    let stage_twice = |id: &str| format!("The stage id \"{id}\" is used twice");
    let one = |first: Vec<Value>, second: Vec<Value>, ids: (&str, &str)| {
        messages(with_paths(vec![path(ids.0, first), path(ids.1, second)]))
    };
    assert_eq!(
        one(vec![implement()], vec![review()], ("a", "a")),
        [twice("a")]
    );
    assert_eq!(
        one(vec![implement()], vec![review()], ("plan", "b")),
        [twice("plan")]
    );
    assert_eq!(
        one(vec![implement()], vec![implement()], ("a", "b")),
        [stage_twice("implement")]
    );
    assert_eq!(
        one(vec![plan()], vec![review()], ("a", "b")),
        [stage_twice("plan")]
    );
}

#[test]
fn takes_the_otherwise_path_with_no_condition_only_last() {
    let message = "Only the last path can be the Otherwise path";
    let implement = || agent("implement");
    assert_eq!(
        messages(with_paths(vec![
            otherwise("a", vec![implement()]),
            path("b", vec![review()])
        ])),
        [message]
    );
    assert_eq!(
        messages(with_paths(vec![
            otherwise("a", vec![implement()]),
            otherwise("b", vec![review()])
        ])),
        [message]
    );
    assert!(valid(with_paths(vec![
        path("a", vec![implement()]),
        otherwise("b", vec![review()])
    ])));
}

#[test]
fn needs_a_stage_in_each_path_and_single_stages_only() {
    assert_eq!(
        messages(with_paths(vec![
            path("a", vec![]),
            path("b", vec![review()])
        ])),
        ["A path has at least one stage"]
    );
    // A group or a branch inside a path cannot be read.
    let group = json!({"id": "build", "parallel": [agent("implement"), agent("docs")]});
    let nested = json!({"id": "inner", "paths": [path("x", vec![agent("implement")])]});
    for inner in [group, nested] {
        let value = with_paths(vec![path("a", vec![inner]), path("b", vec![review()])]);
        assert!(serde_json::from_value::<Vec<Step>>(value).is_err());
    }
}

#[test]
fn takes_no_context_or_release_stage_in_a_path() {
    let context = json!({"id": "more", "kind": "context", "gate": "auto"});
    assert_eq!(
        messages(with_paths(vec![
            path("a", vec![context]),
            path("b", vec![review()])
        ])),
        ["\"more\" is a context stage, which cannot be in a path"]
    );
    let ship = json!({"id": "ship", "kind": "release", "gate": "manual"});
    assert_eq!(
        messages(with_paths(vec![
            path("a", vec![agent("implement"), ship]),
            path("b", vec![review()])
        ])),
        ["\"ship\" is a release stage, which cannot be in a path"]
    );
}

#[test]
fn cannot_be_the_first_step_or_come_right_after_a_branch_but_may_be_the_last() {
    let first = "A branch cannot be the first step";
    assert_eq!(messages(json!([branch(), release()])), [first]);
    assert_eq!(messages(json!([off(plan()), branch(), release()])), [first]);
    assert!(valid(json!([plan(), branch()])));
    let write_docs = || {
        let mut docs = plan();
        docs["id"] = json!("write-docs");
        docs
    };
    let next = |docs: Value| {
        let mut qa = review();
        qa["id"] = json!("qa");
        json!({"id": "after-build", "paths": [path("docs", vec![docs]), otherwise("none", vec![qa])]})
    };
    // A skipped branch chooses no path for the next one, so a stage must end between them.
    let message = "A branch cannot come right after a branch. Put a stage between them";
    assert_eq!(
        messages(json!([plan(), branch(), next(write_docs()), release()])),
        [message]
    );
    let between = agent("docs-check");
    assert_eq!(
        messages(json!([
            plan(),
            branch(),
            off(between.clone()),
            next(write_docs()),
            release()
        ])),
        [message]
    );
    assert!(valid(json!([
        plan(),
        branch(),
        between,
        next(write_docs()),
        release()
    ])));
    // A branch left with only its Otherwise path runs it as plain steps.
    assert!(valid(json!([
        plan(),
        branch(),
        next(off(write_docs())),
        release()
    ])));
}

#[test]
fn checks_the_fields_of_a_condition() {
    let when = |condition: Value| {
        let mut first = path("a", vec![agent("implement")]);
        first["when"] = condition;
        messages(with_paths(vec![first, path("b", vec![review()])]))
    };
    assert!(when(json!({"type": "output", "file": "plan.md", "contains": "small"})).is_empty());
    for file in ["plan.txt", "", "../x.md"] {
        assert_eq!(
            when(json!({"type": "output", "file": file, "contains": "small"})),
            ["A condition file is a markdown file, such as plan.md"]
        );
    }
    assert_eq!(
        when(json!({"type": "output", "file": "plan.md", "contains": "  "})),
        ["Give the text to look for"]
    );
    assert_eq!(
        when(json!({"type": "command", "run": " "})),
        ["Give the command to run"]
    );
    // An unknown type cannot be read.
    let unknown = json!({"type": "label", "has": "x"});
    let value = with_paths(vec![
        path("a", vec![agent("implement")]),
        path("b", vec![review()]),
    ]);
    let mut value = value;
    value[1]["paths"][0]["when"] = unknown;
    assert!(serde_json::from_value::<Vec<Step>>(value).is_err());
}

#[test]
fn needs_another_path_that_is_on_when_the_one_left_has_a_condition() {
    assert_eq!(
        messages(with_paths(vec![
            path("a", vec![agent("implement")]),
            path("b", vec![off(review())])
        ])),
        ["Turn on a stage in another path, or remove the branch"]
    );
}

#[test]
fn leaves_out_stages_and_paths_that_are_off_and_runs_a_lone_otherwise_path_as_steps() {
    let lone = with_paths(vec![
        path("a", vec![off(agent("implement"))]),
        otherwise("b", vec![review(), stage("e2e", "testing")]),
    ]);
    assert!(valid(lone));
    let none = with_paths(vec![
        path("a", vec![off(review())]),
        otherwise("b", vec![off(stage("e2e", "testing"))]),
    ]);
    assert!(valid(none));
}
