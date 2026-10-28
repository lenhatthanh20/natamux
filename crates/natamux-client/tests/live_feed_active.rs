//! The fallback poll waits while no app window is active, and reads once when one is again.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use natamux_client::FeedTimes;
use support::{FakeDaemon, runtime};

fn times() -> FeedTimes {
    FeedTimes {
        fallback_every: Duration::from_millis(300),
        ..FeedTimes::default()
    }
}

async fn pause(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[test]
fn polls_by_default() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let _feed = client.watch_dashboard_every(times());
        let _conn = daemon.next_events().await;
        pause(700).await;
        assert!(daemon.state(|state| state.count("GET /jobs")) >= 2);
    });
}

#[test]
fn no_poll_while_inactive_and_one_when_active_again() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let feed = client.watch_dashboard_every(times());
        let _conn = daemon.next_events().await;
        feed.terminals.set_active(false);
        // The connect read is done; the fallback time passes twice.
        pause(150).await;
        let before = daemon.state(|state| state.count("GET /jobs"));
        pause(800).await;
        assert_eq!(daemon.state(|state| state.count("GET /jobs")), before);
        feed.terminals.set_active(true);
        pause(150).await;
        assert_eq!(daemon.state(|state| state.count("GET /jobs")), before + 1);
        // Active again with no skipped poll reads nothing more.
        feed.terminals.set_active(false);
        feed.terminals.set_active(true);
        pause(100).await;
        assert_eq!(daemon.state(|state| state.count("GET /jobs")), before + 1);
    });
}
