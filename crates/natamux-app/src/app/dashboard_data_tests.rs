use natamux_proto::tasks::task::TaskState;

use super::*;
use crate::tasks::table_rules::waits_for;

#[test]
fn a_bulk_run_does_not_start_twice_while_it_runs() {
    let mut runs = BulkRuns::default();
    assert!(runs.start(BulkRun::RemoveFinished));
    assert!(runs.running(BulkRun::RemoveFinished));
    assert!(!runs.start(BulkRun::RemoveFinished));
    runs.finish(BulkRun::RemoveFinished);
    assert!(!runs.running(BulkRun::RemoveFinished));
    assert!(runs.start(BulkRun::RemoveFinished));
}

fn task(id: &str, state: TaskState, updated_at: &str) -> Task {
    Task {
        id: id.into(),
        state,
        branch: Some(format!("natamux/{id}")),
        updated_at: updated_at.into(),
        ..Task::default()
    }
}

fn step(id: &str, step: Option<PreparingStep>, error: Option<&str>) -> DashboardEvent {
    DashboardEvent::WorkspaceStep(PreparingMessage {
        task_id: id.into(),
        step,
        error: error.map(String::from),
    })
}

#[test]
fn the_task_list_makes_the_link_live_and_drops_what_belongs_to_gone_tasks() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Terminals {
        task_id: "T-2".into(),
        terminals: TerminalList::default(),
    });
    data.set_events("T-2", "a", Vec::new());
    assert!(!data.loaded);
    data.apply(DashboardEvent::Tasks(vec![task(
        "T-1",
        TaskState::Running,
        "a",
    )]));
    assert!(data.loaded);
    assert_eq!(data.link, Link::Live);
    assert!(data.terminals.is_empty());
    assert!(data.events.is_empty());
}

#[test]
fn says_why_the_daemon_is_down_and_whether_it_tries_again() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Status(LinkStatus::Connecting));
    assert_eq!(data.link, Link::Loading);
    data.apply(DashboardEvent::Status(LinkStatus::Refused));
    assert!(matches!(
        data.link,
        Link::Down {
            retrying: false,
            ..
        }
    ));
    data.apply(DashboardEvent::Status(LinkStatus::Reconnecting(
        ClientError::NoToken,
    )));
    assert!(matches!(data.link, Link::Down { retrying: true, .. }));
}

#[test]
fn toasts_the_end_of_an_announced_workspace_only() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![task(
        "T-1",
        TaskState::Running,
        "a",
    )]));
    assert_eq!(data.announce("T-1"), None);
    assert_eq!(
        data.apply(step("T-1", Some(PreparingStep::Fetch), None)),
        None
    );
    assert_eq!(data.preparing.get("T-1"), Some(&PreparingStep::Fetch));
    let ready = data.apply(step("T-1", None, None));
    assert_eq!(
        ready.map(|notice| notice.text),
        Some("T-1 is ready in natamux/T-1.".into())
    );
    assert!(data.preparing.is_empty());
    assert_eq!(data.apply(step("T-9", None, None)), None);
}

#[test]
fn toasts_a_workspace_that_ended_before_it_was_announced() {
    let mut data = DashboardData::default();
    data.apply(step("T-3", None, Some("fetch failed")));
    let failed = data.announce("T-3");
    assert_eq!(
        failed,
        Some(Notice {
            text: "T-3: workspace failed. fetch failed".into(),
            tone: ToastTone::Error,
        })
    );
    assert_eq!(data.announce("T-3"), None);
}

#[test]
fn a_new_worktree_shows_its_first_step_before_the_daemon_tells_it() {
    let mut data = DashboardData::default();
    let before = data.gens;
    data.expect_preparing("T-1");
    assert_eq!(data.preparing.get("T-1"), Some(&PreparingStep::Fetch));
    assert_eq!(bumped(before, data.gens), vec![Part::Preparing]);

    // A step the daemon told already stays, and changes nothing.
    data.apply(step("T-2", Some(PreparingStep::Checkout), None));
    let before = data.gens;
    data.expect_preparing("T-2");
    assert_eq!(data.preparing.get("T-2"), Some(&PreparingStep::Checkout));
    assert_eq!(bumped(before, data.gens), Vec::<Part>::new());

    // The daemon's steps take over, and its end leaves no spinner.
    data.apply(step("T-1", Some(PreparingStep::Checkout), None));
    assert_eq!(data.preparing.get("T-1"), Some(&PreparingStep::Checkout));
    data.apply(step("T-1", None, Some("checkout failed")));
    assert_eq!(data.preparing.get("T-1"), None);
}

#[test]
fn keeps_only_the_last_workspaces_that_ended_unannounced() {
    let mut data = DashboardData::default();
    for n in 0..ENDED_KEPT + 8 {
        data.apply(step(&format!("T-{n}"), None, None));
    }
    assert_eq!(data.ended_early.len(), ENDED_KEPT);
    assert_eq!(data.announce("T-0"), None);
    let newest = format!("T-{}", ENDED_KEPT + 7);
    assert!(data.announce(&newest).is_some());
}

