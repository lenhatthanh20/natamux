use super::*;

// The task list comes newest first, so a task with an event moves to the front.
fn at(id: &str, minute: u32) -> Task {
    let mut task = running(id);
    task.updated_at = format!("2026-10-08T09:{minute:02}:00.000Z");
    task
}

fn with_state(mut task: Task, state: TaskState) -> Task {
    task.state = state;
    task
}

#[test]
fn orders_the_default_grid_by_task_id_not_by_the_last_update() {
    let tasks = [at("T-0012", 3), at("T-0003", 2), at("T-0007", 1)];
    assert_eq!(panes(None, &tasks), ["T-0003", "T-0007", "T-0012"]);
}

#[test]
fn keeps_the_order_when_a_task_gets_an_update() {
    let before = [at("T-0012", 3), at("T-0007", 2), at("T-0003", 1)];
    let after = [
        with_state(at("T-0003", 9), TaskState::NeedsInput),
        at("T-0012", 3),
        at("T-0007", 2),
    ];
    assert_eq!(panes(None, &before), ["T-0003", "T-0007", "T-0012"]);
    assert_eq!(panes(None, &after), ["T-0003", "T-0007", "T-0012"]);
}

#[test]
fn keeps_the_others_in_order_when_a_pane_leaves_or_joins() {
    let left = [
        with_state(at("T-0007", 9), TaskState::Review),
        at("T-0012", 3),
        at("T-0003", 1),
    ];
    assert_eq!(panes(None, &left), ["T-0003", "T-0012"]);
    let joined = [
        at("T-0005", 9),
        at("T-0012", 3),
        at("T-0007", 2),
        at("T-0003", 1),
    ];
    assert_eq!(
        panes(None, &joined),
        ["T-0003", "T-0005", "T-0007", "T-0012"]
    );
}

#[test]
fn compares_ids_by_number() {
    let tasks = [
        at("T-10000", 4),
        at("T-1000", 3),
        at("T-9999", 2),
        at("T-0999", 1),
    ];
    assert_eq!(
        panes(None, &tasks),
        ["T-0999", "T-1000", "T-9999", "T-10000"]
    );
}

#[test]
fn shows_the_six_lowest_ids_whatever_gets_an_update() {
    let many: Vec<Task> = (0..8).map(|n| at(&format!("T-00{}", n + 10), n)).collect();
    let lowest = ["T-0010", "T-0011", "T-0012", "T-0013", "T-0014", "T-0015"];
    let reversed: Vec<Task> = many.iter().rev().cloned().collect();
    assert_eq!(panes(None, &reversed), lowest);
    let mut updated = vec![at("T-0017", 30), at("T-0010", 31)];
    updated.extend(many[1..7].iter().cloned());
    assert_eq!(panes(None, &updated), lowest);
}

#[test]
fn keeps_the_order_the_user_picked_when_tasks_get_updates() {
    let tasks = [at("T-0003", 9), at("T-0012", 3), at("T-0007", 2)];
    assert_eq!(
        panes(Some(&["T-0012", "T-0003"]), &tasks),
        ["T-0012", "T-0003"]
    );
    assert_eq!(
        panes(Some(&["T-0012", "T-0003", "T-0007"]), &tasks),
        ["T-0012", "T-0003", "T-0007"]
    );
    assert_eq!(
        panes(Some(&["T-0012", "T-0007"]), &tasks),
        ["T-0012", "T-0007"]
    );
}

// Running tasks in the order of the task list. The first id is the newest.
fn newest_first(ids: &[&str]) -> Vec<Task> {
    ids.iter()
        .enumerate()
        .map(|(index, id)| {
            let mut task = running(id);
            task.updated_at = format!("2026-10-08T09:00:{:02}.000Z", ids.len() - index);
            task
        })
        .collect()
}

// A task event as the live feed merges it: newer than every task, so it goes first.
fn event(tasks: &[Task], id: &str, change: impl FnOnce(&mut Task)) -> Vec<Task> {
    let newest = tasks.iter().map(|task| task.updated_at.clone()).max();
    let mut known = tasks
        .iter()
        .find(|task| task.id == id)
        .cloned()
        .unwrap_or_else(|| running(id));
    change(&mut known);
    known.updated_at = format!("{}~", newest.unwrap_or_default());
    let mut next = vec![known];
    next.extend(tasks.iter().filter(|task| task.id != id).cloned());
    next
}

fn touch(tasks: &[Task], id: &str) -> Vec<Task> {
    event(tasks, id, |_| {})
}

fn set_state(tasks: &[Task], id: &str, state: TaskState) -> Vec<Task> {
    event(tasks, id, |task| task.state = state)
}

fn grid() -> Vec<Task> {
    newest_first(&["T-0012", "T-0007", "T-0003"])
}

fn id_range(first: u32, last: u32) -> Vec<String> {
    (first..=last).map(|n| format!("T-{n:04}")).collect()
}

#[test]
fn orders_panes_by_task_id() {
    assert_eq!(panes(None, &grid()), ["T-0003", "T-0007", "T-0012"]);
    assert_eq!(panes(None, &newest_first(&["T-0012"])), ["T-0012"]);
    assert!(panes(None, &[]).is_empty());
}

#[test]
fn leaves_out_tasks_without_a_working_agent_or_a_workspace_even_with_the_lowest_id() {
    let mut tasks = set_state(&grid(), "T-0001", TaskState::Ready);
    tasks = set_state(&tasks, "T-0002", TaskState::Review);
    tasks = set_state(&tasks, "T-0000", TaskState::Done);
    tasks = event(&tasks, "T-0004", |task| task.workspace = None);
    assert_eq!(panes(None, &tasks), ["T-0003", "T-0007", "T-0012"]);
    let only_ready: Vec<Task> = tasks
        .into_iter()
        .filter(|task| task.state == TaskState::Ready)
        .collect();
    assert!(panes(None, &only_ready).is_empty());
}

