//! The live dashboard data against the fake daemon: which part each message reads again, one read
//! per burst, the diff stats limit, the fallback polls, and reading everything again.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::{Duration, Instant};

use futures::StreamExt;
use natamux_client::{DashboardEvent, DashboardFeed, FeedTimes, LinkStatus};
use natamux_proto::workspaces::workspace::PreparingStep;
use support::{FakeDaemon, runtime, within};

const BASE_READS: [&str; 7] = [
    "GET /tasks",
    "GET /repos",
    "GET /workspaces/preparing",
    "GET /jobs",
    "GET /activity",
    "GET /usage",
    "GET /diffstats",
];

/// The route whose answer an event carries. Statuses and workspace steps read nothing.
fn route_of(event: &DashboardEvent) -> Option<String> {
    let route = match event {
        DashboardEvent::Tasks(_) => "GET /tasks",
        DashboardEvent::Repos { .. } => "GET /repos",
        DashboardEvent::Preparing(_) => "GET /workspaces/preparing",
        DashboardEvent::Jobs(_) => "GET /jobs",
        DashboardEvent::Activity(_) => "GET /activity",
        DashboardEvent::Usage(_) => "GET /usage",
        DashboardEvent::DiffStats(_) => "GET /diffstats",
        DashboardEvent::Terminals { task_id, .. } => {
            return Some(format!("GET /tasks/{task_id}/terminals"));
        }
        DashboardEvent::WorkspaceStep(_)
        | DashboardEvent::Status(_)
        | DashboardEvent::WorkspaceChanged { .. }
        | DashboardEvent::ShellsChanged { .. }
        | DashboardEvent::TaskLogged { .. }
        | DashboardEvent::ReviewCommentsChanged { .. } => return None,
    };
    Some(route.to_string())
}

/// The next event that is not a status or a task id passed on; a reconnect fails the test.
async fn next_data(feed: &mut DashboardFeed) -> DashboardEvent {
    loop {
        match next_any(feed).await {
            DashboardEvent::WorkspaceChanged { .. }
            | DashboardEvent::ShellsChanged { .. }
            | DashboardEvent::TaskLogged { .. } => {}
            event => return event,
        }
    }
}

/// The next event that is not a status, task ids passed on included.
async fn next_any(feed: &mut DashboardFeed) -> DashboardEvent {
    loop {
        match within(feed.events.next()).await.unwrap() {
            DashboardEvent::Status(LinkStatus::Connecting | LinkStatus::Live) => {}
            DashboardEvent::Status(status) => panic!("unexpected {status:?}"),
            event => return event,
        }
    }
}

/// The reads of a connect, in the feed's order: the task list first.
async fn base_reads(feed: &mut DashboardFeed) {
    for route in BASE_READS {
        assert_eq!(route_of(&next_data(feed).await).as_deref(), Some(route));
    }
}

fn quick(diff_stats_every: Duration) -> FeedTimes {
    FeedTimes {
        diff_stats_every,
        ..FeedTimes::default()
    }
}

#[test]
fn each_kind_reads_only_its_own_part() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        let cases = [
            (
                r#"{"kind":"task.updated","task":{"id":"T-1"}}"#,
                "GET /tasks",
            ),
            (r#"{"kind":"task.deleted","taskId":"T-1"}"#, "GET /tasks"),
            (r#"{"kind":"repos.changed"}"#, "GET /repos"),
            (r#"{"kind":"jobs.changed","jobs":[]}"#, "GET /jobs"),
            (
                r#"{"kind":"activity.changed","taskId":"T-1","activity":{}}"#,
                "GET /activity",
            ),
            (
                r#"{"kind":"usage.changed","taskId":"T-1","usage":{}}"#,
                "GET /usage",
            ),
            (
                r#"{"kind":"workspace.dirty","taskId":"T-1"}"#,
                "GET /diffstats",
            ),
        ];
        for (message, route) in cases {
            let before = daemon.state(|state| state.requests.len());
            conn.text(message).await;
            let event = next_data(&mut feed).await;
            assert_eq!(route_of(&event).as_deref(), Some(route), "{message}");
            let reads: Vec<String> = daemon.state(|state| {
                state.requests[before..]
                    .iter()
                    .map(|(route, _)| route.clone())
                    .collect()
            });
            assert_eq!(reads, [route], "{message}");
        }
    });
}

