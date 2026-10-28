use natamux_proto::tasks::task::TaskState;

use super::*;

#[test]
fn a_bulk_run_does_not_start_twice_while_it_runs() {
    let mut runs = BulkRuns::default();
    assert!(runs.start(BulkRun::RemoveFinished));
    assert!(runs.running(BulkRun::RemoveFinished));
    assert!(!runs.start(BulkRun::RemoveFinished));
    // The other kind is its own run.
    assert!(!runs.running(BulkRun::DeleteTasks));
    assert!(runs.start(BulkRun::DeleteTasks));
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
