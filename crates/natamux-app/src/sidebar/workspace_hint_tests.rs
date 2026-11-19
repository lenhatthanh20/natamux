//! The tests of the folded group's hint, the quiet rule, the times and the hold
//! (`workspace_groups.rs`). They share `make_task` with `workspace_groups_tests.rs`.

use serde_json::json;

use super::tests::make_task;
use super::*;

fn now() -> i64 {
    parse_iso_ms("2026-10-08T09:01:00.000Z").unwrap_or_default()
}

fn group(open: Vec<Task>, finished: Vec<Task>) -> RepoGroup {
    RepoGroup {
        repo: RepoRef {
            id: "default".into(),
            name: "natamux".into(),
        },
        open: open.into_iter().map(Arc::new).collect(),
        finished: finished.into_iter().map(Arc::new).collect(),
        nested: Vec::new(),
    }
}

#[test]
fn shows_a_workspace_that_needs_the_user_in_its_own_state() {
    let waiting = make_task(json!({"id": "T-0001", "state": "review"}));
    let other = make_task(json!({"id": "T-0002", "updatedAt": "2026-10-08T09:00:30.000Z"}));
    let hint = collapsed_hint(&group(vec![other, waiting], vec![]), None, now());
    assert_eq!(hint, Some(TaskState::Review));
}

#[test]
fn shows_a_quiet_workspace_as_one_that_needs_the_user() {
    let quiet = make_task(json!({"state": "running", "updatedAt": "2026-10-08T08:00:00.000Z"}));
    assert_eq!(
        collapsed_hint(&group(vec![quiet], vec![]), None, now()),
        Some(TaskState::NeedsInput)
    );
}

#[test]
fn shows_the_open_workspace_when_nothing_waits_finished_or_not() {
    let running = make_task(json!({"id": "T-0001", "updatedAt": "2026-10-08T09:00:30.000Z"}));
    let done = make_task(json!({"id": "T-0002", "state": "done"}));
    let group = group(vec![running], vec![done]);
    assert_eq!(
        collapsed_hint(&group, Some("T-0001"), now()),
        Some(TaskState::Running)
    );
    assert_eq!(
        collapsed_hint(&group, Some("T-0002"), now()),
        Some(TaskState::Done)
    );
}

#[test]
fn shows_nothing_when_the_group_hides_nothing_of_interest() {
    let running = make_task(json!({"updatedAt": "2026-10-08T09:00:30.000Z"}));
    assert_eq!(
        collapsed_hint(&group(vec![running], vec![]), None, now()),
        None
    );
}

#[test]
fn collapsed_hint_looks_at_nested_subtasks_too() {
    let waiting = make_task(json!({"id": "T-0002", "state": "failed"}));
    let mut folded = group(
        vec![make_task(json!({"updatedAt": "2026-10-08T09:00:30.000Z"}))],
        vec![],
    );
    folded.nested = vec![Arc::new(waiting)];
    assert_eq!(
        collapsed_hint(&folded, None, now()),
        Some(TaskState::Failed)
    );
}

#[test]
fn a_task_waiting_for_background_work_is_not_quiet() {
    let task = make_task(
        json!({"updatedAt": "2026-10-08T08:00:00.000Z", "backgroundSince": "2026-10-08T08:00:00.000Z"}),
    );
    assert!(!is_quiet(&task, now()));
}

#[test]
fn reads_iso_times_in_utc() {
    assert_eq!(parse_iso_ms("1970-01-01T00:00:00.000Z"), Some(0));
    assert_eq!(
        parse_iso_ms("2026-10-08T09:00:00.500Z"),
        Some(1_791_450_000_500)
    );
    assert_eq!(
        parse_iso_ms("2024-02-29T00:00:00Z"),
        Some(1_709_164_800_000)
    );
    assert_eq!(parse_iso_ms("soon"), None);
    // A fraction that starts with a character of more than one byte.
    assert_eq!(parse_iso_ms("2026-10-08T09:00:00.é5Z"), None);
}

fn hold(id: Option<&str>, open: bool) -> Hold {
    Hold {
        id: id.map(str::to_owned),
        open,
    }
}

#[test]
fn holds_the_current_task_when_it_finishes_while_it_is_current() {
    let running = make_task(json!({"id": "T-0001"}));
    let done = Task {
        state: TaskState::Done,
        ..running.clone()
    };
    let opened = next_hold(&hold(None, false), Some("T-0001"), &[running]);
    let finished = next_hold(&opened, Some("T-0001"), &[done]);

    assert_eq!(held_id(&finished), Some("T-0001"));
}

#[test]
fn lets_it_go_once_another_task_is_current() {
    let running = make_task(json!({"id": "T-0001"}));
    let done = Task {
        state: TaskState::Done,
        ..running.clone()
    };
    let opened = next_hold(&hold(None, false), Some("T-0001"), &[running]);
    let finished = next_hold(&opened, Some("T-0001"), std::slice::from_ref(&done));

    assert_eq!(
        held_id(&next_hold(&finished, Some("T-0002"), &[done])),
        None
    );
}

#[test]
fn does_not_hold_a_task_that_was_finished_when_it_was_opened() {
    let done = make_task(json!({"id": "T-0001", "state": "done"}));
    assert_eq!(
        held_id(&next_hold(&hold(None, false), Some("T-0001"), &[done])),
        None
    );
}

#[test]
fn holds_a_task_whose_row_came_after_the_selection() {
    let running = make_task(json!({"id": "T-0001"}));
    let done = Task {
        state: TaskState::Done,
        ..running.clone()
    };
    let early = next_hold::<Task>(&hold(None, false), Some("T-0001"), &[]);
    let loaded = next_hold(&early, Some("T-0001"), &[running]);

    assert_eq!(
        held_id(&next_hold(&loaded, Some("T-0001"), &[done])),
        Some("T-0001")
    );
}

#[test]
fn the_hint_of_a_folded_group_changes_when_its_first_task_turns_quiet() {
    let running = make_task(json!({"id": "T-0001", "updatedAt": "2026-10-08T09:00:30.000Z"}));
    let waiting = make_task(json!({"id": "T-0002", "state": "review"}));
    let mut nested = make_task(json!({"id": "T-0003", "updatedAt": "2026-10-08T09:00:10.000Z"}));
    nested.parent_id = Some("T-0001".into());
    let mut group = group(vec![running, waiting], vec![]);
    assert_eq!(next_hint_change_ms(&group, now()), Some(570_000));
    group.nested = vec![Arc::new(nested)];
    assert_eq!(next_hint_change_ms(&group, now()), Some(550_000));
}
