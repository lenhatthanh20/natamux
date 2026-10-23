//! The task ids the live feed passes on with no read of their own: `task.updated` adds a line to a
//! task's log (`TaskLogged`), `task.changed` moves only its time.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use futures::StreamExt;
use natamux_client::{DashboardEvent, DashboardFeed, LinkStatus};
use support::{FakeDaemon, runtime, within};

/// The events up to and including the first one `stop` accepts, statuses left out.
async fn until(feed: &mut DashboardFeed, stop: fn(&DashboardEvent) -> bool) -> Vec<DashboardEvent> {
    let mut seen = Vec::new();
    loop {
        match within(feed.events.next()).await.unwrap() {
            DashboardEvent::Status(LinkStatus::Connecting | LinkStatus::Live) => {}
            DashboardEvent::Status(status) => panic!("unexpected {status:?}"),
            event => {
                let done = stop(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
        }
    }
}

fn logged(task_id: &str) -> DashboardEvent {
    DashboardEvent::TaskLogged {
        task_id: task_id.into(),
    }
}

#[test]
fn a_task_updated_message_passes_the_task_id_on_and_still_reads_the_list() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        until(&mut feed, |event| {
            matches!(event, DashboardEvent::DiffStats(_))
        })
        .await;
        let before = daemon.state(|state| state.count("GET /tasks"));

        conn.text(r#"{"kind":"task.updated","task":{"id":"T-1"}}"#)
            .await;
        let seen = until(&mut feed, |event| matches!(event, DashboardEvent::Tasks(_))).await;
        assert!(seen.contains(&logged("T-1")), "{seen:?}");
        // The id is passed on at once: the list read is the only read.
        daemon.state(|state| {
            assert_eq!(state.count("GET /tasks"), before + 1);
        });
    });
}

#[test]
fn a_task_changed_message_does_not_pass_a_task_id_on() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        until(&mut feed, |event| {
            matches!(event, DashboardEvent::DiffStats(_))
        })
        .await;

        conn.text(r#"{"kind":"task.changed","task":{"id":"T-1"}}"#)
            .await;
        let seen = until(&mut feed, |event| matches!(event, DashboardEvent::Tasks(_))).await;
        assert!(!seen.contains(&logged("T-1")), "{seen:?}");
        // A later message ends the wait: nothing for T-1 came in between.
        conn.text(r#"{"kind":"repos.changed"}"#).await;
        let seen = until(&mut feed, |event| {
            matches!(event, DashboardEvent::Repos { .. })
        })
        .await;
        assert!(!seen.contains(&logged("T-1")), "{seen:?}");
    });
}