#[test]
fn a_workspace_step_comes_from_the_message_without_a_read() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        conn.text(r#"{"kind":"workspace.preparing","taskId":"T-1","step":"fetch"}"#)
            .await;
        let DashboardEvent::WorkspaceStep(step) = next_data(&mut feed).await else {
            panic!("no step");
        };
        assert_eq!(step.step, Some(PreparingStep::Fetch));
        conn.text(
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":null,"error":"no space"}"#,
        )
        .await;
        let DashboardEvent::WorkspaceStep(end) = next_data(&mut feed).await else {
            panic!("no end");
        };
        assert!(end.failed());
        assert_eq!(daemon.state(|state| state.requests.len()), BASE_READS.len());
    });
}

#[test]
fn terminal_rows_follow_the_tasks_the_app_shows() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| {
        state.answer(
            "GET /tasks/T-1/terminals",
            "200 OK",
            r#"{"terminals":["1"],"agents":["1"],"lanes":{},"programs":{},"restoring":{}}"#,
        )
    });
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        feed.terminals.show(["T-1".to_string()]);
        let DashboardEvent::Terminals { task_id, terminals } = next_data(&mut feed).await else {
            panic!("no terminals");
        };
        assert_eq!(
            (task_id.as_str(), terminals.terminals),
            ("T-1", vec!["1".into()])
        );

        conn.text(r#"{"kind":"shells.changed","taskId":"T-2"}"#)
            .await;
        conn.text(r#"{"kind":"shells.changed","taskId":"T-1"}"#)
            .await;
        let event = next_data(&mut feed).await;
        assert_eq!(
            route_of(&event).as_deref(),
            Some("GET /tasks/T-1/terminals")
        );
        daemon.state(|state| {
            assert_eq!(state.count("GET /tasks/T-1/terminals"), 2);
            assert_eq!(state.count("GET /tasks/T-2/terminals"), 0);
        });
    });
}

#[test]
fn shell_and_workspace_changes_pass_their_task_id_on_and_still_read() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer("GET /tasks/T-1/terminals", "200 OK", r#"{"terminals":[]}"#));
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        feed.terminals.show(["T-1".to_string()]);
        assert!(matches!(
            next_any(&mut feed).await,
            DashboardEvent::Terminals { .. }
        ));

        // T-2 is not shown: its id still comes, and no terminals read happens.
        conn.text(r#"{"kind":"shells.changed","taskId":"T-2"}"#)
            .await;
        assert_eq!(
            next_any(&mut feed).await,
            DashboardEvent::ShellsChanged {
                task_id: "T-2".into()
            }
        );
        conn.text(r#"{"kind":"shells.changed","taskId":"T-1"}"#)
            .await;
        assert_eq!(
            next_any(&mut feed).await,
            DashboardEvent::ShellsChanged {
                task_id: "T-1".into()
            }
        );
        assert!(matches!(
            next_any(&mut feed).await,
            DashboardEvent::Terminals { .. }
        ));

        conn.text(r#"{"kind":"workspace.dirty","taskId":"T-3"}"#)
            .await;
        assert_eq!(
            next_any(&mut feed).await,
            DashboardEvent::WorkspaceChanged {
                task_id: "T-3".into()
            }
        );
        assert!(matches!(
            next_any(&mut feed).await,
            DashboardEvent::DiffStats(_)
        ));
        daemon.state(|state| {
            assert_eq!(state.count("GET /tasks/T-1/terminals"), 2);
            assert_eq!(state.count("GET /tasks/T-2/terminals"), 0);
            assert_eq!(state.count("GET /diffstats"), 2);
        });
    });
}

#[test]
fn a_burst_of_changes_is_one_read_per_part() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        // Each read takes a while, so the rest of the burst comes while the first one runs.
        daemon.state(|state| state.delay = Duration::from_millis(300));
        conn.text(r#"{"kind":"task.changed","taskId":"T-1"}"#).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        for _ in 0..5 {
            conn.text(r#"{"kind":"task.changed","taskId":"T-1"}"#).await;
            conn.text(r#"{"kind":"repos.changed"}"#).await;
        }
        let mut routes = Vec::new();
        for _ in 0..3 {
            routes.push(route_of(&next_data(&mut feed).await).unwrap());
        }
        assert_eq!(routes, ["GET /tasks", "GET /tasks", "GET /repos"]);
        // Nothing more comes.
        let more = tokio::time::timeout(Duration::from_millis(800), next_data(&mut feed)).await;
        assert!(more.is_err(), "one more read: {more:?}");
        daemon.state(|state| {
            assert_eq!(state.count("GET /tasks"), 3);
            assert_eq!(state.count("GET /repos"), 2);
        });
    });
}

