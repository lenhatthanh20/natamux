use super::*;

fn shown(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn shells_of_a_task_not_shown_are_not_read() {
    let mut stale = Stale::default();
    let shown = shown(&["T-1"]);
    stale.note(r#"{"kind":"shells.changed","taskId":"T-2"}"#, &shown);
    assert!(stale.terminals.is_empty());
    stale.note(r#"{"kind":"shells.changed","taskId":"T-1"}"#, &shown);
    assert_eq!(stale.terminals, shown);
}

#[test]
fn shells_and_workspace_changes_pass_every_task_id_on() {
    let mut stale = Stale::default();
    stale.note(
        r#"{"kind":"shells.changed","taskId":"T-2"}"#,
        &shown(&["T-1"]),
    );
    stale.note(r#"{"kind":"workspace.dirty","taskId":"T-3"}"#, &shown(&[]));
    stale.note(
        r#"{"kind":"review-comments.changed","taskId":"T-4"}"#,
        &shown(&[]),
    );
    assert_eq!(
        stale.passed,
        [
            DashboardEvent::ShellsChanged {
                task_id: "T-2".into()
            },
            DashboardEvent::WorkspaceChanged {
                task_id: "T-3".into()
            },
            DashboardEvent::ReviewCommentsChanged {
                task_id: "T-4".into()
            },
        ]
    );
}

#[test]
fn a_workspace_step_is_passed_on_and_marks_nothing() {
    let mut stale = Stale::default();
    let step = stale.note(
        r#"{"kind":"workspace.preparing","taskId":"T-1","step":"fetch"}"#,
        &BTreeSet::new(),
    );
    assert_eq!(step.map(|step| step.task_id).as_deref(), Some("T-1"));
    assert!(!stale.preparing && !stale.tasks);
}

#[test]
fn resync_marks_everything_but_keeps_the_diff_stats_time() {
    let at = Instant::now();
    let mut stale = Stale {
        dirty_read_at: Some(at),
        ..Stale::default()
    };
    stale.note(r#"{"kind":"resync"}"#, &shown(&["T-1"]));
    assert!(stale.tasks && stale.repos && stale.preparing && stale.diff_stats);
    assert!(stale.jobs && stale.activity && stale.usage);
    assert_eq!(stale.terminals, shown(&["T-1"]));
    assert_eq!(stale.dirty_read_at, Some(at));
}

#[test]
fn diff_stats_wait_only_after_a_read_for_a_change() {
    let every = Duration::from_secs(15);
    let mut stale = Stale::default();
    stale.note(
        r#"{"kind":"workspace.dirty","taskId":"T-1"}"#,
        &BTreeSet::new(),
    );
    assert_eq!(stale.due(every), None);
    let at = Instant::now();
    stale.dirty_read_at = Some(at);
    assert_eq!(stale.due(every), Some(at + every));
}

#[test]
fn due_is_the_retry_time_while_a_diff_stats_retry_waits() {
    let every = Duration::ZERO;
    let retry_at = Instant::now() + Duration::from_secs(5);
    let mut stale = Stale {
        dirty: true,
        dirty_read_at: Some(Instant::now()),
        ..Stale::default()
    };
    stale.retries.insert(
        "diff_stats".into(),
        Retry {
            tries: 1,
            not_before: retry_at,
        },
    );
    assert_eq!(stale.due(every), Some(retry_at));
}

/// A daemon address with nothing listening: every read fails to connect at once.
fn closed_config() -> Config {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map_or(1, |address| address.port());
    Config {
        base_url: format!("http://127.0.0.1:{port}"),
        token: "t".into(),
    }
}

#[allow(clippy::panic)]
fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(future),
        Err(error) => panic!("no runtime: {error}"),
    }
}

fn only_jobs() -> Stale {
    Stale {
        jobs: true,
        ..Stale::default()
    }
}

#[test]
fn a_read_that_gets_no_answer_keeps_its_part_stale_and_waits_to_read_it_again() {
    let config = closed_config();
    let (events, _receiver) = futures::channel::mpsc::unbounded();
    let mut stale = only_jobs();
    let mut live = true;
    let read = block_on(stale.read(&config, Duration::from_secs(15), &mut live, &events));
    assert!(read.is_ok(), "the link is not lost");
    assert!(stale.jobs);
    let due = stale.due(Duration::from_secs(15));
    assert!(due.is_some_and(|at| at > Instant::now()));
    // Not read again before its time: still one failed try.
    let tries = |stale: &Stale| stale.retries.get("jobs").map(|retry| retry.tries);
    assert_eq!(tries(&stale), Some(1));
    assert!(block_on(stale.read(&config, Duration::from_secs(15), &mut live, &events)).is_ok());
    assert_eq!(tries(&stale), Some(1));
    // Once its time has come, it is read again.
    stale.retries.insert(
        "jobs".into(),
        Retry {
            tries: 1,
            not_before: Instant::now(),
        },
    );
    assert!(block_on(stale.read(&config, Duration::from_secs(15), &mut live, &events)).is_ok());
    assert_eq!(tries(&stale), Some(2));
}

#[test]
fn three_failed_task_list_reads_in_a_row_lose_the_link() {
    let config = closed_config();
    let (events, _receiver) = futures::channel::mpsc::unbounded();
    let mut live = true;
    let mut stale = Stale::default();
    for round in 1..=3 {
        stale.tasks = true;
        stale.retries.clear();
        let read = block_on(stale.read(&config, Duration::from_secs(15), &mut live, &events));
        match (round, read) {
            (1 | 2, Ok(())) => assert!(stale.tasks, "read again later"),
            (3, Err(Halt::Lost(ClientError::Unreachable { reason, .. }))) => {
                assert!(reason.starts_with("the task list could not be read 3 times: "));
            }
            _ => panic!("round {round}"),
        }
    }
}

#[test]
fn a_refused_token_stops_the_feed() {
    let (events, _receiver) = futures::channel::mpsc::unbounded();
    let mut stale = Stale::default();
    let read: Result<(), ClientError> = Err(ClientError::Unauthorized);
    let halt = stale.pass("jobs", read, &events, |()| DashboardEvent::Jobs(Vec::new()));
    assert!(matches!(halt, Err(Halt::Refused)));
}

#[test]
fn an_answer_that_is_not_2xx_leaves_the_part_out_without_a_retry() {
    let (events, _receiver) = futures::channel::mpsc::unbounded();
    let mut stale = Stale::default();
    let read: Result<(), ClientError> = Err(ClientError::Status {
        status: 404,
        message: "gone".into(),
    });
    let again = stale.pass("jobs", read, &events, |()| DashboardEvent::Jobs(Vec::new()));
    assert!(matches!(again, Ok(false)));
    assert!(stale.retries.is_empty());
}
