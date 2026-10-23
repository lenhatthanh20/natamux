use natamux_proto::stages::catalog::{STAGE_PHASES, catalog_entries, default_pipeline};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue, pipeline_issues};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};

use super::*;
use crate::stages::editor_branches::add_branch;
use crate::stages::editor_fields::{TextField, set_enabled, set_text};
use crate::stages::editor_find::find_stage;
use crate::stages::editor_fixtures::{branch_ref, branched, ids, sid};
use crate::stages::editor_moves::remove_stage;

fn last_two(steps: &[Step]) -> Vec<String> {
    steps[steps.len() - 2..]
        .iter()
        .map(|step| step.id().to_string())
        .collect()
}

fn input(name: &str, id: &str, kind: CustomKind, output: &str) -> CustomStageInput {
    CustomStageInput {
        name: name.into(),
        id: id.into(),
        kind,
        output: output.into(),
    }
}

fn offered(steps: &[Step]) -> Vec<&'static str> {
    catalog_choices(steps)
        .iter()
        .map(|entry| entry.stage.id.as_str())
        .collect()
}

#[test]
fn adds_a_catalog_stage_before_the_release_with_its_output_and_sections() {
    let steps = add_catalog_stage(&default_pipeline(), "security", None);
    assert_eq!(last_two(&steps), ["security", "release"]);
    let mut want = Stage::new("security", StageKind::Agent, Gate::Manual);
    want.name = Some("Security review".into());
    want.output = Some("security.md".into());
    want.sections = Some(vec!["Findings".into()]);
    want.own_shell = Some(true);
    assert_eq!(find_stage(&steps, "security"), Some(&want));
    let no_release = remove_stage(&default_pipeline(), "release");
    assert_eq!(
        add_catalog_stage(&no_release, "qa", None)
            .last()
            .map(|s| s.id()),
        Some("qa")
    );
}

#[test]
fn offers_only_the_catalog_stages_that_are_not_in_the_pipeline() {
    let steps = default_pipeline();
    assert!(offered(&steps).contains(&"security"));
    assert!(!offered(&steps).contains(&"spec"));
    let without_spec = remove_stage(&steps, "spec");
    assert!(offered(&without_spec).contains(&"spec"));
}

#[test]
fn adds_a_stage_of_your_own_that_waits_for_approval() {
    let steps = add_custom_stage(
        &default_pipeline(),
        &input("Load test", "load-test", CustomKind::Agent, "load.md"),
    );
    let mut want = Stage::new("load-test", StageKind::Agent, Gate::Manual);
    want.output = Some("load.md".into());
    assert_eq!(find_stage(&steps, "load-test"), Some(&want));
    let named = add_custom_stage(
        &default_pipeline(),
        &input("Ask legal", "legal", CustomKind::Manual, "ignored.md"),
    );
    let mut want = Stage::new("legal", StageKind::Manual, Gate::Manual);
    want.name = Some("Ask legal".into());
    assert_eq!(find_stage(&named, "legal"), Some(&want));
}

#[test]
fn refuses_a_custom_stage_with_a_used_wrong_or_catalog_id() {
    let steps = default_pipeline();
    let base = input("Spec", "spec", CustomKind::Agent, "");
    assert_eq!(
        custom_stage_problems(&steps, &base).id.as_deref(),
        Some("Another stage uses this id")
    );
    let with_id = |id: &str| CustomStageInput {
        id: id.into(),
        ..base.clone()
    };
    assert_eq!(
        custom_stage_problems(&steps, &with_id("Load Test"))
            .id
            .as_deref(),
        Some("A stage id is lower case words joined by \"-\"")
    );
    assert!(
        custom_stage_problems(&steps, &with_id("security"))
            .id
            .is_some_and(|id| id.starts_with("Security review is a stage of the list"))
    );
    let blank_name = CustomStageInput {
        name: " ".into(),
        id: "x".into(),
        ..base.clone()
    };
    assert_eq!(
        custom_stage_problems(&steps, &blank_name),
        CustomStageProblems {
            name: Some("Give the stage a name".into()),
            ..CustomStageProblems::default()
        }
    );
    let wrong_output = CustomStageInput {
        id: "x".into(),
        output: "x.txt".into(),
        ..base.clone()
    };
    assert_eq!(
        custom_stage_problems(&steps, &wrong_output)
            .output
            .as_deref(),
        Some("An output is a markdown file name, such as spec.md")
    );
    assert_eq!(add_custom_stage(&steps, &base), steps);
}

