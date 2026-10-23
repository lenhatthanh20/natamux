use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::{IssueLevel, pipeline_issues};
use natamux_proto::stages::schema::{Gate, PathRef, StageKind, Step, StepRef};

use super::*;
use crate::stages::editor_find::{find_branch, find_stage};
use crate::stages::editor_fixtures::{branched, ids, sid, stage_step};

fn plain_branch(id: &str, paths: &[(&str, &[&str])]) -> StepRef {
    StepRef::Branch {
        id: id.to_string(),
        paths: paths
            .iter()
            .map(|(path, stages)| PathRef {
                id: path.to_string(),
                stages: stages.iter().map(|stage| stage.to_string()).collect(),
            })
            .collect(),
    }
}

fn path_ids(steps: &[Step], branch: &str) -> Vec<String> {
    find_branch(steps, branch)
        .map(|branch| branch.paths.iter().map(|path| path.id.clone()).collect())
        .unwrap_or_default()
}

fn output_when(contains: &str) -> Condition {
    Condition::Output {
        file: "plan.md".to_string(),
        section: None,
        contains: contains.to_string(),
    }
}

#[test]
fn adds_a_branch_after_the_step_of_the_id_with_a_path_with_a_condition_and_an_otherwise_path() {
    let steps = add_branch(&default_pipeline(), Some("plan"));
    assert_eq!(
        ids(&steps)[2..5].to_vec(),
        vec![
            sid("plan"),
            plain_branch("branch", &[("path-1", &[]), ("path-2", &[])]),
            StepRef::Group {
                id: "build".into(),
                parallel: vec!["test-spec".into(), "implement".into()]
            }
        ]
    );
    let branch = find_branch(&steps, "branch").unwrap();
    assert_eq!(branch.paths[0].when, Some(output_when("")));
    assert_eq!(branch.paths[1].when, None);
}

#[test]
fn goes_after_the_branch_when_the_stage_is_in_a_path_and_before_the_release_at_the_end() {
    let steps = branched();
    assert_eq!(add_branch(&steps, Some("testing"))[3].id(), "branch");
    for added in [
        add_branch(&steps, Some("release")),
        add_branch(&steps, None),
    ] {
        let n = added.len();
        assert_eq!(added[n - 2].id(), "branch");
        assert_eq!(added[n - 1].id(), "release");
    }
}

#[test]
fn gives_ids_no_stage_group_or_path_uses() {
    let once = add_branch(&branched(), Some("plan"));
    let twice = add_branch(&once, Some("plan"));
    let all: Vec<String> = twice
        .iter()
        .flat_map(|step| match step {
            Step::Branch(branch) => [
                vec![branch.id.clone()],
                branch.paths.iter().map(|p| p.id.clone()).collect(),
            ]
            .concat(),
            _ => Vec::new(),
        })
        .collect();
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), all.len());
    assert_eq!(path_ids(&twice, "branch-2"), ["path-3", "path-4"]);
}

#[test]
fn is_an_error_until_each_path_has_a_stage() {
    let issues = pipeline_issues(&add_branch(&branched()[..2], Some("plan")));
    assert!(
        issues
            .iter()
            .any(|issue| issue.stage.as_deref() == Some("branch")
                && issue.level == IssueLevel::Error)
    );
}

#[test]
fn adds_a_path_with_a_condition_at_the_end_before_an_otherwise_path() {
    let three = add_path(&branched(), "after-plan");
    assert_eq!(
        path_ids(&three, "after-plan"),
        ["small", "pr-review", "path-3"]
    );
    assert!(matches!(
        find_branch(&three, "after-plan").unwrap().paths[2].when,
        Some(Condition::Output { .. })
    ));
    let otherwise = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let added = add_path(&otherwise, "after-plan");
    assert_eq!(
        path_ids(&added, "after-plan"),
        ["small", "path-3", "pr-review"]
    );
}

#[test]
fn stops_at_4_paths_and_for_a_branch_that_is_not_there() {
    let four = add_path(&add_path(&branched(), "after-plan"), "after-plan");
    assert_eq!(find_branch(&four, "after-plan").unwrap().paths.len(), 4);
    assert_eq!(add_path(&four, "after-plan"), four);
    assert_eq!(add_path(&branched(), "plan"), branched());
}

#[test]
fn removes_a_path_with_its_stages() {
    let steps = remove_path(&branched(), "after-plan", "small");
    assert_eq!(
        ids(&steps)[2],
        plain_branch("after-plan", &[("pr-review", &["review"])])
    );
    assert!(find_stage(&steps, "implement").is_none());
}

#[test]
fn keeps_the_last_path_and_ignores_an_unknown_branch() {
    let one = remove_path(&branched(), "after-plan", "small");
    assert_eq!(remove_path(&one, "after-plan", "pr-review"), one);
    assert_eq!(remove_path(&branched(), "nope", "small"), branched());
}

#[test]
fn sets_the_condition_and_does_not_hold_with_not_true() {
    let command = Condition::Command { run: "true".into() };
    let steps = set_path_condition(&branched(), "after-plan", "small", &command, true);
    let first = &find_branch(&steps, "after-plan").unwrap().paths[0];
    assert_eq!(first.when, Some(command));
    assert_eq!(first.not, Some(true));
    let holds = set_path_condition(&steps, "after-plan", "small", &output_when("small"), false);
    let first = &find_branch(&holds, "after-plan").unwrap().paths[0];
    assert_eq!(first.not, None);
    assert_eq!(first.when, Some(output_when("small")));
}

