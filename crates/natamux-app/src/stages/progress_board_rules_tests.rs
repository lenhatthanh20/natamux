use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::progress::{StageStatus, stage_progress};
use serde_json::{Map, json};

use super::*;

const NOW: i64 = 1_791_540_000_000;

fn task(id: &str, updated_at: &str) -> Task {
    Task {
        id: id.into(),
        state: TaskState::Running,
        stage: "context".into(),
        stages: vec![json!("context"), json!("spec")],
        updated_at: updated_at.into(),
        ..Task::default()
    }
}

fn ids(rows: &[Task]) -> Vec<&str> {
    rows.iter().map(|row| row.id.as_str()).collect()
}

fn default_pipelines() -> BTreeMap<String, Vec<Step>> {
    BTreeMap::from([(DEFAULT_REPO_ID.to_owned(), default_pipeline())])
}

#[test]
fn keeps_the_order_of_the_tasks_within_equal_rows_whatever_their_updated_at_says() {
    let tasks = [
        task("a", "2026-10-09T09:59:00Z"),
        task("b", "2026-10-09T09:59:30Z"),
        task("c", "2026-10-09T09:59:10Z"),
    ];
    let sections = board_sections(&tasks, &[], &default_pipelines(), NOW);
    assert_eq!(ids(&sections[0].open), ["a", "b", "c"]);
}

#[test]
fn still_shows_the_tasks_while_the_settings_are_not_loaded() {
    let tasks = [task("a", "2026-10-09T09:59:00Z")];
    let sections = board_sections(&tasks, &[], &BTreeMap::new(), NOW);
    assert_eq!(sections[0].open.len(), 1);
    assert_eq!(sections[0].title, DEFAULT_REPO_ID);
    let steps: Vec<&str> = sections[0].steps.iter().map(Step::id).collect();
    assert_eq!(steps, ["context", "spec"]);
}

#[test]
fn takes_the_tasks_made_with_the_stages_that_are_on_as_the_current_pipeline() {
    let mut pipeline = default_pipeline();
    for step in &mut pipeline {
        if let Step::Stage(stage) = step
            && stage.id == "plan"
        {
            stage.enabled = Some(false);
        }
    }
    let mut made = task("a", "2026-10-09T09:59:00Z");
    made.stages = step_refs_of(&active_steps(&pipeline))
        .iter()
        .filter_map(|reference| serde_json::to_value(reference).ok())
        .collect();
    let pipelines = BTreeMap::from([(DEFAULT_REPO_ID.to_owned(), pipeline)]);
    let sections = board_sections(&[made], &[], &pipelines, NOW);
    let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, [DEFAULT_REPO_ID]);
    assert!(sections[0].steps.iter().all(|step| step.id() != "plan"));
}

#[test]
fn shows_nothing_when_there_are_no_projects_and_no_tasks() {
    assert!(board_sections(&[], &[], &BTreeMap::new(), NOW).is_empty());
}

fn branched() -> Task {
    let mut task = task("T-0001", "2026-10-09T09:59:00Z");
    task.stage = "implement".into();
    task.stages = vec![
        json!("plan"),
        json!({"id": "after-plan", "paths": [
            {"id": "small", "stages": ["implement", "testing"]},
            {"id": "big", "stages": ["agent-review"]}]}),
        json!("release"),
    ];
    task.paths = Map::from_iter([("after-plan".to_owned(), json!("small"))]);
    task
}

#[test]
fn counts_a_task_in_the_column_of_its_own_path_only_and_marks_the_other_path_not_taken() {
    let task = branched();
    let sections = board_sections(std::slice::from_ref(&task), &[], &BTreeMap::new(), NOW);
    let steps = &sections[0].steps;
    let columns = stage_columns(steps, &sections[0].open);
    let here = |id: &str| columns.iter().find(|c| c.stage_id == id).map(|c| c.here);
    assert_eq!(here("implement"), Some(1));
    assert_eq!(here("agent-review"), Some(0));
    let progress = stage_progress(&task, &[], steps);
    let review = progress.iter().find(|entry| entry.stage == "agent-review");
    assert_eq!(
        review.map(|entry| entry.status),
        Some(StageStatus::NotTaken)
    );
}

fn group_task(lane: TaskState) -> Task {
    let mut task = task("T-0002", "2026-10-09T09:59:00Z");
    task.stage = "build".into();
    task.state = TaskState::Review;
    task.stages = vec![json!({"id": "build", "parallel": ["test-spec", "implement"]})];
    task.lanes = Map::from_iter([
        ("test-spec".to_owned(), json!("done")),
        ("implement".to_owned(), json!(lane)),
    ]);
    task
}

#[test]
fn counts_a_group_stage_until_its_lane_is_done() {
    let task = group_task(TaskState::Review);
    let steps = resolve_steps(&steps_of(&task), &[]);
    let columns = stage_columns(&steps, &[task]);
    let column = |id: &str| columns.iter().find(|c| c.stage_id == id).cloned();
    let test_spec = column("test-spec").map(|c| (c.here, c.waiting));
    let implement = column("implement").map(|c| (c.here, c.waiting));
    assert_eq!(test_spec, Some((0, 0)));
    assert_eq!(implement, Some((1, 1)));
}