#[test]
fn makes_an_id_from_a_name() {
    assert_eq!(id_from_name("Load test (staging)"), "load-test-staging");
    assert_eq!(id_from_name("  \u{c9}t\u{e9}  "), "ete");
}

#[test]
fn c2_adds_a_stage_before_release_also_when_release_is_off() {
    let off = set_enabled(&default_pipeline(), "release", false);
    let steps = add_catalog_stage(&off, "research", None);
    assert_eq!(last_two(&steps), ["research", "release"]);
}

#[test]
fn c2_does_not_offer_a_stage_that_is_in_the_file_but_off() {
    let off = set_enabled(&default_pipeline(), "spec", false);
    assert!(!offered(&off).contains(&"spec"));
}

#[test]
fn c3_refuses_the_id_of_a_stage_that_is_off_since_ids_are_unique_in_the_file() {
    let off = set_enabled(&default_pipeline(), "plan", false);
    let problems = custom_stage_problems(&off, &input("Plan", "plan", CustomKind::Agent, ""));
    assert_eq!(problems.id.as_deref(), Some("Another stage uses this id"));
}

#[test]
fn c3_follows_the_stage_id_rule_of_core_where_a_leading_digit_is_allowed() {
    let problem = |id: &str| {
        custom_stage_problems(&default_pipeline(), &input("X", id, CustomKind::Agent, "")).id
    };
    let rule = Some("A stage id is lower case words joined by \"-\"".to_string());
    for id in ["-load", "load_test", "load!", "load test"] {
        assert_eq!(problem(id), rule, "{id}");
    }
    assert_eq!(
        problem(&"a".repeat(41)).as_deref(),
        Some("An id has 40 characters at most")
    );
    assert_eq!(problem("1st-check"), None);
    assert_eq!(problem(&"a".repeat(40)), None);
}

#[test]
fn c3_makes_a_valid_id_from_non_ascii_names_or_none() {
    assert_eq!(id_from_name("Ki\u{1ec3}m th\u{1eed}"), "kiem-thu");
    assert_eq!(id_from_name("\u{65e5}\u{672c}\u{8a9e}"), "");
    let name = "\u{65e5}\u{672c}\u{8a9e}";
    let problems = custom_stage_problems(
        &default_pipeline(),
        &input(name, &id_from_name(name), CustomKind::Agent, ""),
    );
    assert_eq!(problems.id.as_deref(), Some("Give the stage an id"));
}

#[test]
fn c3_takes_only_a_markdown_file_name_as_output_never_a_path() {
    let problem = |output: &str| {
        custom_stage_problems(
            &default_pipeline(),
            &input("X", "x", CustomKind::Agent, output),
        )
        .output
    };
    let rule = Some("An output is a markdown file name, such as spec.md".to_string());
    for output in ["../x.md", "a/b.md", "notes.txt"] {
        assert_eq!(problem(output), rule, "{output}");
    }
    assert_eq!(problem("load-test.notes.md"), None);
}

#[test]
fn c12_treats_a_command_of_spaces_as_empty() {
    let steps = add_catalog_stage(&default_pipeline(), "lint", None);
    let steps = set_text(&steps, "lint", TextField::Command, "   ");
    assert_eq!(find_stage(&steps, "lint").unwrap().command, None);
    let kept = set_text(
        &steps,
        "lint",
        TextField::Command,
        "pnpm lint && echo \"ok\" | cat",
    );
    assert_eq!(
        find_stage(&kept, "lint").unwrap().command.as_deref(),
        Some("pnpm lint && echo \"ok\" | cat")
    );
}

#[test]
fn adds_a_catalog_stage_to_the_end_of_a_path() {
    assert_eq!(
        ids(&add_catalog_stage(&branched(), "qa", Some("pr-review")))[2],
        branch_ref(&["implement", "testing"], &["review", "qa"])
    );
    assert_eq!(
        ids(&add_catalog_stage(&branched(), "qa", None))[3],
        sid("qa")
    );
}

