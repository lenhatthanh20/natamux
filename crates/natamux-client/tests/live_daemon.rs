//! The client against a real Natamux daemon. Ignored by default; the `live-daemon` job of
//! .github/workflows/rust.yml starts the daemon image and runs these with `--ignored`.
//!
//! Env: NATAMUX_URL, NATAMUX_TOKEN, and NATAMUX_LIVE_TASK, a task with a workspace. The screen 1
//! flows of screen 1, 2 and 3 make and delete their own tasks, and clean up when a step fails.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use natamux_client::{
    Client, Config, DashboardEvent, DashboardFeed, LinkStatus, SessionOptions, TaskListEvent,
    TerminalEvent, TerminalHandle, TerminalStatus,
};
use natamux_proto::stages::actions::{StageAction, StageActionInput};
use natamux_proto::tasks::task::{NewTaskInput, TaskChanges};
use natamux_proto::tasks::template::TemplateInput;
use natamux_proto::workspaces::workspace::{FanOutInput, NewWorkspaceInput};

mod live;

use live::{Made, finish, runtime};

const WAIT: Duration = Duration::from_secs(20);
// Not Terminal 1, where the task's agent may run.
const TERMINAL: &str = "7";

fn client() -> (Client, String) {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let task = std::env::var("NATAMUX_LIVE_TASK").expect("NATAMUX_LIVE_TASK");
    (Client::new(config).unwrap(), task)
}

/// What one socket of a terminal showed: the bytes, and the catch-up bytes apart.
#[derive(Default)]
struct Screen {
    live: Vec<u8>,
    caught_up: Vec<u8>,
    offset: u64,
}

impl Screen {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.live).into_owned()
    }
}

