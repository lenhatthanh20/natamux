//! The live task list and plain HTTP calls against the fake daemon.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use futures::StreamExt;
use natamux_client::{Client, Config, LinkStatus, TaskListEvent};
use support::{FakeDaemon, runtime, tasks_json, within};

#[test]
fn the_task_list_follows_the_event_socket() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut events = client.watch_tasks();
        let mut conn = daemon.next_events().await;
        assert_eq!(conn.target, "/events");
        let mut ids = Vec::new();
        while ids.is_empty() {
            match within(events.next()).await.unwrap() {
                TaskListEvent::Tasks(tasks) => {
                    ids = tasks.into_iter().map(|task| task.id).collect();
                }
                TaskListEvent::Status(LinkStatus::Connecting | LinkStatus::Live) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(ids, ["T-1"]);

        daemon.state(|state| state.tasks = tasks_json(&["T-1", "T-2"]));
        conn.text(r#"{"kind":"shells.changed","taskId":"T-1"}"#)
            .await;
        conn.text(r#"{"kind":"task.created","task":{}}"#).await;
        let TaskListEvent::Tasks(tasks) = within(events.next()).await.unwrap() else {
            panic!("no new list");
        };
        assert_eq!(tasks.len(), 2);
        assert_eq!(daemon.state(|state| state.task_fetches), 2);
    });
}

#[test]
fn a_wrong_token_is_refused_for_good() {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    let client = Client::new(Config {
        base_url: format!("http://{}", daemon.address),
        token: "wrong".into(),
    })
    .unwrap();
    rt.block_on(async {
        let mut events = client.watch_tasks();
        assert_eq!(
            within(events.next()).await,
            Some(TaskListEvent::Status(LinkStatus::Connecting))
        );
        assert_eq!(
            within(events.next()).await,
            Some(TaskListEvent::Status(LinkStatus::Refused))
        );
        assert_eq!(within(events.next()).await, None);
    });
}

#[test]
fn closes_a_terminal_over_http() {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        within(client.close_terminal("T-1", "7")).await.unwrap();
        let tasks = within(client.list_tasks()).await.unwrap();
        assert_eq!(tasks[0].id, "T-1");
    });
    assert_eq!(
        daemon.state(|state| state.closed_terminals.clone()),
        ["/tasks/T-1/terminals/7"]
    );
}