#[test]
fn counts_the_tasks_that_wait_for_the_user_in_a_plain_stage() {
    let mut waiting = task("a", "2026-10-09T09:59:00Z");
    waiting.state = TaskState::NeedsInput;
    let working = task("b", "2026-10-09T09:59:00Z");
    let steps = resolve_steps(&steps_of(&waiting), &[]);
    let columns = stage_columns(&steps, &[waiting, working]);
    assert_eq!((columns[0].here, columns[0].waiting), (2, 1));
    assert_eq!((columns[1].here, columns[1].waiting), (0, 0));
}

#[test]
fn sorts_waiting_tasks_first_then_the_furthest_stage() {
    let mut late = task("late", "2026-10-09T09:59:00Z");
    late.stage = "spec".into();
    let early = task("early", "2026-10-09T09:59:00Z");
    let mut waiting = task("waiting", "2026-10-09T09:59:00Z");
    waiting.state = TaskState::Review;
    let steps = resolve_steps(&steps_of(&late), &[]);
    let rows = sort_rows(&[early, late, waiting], &steps, NOW);
    assert_eq!(ids(&rows), ["waiting", "late", "early"]);
}

#[test]
fn puts_a_quiet_task_with_the_waiting_ones() {
    let mut quiet = task("quiet", "2026-10-09T09:00:00Z");
    quiet.stage = "context".into();
    let mut ahead = task("ahead", "2026-10-09T09:59:30Z");
    ahead.stage = "spec".into();
    let now = natamux_ms("2026-10-09T10:00:00Z");
    let steps = resolve_steps(&steps_of(&quiet), &[]);
    assert_eq!(
        ids(&sort_rows(&[ahead, quiet], &steps, now)),
        ["quiet", "ahead"]
    );
}

fn natamux_ms(iso: &str) -> i64 {
    crate::tasks::task_time::parse_iso_ms(iso).unwrap_or_default()
}

#[test]
fn leaves_cancelled_tasks_out_and_splits_done_ones() {
    let mut done = task("done", "2026-10-09T09:59:00Z");
    done.state = TaskState::Done;
    let mut cancelled = task("cancelled", "2026-10-09T09:59:00Z");
    cancelled.state = TaskState::Cancelled;
    let open = task("open", "2026-10-09T09:59:00Z");
    let sections = board_sections(&[done, cancelled, open], &[], &BTreeMap::new(), NOW);
    assert_eq!(ids(&sections[0].open), ["open"]);
    assert_eq!(ids(&sections[0].done), ["done"]);
}

#[test]
fn makes_an_earlier_pipeline_section_for_tasks_with_other_stages() {
    let current = default_pipeline();
    let mut fresh = task("fresh", "2026-10-09T09:59:00Z");
    fresh.stages = step_refs_of(&active_steps(&current))
        .iter()
        .filter_map(|reference| serde_json::to_value(reference).ok())
        .collect();
    let old = task("old", "2026-10-09T09:59:00Z");
    let repos = [RepoRef {
        id: DEFAULT_REPO_ID.into(),
        name: "Natamux".into(),
    }];
    let sections = board_sections(&[old, fresh], &repos, &default_pipelines(), NOW);
    let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Natamux", "Natamux, earlier pipeline"]);
    assert_eq!(ids(&sections[0].open), ["fresh"]);
}

#[test]
fn keys_the_sets_by_the_stages_a_task_carries() {
    let a = task("a", "x");
    let mut b = task("b", "x");
    assert_eq!(steps_key(&a), steps_key(&b));
    b.stages.push(json!("release"));
    assert_ne!(steps_key(&a), steps_key(&b));
}

#[test]
fn labels_the_next_button_by_state() {
    assert_eq!(next_label(TaskState::NeedsInput), Some("Answer"));
    assert_eq!(next_label(TaskState::Review), Some("Review"));
    assert_eq!(next_label(TaskState::Failed), Some("Open"));
    assert_eq!(next_label(TaskState::Blocked), Some("Open"));
    assert_eq!(next_label(TaskState::Running), None);
}

#[test]
fn shows_the_button_when_the_task_waits_and_the_activity_when_it_works() {
    let mut waiting = task("a", "2026-10-09T09:59:30Z");
    waiting.state = TaskState::NeedsInput;
    let running = task("b", "2026-10-09T09:59:30Z");
    let now = natamux_ms("2026-10-09T10:00:00Z");
    assert_eq!(next_button(&waiting, now), Some("Answer"));
    assert_eq!(next_button(&running, now), None);
    assert_eq!(next_text(&running, Some("Editing x.rs")), "Editing x.rs");
    assert_eq!(next_text(&running, None), "Working");
    assert_eq!(next_text(&waiting, Some("x")), "");
}

#[test]
fn draws_no_line_between_the_stages_of_a_group_or_of_two_paths() {
    let task = group_task(TaskState::Running);
    let steps = resolve_steps(&steps_of(&task), &[]);
    let progress = stage_progress(&task, &[], &steps);
    let links = cell_links(&progress);
    assert_eq!(links[0].1, LinkKind::Hidden);
    assert_eq!(links[1].0, LinkKind::Hidden);

    let branched = branched();
    let steps = resolve_steps(&steps_of(&branched), &[]);
    let progress = stage_progress(&branched, &[], &steps);
    let at = |id: &str| progress.iter().position(|entry| entry.stage == id);
    let links = cell_links(&progress);
    let (testing, review) = (at("testing"), at("agent-review"));
    assert!(testing.is_some() && review.is_some());
    assert_eq!(links[testing.unwrap_or(0)].1, LinkKind::Hidden);
    assert_eq!(links[review.unwrap_or(0)].0, LinkKind::Hidden);
}
