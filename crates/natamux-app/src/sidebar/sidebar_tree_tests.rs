use serde_json::json;

use super::*;

fn make_task(overrides: Value) -> Task {
    let mut base = json!({
        "id": "T-0001", "title": "Example", "state": "running", "workspace": "/work/T-0001",
        "createdAt": "2026-10-08T09:00:00.000Z", "updatedAt": "2026-10-08T09:00:00.000Z"
    });
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), overrides) {
        base.extend(extra);
    }
    serde_json::from_value(base).unwrap_or_default()
}

fn folds(entries: &[(&str, bool)]) -> Folds {
    entries
        .iter()
        .map(|&(key, open)| (key.to_owned(), open))
        .collect()
}

#[test]
fn reads_saved_folds_and_drops_anything_that_is_not_true_or_false() {
    let raw = r#"{"repo:a":false,"task:T-1":true,"bad":"yes"}"#;
    assert_eq!(
        parse_folds(Some(raw)),
        folds(&[("repo:a", false), ("task:T-1", true)])
    );
    assert!(parse_folds(Some("[1]")).is_empty());
    assert!(parse_folds(Some("not json")).is_empty());
    assert!(parse_folds(None).is_empty());
}

#[test]
fn uses_the_default_until_the_user_picks() {
    assert!(is_fold_open(&[], "task:T-1", true));
    assert!(!is_fold_open(
        &folds(&[("task:T-1", false)]),
        "task:T-1",
        true
    ));
}

#[test]
fn keeps_only_the_newest_choices() {
    let mut current: Folds = Vec::new();
    for index in 0..305 {
        current = with_fold(&current, &format!("task:T-{index}"), true);
    }
    assert_eq!(current.len(), 300);
    assert!(!current.iter().any(|(key, _)| key == "task:T-0"));
    assert!(is_fold_open(&current, "task:T-304", false));
}

#[test]
fn picks_the_state_that_needs_the_user_most() {
    use TaskState::*;
    assert_eq!(
        urgent_state(&[Running, Failed, NeedsInput]),
        Some(NeedsInput)
    );
    assert_eq!(urgent_state(&[Ready, Running, Failed]), Some(Failed));
    assert_eq!(urgent_state(&[Ready, Review, Blocked]), Some(Blocked));
    assert_eq!(urgent_state(&[Ready, Review]), Some(Review));
    assert_eq!(urgent_state(&[Ready, Running]), Some(Running));
    assert_eq!(urgent_state(&[Done, Ready]), Some(Ready));
}

#[test]
fn is_null_when_nothing_is_open() {
    use TaskState::*;
    assert_eq!(urgent_state(&[Done, Cancelled, Draft]), None);
    assert_eq!(urgent_state(&[]), None);
}

#[test]
fn opens_when_a_child_works_waits_or_failed() {
    use TaskState::*;
    assert!(opens_by_default(&[Ready, Running]));
    assert!(opens_by_default(&[NeedsInput]));
    assert!(opens_by_default(&[Failed]));
    assert!(!opens_by_default(&[Ready, Review, Done]));
}

#[test]
fn makes_a_card_of_what_needs_the_user_and_one_line_of_what_is_finished() {
    use TaskState::*;
    let tiers = |states: [TaskState; 3]| states.map(row_tier);
    assert_eq!(
        tiers([NeedsInput, Failed, Blocked]),
        [RowTier::Attention; 3]
    );
    assert_eq!(tiers([Running, Review, Ready]), [RowTier::Live; 3]);
    assert_eq!(tiers([Done, Cancelled, Draft]), [RowTier::Quiet; 3]);
}

fn position(overrides: Value) -> Option<(usize, usize)> {
    stage_position(&make_task(overrides)).map(|p| (p.at, p.of))
}

#[test]
fn counts_the_current_step_from_the_stages_of_the_task() {
    let stages = json!(["spec", "implement", "review", "release"]);
    assert_eq!(
        position(json!({"stage": "review", "stages": stages})),
        Some((3, 4))
    );
}

