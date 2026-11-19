//! The parts the live feed takes from a message body with no read: one task's activity or usage,
//! and every job.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use futures::StreamExt;
use natamux_client::{DashboardEvent, DashboardFeed, LinkStatus};
use natamux_proto::agents::activity::TaskUsage;
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

#[test]
fn activity_usage_and_jobs_bodies_pass_their_data_on_without_a_read() {
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
        let before = daemon.state(|state| state.requests.len());

        conn.text(r#"{"kind":"activity.changed","taskId":"T-1","activity":{}}"#)
            .await;
        conn.text(r#"{"kind":"usage.changed","taskId":"T-1","usage":{"tokens":3}}"#)
            .await;
        conn.text(r#"{"kind":"jobs.changed","jobs":[]}"#).await;
        // A later message ends the wait: its read is the only read.
        conn.text(r#"{"kind":"repos.changed"}"#).await;
        let seen = until(&mut feed, |event| {
            matches!(event, DashboardEvent::Repos { .. })
        })
        .await;
        let activity = DashboardEvent::ActivityOf {
            task_id: "T-1".into(),
            activity: Default::default(),
        };
        let usage = DashboardEvent::UsageOf {
            task_id: "T-1".into(),
            usage: TaskUsage {
                tokens: 3,
                ..TaskUsage::default()
            },
        };
        let jobs = DashboardEvent::Jobs(Vec::new());
        for event in [activity, usage, jobs] {
            assert!(seen.contains(&event), "{event:?} in {seen:?}");
        }
        let reads: Vec<String> = daemon.state(|state| {
            state.requests[before..]
                .iter()
                .map(|(route, _)| route.clone())
                .collect()
        });
        assert_eq!(reads, ["GET /repos"]);
    });
}
