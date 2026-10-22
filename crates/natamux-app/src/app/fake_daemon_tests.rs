use natamux_client::TerminalEvent;
use natamux_proto::workspaces::workspace::PushStatus;

use super::*;

fn drain(receiver: &mut UnboundedReceiver<DashboardEvent>) -> Vec<DashboardEvent> {
    let mut out = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        out.push(event);
    }
    out
}

fn tasks_of(events: &[DashboardEvent]) -> Option<&Vec<Task>> {
    events.iter().rev().find_map(|event| match event {
        DashboardEvent::Tasks(tasks) => Some(tasks),
        _ => None,
    })
}

#[test]
fn every_part_of_the_seed_reads() {
    let seed: Value = serde_json::from_str(SEED).unwrap();
    let data = Data::seed();
    assert_eq!(data.tasks.len(), seed["tasks"].as_array().unwrap().len());
    assert_eq!(data.repos.len(), 2);
    assert_eq!(
        data.worktrees_dir.as_deref(),
        Some("/work/.natamux/worktrees")
    );
    assert_eq!(data.jobs.len(), 2);
    assert_eq!(data.activity.len(), 3);
    assert_eq!(data.usage.len(), 3);
    assert_eq!(data.diff_stats.len(), 5);
    assert_eq!(data.terminals.len(), 4);
    assert_eq!(data.events.len(), 2);
    assert_eq!(data.templates.len(), 2);
    assert_eq!(data.branches.default_branch.as_deref(), Some("main"));
    assert_eq!(data.skills.skills.len(), 1);
    assert_eq!(data.changes.len(), 1);
    assert_eq!(data.graphs.len(), 5);
    assert_eq!(data.ended["T-0139"].len(), 5);
}

#[test]
fn has_every_task_state_a_subtask_and_two_projects() {
    let data = Data::seed();
    let states: BTreeSet<&str> = data.tasks.iter().map(|task| task.state.name()).collect();
    for state in [
        "draft",
        "ready",
        "running",
        "needs-input",
        "review",
        "blocked",
        "failed",
        "done",
        "cancelled",
        "unknown",
    ] {
        assert!(states.contains(state), "{state}");
    }
    assert!(data.tasks.iter().any(|task| task.parent_id.is_some()));
    let repos: BTreeSet<_> = data
        .tasks
        .iter()
        .filter_map(|task| task.repo.clone())
        .collect();
    assert_eq!(repos.len(), 2);
    for id in PINNED {
        assert!(data.tasks.iter().any(|task| task.id == *id));
    }
}

#[test]
fn has_a_question_and_a_failure_reason() {
    let fake = FakeDaemon::default();
    let asked = fake.list_events("T-0141").unwrap();
    assert!(natamux_proto::events::task_event::latest_question(&asked).is_some());
    let failed = fake.list_events("T-0137").unwrap();
    assert_eq!(
        failed[0].event.failure_reason(),
        Some("The check command exited with 1: 3 tests failed.")
    );
}

#[test]
fn a_feed_gets_everything_then_each_change() {
    let fake = FakeDaemon::default();
    let (mut receiver, _) = fake.watch();
    let first = drain(&mut receiver);
    assert_eq!(first[0], DashboardEvent::Status(LinkStatus::Live));
    assert_eq!(first.len(), 8);
    let task = fake
        .create_task(NewTaskInput {
            title: "Add a subtask".into(),
            parent_id: Some("T-0146".into()),
            repo: None,
        })
        .unwrap();
    assert_eq!(task.id, "T-0201");
    assert_eq!(task.repo.as_deref(), Some("natamux"));
    let after = drain(&mut receiver);
    assert_eq!(
        tasks_of(&after).map(|tasks| tasks[0].id.as_str()),
        Some("T-0201")
    );
}

#[test]
fn renames_moves_state_and_deletes() {
    let fake = FakeDaemon::default();
    assert_eq!(fake.rename_task("T-0131", "Telex").unwrap().title, "Telex");
    let answer = fake
        .send_event("T-0143", TaskEventInput::human("review.approved"))
        .unwrap();
    assert!(answer.accepted);
    assert_eq!(answer.task.state, TaskState::Done);
    let ignored = fake
        .send_event("T-0143", TaskEventInput::human("note.added"))
        .unwrap();
    assert!(!ignored.accepted);
    assert_eq!(fake.delete_task("T-0131").unwrap().id, "T-0131");
    assert!(matches!(
        fake.delete_task("T-0131"),
        Err(ClientError::Status { status: 404, .. })
    ));
}

#[test]
fn makes_a_workspace_and_sends_its_steps() {
    let fake = FakeDaemon::default();
    let (mut receiver, _) = fake.watch();
    drain(&mut receiver);
    let task = fake
        .new_workspace(NewWorkspaceInput {
            title: "Try it".into(),
            repo: Some("natamux-site".into()),
            ..NewWorkspaceInput::default()
        })
        .unwrap();
    assert_eq!(
        task.workspace.as_deref(),
        Some("/work/.natamux/worktrees/T-0201")
    );
    let steps: Vec<Option<PreparingStep>> = drain(&mut receiver)
        .into_iter()
        .filter_map(|event| match event {
            DashboardEvent::WorkspaceStep(message) => Some(message.step),
            _ => None,
        })
        .collect();
    assert_eq!(
        steps,
        [
            Some(PreparingStep::Fetch),
            Some(PreparingStep::Checkout),
            None
        ]
    );
}