#[test]
fn does_not_add_a_used_stage_a_release_or_context_stage_or_to_an_unknown_path() {
    let steps = branched();
    assert_eq!(
        add_catalog_stage(&steps, "implement", Some("pr-review")),
        steps
    );
    assert_eq!(add_catalog_stage(&steps, "qa", Some("nope")), steps);
    let bare: Vec<Step> = steps
        .iter()
        .filter(|step| step.id() != "release" && step.id() != "context")
        .cloned()
        .collect();
    assert_eq!(add_catalog_stage(&bare, "release", Some("small")), bare);
    assert_eq!(add_catalog_stage(&bare, "context", Some("small")), bare);
}

#[test]
fn does_not_offer_a_stage_that_is_in_a_path() {
    let choices = offered(&branched());
    assert!(!choices.contains(&"implement"));
    assert!(!choices.contains(&"review"));
    assert!(choices.contains(&"qa"));
}

#[test]
fn shows_a_message_of_a_branch_once_however_many_paths_break_the_rule() {
    let issues = pipeline_issues(&add_branch(&branched()[..2], Some("plan")));
    let messages: Vec<String> = unique_issues(&issues)
        .iter()
        .map(|issue| format!("{:?}: {}", issue.stage, issue.message))
        .collect();
    let mut unique = messages.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), messages.len());
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("Some(\"branch\"): "))
    );
}

#[test]
fn keeps_the_same_message_on_another_stage() {
    let error = |stage: &str| PipelineIssue {
        stage: Some(stage.to_string()),
        message: "Wrong".into(),
        level: IssueLevel::Error,
    };
    let kept = unique_issues(&[error("a"), error("a"), error("b")]);
    assert_eq!(kept, vec![error("a"), error("b")]);
}

#[test]
fn picker_lists_every_catalog_stage_that_is_not_in_the_pipeline_by_phase() {
    let groups = picker_groups(&default_pipeline(), "");
    let names: Vec<&str> = groups.iter().map(|(phase, _)| phase.name()).collect();
    assert_eq!(names, ["Understand", "Design", "Build", "Check", "Ship"]);
    let total: usize = groups.iter().map(|(_, entries)| entries.len()).sum();
    assert_eq!(total, catalog_entries().len() - 8);
    let check = groups
        .iter()
        .find(|(phase, _)| phase.name() == "Check")
        .unwrap();
    let security = check
        .1
        .iter()
        .find(|entry| entry.stage.id == "security")
        .unwrap();
    assert_eq!(
        security.description,
        "Checks the diff for security problems."
    );
    assert_eq!(
        picker_detail(security),
        "Agent \u{b7} Waits for your approval"
    );
    assert!(
        groups
            .iter()
            .all(|(_, entries)| entries.iter().all(|e| e.stage.id != "spec"))
    );
}

#[test]
fn picker_filters_the_stages_by_name() {
    let groups = picker_groups(&default_pipeline(), "secur");
    let found: Vec<&str> = groups
        .iter()
        .flat_map(|(_, e)| e)
        .map(|e| e.stage.id.as_str())
        .collect();
    assert!(found.contains(&"security"));
    assert!(!found.contains(&"research"));
    assert_eq!(
        picker_empty_text(&default_pipeline(), "securxyz").as_deref(),
        Some("No stage of the list matches \u{201c}securxyz\u{201d}.")
    );
}