#[test]
fn workspace_changes_read_the_diff_stats_at_most_once_per_limit() {
    let every = Duration::from_millis(600);
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard_every(quick(every));
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        let dirty = r#"{"kind":"workspace.dirty","taskId":"T-1"}"#;
        // The read of the connect starts no wait: the first change reads at once.
        let start = Instant::now();
        conn.text(dirty).await;
        assert!(matches!(
            next_data(&mut feed).await,
            DashboardEvent::DiffStats(_)
        ));
        assert!(start.elapsed() < Duration::from_millis(400));

        let first = Instant::now();
        for _ in 0..3 {
            conn.text(dirty).await;
        }
        assert!(matches!(
            next_data(&mut feed).await,
            DashboardEvent::DiffStats(_)
        ));
        assert!(
            first.elapsed() >= Duration::from_millis(450),
            "read after {:?}",
            first.elapsed()
        );
        let more = tokio::time::timeout(every + Duration::from_millis(300), next_data(&mut feed));
        assert!(more.await.is_err());
        assert_eq!(daemon.state(|state| state.count("GET /diffstats")), 3);
    });
}

#[test]
fn resync_reads_everything_again() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer("GET /tasks/T-1/terminals", "200 OK", r#"{"terminals":[]}"#));
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        feed.terminals.show(["T-1".to_string()]);
        let mut conn = daemon.next_events().await;
        let mut seen = Vec::new();
        while seen.len() < BASE_READS.len() + 1 {
            seen.push(route_of(&next_data(&mut feed).await).unwrap());
        }
        conn.text(r#"{"kind":"resync"}"#).await;
        base_reads(&mut feed).await;
        let event = next_data(&mut feed).await;
        assert_eq!(
            route_of(&event).as_deref(),
            Some("GET /tasks/T-1/terminals")
        );
        daemon.state(|state| {
            for route in BASE_READS {
                assert_eq!(state.count(route), 2, "{route}");
            }
        });
    });
}

#[test]
fn a_reconnect_reads_everything_again() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        base_reads(&mut feed).await;
        conn.close(1001).await;
        let lost = within(feed.events.next()).await.unwrap();
        assert!(
            matches!(lost, DashboardEvent::Status(LinkStatus::Reconnecting(_))),
            "{lost:?}"
        );
        let _again = daemon.next_events().await;
        base_reads(&mut feed).await;
        daemon.state(|state| {
            for route in BASE_READS {
                assert_eq!(state.count(route), 2, "{route}");
            }
        });
    });
}

#[test]
fn fallback_polls_read_what_the_socket_may_have_missed() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer("GET /tasks/T-1/terminals", "200 OK", r#"{"terminals":[]}"#));
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard_every(FeedTimes {
            fallback_every: Duration::from_millis(400),
            terminals_every: Duration::from_millis(700),
            ..FeedTimes::default()
        });
        feed.terminals.show(["T-1".to_string()]);
        let _conn = daemon.next_events().await;
        let mut routes = Vec::new();
        while routes.len() < BASE_READS.len() + 1 {
            routes.push(route_of(&next_data(&mut feed).await).unwrap());
        }
        // The first poll, then the terminal rows.
        let mut polled = Vec::new();
        for _ in 0..5 {
            polled.push(route_of(&next_data(&mut feed).await).unwrap());
        }
        assert_eq!(
            polled,
            [
                "GET /jobs",
                "GET /activity",
                "GET /usage",
                "GET /diffstats",
                "GET /tasks/T-1/terminals"
            ]
        );
        daemon.state(|state| {
            assert_eq!(state.count("GET /tasks"), 1);
            assert_eq!(state.count("GET /repos"), 1);
        });
    });
}

#[test]
fn a_part_the_daemon_cannot_give_does_not_stop_the_rest() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| {
        state.answer(
            "GET /usage",
            "404 Not Found",
            r#"{"message":"Route not found"}"#,
        )
    });
    let client = daemon.client();
    rt.block_on(async {
        let mut feed = client.watch_dashboard();
        let mut conn = daemon.next_events().await;
        let mut routes = Vec::new();
        while routes.len() < BASE_READS.len() - 1 {
            routes.push(route_of(&next_data(&mut feed).await).unwrap());
        }
        assert!(!routes.iter().any(|route| route == "GET /usage"));
        conn.text(r#"{"kind":"task.changed","taskId":"T-1"}"#).await;
        assert!(matches!(
            next_data(&mut feed).await,
            DashboardEvent::Tasks(_)
        ));
    });
}