#[test]
fn does_not_move_panes_on_task_events() {
    let expected = ["T-0003", "T-0007", "T-0012"];
    let mut tasks = touch(&grid(), "T-0012");
    assert_eq!(tasks[0].id, "T-0012");
    assert_eq!(panes(None, &tasks), expected);
    tasks = touch(&tasks, "T-0003");
    assert_eq!(panes(None, &tasks), expected);
    tasks = set_state(&tasks, "T-0007", TaskState::NeedsInput);
    assert_eq!(panes(None, &tasks), expected);
    for _ in 0..5 {
        tasks = touch(&tasks, "T-0012");
    }
    assert_eq!(panes(None, &tasks), expected);
    tasks = set_state(&tasks, "T-0001", TaskState::Ready);
    tasks = event(&tasks, "T-0001", |task| task.title = "Renamed".into());
    assert_eq!(panes(None, &tasks), expected);
}

#[test]
fn orders_by_id_when_tasks_have_the_same_update_time() {
    let same = [running("T-0012"), running("T-0003"), running("T-0007")];
    assert_eq!(panes(None, &same), ["T-0003", "T-0007", "T-0012"]);
}

#[test]
fn keeps_the_others_in_order_when_a_pane_leaves() {
    assert_eq!(
        panes(None, &set_state(&grid(), "T-0007", TaskState::Review)),
        ["T-0003", "T-0012"]
    );
    assert_eq!(
        panes(None, &set_state(&grid(), "T-0003", TaskState::Done)),
        ["T-0007", "T-0012"]
    );
    let no_workspace = event(&grid(), "T-0012", |task| task.workspace = None);
    assert_eq!(panes(None, &no_workspace), ["T-0003", "T-0007"]);
    let all = ["T-0003", "T-0007", "T-0012"]
        .iter()
        .fold(grid(), |tasks, id| set_state(&tasks, id, TaskState::Review));
    assert!(panes(None, &all).is_empty());
}

#[test]
fn puts_a_pane_that_joins_in_its_place_by_id() {
    assert_eq!(
        panes(None, &touch(&grid(), "T-0005")),
        ["T-0003", "T-0005", "T-0007", "T-0012"]
    );
    assert_eq!(
        panes(None, &touch(&grid(), "T-0001")),
        ["T-0001", "T-0003", "T-0007", "T-0012"]
    );
    assert_eq!(
        panes(None, &touch(&grid(), "T-0020")),
        ["T-0003", "T-0007", "T-0012", "T-0020"]
    );
    let left = set_state(&grid(), "T-0007", TaskState::Review);
    assert_eq!(panes(None, &left), ["T-0003", "T-0012"]);
    let back = set_state(&left, "T-0007", TaskState::Running);
    assert_eq!(panes(None, &back), ["T-0003", "T-0007", "T-0012"]);
}

#[test]
fn compares_ids_by_number_not_by_text() {
    // A text sort puts T-10000 before T-9999.
    let tasks = newest_first(&["T-1000", "T-0999", "T-10000", "T-9999"]);
    assert_eq!(
        panes(None, &tasks),
        ["T-0999", "T-1000", "T-9999", "T-10000"]
    );
}

#[test]
fn shows_the_six_lowest_ids_when_more_agents_run_whatever_gets_an_event() {
    let shuffled = [
        "T-0005", "T-0008", "T-0002", "T-0007", "T-0001", "T-0004", "T-0006", "T-0003",
    ];
    let mut tasks = newest_first(&shuffled);
    assert_eq!(MAX_PANES, 6);
    assert_eq!(panes(None, &tasks), id_range(1, 6));
    for id in id_range(1, 8) {
        tasks = touch(&tasks, &id);
        assert_eq!(panes(None, &tasks), id_range(1, 6));
    }
    assert_eq!(panes(None, &touch(&tasks, "T-0008")), id_range(1, 6));
    let mut reversed = id_range(1, 6);
    reversed.reverse();
    let reversed: Vec<&str> = reversed.iter().map(String::as_str).collect();
    assert_eq!(panes(None, &newest_first(&reversed)), id_range(1, 6));
    assert_eq!(
        panes(None, &set_state(&tasks, "T-0002", TaskState::Review)),
        ["T-0001", "T-0003", "T-0004", "T-0005", "T-0006", "T-0007"]
    );
}

#[test]
fn keeps_the_list_order_on_task_events_adds_last_and_closes_in_place() {
    let tasks = newest_first(&["T-0003", "T-0007", "T-0012"]);
    // Neither id nor recency order: a sort would fail this.
    let chosen = ["T-0012", "T-0003"];
    assert_eq!(panes(Some(&chosen), &tasks), chosen);
    let later = touch(&touch(&tasks, "T-0003"), "T-0012");
    assert_eq!(panes(Some(&chosen), &touch(&tasks, "T-0003")), chosen);
    assert_eq!(panes(Some(&chosen), &later), chosen);
    assert_eq!(
        panes(Some(&["T-0012", "T-0003", "T-0007"]), &later),
        ["T-0012", "T-0003", "T-0007"]
    );
    assert_eq!(
        panes(Some(&["T-0012", "T-0007"]), &later),
        ["T-0012", "T-0007"]
    );
}

#[test]
fn stays_empty_after_the_last_pane_closes_without_the_default_panes() {
    let tasks = newest_first(&["T-0003", "T-0007", "T-0012"]);
    assert!(panes(Some(&[]), &tasks).is_empty());
}
