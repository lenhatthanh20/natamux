use super::*;
use crate::stages::schema::{Gate, Stage, StageGroup, StageKind};
use serde_json::json;

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

fn one(id: &str, kind: StageKind, gate: Gate) -> Step {
    Step::Stage(stage(id, kind, gate))
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
            one("implement", StageKind::Agent, Gate::Auto),
            one("testing", StageKind::Testing, Gate::Auto),
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
        stages: vec![one("review", StageKind::Review, Gate::Manual)],
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
            index: 1,
            item: &small.stages[1],
        })
    );
    assert_eq!(
        path_of(&steps, "review"),
        Some(PathPlace {
            branch: &branch,
            path: &pr_review,
            index: 0,
            item: &pr_review.stages[0],
        })
    );
    assert_eq!(path_of(&steps, "plan"), None);
    assert_eq!(path_of(&steps, "nope"), None);
}

/// A branch whose bug path runs Reproduce and Investigate side by side, then Fix.
fn with_path_group() -> Vec<Step> {
    let triage = Step::Group(StageGroup {
        id: "triage".into(),
        name: None,
        parallel: vec![
            stage("repro", StageKind::Agent, Gate::Auto),
            stage("look", StageKind::Agent, Gate::Manual),
        ],
    });
    let bug = BranchPath {
        id: "bug".into(),
        name: None,
        when: Some(Condition::Command {
            run: "true".into(),
        }),
        not: None,
        stages: vec![triage, one("fix", StageKind::Agent, Gate::Auto)],
    };
    let other = BranchPath {
        id: "other".into(),
        name: None,
        when: None,
        not: None,
        stages: vec![one("plan", StageKind::Agent, Gate::Manual)],
    };
    vec![
        one("spec", StageKind::Agent, Gate::Manual),
        Step::Branch(StageBranch {
            id: "kind".into(),
            name: None,
            paths: vec![bug, other],
        }),
    ]
}

#[test]
fn finds_a_group_inside_a_path_and_runs_it_as_a_whole() {
    let steps = with_path_group();
    assert_eq!(step_index(&steps, "triage"), Some(1));
    assert_eq!(step_index(&steps, "look"), Some(1));
    for id in ["triage", "repro", "look"] {
        assert!(
            matches!(step_to_run(&steps, id), Some(Step::Group(group)) if group.id == "triage")
        );
    }
    assert!(matches!(step_to_run(&steps, "fix"), Some(Step::Stage(stage)) if stage.id == "fix"));
    assert!(matches!(step_to_run(&steps, "kind"), Some(Step::Branch(_))));
    let repro = path_of(&steps, "repro").unwrap();
    let look = path_of(&steps, "look").unwrap();
    assert_eq!((repro.path.id.as_str(), repro.index), ("bug", 0));
    assert_eq!(look.index, 0);
    assert_eq!(path_of(&steps, "triage").unwrap().index, 0);
    assert_eq!(path_of(&steps, "fix").unwrap().index, 1);
    let taken = paths(json!({"kind": "other"}));
    assert!(is_not_taken(&steps, &taken, "look"));
    assert!(!is_not_taken(&steps, &taken, "plan"));
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