#[test]
fn removes_a_workspace_with_or_without_its_branch() {
    let fake = FakeDaemon::default();
    let kept = fake.remove_workspace("T-0139", false).unwrap();
    assert!(kept.task.workspace.is_none());
    assert!(kept.task.branch.is_some());
    assert!(!kept.branch_deleted);
    assert!(fake.list_terminals("T-0139").is_err());
    let gone = fake.remove_workspace("T-0120", true).unwrap();
    assert!(gone.task.branch.is_none());
    assert!(gone.branch_deleted);
}

#[test]
fn sends_terminal_rows_only_for_tasks_that_have_them() {
    let fake = FakeDaemon::default();
    let (mut receiver, sender) = fake.watch();
    drain(&mut receiver);
    let ids: BTreeSet<String> = ["T-0146".into(), "T-0131".into()].into();
    fake.send_terminals(&sender, &ids);
    let rows = drain(&mut receiver);
    assert_eq!(rows.len(), 1);
    fake.close_terminal("T-0146", "2").unwrap();
    let Some(DashboardEvent::Terminals { terminals, .. }) = drain(&mut receiver).pop() else {
        panic!("no terminal rows");
    };
    assert_eq!(terminals.terminals, ["1", "10", "12"]);
}

#[test]
fn makes_a_workspace_for_a_task_that_has_none() {
    let fake = FakeDaemon::default();
    let (mut receiver, _) = fake.watch();
    drain(&mut receiver);
    let task = fake.create_workspace("T-0135", Some("dev")).unwrap();
    assert_eq!(
        task.workspace.as_deref(),
        Some("/work/.natamux/worktrees/T-0135")
    );
    assert_eq!(task.branch.as_deref(), Some("natamux/t-0135"));
    let events = drain(&mut receiver);
    assert!(tasks_of(&events).is_some());
    assert!(events.iter().any(|event| matches!(
        event,
        DashboardEvent::WorkspaceStep(message) if message.step.is_none()
    )));
    assert!(fake.create_workspace("T-9999", None).is_err());
}

#[test]
fn a_push_leaves_nothing_to_push_and_a_pull_request_gets_its_link() {
    let fake = FakeDaemon::default();
    fake.push("T-0146").unwrap();
    let pushed = fake.graph("T-0146").unwrap();
    assert_eq!(
        pushed.push,
        Some(PushStatus::Tracked {
            unpushed: 0,
            unpulled: 0
        })
    );
    assert!(fake.push("T-0131").is_err());
    let task = fake.open_pull_request("T-0146").unwrap();
    assert_eq!(
        task.pr.as_deref(),
        Some("https://github.com/lenhatthanh20/natamux/pull/146")
    );
}

#[test]
fn a_reopened_agent_comes_back_restoring_in_terminal_1() {
    let fake = FakeDaemon::default();
    fake.reopen_agent("T-0139").unwrap();
    let list = fake.list_terminals("T-0139").unwrap();
    assert_eq!(list.agents.first().map(String::as_str), Some("1"));
    assert_eq!(list.restoring["1"].as_deref(), Some("claude-code"));
    assert_eq!(fake.ended_shells("T-0139").unwrap().len(), 5);
    assert!(fake.ended_shells("T-0146").unwrap().is_empty());
}

#[test]
fn each_terminal_opens_with_its_own_screen() {
    let fake = FakeDaemon::default();
    let text = |task: &str, terminal: &str| {
        let mut events = fake.terminal(task, terminal);
        let mut bytes = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let TerminalEvent::Output { bytes: more, .. } = event {
                bytes.extend(more);
            }
        }
        String::from_utf8(bytes).unwrap()
    };
    assert!(text("T-0141", "1").contains("Show the cost in USD"));
    assert_eq!(text("T-0143", "1").matches("src").count(), 5);
    assert_ne!(text("T-0146", "1"), text("T-0146", "12"));
}

#[test]
fn removes_a_project_and_creates_from_a_template() {
    let fake = FakeDaemon::default();
    fake.remove_repo("natamux-site").unwrap();
    assert!(fake.remove_repo("natamux-site").is_err());
    let task = fake
        .create_from_template(
            "bug",
            TemplateTaskInput {
                title: "Fix: the toast".into(),
                repo: None,
            },
        )
        .unwrap();
    assert_eq!(task.labels, ["bug"]);
    assert_eq!(task.priority, "high");
}

#[test]
fn a_dropped_feed_is_forgotten() {
    let fake = FakeDaemon::default();
    let (receiver, sender) = fake.watch();
    drop(receiver);
    drop(sender);
    fake.rename_task("T-0131", "x").unwrap();
    assert!(fake.data().feeds.is_empty());
}

#[test]
fn a_stalled_feed_sends_nothing_and_a_down_one_only_its_link() {
    let stalled = FakeDaemon::default();
    stalled.set_up(FakeLink::Stalled, true, true);
    let (mut events, _feed) = stalled.watch();
    assert!(drain(&mut events).is_empty());

    let down = FakeDaemon::default();
    down.set_up(FakeLink::Down, true, true);
    let (mut events, _feed) = down.watch();
    let events = drain(&mut events);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        DashboardEvent::Status(LinkStatus::Reconnecting(_))
    ));
}

#[test]
fn set_up_can_leave_out_the_tasks_and_the_projects() {
    let fake = FakeDaemon::default();
    fake.set_up(FakeLink::Live, false, false);
    let (mut events, _feed) = fake.watch();
    let events = drain(&mut events);
    assert_eq!(tasks_of(&events).map(Vec::len), Some(0));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DashboardEvent::Repos { repos, .. } if repos.is_empty()))
    );
}
