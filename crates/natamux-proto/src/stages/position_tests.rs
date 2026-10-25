use super::*;
use crate::stages::schema::{Gate, Stage, StageGroup, StageKind};
use serde_json::json;

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

fn small() -> BranchPath {
    BranchPath {
        id: "small".into(),
        name: Some("Small change".into()),
        when: Some(Condition::Output {
            file: "plan.md".into(),
            section: Some("Size".into()),
            contains: "small".into(),
        }),
        not: None,
        stages: vec![
            stage("implement", StageKind::Agent, Gate::Auto),
            stage("testing", StageKind::Testing, Gate::Auto),
        ],
    }
}

fn pr_review() -> BranchPath {
    BranchPath {
        id: "pr-review".into(),
        name: Some("Review PR".into()),
        when: Some(Condition::Command {
            run: "test -f .no-review".into(),
        }),
        not: Some(true),
        stages: vec![stage("review", StageKind::Review, Gate::Manual)],
    }
}

fn branch() -> StageBranch {
    StageBranch {
        id: "after-plan".into(),
        name: Some("After plan".into()),
        paths: vec![small(), pr_review()],
    }
}

fn steps() -> Vec<Step> {
    let mut plan = stage("plan", StageKind::Agent, Gate::Manual);
    plan.output = Some("plan.md".into());
    vec![
        Step::Stage(plan),
        Step::Branch(branch()),
        Step::Stage(stage("release", StageKind::Release, Gate::Manual)),
    ]
}

fn only(condition: Option<Condition>, not: Option<bool>) -> BranchPath {
    BranchPath {
        id: "x".into(),
        name: None,
        when: condition,
        not,
        stages: Vec::new(),
    }
}

#[test]
fn finds_the_step_of_a_stage_inside_a_group() {
    let build = Step::Group(StageGroup {
        id: "build".into(),
        name: None,
        parallel: vec![
            stage("test-spec", StageKind::Agent, Gate::Manual),
            stage("implement", StageKind::Agent, Gate::Auto),
        ],
    });
    let steps = [
        Step::Stage(stage("context", StageKind::Context, Gate::Manual)),
        build,
    ];
    assert_eq!(step_index(&steps, "implement"), Some(1));
    assert_eq!(step_index(&steps, "build"), Some(1));
    assert_eq!(step_index(&steps, "nope"), None);
}

#[test]
fn finds_the_stages_of_its_paths_and_the_path_of_a_stage() {
    let steps = steps();
    let ids: Vec<&str> = steps[1]
        .stages()
        .iter()
        .map(|each| each.id.as_str())
        .collect();
    assert_eq!(ids, ["implement", "testing", "review"]);
    assert_eq!(step_index(&steps, "testing"), Some(1));
    assert_eq!(step_index(&steps, "after-plan"), Some(1));
    let (branch, small, pr_review) = (branch(), small(), pr_review());
    assert_eq!(
        path_of(&steps, "testing"),
        Some(PathPlace {
            branch: &branch,
            path: &small,
            index: 1
        })
    );
    assert_eq!(
        path_of(&steps, "review"),
        Some(PathPlace {
            branch: &branch,
            path: &pr_review,
            index: 0
        })
    );
    assert_eq!(path_of(&steps, "plan"), None);
    assert_eq!(path_of(&steps, "nope"), None);
}

#[test]
fn says_its_condition_in_words() {
    assert_eq!(
        describe_condition(&small()),
        "plan.md \"Size\" contains \"small\""
    );
    let file = Condition::Output {
        file: "plan.md".into(),
        section: None,
        contains: "small".into(),
    };
    assert_eq!(
        describe_condition(&only(Some(file.clone()), None)),
        "plan.md contains \"small\""
    );
    assert_eq!(
        describe_condition(&only(Some(file), Some(true))),
        "plan.md does not contain \"small\""
    );
    let command = Condition::Command {
        run: "test -f x".into(),
    };
    assert_eq!(
        describe_condition(&only(Some(command), None)),
        "test -f x succeeds"
    );
    assert_eq!(
        describe_condition(&pr_review()),
        "test -f .no-review does not succeed"
    );
    assert_eq!(describe_condition(&only(None, None)), "Otherwise");
    assert_eq!(describe_condition(&only(None, Some(true))), "Otherwise");
}

fn paths(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn tells_the_stages_of_a_path_the_task_did_not_take() {
    let steps = steps();
    let small = paths(json!({"after-plan": "small"}));
    assert!(is_not_taken(&steps, &small, "review"));
    assert!(!is_not_taken(&steps, &small, "implement"));
    assert!(is_not_taken(
        &steps,
        &paths(json!({"after-plan": ""})),
        "implement"
    ));
    assert!(!is_not_taken(&steps, &small, "plan"));
    assert!(!is_not_taken(&steps, &Map::new(), "review"));
}