/// Reads events, acting as the app does, until `done` holds or WAIT passes.
async fn read_until(
    handle: &mut TerminalHandle,
    screen: &mut Screen,
    done: impl Fn(&Screen) -> bool,
) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !done(screen) {
        let event = tokio::time::timeout_at(deadline, handle.events.next())
            .await
            .expect("timed out")
            .expect("the session ended");
        match event {
            TerminalEvent::Output {
                bytes,
                end: Some(end),
            } => {
                screen.live.extend(bytes);
                screen.offset = end;
                handle.parsed(end);
            }
            TerminalEvent::Output { bytes, end: None } => screen.caught_up.extend(bytes),
            TerminalEvent::CaughtUp { offset } => {
                screen.offset = offset;
                handle.caught_up(offset);
            }
            TerminalEvent::Status(TerminalStatus::Failed(reason)) => panic!("failed: {reason}"),
            TerminalEvent::Status(TerminalStatus::Ended { code }) => panic!("ended: {code:?}"),
            TerminalEvent::Status(_) | TerminalEvent::Reset { .. } => {}
        }
    }
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN, NATAMUX_LIVE_TASK"]
fn types_into_a_shell_and_resumes_without_repeating_bytes() {
    let (client, task) = client();
    runtime().block_on(async {
        let mut handle = client.open_terminal(SessionOptions::new(&task, TERMINAL, 100, 30, true));
        let mut screen = Screen::default();
        // The shell's first prompt, then the echo of a command and its output.
        read_until(&mut handle, &mut screen, |screen| !screen.live.is_empty()).await;
        handle.input("echo natamux-$((40+2))\r");
        read_until(&mut handle, &mut screen, |screen| {
            screen.text().contains("natamux-42")
        })
        .await;

        // A lost socket resumes from its offset: nothing it already had comes again.
        let before = screen.offset;
        handle.reconnect();
        let mut after = Screen::default();
        read_until(&mut handle, &mut after, |screen| screen.offset >= before).await;
        assert!(
            !String::from_utf8_lossy(&after.caught_up).contains("natamux-42"),
            "the resume repeated old output"
        );
        handle.input("echo natamux-$((50+5))\r");
        read_until(&mut handle, &mut after, |screen| {
            screen.text().contains("natamux-55")
        })
        .await;
        assert!(!after.text().contains("natamux-42"));
    });
    runtime().block_on(async {
        client.close_terminal(&task, TERMINAL).await.unwrap();
    });
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN, NATAMUX_LIVE_TASK"]
fn lists_tasks_live() {
    let (client, task) = client();
    runtime().block_on(async {
        let mut events = client.watch_tasks();
        loop {
            match tokio::time::timeout(WAIT, events.next()).await.unwrap() {
                Some(TaskListEvent::Tasks(tasks)) => {
                    assert!(tasks.iter().any(|listed| listed.id == task));
                    break;
                }
                Some(TaskListEvent::Status(LinkStatus::Connecting | LinkStatus::Live)) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    });
}

/// The next event `pick` takes, within `wait`. A lost or refused link fails the test.
async fn next_event<T>(
    feed: &mut DashboardFeed,
    wait: Duration,
    pick: impl Fn(DashboardEvent) -> Option<T>,
) -> T {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let event = tokio::time::timeout_at(deadline, feed.events.next())
            .await
            .expect("timed out")
            .expect("the feed ended");
        if let DashboardEvent::Status(
            status @ (LinkStatus::Refused | LinkStatus::Reconnecting(_)),
        ) = &event
        {
            panic!("the link failed: {status:?}");
        }
        if let Some(picked) = pick(event) {
            return picked;
        }
    }
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_1() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let repos = client.list_repos().await.unwrap();
        let repo = repos.repos.first().map(|repo| repo.id.clone());
        client.list_templates().await.unwrap();
        client.list_jobs().await.unwrap();
        client.activity().await.unwrap();
        client.usage().await.unwrap();
        client.diff_stats().await.unwrap();
        client.list_preparing().await.unwrap();
        client.list_branches(repo.as_deref()).await.unwrap();
        if let Some(repo) = &repo {
            client.load_settings(repo).await.unwrap();
            client.list_skills(repo).await.unwrap();
        }

        let task = client
            .create_task(NewTaskInput {
                title: "Rust screen 1 test".into(),
                parent_id: None,
                repo: repo.clone(),
            })
            .await
            .unwrap();
        let renamed = client
            .rename_task(&task.id, "Rust screen 1 test, renamed")
            .await
            .unwrap();
        assert_eq!(renamed.title, "Rust screen 1 test, renamed");
        let subtask = client
            .create_task(NewTaskInput {
                title: "Rust screen 1 subtask".into(),
                parent_id: Some(task.id.clone()),
                repo: repo.clone(),
            })
            .await
            .unwrap();
        assert_eq!(subtask.parent_id.as_deref(), Some(task.id.as_str()));
        client.list_events(&task.id).await.unwrap();

        // Subscribed before the workspace starts, so its end is not missed.
        let mut feed = client.watch_dashboard();
        next_event(&mut feed, WAIT, |event| {
            matches!(event, DashboardEvent::Tasks(_)).then_some(())
        })
        .await;
        let made = client
            .new_workspace(NewWorkspaceInput {
                title: "Rust screen 1 workspace".into(),
                repo: repo.clone(),
                ..NewWorkspaceInput::default()
            })
            .await
            .unwrap();
        let end = next_event(&mut feed, Duration::from_secs(60), |event| match event {
            DashboardEvent::WorkspaceStep(step)
                if step.task_id == made.id && step.step.is_none() =>
            {
                Some(step)
            }
            _ => None,
        })
        .await;
        assert!(!end.failed(), "the workspace failed: {:?}", end.error);
        client.list_terminals(&made.id).await.unwrap();
        client.list_changes(&made.id, false).await.unwrap();
        client.graph(&made.id, false).await.unwrap();

        let removed = client.remove_workspace(&made.id, true, false).await.unwrap();
        assert_eq!(removed.task.workspace, None);
        for id in [&made.id, &subtask.id, &task.id] {
            let deleted = client.delete_task(id, false).await.unwrap();
            assert_eq!(&deleted.id, id);
        }
        let tasks = client.list_tasks().await.unwrap();
        assert!(!tasks.iter().any(|listed| listed.id == task.id));
    });
}

/// Reads the terminal list of a task until `done` holds or WAIT passes.
async fn wait_for_terminals(client: &Client, task: &str, done: impl Fn(&[String]) -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let list = client.list_terminals(task).await.unwrap();
        if done(&list.terminals) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out, terminals: {:?}",
            list.terminals
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_2() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let made = Made::default();
        let result = AssertUnwindSafe(screen_2(&client, &made))
            .catch_unwind()
            .await;
        finish(&client, &made, result).await;
    });
}