#[test]
fn picker_c2_shows_the_phases_in_order_and_hides_a_phase_with_nothing_left() {
    let groups = picker_groups(&default_pipeline(), "  SEC ");
    assert_eq!(
        groups
            .iter()
            .map(|(_, entries)| entries.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(groups[0].1[0].stage.id, "security");
    assert!(groups.iter().all(|(phase, _)| phase.name() != "Understand"));
    let order: Vec<_> = picker_groups(&default_pipeline(), "")
        .iter()
        .map(|(p, _)| *p)
        .collect();
    assert_eq!(order, STAGE_PHASES);
}

#[test]
fn picker_c2_keeps_custom_stage_when_nothing_matches() {
    assert_eq!(
        picker_empty_text(&default_pipeline(), "nothing like this").as_deref(),
        Some("No stage of the list matches \u{201c}nothing like this\u{201d}.")
    );
    assert!(picker_empty_text(&default_pipeline(), "").is_none());
}

#[test]
fn picker_c2_says_so_when_every_catalog_stage_is_in_the_pipeline() {
    let all: Vec<Step> = catalog_entries()
        .iter()
        .map(|e| Step::Stage(e.stage.clone()))
        .collect();
    assert!(picker_groups(&all, "").is_empty());
    assert_eq!(
        picker_empty_text(&all, "").as_deref(),
        Some("Every stage of the list is in the pipeline.")
    );
}

#[test]
fn picker_c2_does_not_offer_a_stage_that_is_in_the_pipeline_but_off() {
    let off = set_enabled(&default_pipeline(), "spec", false);
    let groups = picker_groups(&off, "");
    assert!(
        groups
            .iter()
            .all(|(_, entries)| entries.iter().all(|e| e.stage.id != "spec"))
    );
}

#[test]
fn picker_adds_a_custom_stage_with_an_id_from_its_name() {
    assert_eq!(form_id("Load test", None), "load-test");
    let steps = default_pipeline();
    let form = input(
        "Load test",
        &form_id("Load test", None),
        CustomKind::Testing,
        "",
    );
    assert!(custom_stage_problems(&steps, &form).is_empty());
    assert_eq!(form.kind.stage_kind(), StageKind::Testing);
}

#[test]
fn picker_refuses_an_id_that_is_used_or_breaks_the_id_rule() {
    let steps = default_pipeline();
    let used = input("Plan", &form_id("Plan", None), CustomKind::Agent, "");
    assert_eq!(
        visible_problems(&custom_stage_problems(&steps, &used), &used, false)
            .id
            .as_deref(),
        Some("Another stage uses this id")
    );
    let bad = input(
        "Plan",
        &form_id("Plan", Some("My Plan")),
        CustomKind::Agent,
        "",
    );
    assert_eq!(
        custom_stage_problems(&steps, &bad).id.as_deref(),
        Some("A stage id is lower case words joined by \"-\"")
    );
}

#[test]
fn picker_asks_for_a_name_before_it_adds_a_stage() {
    let steps = default_pipeline();
    let blank = input("", "", CustomKind::Agent, "");
    let problems = custom_stage_problems(&steps, &blank);
    assert_eq!(visible_problems(&problems, &blank, false).name, None);
    assert_eq!(
        visible_problems(&problems, &blank, true).name.as_deref(),
        Some("Give the stage a name")
    );
}

#[test]
fn picker_c3_refuses_a_name_of_spaces() {
    let steps = default_pipeline();
    let spaces = input("   ", &form_id("   ", None), CustomKind::Agent, "");
    let problems = custom_stage_problems(&steps, &spaces);
    assert_eq!(
        visible_problems(&problems, &spaces, true).name.as_deref(),
        Some("Give the stage a name")
    );
}

#[test]
fn picker_c3_keeps_an_id_edited_by_hand_when_the_name_changes() {
    assert_eq!(form_id("Load", None), "load");
    assert_eq!(form_id("Load test", Some("perf")), "perf");
}

#[test]
fn picker_c3_refuses_the_id_of_a_catalog_stage_which_belongs_to_the_list() {
    let steps = default_pipeline();
    let research = input(
        "Research",
        &form_id("Research", None),
        CustomKind::Agent,
        "",
    );
    assert!(
        custom_stage_problems(&steps, &research)
            .id
            .is_some_and(|id| id.starts_with("Research is a stage of the list"))
    );
}

#[test]
fn picker_c3_makes_a_valid_id_from_a_vietnamese_name() {
    assert_eq!(form_id("Ki\u{1ec3}m th\u{1eed}", None), "kiem-thu");
}

#[test]
fn picker_c3_shows_the_output_file_for_agent_stages_only() {
    let steps = default_pipeline();
    let manual = input("X", "x", CustomKind::Manual, "x.txt");
    assert_eq!(custom_stage_problems(&steps, &manual).output, None);
    let agent = input("X", "x", CustomKind::Agent, "x.txt");
    assert!(custom_stage_problems(&steps, &agent).output.is_some());
}

#[test]
fn picker_hides_an_output_problem_until_something_is_typed() {
    let steps = default_pipeline();
    let form = input("X", "x", CustomKind::Agent, "");
    let problems = CustomStageProblems {
        output: Some("wrong".into()),
        ..custom_stage_problems(&steps, &form)
    };
    assert_eq!(visible_problems(&problems, &form, false).output, None);
}