#[test]
fn leaves_the_other_paths_and_an_unknown_branch_alone() {
    let steps = set_path_condition(
        &branched(),
        "after-plan",
        "small",
        &output_when("small"),
        false,
    );
    assert_eq!(
        find_branch(&steps, "after-plan").unwrap().paths[1],
        find_branch(&branched(), "after-plan").unwrap().paths[1]
    );
    assert_eq!(
        set_path_condition(&branched(), "nope", "small", &output_when("small"), false),
        branched()
    );
}

#[test]
fn edits_one_field_and_leaves_an_empty_section_out() {
    let when = output_when("small");
    let with_section = |section: &str| Condition::Output {
        file: "plan.md".into(),
        section: Some(section.into()),
        contains: "small".into(),
    };
    assert_eq!(
        edit_condition(&when, ConditionField::File, "notes.md"),
        Condition::Output {
            file: "notes.md".into(),
            section: None,
            contains: "small".into()
        }
    );
    assert_eq!(
        edit_condition(&when, ConditionField::Contains, "big"),
        output_when("big")
    );
    assert_eq!(
        edit_condition(&when, ConditionField::Section, "Size"),
        with_section("Size")
    );
    assert_eq!(
        edit_condition(&with_section("Size"), ConditionField::Section, " "),
        when
    );
    let command = Condition::Command { run: "a".into() };
    assert_eq!(
        edit_condition(&command, ConditionField::Run, "b"),
        Condition::Command { run: "b".into() }
    );
    assert_eq!(edit_condition(&command, ConditionField::File, "b"), command);
}

#[test]
fn starts_a_blank_condition_of_a_type_and_keeps_the_one_of_the_same_type() {
    let when = output_when("small");
    assert_eq!(
        blank_condition(ConditionType::Command),
        Condition::Command { run: String::new() }
    );
    assert_eq!(blank_condition(ConditionType::Output), output_when(""));
    assert_eq!(retype_condition(Some(&when), ConditionType::Output), when);
    assert_eq!(
        retype_condition(Some(&when), ConditionType::Command),
        Condition::Command { run: String::new() }
    );
    assert_eq!(
        retype_condition(None, ConditionType::Command),
        Condition::Command { run: String::new() }
    );
}

#[test]
fn names_what_is_wrong_in_a_condition_by_field() {
    assert_eq!(
        condition_problems_by_field(&output_when("small")),
        ConditionProblems::default()
    );
    let bad = Condition::Output {
        file: "plan".into(),
        section: None,
        contains: " ".into(),
    };
    assert_eq!(
        condition_problems_by_field(&bad),
        ConditionProblems {
            file: Some("A condition file is a markdown file, such as plan.md".into()),
            contains: Some("Give the text to look for".into()),
            ..ConditionProblems::default()
        }
    );
    assert_eq!(
        condition_problems_by_field(&Condition::Command { run: String::new() }),
        ConditionProblems {
            run: Some("Give the command to run".into()),
            ..ConditionProblems::default()
        }
    );
}

#[test]
fn makes_the_last_path_the_otherwise_path_no_condition_no_not() {
    let steps = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let last = &find_branch(&steps, "after-plan").unwrap().paths[1];
    assert_eq!(last.when, None);
    assert_eq!(last.not, None);
    assert_eq!(last.stages.len(), 1);
}

#[test]
fn gives_a_blank_condition_back_when_it_is_turned_off() {
    let otherwise = set_otherwise(&branched(), "after-plan", "pr-review", true);
    let off = set_otherwise(&otherwise, "after-plan", "pr-review", false);
    assert_eq!(
        find_branch(&off, "after-plan").unwrap().paths[1].when,
        Some(blank_condition(ConditionType::Output))
    );
}

#[test]
fn works_on_the_last_path_only_and_does_nothing_when_it_is_already_so() {
    assert_eq!(
        set_otherwise(&branched(), "after-plan", "small", true),
        branched()
    );
    assert_eq!(
        set_otherwise(&branched(), "after-plan", "pr-review", false),
        branched()
    );
    let otherwise = set_otherwise(&branched(), "after-plan", "pr-review", true);
    assert_eq!(
        set_otherwise(&otherwise, "after-plan", "pr-review", true),
        otherwise
    );
}

#[test]
fn names_a_branch_and_a_path_and_an_empty_name_leaves_the_field_out() {
    let named = set_path_name(
        &set_branch_name(&branched(), "after-plan", "Choose"),
        "after-plan",
        "small",
        "Tiny",
    );
    let branch = find_branch(&named, "after-plan").unwrap();
    assert_eq!(branch.name.as_deref(), Some("Choose"));
    assert_eq!(branch.paths[0].name.as_deref(), Some("Tiny"));
    let plain = set_path_name(
        &set_branch_name(&named, "after-plan", " "),
        "after-plan",
        "small",
        "",
    );
    let branch = find_branch(&plain, "after-plan").unwrap();
    assert_eq!(branch.name, None);
    assert_eq!(branch.paths[0].name, None);
    assert_eq!(set_path_name(&branched(), "nope", "small", "x"), branched());
}

#[test]
fn adds_a_branch_before_a_missing_release() {
    let steps = vec![stage_step("docs", StageKind::Agent, Gate::Auto)];
    assert_eq!(add_branch(&steps, None).len(), 2);
}