#[test]
fn counts_a_group_of_parallel_stages_as_one_step_by_its_id_or_one_of_its_stages() {
    let stages = json!(["spec", {"id": "checks", "parallel": ["lint", "security"]}, "release"]);
    assert_eq!(
        position(json!({"stage": "checks", "stages": stages})),
        Some((2, 3))
    );
    assert_eq!(
        position(json!({"stage": "security", "stages": stages})),
        Some((2, 3))
    );
}

#[test]
fn counts_a_branch_as_one_step_by_its_id_or_a_stage_of_one_of_its_paths() {
    let stages = json!(["plan", {"id": "after-plan", "paths": [
        {"id": "small", "stages": ["implement", "testing"]},
        {"id": "pr-review", "stages": ["review"]}
    ]}, "release"]);
    assert_eq!(
        position(json!({"stage": "testing", "stages": stages})),
        Some((2, 3))
    );
    assert_eq!(
        position(json!({"stage": "review", "stages": stages})),
        Some((2, 3))
    );
    assert_eq!(
        position(json!({"stage": "release", "stages": stages})),
        Some((3, 3))
    );
}

#[test]
fn is_null_for_one_step_an_unknown_stage_or_a_task_with_no_stage_to_show() {
    assert_eq!(
        position(json!({"stage": "implement", "stages": ["implement"]})),
        None
    );
    assert_eq!(
        position(json!({"stage": "other", "stages": ["spec", "implement"]})),
        None
    );
    let stages = json!(["spec", "implement"]);
    assert_eq!(
        position(json!({"state": "done", "stage": "implement", "stages": stages})),
        None
    );
    assert_eq!(
        position(json!({"state": "draft", "stage": "spec", "stages": stages})),
        None
    );
}

#[test]
fn counts_every_subtask_of_the_task_and_the_done_ones() {
    let tasks = [
        make_task(json!({"id": "T-0001"})),
        make_task(json!({"id": "T-0002", "parentId": "T-0001", "state": "done"})),
        make_task(
            json!({"id": "T-0003", "parentId": "T-0001", "workspace": null, "state": "ready"}),
        ),
        make_task(json!({"id": "T-0004", "parentId": "T-0009"})),
    ];
    assert_eq!(
        subtask_count("T-0001", &tasks),
        Some(SubtaskCount { done: 1, total: 2 })
    );
    assert_eq!(subtask_count("T-0004", &tasks), None);
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|task| task.id.as_str()).collect()
}

#[test]
fn brings_back_the_parent_of_a_subtask_that_the_filter_matched() {
    let parent = make_task(json!({"id": "T-0001", "title": "Parent"}));
    let child = make_task(json!({"id": "T-0002", "title": "Child", "parentId": "T-0001"}));
    let other = make_task(json!({"id": "T-0003", "title": "Other"}));
    let all = [parent.clone(), child.clone(), other.clone()];
    assert_eq!(
        ids(&with_parents(std::slice::from_ref(&child), &all)),
        ["T-0002", "T-0001"]
    );
    assert_eq!(
        ids(&with_parents(&[parent, child], &all)),
        ["T-0001", "T-0002"]
    );
    assert_eq!(with_parents(std::slice::from_ref(&other), &all), [other]);
}

#[test]
fn does_not_bring_back_a_parent_without_a_workspace() {
    let parent = make_task(json!({"id": "T-0001", "workspace": null}));
    let child = make_task(json!({"id": "T-0002", "parentId": "T-0001"}));
    assert_eq!(
        with_parents(std::slice::from_ref(&child), &[parent, child.clone()]),
        [child]
    );
}

#[test]
fn names_stages_like_core_does() {
    assert_eq!(stage_name("test-spec"), "Test spec");
    assert_eq!(stage_name("testing"), "Automated tests");
    assert_eq!(stage_name("review"), "Manual review");
    assert_eq!(stage_name("alpha"), "Alpha");
}

#[test]
fn labels_states_and_agents_like_the_dashboard() {
    assert_eq!(state_label(TaskState::Review), "In review");
    assert_eq!(state_label(TaskState::NeedsInput), "Needs input");
    assert_eq!(agent_label(Some("claude-code")), "Claude Code");
    assert_eq!(agent_label(Some("mystery")), "mystery");
    assert_eq!(agent_label(None), "No agent");
}