#[test]
fn reads_the_events_of_attention_cards_again_when_the_task_changed() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![
        task("T-1", TaskState::NeedsInput, "a"),
        task("T-2", TaskState::Running, "a"),
        task("T-3", TaskState::Failed, "a"),
    ]));
    assert_eq!(
        data.events_to_read(),
        [("T-1".into(), "a".into()), ("T-3".into(), "a".into())]
    );
    data.set_events("T-1", "a", Vec::new());
    data.set_events("T-3", "old", Vec::new());
    assert_eq!(data.events_to_read(), [("T-3".into(), "a".into())]);
}

#[test]
fn merges_an_answer_and_forgets_a_deleted_task() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![task(
        "T-1",
        TaskState::Draft,
        "a",
    )]));
    data.merge_task(task("T-1", TaskState::Ready, "b"));
    data.merge_task(task("T-2", TaskState::Draft, "c"));
    assert_eq!(data.tasks.len(), 2);
    assert_eq!(data.tasks[0].id, "T-2");
    assert_eq!(
        data.task("T-1").map(|task| task.state),
        Some(TaskState::Ready)
    );
    data.forget_task("T-1");
    assert!(data.task("T-1").is_none());
}

#[test]
fn hides_a_task_while_it_is_deleted_and_keeps_its_newest_state() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![
        task("T-1", TaskState::Running, "a"),
        task("T-2", TaskState::Draft, "a"),
    ]));
    data.apply(DashboardEvent::Terminals {
        task_id: "T-1".into(),
        terminals: TerminalList::default(),
    });
    data.hide_task("T-1");
    assert!(data.task("T-1").is_none());
    assert!(data.is_deleting("T-1"));
    assert!(!data.terminals.contains_key("T-1"));
    // The daemon writes the task while it deletes it: the feed must not show it again.
    data.apply(DashboardEvent::TaskChanged(Box::new(task(
        "T-1",
        TaskState::Running,
        "b",
    ))));
    data.apply(DashboardEvent::Tasks(vec![
        task("T-1", TaskState::Done, "c"),
        task("T-2", TaskState::Draft, "a"),
    ]));
    data.merge_task(task("T-1", TaskState::Ready, "0"));
    assert!(data.task("T-1").is_none());
    assert_eq!(data.tasks.len(), 1);
    // The delete failed: the task shows again as the feed last brought it.
    data.restore_task("T-1");
    assert!(!data.is_deleting("T-1"));
    assert_eq!(
        data.task("T-1").map(|task| task.state),
        Some(TaskState::Done)
    );
}

#[test]
fn a_hidden_task_draws_nothing_and_goes_when_its_delete_worked() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![task(
        "T-1",
        TaskState::Running,
        "a",
    )]));
    data.hide_task("T-1");
    let before = data.gens;
    let changed = DashboardEvent::TaskChanged(Box::new(task("T-1", TaskState::Done, "b")));
    assert!(!data.changes(&changed));
    data.apply(changed);
    assert!(bumped(before, data.gens).is_empty());
    data.forget_task("T-1");
    assert!(!data.is_deleting("T-1"));
    data.restore_task("T-1");
    assert!(data.task("T-1").is_none());
}

#[test]
fn hiding_a_task_the_app_does_not_know_does_nothing() {
    let mut data = DashboardData::default();
    data.hide_task("T-9");
    assert!(!data.is_deleting("T-9"));
}

#[test]
fn a_late_answer_keeps_the_task_the_feed_brought_since() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![task(
        "T-1",
        TaskState::Running,
        "2026-10-08T10:00:00.000Z",
    )]));
    data.merge_task(task("T-1", TaskState::Draft, "2026-10-08T09:00:00.000Z"));
    assert_eq!(
        data.task("T-1").map(|task| task.state),
        Some(TaskState::Running)
    );
}

/// The parts whose generation moved between two snapshots.
fn bumped(before: Generations, after: Generations) -> Vec<Part> {
    let all = [
        Part::Tasks,
        Part::Repos,
        Part::Jobs,
        Part::Activity,
        Part::Usage,
        Part::Limits,
        Part::DiffStats,
        Part::Terminals,
        Part::Preparing,
        Part::Link,
        Part::Events,
        Part::View,
    ];
    all.into_iter()
        .filter(|&part| before.get(part) != after.get(part))
        .collect()
}

