use serde_json::{Value, json};

use super::*;

fn make_task(overrides: Value) -> Task {
    let mut base = json!({
        "id": "T-0001", "title": "Example", "state": "running", "workspace": "/work/T-0001",
        "stage": "implement", "stages": ["implement", "review", "release"],
        "createdAt": "2026-10-08T09:00:00.000Z", "updatedAt": "2026-10-08T09:00:00.000Z"
    });
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), overrides) {
        base.extend(extra);
    }
    serde_json::from_value(base).unwrap_or_default()
}

fn repos() -> Vec<RepoRef> {
    [("default", "natamux"), ("web", "Web app")]
        .iter()
        .map(|&(id, name)| RepoRef {
            id: id.into(),
            name: name.into(),
        })
        .collect()
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|task| task.id.as_str()).collect()
}

fn pins(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn groups_workspaces_by_repository_the_newest_first_and_leaves_out_tasks_without_one() {
    let older = make_task(json!({"id": "T-0001"}));
    let newer = make_task(json!({"id": "T-0002", "createdAt": "2026-10-08T10:00:00.000Z"}));
    let web = make_task(json!({"id": "T-0003", "repo": "web"}));
    let backlog = make_task(json!({"id": "T-0004", "workspace": null}));

    let result = workspace_groups(&repos(), &[older, newer, web, backlog], &[], None);

    let names: Vec<&str> = result.groups.iter().map(|g| g.repo.name.as_str()).collect();
    assert_eq!(names, ["natamux", "Web app"]);
    assert_eq!(ids(&result.groups[0].open), ["T-0002", "T-0001"]);
    assert_eq!(ids(&result.groups[1].open), ["T-0003"]);
}

#[test]
fn moves_pinned_workspaces_to_their_own_list_and_finished_ones_to_the_end() {
    let pinned = make_task(json!({"id": "T-0001"}));
    let done = make_task(json!({"id": "T-0002", "state": "done"}));

    let result = workspace_groups(
        &repos(),
        &[pinned.clone(), done.clone()],
        &pins(&["T-0001"]),
        None,
    );

    assert_eq!(result.pinned, [pinned]);
    assert!(result.groups[0].open.is_empty());
    assert_eq!(result.groups[0].finished, [done]);
}

#[test]
fn keeps_a_held_task_that_finished_among_the_open_rows_in_its_place() {
    let older = make_task(json!({"id": "T-0001"}));
    let held = make_task(
        json!({"id": "T-0002", "state": "done", "createdAt": "2026-10-08T10:00:00.000Z"}),
    );
    let newer = make_task(json!({"id": "T-0003", "createdAt": "2026-10-08T11:00:00.000Z"}));

    let result = workspace_groups(&repos(), &[older, held, newer], &[], Some("T-0002"));

    assert_eq!(ids(&result.groups[0].open), ["T-0003", "T-0002", "T-0001"]);
    assert!(result.groups[0].finished.is_empty());
}

#[test]
fn drops_a_task_when_its_workspace_is_removed_and_lists_it_again_in_a_new_one() {
    let removed = make_task(json!({"id": "T-0001", "state": "ready", "workspace": null}));
    let result = workspace_groups(
        &repos(),
        std::slice::from_ref(&removed),
        &pins(&["T-0001"]),
        None,
    );
    assert!(result.pinned.is_empty());
    assert_eq!(result.groups.len(), 2);
    assert!(result.groups.iter().all(|group| group.open.is_empty()));

    let again = Task {
        workspace: Some("/work/T-0001".into()),
        ..removed
    };
    let result = workspace_groups(&repos(), std::slice::from_ref(&again), &[], None);
    assert_eq!(result.groups[0].open, [again]);
}

#[test]
fn keeps_an_empty_repository_and_adds_a_group_for_a_repository_it_does_not_know() {
    let stray = make_task(json!({"id": "T-0001", "repo": "api"}));

    let result = workspace_groups(&repos(), &[stray], &[], None);

    let seen: Vec<(&str, usize)> = result
        .groups
        .iter()
        .map(|g| (g.repo.id.as_str(), g.open.len()))
        .collect();
    assert_eq!(seen, [("default", 0), ("web", 0), ("api", 1)]);
}

#[test]
fn has_no_groups_when_there_are_no_repositories_and_no_workspaces() {
    let backlog = make_task(json!({"id": "T-0001", "workspace": null}));
    assert!(
        workspace_groups(&[], &[backlog], &[], None)
            .groups
            .is_empty()
    );
}

fn parent() -> Task {
    make_task(json!({"id": "T-0001"}))
}

fn later() -> Task {
    make_task(
        json!({"id": "T-0003", "parentId": "T-0001", "createdAt": "2026-10-08T11:00:00.000Z"}),
    )
}

fn first() -> Task {
    make_task(
        json!({"id": "T-0002", "parentId": "T-0001", "createdAt": "2026-10-08T10:00:00.000Z"}),
    )
}

#[test]
fn nests_open_subtasks_under_their_parent_the_oldest_first_and_not_at_the_top() {
    let result = workspace_groups(&repos(), &[parent(), later(), first()], &[], None);

    assert_eq!(ids(&result.groups[0].open), ["T-0001"]);
    assert_eq!(ids(&result.children["T-0001"]), ["T-0002", "T-0003"]);
    assert_eq!(ids(&result.groups[0].nested), ["T-0002", "T-0003"]);
}

#[test]
fn nests_under_a_pinned_parent_too() {
    let result = workspace_groups(&repos(), &[parent(), first()], &pins(&["T-0001"]), None);

    assert_eq!(result.pinned, [parent()]);
    assert_eq!(result.children["T-0001"], [first()]);
    assert!(result.groups[0].open.is_empty());
}

#[test]
fn keeps_a_subtask_at_the_top_when_its_parent_is_not_shown_or_is_in_another_repository() {
    let no_workspace = Task {
        workspace: None,
        ..parent()
    };
    let elsewhere = Task {
        repo: Some("web".into()),
        ..parent()
    };
    for shown_parent in [no_workspace, elsewhere] {
        let result = workspace_groups(&repos(), &[shown_parent, first()], &[], None);
        assert_eq!(result.groups[0].open, [first()]);
        assert!(result.children.is_empty());
    }
    assert_eq!(
        workspace_groups(&repos(), &[first()], &[], None).groups[0].open,
        [first()]
    );
}

#[test]
fn keeps_an_open_subtask_of_a_finished_parent_at_the_top_so_it_never_hides() {
    let done = Task {
        state: TaskState::Done,
        ..parent()
    };
    let result = workspace_groups(&repos(), &[done.clone(), first()], &[], None);

    assert_eq!(result.groups[0].open, [first()]);
    assert_eq!(result.groups[0].finished, [done]);
    assert!(result.children.is_empty());
}

#[test]
fn nests_one_level_only_and_never_under_itself() {
    let grandchild = make_task(json!({"id": "T-0004", "parentId": "T-0002"}));
    let own = make_task(json!({"id": "T-0005", "parentId": "T-0005"}));
    let result = workspace_groups(&repos(), &[parent(), first(), grandchild, own], &[], None);

    assert_eq!(result.children.len(), 1);
    assert_eq!(result.children["T-0001"], [first()]);
    let mut open: Vec<&str> = ids(&result.groups[0].open);
    open.sort_unstable();
    assert_eq!(open, ["T-0001", "T-0004", "T-0005"]);
}

#[test]
fn orders_children_by_creation_then_by_id_and_keeps_the_order_on_live_updates() {
    let twin =
        make_task(json!({"id": "T-0006", "parentId": "T-0001", "createdAt": first().created_at}));
    let order = |tasks: &[Task]| -> Vec<String> {
        let result = workspace_groups(&repos(), tasks, &[], None);
        result.children["T-0001"]
            .iter()
            .map(|task| task.id.clone())
            .collect()
    };

    assert_eq!(
        order(&[parent(), later(), twin.clone(), first()]),
        ["T-0002", "T-0006", "T-0003"]
    );
    let updated = Task {
        state: TaskState::NeedsInput,
        updated_at: "2026-10-09T09:00:00.000Z".into(),
        ..first()
    };
    assert_eq!(
        order(&[parent(), later(), twin, updated]),
        ["T-0002", "T-0006", "T-0003"]
    );
}

#[test]
fn keeps_the_order_of_top_level_rows_when_one_of_them_changes() {
    let a = make_task(json!({"id": "T-0007", "createdAt": "2026-10-08T12:00:00.000Z"}));
    let b = make_task(json!({"id": "T-0008", "createdAt": "2026-10-08T13:00:00.000Z"}));
    let order = |tasks: &[Task]| -> Vec<String> {
        let result = workspace_groups(&repos(), tasks, &[], None);
        result.groups[0]
            .open
            .iter()
            .map(|task| task.id.clone())
            .collect()
    };

    assert_eq!(
        order(&[a.clone(), b.clone(), parent()]),
        ["T-0008", "T-0007", "T-0001"]
    );
    let touched = Task {
        title: "Renamed".into(),
        updated_at: "2026-10-09T09:00:00.000Z".into(),
        ..a
    };
    assert_eq!(
        order(&[touched, b, parent()]),
        ["T-0008", "T-0007", "T-0001"]
    );
}

#[test]
fn keeps_finished_and_pinned_subtasks_where_they_were() {
    let done = Task {
        state: TaskState::Done,
        ..first()
    };
    let result = workspace_groups(
        &repos(),
        &[parent(), done.clone(), later()],
        &pins(&["T-0003"]),
        None,
    );

    assert_eq!(result.groups[0].finished, [done]);
    assert_eq!(result.pinned, [later()]);
    assert!(result.children.is_empty());
}

#[test]
fn adds_the_repositories_that_only_tasks_name_default_for_tasks_without_one() {
    let tasks = [
        make_task(json!({"id": "T-0001"})),
        make_task(json!({"id": "T-0002", "repo": "api"})),
    ];
    let web = RepoRef {
        id: "web".into(),
        name: "Web app".into(),
    };

    let known = known_repos(std::slice::from_ref(&web), &tasks);

    let pair = |id: &str, name: &str| RepoRef {
        id: id.into(),
        name: name.into(),
    };
    assert_eq!(known, [web, pair("default", "default"), pair("api", "api")]);
    assert!(known_repos(&[], &[]).is_empty());
}

#[test]
fn drops_the_part_that_names_the_task() {
    let branch = |id: &str, branch: Option<&str>| {
        short_branch(&Task {
            id: id.into(),
            branch: branch.map(Into::into),
            ..Task::default()
        })
    };
    assert_eq!(
        branch("T-0012", Some("natamux/t-0012-fix-login")),
        "fix-login"
    );
    assert_eq!(branch("T-0012", Some("feature/x")), "feature/x");
    assert_eq!(branch("T-0012", None), "");
}

fn now() -> i64 {
    parse_iso_ms("2026-10-08T09:01:00.000Z").unwrap_or_default()
}

fn group(open: Vec<Task>, finished: Vec<Task>) -> RepoGroup {
    RepoGroup {
        repo: RepoRef {
            id: "default".into(),
            name: "natamux".into(),
        },
        open,
        finished,
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
    folded.nested = vec![waiting];
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
    let early = next_hold(&hold(None, false), Some("T-0001"), &[]);
    let loaded = next_hold(&early, Some("T-0001"), &[running]);

    assert_eq!(
        held_id(&next_hold(&loaded, Some("T-0001"), &[done])),
        Some("T-0001")
    );
}
