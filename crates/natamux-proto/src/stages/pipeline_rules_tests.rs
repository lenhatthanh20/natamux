use serde_json::{Value, json};

use super::*;
use crate::stages::catalog::default_pipeline;
use crate::stages::schema::Step;

pub(super) fn steps(value: Value) -> Vec<Step> {
    serde_json::from_value(value).unwrap()
}

/// The messages of `pipelineSchema.safeParse(value)`.
pub(super) fn messages(value: Value) -> Vec<String> {
    pipeline_problems(&steps(value))
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

pub(super) fn valid(value: Value) -> bool {
    messages(value).is_empty()
}

pub(super) fn stage(id: &str, kind: &str) -> Value {
    json!({"id": id, "kind": kind, "gate": "manual"})
}

pub(super) fn agent(id: &str) -> Value {
    json!({"id": id, "kind": "agent", "gate": "auto"})
}

#[test]
fn accepts_the_default_pipeline() {
    assert!(pipeline_problems(&default_pipeline()).is_empty());
}

#[test]
fn rejects_a_stage_id_used_twice_also_inside_a_group() {
    let twice = json!([
        stage("spec", "agent"),
        {"id": "build", "parallel": [stage("spec", "agent"), agent("implement")]}
    ]);
    assert!(!valid(twice));
}

#[test]
fn allows_only_agent_and_manual_stages_2_to_4_in_a_group() {
    let group = |parallel: Vec<Value>| json!([{"id": "build", "parallel": parallel}]);
    let size = "A group holds 2 to 4 stages";
    assert!(messages(group(vec![agent("a")])).contains(&size.to_string()));
    assert!(valid(group(vec![agent("a"), agent("b")])));
    let five = ["a", "b", "c", "d", "e"].map(agent).to_vec();
    assert_eq!(messages(group(five)), [size]);
    assert!(!valid(group(vec![agent("a"), stage("r", "review")])));
}

#[test]
fn keeps_the_context_first_and_the_release_last_one_of_each() {
    assert!(valid(json!([
        stage("context", "context"),
        stage("ship", "release")
    ])));
    assert!(!valid(json!([
        stage("check", "manual"),
        stage("context", "context")
    ])));
    assert!(!valid(json!([
        stage("context", "context"),
        stage("more", "context")
    ])));
    assert!(!valid(json!([
        stage("ship", "release"),
        stage("check", "manual")
    ])));
    assert!(!valid(json!([
        stage("ship", "release"),
        stage("again", "release")
    ])));
}

#[test]
fn takes_ids_of_lower_case_words_and_digits_joined_by_up_to_40_characters() {
    for id in ["load-test", "e2e", "2fa-check"] {
        assert_eq!(stage_id_problem(id), None, "{id}");
    }
    let long = "a".repeat(41);
    let wrong = [
        "Load",
        "load test",
        "-load",
        "load-",
        "load--test",
        "lo_ad",
        "",
        &long,
    ];
    for bad in wrong {
        assert!(stage_id_problem(bad).is_some(), "{bad}");
    }
    assert_eq!(
        stage_id_problem("Load").as_deref(),
        Some("A stage id is lower case words joined by \"-\"")
    );
    assert_eq!(
        stage_id_problem(&long).as_deref(),
        Some("Too big: expected string to have <=40 characters")
    );
}

#[test]
fn takes_only_true_or_false_for_enabled() {
    let plan = json!({"id": "plan", "kind": "agent", "gate": "manual"});
    let with = |enabled: Value| json!([{"id": "plan", "kind": "agent", "gate": "manual", "enabled": enabled}]);
    assert!(valid(with(json!(true))));
    // A wrong type cannot be read, so the file is refused before the rules run.
    assert!(serde_json::from_value::<Vec<Step>>(with(json!("no"))).is_err());
    assert!(serde_json::from_value::<Vec<Step>>(json!([plan])).is_ok());
}

#[test]
fn lets_a_manual_stage_run_beside_an_agent_stage() {
    let steps = json!([{"id": "build", "parallel": [agent("implement"), stage("qa", "manual")]}]);
    assert!(valid(steps));
}

#[test]
fn takes_a_workflow_on_agent_stages_only_also_inside_a_group() {
    let flow =
        |id: &str, kind: &str| json!({"id": id, "kind": kind, "gate": "manual", "workflow": true});
    assert!(valid(json!([flow("plan", "agent")])));
    assert!(valid(
        json!([{"id": "build", "parallel": [flow("a", "agent"), stage("b", "manual")]}])
    ));
    for kind in ["context", "review", "testing", "release", "manual"] {
        let found = messages(json!([flow("x", kind)]));
        assert!(!found.is_empty() && found[0].contains("workflow"), "{kind}");
    }
    assert!(valid(
        json!([{"id": "r", "kind": "review", "gate": "manual", "workflow": false}])
    ));
}

#[test]
fn takes_endshell_on_stages_that_start_an_agent_or_a_command_only() {
    let ends =
        |kind: &str, end: bool| json!([{"id": "x", "kind": kind, "gate": "auto", "endShell": end}]);
    for kind in ["context", "agent", "testing"] {
        assert!(valid(ends(kind, true)), "{kind}");
    }
    for kind in ["review", "release", "manual"] {
        assert_eq!(
            messages(ends(kind, true)),
            [format!(
                "Only a stage that starts an agent or a command can end its terminal, and \"x\" is a {kind} stage"
            )]
        );
    }
    assert!(valid(ends("review", false)));
    let wrong = json!([{"id": "plan", "kind": "agent", "gate": "manual", "endShell": "yes"}]);
    assert!(serde_json::from_value::<Vec<Step>>(wrong).is_err());
}

#[test]
fn rejects_a_workflow_that_is_not_true_or_false() {
    for workflow in [json!("yes"), json!(1), Value::Null] {
        let steps =
            json!([{"id": "plan", "kind": "agent", "gate": "manual", "workflow": workflow}]);
        let read = serde_json::from_value::<Vec<Step>>(steps);
        // null reads as "not given", which the daemon refuses on its own.
        assert!(read.is_err() || workflow.is_null());
    }
}

#[test]
fn rejects_a_workflow_on_a_manual_stage_inside_a_group_and_names_the_first_of_several() {
    let in_group = messages(json!([{"id": "build", "parallel": [
        {"id": "code", "kind": "agent", "gate": "auto", "workflow": true},
        {"id": "sign-off", "kind": "manual", "gate": "manual", "workflow": true}]}]));
    assert_eq!(
        in_group,
        ["Only agent stages can run with a workflow, and \"sign-off\" is a manual stage"]
    );
    let several = messages(json!([
        {"id": "check", "kind": "manual", "gate": "manual", "workflow": true},
        {"id": "review", "kind": "review", "gate": "manual", "workflow": true}]));
    assert!(several[0].contains("\"check\""));
    assert_eq!(several.len(), 2);
}

mod own_agent {
    use super::*;

    fn own() -> Value {
        json!({"agent": "codex", "args": "--model gpt-5"})
    }

    fn with(base: Value, extra: Value) -> Value {
        let mut merged = base;
        for (key, value) in extra.as_object().unwrap() {
            merged[key] = value.clone();
        }
        merged
    }

    fn build(test_spec: Value, implement: Value) -> Value {
        json!({"id": "build", "parallel": [
            with(json!({"id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md"}), test_spec),
            with(agent("implement"), implement)]})
    }

    fn message(id: &str) -> String {
        format!(
            "Only a stage outside Terminal 1, or a custom stage that an agent works on, can have its own agent or arguments, and \"{id}\" cannot"
        )
    }

    #[test]
    fn takes_them_on_a_stage_that_runs_beside_the_stage_in_shell_1() {
        let parsed = steps(json!([build(own(), json!({}))]));
        assert_eq!(parsed[0].stages()[0].agent.as_deref(), Some("codex"));
        assert_eq!(parsed[0].stages()[0].args.as_deref(), Some("--model gpt-5"));
        assert!(valid(json!([build(
            json!({"args": "--model x"}),
            json!({})
        )])));
    }

    #[test]
    fn rejects_them_on_the_stage_in_shell_1_outside_a_group_and_on_a_manual_stage() {
        assert_eq!(
            messages(json!([build(json!({}), own())])),
            [message("implement")]
        );
        let spec = with(
            json!({"id": "spec", "kind": "agent", "gate": "manual"}),
            own(),
        );
        assert_eq!(messages(json!([spec])), [message("spec")]);
        let group = json!([{"id": "build", "parallel": [agent("implement"),
            {"id": "sign-off", "kind": "manual", "gate": "manual", "agent": "codex"}]}]);
        assert_eq!(messages(group), [message("sign-off")]);
    }

    #[test]
    fn takes_them_on_a_stage_that_runs_in_its_own_shell_outside_a_group() {
        let review =
            json!({"id": "agent-review", "kind": "agent", "gate": "auto", "output": "review.md"});
        assert!(valid(json!([with(
            with(review.clone(), json!({"ownShell": true})),
            own()
        )])));
        assert!(!valid(json!([with(review, own())])));
    }

    #[test]
    fn rejects_empty_words() {
        assert!(!valid(json!([build(json!({"args": " "}), json!({}))])));
        assert!(!valid(json!([build(json!({"agent": ""}), json!({}))])));
    }

    #[test]
    fn takes_them_on_a_stage_of_your_own_that_an_agent_works_on_also_where_shell_1_is() {
        let docs = with(
            json!({"id": "write-docs", "kind": "agent", "gate": "auto"}),
            own(),
        );
        let lint = with(
            json!({"id": "check-types", "kind": "testing", "gate": "auto"}),
            own(),
        );
        assert!(valid(json!([docs.clone()])));
        assert!(valid(json!([lint])));
        let notes = json!({"id": "notes", "kind": "agent", "gate": "auto", "output": "notes.md"});
        assert!(valid(json!([{"id": "build", "parallel": [docs, notes]}])));
    }

    #[test]
    fn rejects_them_on_a_stage_of_your_own_that_no_agent_works_on() {
        for kind in ["context", "review", "release", "manual"] {
            let stage = with(stage("my-step", kind), own());
            assert_eq!(messages(json!([stage])), [message("my-step")], "{kind}");
        }
    }
}

#[test]
fn is_only_for_an_agent_stage_that_writes_a_report() {
    let message =
        "Only an agent stage with an output file can run in its own terminal, and \"x\" cannot";
    let issues = |stage: Value| {
        let mut full = json!({"id": "x", "gate": "auto", "ownShell": true});
        for (key, value) in stage.as_object().unwrap() {
            full[key] = value.clone();
        }
        messages(json!([full]))
    };
    assert!(issues(json!({"kind": "agent", "output": "x.md"})).is_empty());
    assert_eq!(issues(json!({"kind": "agent"}))[0], message);
    assert_eq!(issues(json!({"kind": "review"}))[0], message);
}

#[test]
fn rejects_an_empty_pipeline_and_a_bad_stage_id() {
    assert!(!valid(json!([])));
    assert!(!valid(
        json!([{"id": "Spec", "kind": "agent", "gate": "auto"}])
    ));
}

#[test]
fn leaves_them_out_of_the_rules_for_the_context_and_the_release() {
    let off =
        |id: &str, kind: &str| json!({"id": id, "kind": kind, "gate": "manual", "enabled": false});
    assert!(valid(json!([
        off("check", "manual"),
        stage("context", "context")
    ])));
    assert!(valid(json!([
        stage("ship", "release"),
        off("check", "manual")
    ])));
    assert!(valid(json!([
        stage("context", "context"),
        off("more", "context")
    ])));
}

#[test]
fn still_needs_unique_ids_and_one_stage_that_is_on() {
    let off = |id: &str| json!({"id": id, "kind": "manual", "gate": "manual", "enabled": false});
    assert!(!valid(json!([stage("check", "manual"), off("check")])));
    assert!(!valid(json!([off("check")])));
}

#[test]
fn needs_two_stages_that_are_on_in_a_group() {
    let off = |id: &str| json!({"id": id, "kind": "agent", "gate": "manual", "enabled": false});
    let group = |parallel: Vec<Value>| json!([stage("first", "manual"), {"id": "build", "parallel": parallel}]);
    let on = |id: &str| stage(id, "agent");
    assert!(!valid(group(vec![on("a"), off("b")])));
    assert!(valid(group(vec![off("a"), off("b")])));
    assert!(valid(group(vec![on("a"), on("b"), off("c")])));
}