#[test]
fn bumps_only_the_parts_an_event_changed() {
    let mut data = DashboardData::default();
    let check = |data: &mut DashboardData, event: DashboardEvent, parts: &[Part]| {
        let before = data.gens;
        data.apply(event);
        assert_eq!(bumped(before, data.gens), parts);
    };
    let list = DashboardEvent::Tasks(vec![task("T-1", TaskState::Running, "a")]);
    let first = [Part::Tasks, Part::Terminals, Part::Link, Part::Events];
    check(&mut data, list.clone(), &first);
    check(&mut data, list, &[]);
    let one = Box::new(task("T-1", TaskState::Review, "b"));
    check(&mut data, DashboardEvent::TaskChanged(one), &[Part::Tasks]);
    let activity = DashboardEvent::Activity([("T-1".to_string(), BTreeMap::new())].into());
    check(&mut data, activity.clone(), &[Part::Activity]);
    check(&mut data, activity, &[]);
    let fetch = step("T-1", Some(PreparingStep::Fetch), None);
    check(&mut data, fetch, &[Part::Preparing]);
    let refused = DashboardEvent::Status(LinkStatus::Refused);
    check(&mut data, refused, &[Part::Link]);

    let before = data.gens;
    data.set_events("T-1", "b", Vec::new());
    assert_eq!(bumped(before, data.gens), [Part::Events]);
    let before = data.gens;
    data.forget_task("T-1");
    let gone = [Part::Tasks, Part::Terminals, Part::Events];
    assert_eq!(bumped(before, data.gens), gone);
}

#[test]
fn a_task_that_did_not_change_keeps_its_arc_and_is_found_by_its_id() {
    let mut data = DashboardData::default();
    data.apply(DashboardEvent::Tasks(vec![
        task("T-1", TaskState::Running, "a"),
        task("T-2", TaskState::Draft, "a"),
    ]));
    let held = |data: &DashboardData, id: &str| data.shared_task(id).cloned();
    let (one, two) = (held(&data, "T-1"), held(&data, "T-2"));
    assert!(one.is_some() && two.is_some());
    let same = |a: &Option<Arc<Task>>, b: &Option<Arc<Task>>| match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    };

    // One task of the feed: the other task is the one held, not a copy.
    data.apply(DashboardEvent::TaskChanged(Box::new(task(
        "T-1",
        TaskState::Review,
        "b",
    ))));
    assert!(!same(&held(&data, "T-1"), &one));
    assert!(same(&held(&data, "T-2"), &two));
    let listed = data.tasks.iter().find(|task| task.id == "T-2").cloned();
    assert!(same(&listed, &two));
    let one = held(&data, "T-1");

    // An equal task, and a list with one new task: what did not change stays.
    data.merge_task(task("T-1", TaskState::Review, "b"));
    assert!(same(&held(&data, "T-1"), &one));
    data.apply(DashboardEvent::Tasks(vec![
        task("T-3", TaskState::Draft, "c"),
        task("T-1", TaskState::Review, "b"),
        task("T-2", TaskState::Ready, "b"),
    ]));
    assert!(same(&held(&data, "T-1"), &one));
    assert!(!same(&held(&data, "T-2"), &two));
    let ids: Vec<&str> = data.tasks.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, ["T-3", "T-1", "T-2"]);

    // The lookup follows every change of the list.
    for task in &data.tasks {
        assert!(same(&held(&data, &task.id), &Some(task.clone())));
    }
    let state = |data: &DashboardData, id: &str| data.task(id).map(|task| task.state);
    data.hide_task("T-3");
    assert_eq!(state(&data, "T-3"), None);
    data.restore_task("T-3");
    assert_eq!(state(&data, "T-3"), Some(TaskState::Draft));
    data.forget_task("T-1");
    assert_eq!(state(&data, "T-1"), None);
    assert!(data.shared_task("T-1").is_none());
    assert_eq!(state(&data, "T-2"), Some(TaskState::Ready));
}

#[test]
fn what_a_task_waits_for_follows_every_change_of_the_list() {
    let child = |id: &str, state: TaskState, updated_at: &str| Task {
        parent_id: Some("T-1".into()),
        ..task(id, state, updated_at)
    };
    let mut data = DashboardData::default();
    let mut parent = task("T-1", TaskState::Ready, "a");
    parent.depends_on = vec!["T-4".into()];
    data.apply(DashboardEvent::Tasks(vec![
        parent.clone(),
        child("T-2", TaskState::Running, "a"),
        task("T-4", TaskState::Review, "a"),
    ]));
    let waits = |data: &DashboardData| {
        let waits = data.waits_for(&parent);
        // The lookups give what a read of the whole list gives.
        assert_eq!(waits, waits_for(&parent, &data.tasks));
        waits
    };
    assert_eq!(waits(&data), ["T-4", "T-2"]);
    data.merge_task(child("T-3", TaskState::Draft, "b"));
    assert_eq!(waits(&data), ["T-4", "T-3", "T-2"]);
    data.merge_task(task("T-4", TaskState::Done, "c"));
    assert_eq!(waits(&data), ["T-3", "T-2"]);
    data.hide_task("T-3");
    assert_eq!(waits(&data), ["T-2"]);
    data.restore_task("T-3");
    assert_eq!(waits(&data), ["T-3", "T-2"]);
    data.forget_task("T-2");
    assert_eq!(waits(&data), ["T-3"]);
    data.apply(DashboardEvent::Tasks(vec![parent.clone()]));
    assert!(waits(&data).is_empty());
}