async fn screen_2(client: &Client, made: &Made) {
    let repos = client.list_repos().await.unwrap();
    let repo = repos.repos.first().map(|repo| repo.id.clone());
    let task = client
        .create_task(NewTaskInput {
            title: "Rust screen 2 test".into(),
            parent_id: None,
            repo,
        })
        .await
        .unwrap();
    made.add(&task.id);
    let made_workspace = client.create_workspace(&task.id, None).await.unwrap();
    assert!(made_workspace.workspace.is_some());

    // Terminal 2 exists as long as a socket to it is open.
    let _handle = client.open_terminal(SessionOptions::new(&task.id, "2", 100, 30, true));
    wait_for_terminals(client, &task.id, |names| names.iter().any(|n| n == "2")).await;
    client.close_terminal(&task.id, "2").await.unwrap();
    wait_for_terminals(client, &task.id, |names| !names.iter().any(|n| n == "2")).await;

    client.ended_shells(&task.id).await.unwrap();

    // The test repo has no origin, so the push is refused with the daemon's message.
    let refused = client.push(&task.id).await.unwrap_err();
    assert!(
        matches!(refused, natamux_client::ClientError::Status { .. }),
        "{refused:?}"
    );

    client.remove_workspace(&task.id, true, false).await.unwrap();
    let deleted = client.delete_task(&task.id, false).await.unwrap();
    assert_eq!(deleted.id, task.id);
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_3() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let made = Made::default();
        let result = AssertUnwindSafe(screen_3(&client, &made))
            .catch_unwind()
            .await;
        finish(&client, &made, result).await;
    });
}

async fn screen_3(client: &Client, made: &Made) {
    let repos = client.list_repos().await.unwrap();
    let repo = repos.repos.first().map(|repo| repo.id.clone());
    let task = client
        .create_task(NewTaskInput {
            title: "Rust screen 3 test".into(),
            parent_id: None,
            repo: repo.clone(),
        })
        .await
        .unwrap();
    made.add(&task.id);
    client.create_workspace(&task.id, None).await.unwrap();

    // The daemon fills the default pipeline when the project has none of its own.
    let repo = repo.expect("the daemon has a project");
    let settings = client.load_settings(&repo).await.unwrap().settings;
    assert!(!settings.stages.is_empty());
    let stages: Vec<String> = settings
        .stages
        .iter()
        .flat_map(|step| step.stages().into_iter().map(|stage| stage.id.clone()))
        .collect();
    let (first, last) = (stages[0].clone(), stages[stages.len() - 1].clone());
    client.list_stage_files(&task.id).await.unwrap();

    let changes = TaskChanges {
        description: Some("Written by the screen 3 test".into()),
        priority: Some("high".into()),
        ..TaskChanges::default()
    };
    let edited = client.update_task(&task.id, changes).await.unwrap();
    assert_eq!(edited.description, "Written by the screen 3 test");
    assert_eq!(edited.priority, "high");

    // A mark on a stage that is not reached yet, then cleared.
    let marked = client.set_skip(&task.id, &last, true).await.unwrap();
    assert!(marked.skip_stages.unwrap_or_default().contains(&last));
    let cleared = client.set_skip(&task.id, &last, false).await.unwrap();
    assert!(!cleared.skip_stages.unwrap_or_default().contains(&last));

    // Skip the current stage, then send the task back to the first one.
    let skipped = client
        .stage_action(&task.id, StageActionInput::new(StageAction::Skip))
        .await
        .unwrap();
    assert_ne!(skipped.stage, first, "the skip moved the task on");
    let back = StageActionInput {
        to: Some(first.clone()),
        reason: Some("Sent back by the screen 3 test".into()),
        ..StageActionInput::new(StageAction::Reopen)
    };
    // The daemon takes the send back; where it lands depends on the stages that ran (a skipped
    // context is not run again on a draft task), so only check it is a stage of the pipeline.
    let reopened = client.stage_action(&task.id, back).await.unwrap();
    assert!(stages.contains(&reopened.stage), "{}", reopened.stage);

    let try_id = template_and_try(client, made, &task.id).await;
    client.delete_task(&try_id, false).await.unwrap();
    client.remove_workspace(&task.id, true, false).await.unwrap();
    client.delete_task(&task.id, false).await.unwrap();
}

/// Saves a template, and runs one more try of the task without an agent. Gives the try's id.
async fn template_and_try(client: &Client, made: &Made, task_id: &str) -> String {
    let template = TemplateInput {
        name: "Rust screen 3 test".into(),
        title: "Screen 3: ".into(),
        description: String::new(),
        priority: "normal".into(),
        labels: Vec::new(),
    };
    client.save_template(template).await.unwrap();

    let tries = client
        .fan_out(
            task_id,
            FanOutInput {
                count: 1,
                start: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(tries.len(), 1);
    made.add(&tries[0].id);

    tries[0].id.clone()
}
