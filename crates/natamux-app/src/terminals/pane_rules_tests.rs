use super::*;
use serde_json::{Value, json};

fn make_task(id: &str, state: &str, workspace: Option<&str>) -> Task {
    serde_json::from_value(json!({
        "id": id, "title": "Example", "state": state, "workspace": workspace,
        "stage": "implement", "updatedAt": "2026-10-08T09:00:00.000Z",
    }))
    .expect("a task")
}

fn running(id: &str) -> Task {
    make_task(id, "running", Some("/work/T-0001"))
}

fn grouped(stage: &str, state: &str, lanes: Value) -> Task {
    let mut task = make_task("T-0001", state, Some("/work/T-0001"));
    task.stage = stage.into();
    task.lanes = lanes.as_object().cloned().unwrap_or_default();
    task
}

fn ids(tasks: &[&Task]) -> Vec<String> {
    tasks.iter().map(|task| task.id.clone()).collect()
}

fn picked(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn panes(chosen: Option<&[&str]>, tasks: &[Task]) -> Vec<String> {
    let chosen = chosen.map(picked);
    ids(&agent_pane_tasks(chosen.as_deref(), tasks))
}

#[test]
fn shows_the_agents_that_work_or_wait_until_the_user_picks() {
    let tasks = [
        running("T-0001"),
        make_task("T-0002", "needs-input", Some("/w")),
        make_task("T-0003", "ready", Some("/w")),
        make_task("T-0004", "running", None),
    ];
    assert_eq!(panes(None, &tasks), ["T-0001", "T-0002"]);
}

#[test]
fn keeps_the_order_the_user_picked_and_drops_tasks_without_a_workspace() {
    let tasks = [
        running("T-0001"),
        make_task("T-0003", "ready", Some("/w")),
        make_task("T-0004", "running", None),
    ];
    let chosen = ["T-0003", "T-0004", "T-0001"];
    assert_eq!(panes(Some(&chosen), &tasks), ["T-0003", "T-0001"]);
}

#[test]
fn shows_at_most_six_panes_by_default() {
    let many: Vec<Task> = (10..18).map(|n| running(&format!("T-00{n}"))).collect();
    assert_eq!(agent_pane_tasks(None, &many).len(), MAX_PANES);
}

#[path = "pane_order_tests.rs"]
mod order;

#[test]
fn fits_panes_of_at_least_320px_with_the_gaps() {
    let widths = [300.0, 647.0, 648.0, 1000.0, 1440.0, 2000.0];
    let got: Vec<usize> = widths.iter().map(|w| max_columns(Some(*w))).collect();
    assert_eq!(got, [1, 1, 2, 3, 4, 6]);
}

#[test]
fn assumes_three_columns_before_the_grid_is_measured() {
    assert_eq!(max_columns(None), 3);
    assert_eq!(max_columns(Some(0.0)), 3);
}

#[test]
fn lets_auto_use_the_width_with_balanced_rows() {
    assert_eq!(columns_for(PaneLayout::Auto, 2, Some(1440.0)), 2);
    assert_eq!(columns_for(PaneLayout::Auto, 5, Some(1440.0)), 3);
    assert_eq!(columns_for(PaneLayout::Auto, 6, Some(1440.0)), 3);
    assert_eq!(columns_for(PaneLayout::Auto, 1, Some(1440.0)), 1);
    assert_eq!(columns_for(PaneLayout::Auto, 0, Some(1440.0)), 1);
}

#[test]
fn gives_the_columns_that_were_picked_as_far_as_they_fit_and_as_there_are_panes() {
    assert_eq!(columns_for(PaneLayout::Two, 6, Some(1440.0)), 2);
    assert_eq!(columns_for(PaneLayout::Four, 6, Some(1440.0)), 4);
    assert_eq!(columns_for(PaneLayout::Four, 6, Some(700.0)), 2);
    assert_eq!(columns_for(PaneLayout::Four, 3, Some(1440.0)), 3);
    assert_eq!(columns_for(PaneLayout::One, 3, Some(1440.0)), 1);
}

#[test]
fn stays_at_one_column_on_a_phone() {
    assert_eq!(columns_for(PaneLayout::Auto, 4, Some(358.0)), 1);
    assert_eq!(columns_for(PaneLayout::Four, 4, Some(358.0)), 1);
}

#[test]
fn stretches_the_last_pane_over_the_rest_of_its_row() {
    let five: Vec<usize> = (0..5).map(|index| pane_span(index, 5, 3)).collect();
    assert_eq!(five, [1, 1, 1, 1, 2]);
    let three: Vec<usize> = (0..3).map(|index| pane_span(index, 3, 2)).collect();
    assert_eq!(three, [1, 1, 2]);
}

#[test]
fn leaves_a_full_row_alone() {
    let four: Vec<usize> = (0..4).map(|index| pane_span(index, 4, 2)).collect();
    assert_eq!(four, [1, 1, 1, 1]);
    assert_eq!(pane_span(0, 1, 1), 1);
}

#[test]
fn knows_the_four_layouts_and_falls_back_to_auto() {
    let raw = [
        Some("1"),
        Some("2"),
        Some("4"),
        Some("auto"),
        Some("3"),
        None,
    ];
    let got: Vec<PaneLayout> = raw.into_iter().map(parse_pane_layout).collect();
    assert_eq!(
        got,
        [
            PaneLayout::One,
            PaneLayout::Two,
            PaneLayout::Four,
            PaneLayout::Auto,
            PaneLayout::Auto,
            PaneLayout::Auto,
        ]
    );
    assert!(
        PANE_LAYOUTS
            .iter()
            .all(|l| parse_pane_layout(Some(l.as_str())) == *l)
    );
}

#[test]
fn shows_what_a_running_agent_does_and_says_plainly_that_it_waits_for_the_user() {
    let text = pane_state_text;
    assert_eq!(
        text(TaskState::Running, Some("Edit notes/T-0018.md")),
        "Running · Edit notes/T-0018.md"
    );
    assert_eq!(text(TaskState::Running, None), "Running");
    assert_eq!(
        text(TaskState::NeedsInput, Some("Edit a.ts")),
        "Needs input"
    );
    assert_eq!(text(TaskState::Review, Some("Edit a.ts")), "In review");
}

fn lane(stage: &str, state: TaskState) -> TerminalLane {
    TerminalLane {
        stage: Some(stage.into()),
        state: Some(state),
        agent: None,
    }
}

fn no_stage(state: Option<TaskState>) -> TerminalLane {
    TerminalLane {
        stage: None,
        state,
        agent: None,
    }
}

fn lanes(entries: &[(&str, TerminalLane)]) -> BTreeMap<String, TerminalLane> {
    entries
        .iter()
        .map(|(name, lane)| (name.to_string(), lane.clone()))
        .collect()
}

fn hint(text: &str, needs_you: bool) -> Option<PaneHint> {
    Some(PaneHint {
        text: text.into(),
        needs_you,
    })
}

fn build_task() -> Task {
    let group = json!({"implement": "running", "test-spec": "needs-input"});
    grouped("build", "needs-input", group)
}

#[test]
fn names_another_terminal_that_waits_for_the_user() {
    let shells = lanes(&[
        ("1", lane("implement", TaskState::Running)),
        ("2", lane("test-spec", TaskState::Running)),
    ]);
    assert_eq!(
        other_terminal_hint(&build_task(), &shells),
        hint("Terminal 2 needs input", true)
    );
}

#[test]
fn names_a_failed_terminal_after_one_that_waits() {
    let group = json!({"implement": "running", "test-spec": "failed"});
    let failed = grouped("build", "failed", group);
    let shells = lanes(&[
        ("1", lane("implement", TaskState::Running)),
        ("2", lane("test-spec", TaskState::Failed)),
    ]);
    assert_eq!(
        other_terminal_hint(&failed, &shells),
        hint("Terminal 2 failed", true)
    );
}

#[test]
fn says_nothing_about_the_shown_terminal_or_when_the_others_are_fine() {
    let group = json!({"implement": "needs-input", "test-spec": "review"});
    let own = grouped("build", "needs-input", group);
    let shells = lanes(&[
        ("1", lane("implement", TaskState::NeedsInput)),
        ("2", lane("test-spec", TaskState::Review)),
    ]);
    assert_eq!(other_terminal_hint(&own, &shells), None);
    assert_eq!(other_terminal_hint(&build_task(), &BTreeMap::new()), None);
}

// An ownShell Spec runs in Terminal 5 and leaves Terminal 1 a plain shell.
fn spec() -> Task {
    grouped("spec", "running", json!({}))
}

#[test]
fn names_the_working_terminal_while_terminal_1_has_no_agent() {
    let shells = lanes(&[("5", lane("spec", TaskState::Running))]);
    assert_eq!(
        other_terminal_hint(&spec(), &shells),
        hint("Terminal 5 is working", false)
    );
}

#[test]
fn says_nothing_of_a_working_terminal_when_terminal_1_has_an_agent() {
    let shells = lanes(&[
        ("1", no_stage(None)),
        ("5", lane("spec", TaskState::Running)),
    ]);
    assert_eq!(other_terminal_hint(&spec(), &shells), None);
}

#[test]
fn names_a_terminal_that_waits_or_failed_before_one_that_works() {
    let working = no_stage(Some(TaskState::Running));
    let waiting = lanes(&[
        ("5", no_stage(Some(TaskState::NeedsInput))),
        ("6", working.clone()),
    ]);
    let failed = lanes(&[("5", no_stage(Some(TaskState::Failed))), ("6", working)]);
    assert_eq!(
        other_terminal_hint(&spec(), &waiting),
        hint("Terminal 5 needs input", true)
    );
    assert_eq!(
        other_terminal_hint(&spec(), &failed),
        hint("Terminal 5 failed", true)
    );
}

#[test]
fn names_the_lowest_working_terminal_whatever_the_key_order() {
    let working = no_stage(Some(TaskState::Running));
    let shells = lanes(&[("6", working.clone()), ("5", working)]);
    assert_eq!(
        other_terminal_hint(&spec(), &shells),
        hint("Terminal 5 is working", false)
    );
}

#[test]
fn compares_terminal_names_by_number_not_by_text() {
    let working = no_stage(Some(TaskState::Running));
    let shells = lanes(&[("10", working.clone()), ("9", working)]);
    assert_eq!(
        other_terminal_hint(&spec(), &shells),
        hint("Terminal 9 is working", false)
    );
}
