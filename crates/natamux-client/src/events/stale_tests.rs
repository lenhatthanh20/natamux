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

fn task_at(id: &str, title: &str, updated_at: &str) -> Task {
    Task {
        id: id.into(),
        title: title.into(),
        updated_at: updated_at.into(),
        ..Task::default()
    }
}

/// A task message as the daemon writes it (`DaemonMessage::to_json`).
fn task_message(kind: &str, task: &Task, moved: Option<bool>) -> String {
    let mut message = serde_json::json!({ "kind": kind, "task": task });
    if kind == "task.updated" {
        message["event"] = serde_json::json!({ "kind": "stage.started" });
        message["accepted"] = serde_json::json!(true);
    }
    if let Some(moved) = moved {
        message["moved"] = serde_json::json!(moved);
    }
    message.to_string()
}

#[test]
fn a_task_body_is_passed_on_and_reads_no_list() {
    let mut stale = Stale::default();
    let none = shown(&[]);
    let task = task_at("T-1", "One", "a");
    stale.note(&task_message("task.created", &task, None), &none);
    stale.note(&task_message("task.changed", &task, None), &none);
    stale.note(&task_message("task.updated", &task, Some(false)), &none);
    assert!(!stale.tasks);
    let changed = DashboardEvent::TaskChanged(Box::new(task));
    let logged = DashboardEvent::TaskLogged {
        task_id: "T-1".into(),
    };
    // The task before its log line: the app reads the log of the task it holds then.
    let expected = [changed.clone(), changed.clone(), changed, logged];
    assert_eq!(stale.passed, expected);
}

#[test]
fn a_deleted_moved_or_unreadable_task_reads_the_list() {
    let task = task_at("T-1", "One", "a");
    let messages = [
        r#"{"kind":"task.deleted","taskId":"T-1"}"#.to_string(),
        task_message("task.updated", &task, Some(true)),
        r#"{"kind":"task.changed","task":{"id":"T-1"}}"#.to_string(),
    ];
    for message in messages {
        let mut stale = Stale::default();
        stale.note(&message, &shown(&[]));
        assert!(stale.tasks, "{message}");
        let changed = DashboardEvent::TaskChanged(Box::new(task.clone()));
        assert!(!stale.passed.contains(&changed), "{message}");
    }
}

#[test]
fn the_fallback_poll_reads_the_task_list() {
    let mut stale = Stale::default();
    stale.poll();
    assert!(stale.tasks);
}

#[test]
fn only_the_poll_reads_the_task_list_while_tasks_change() {
    // Every list read fails at once here, so the failure count is the count of GET /tasks.
    let config = closed_config();
    let (events, _receiver) = futures::channel::mpsc::unbounded();
    let mut stale = Stale::default();
    let mut live = true;
    let every = Duration::from_secs(15);
    block_on(async {
        for n in 0..30 {
            let task = task_at(&format!("T-{n}"), "Busy", &format!("{n:04}"));
            let updated = task_message("task.updated", &task, Some(false));
            stale.note(&updated, &shown(&[]));
            stale.note(&task_message("task.changed", &task, None), &shown(&[]));
            assert!(stale.read(&config, every, &mut live, &events).await.is_ok());
        }
        assert_eq!(stale.task_failures, 0, "no task list read");
        stale.poll();
        assert!(stale.read(&config, every, &mut live, &events).await.is_ok());
        assert_eq!(stale.task_failures, 1, "one read for the poll");
    });
}

/// A small generator with a fixed seed, so a failing stream repeats.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// The daemon's tasks, and the messages its socket holds that the feed did not take yet.
#[derive(Default)]
struct Daemon {
    tasks: BTreeMap<String, Task>,
    clock: u32,
    next_id: u32,
    socket: std::collections::VecDeque<String>,
}

/// `Store::list_tasks` order: newest `updatedAt` first, then id.
fn newest(a: &Task, b: &Task) -> std::cmp::Ordering {
    (&b.updated_at, &a.id).cmp(&(&a.updated_at, &b.id))
}

impl Daemon {
    /// `GET /tasks`.
    fn list(&self) -> Vec<Task> {
        let mut list: Vec<Task> = self.tasks.values().cloned().collect();
        list.sort_by(newest);
        list
    }

    /// Time moves forward, but some changes keep it, so tasks share times too.
    fn now(&mut self, rng: &mut Rng) -> String {
        if rng.below(3) > 0 {
            self.clock += 1;
        }
        format!("2026-10-29T10:00:00.{:06}Z", self.clock)
    }

    fn send(&mut self, message: String) {
        self.socket.push_back(message);
    }

    fn change(&mut self, rng: &mut Rng) {
        let ids: Vec<String> = self.tasks.keys().cloned().collect();
        let picked = rng.below(ids.len().max(1) as u64) as usize;
        let roll = rng.below(20);
        let Some(id) = ids.get(picked).cloned().filter(|_| roll >= 6) else {
            return self.create(rng);
        };
        let task = task_at(&id, &format!("Step {}", rng.below(1000)), &self.now(rng));
        let message = match roll {
            6..=11 => task_message("task.updated", &task, Some(rng.below(4) == 0)),
            12..=15 => task_message("task.changed", &task, None),
            16 | 17 => {
                self.tasks.remove(&id);
                return self.send(format!(r#"{{"kind":"task.deleted","taskId":"{id}"}}"#));
            }
            18 => r#"{"kind":"resync"}"#.to_string(),
            // A body the feed cannot read, such as one from a newer daemon.
            _ => format!(r#"{{"kind":"task.changed","task":{{"id":"{id}"}}}}"#),
        };
        if roll != 18 {
            self.tasks.insert(id, task);
        }
        self.send(message);
    }

    fn create(&mut self, rng: &mut Rng) {
        self.next_id += 1;
        let task = task_at(&format!("T-{}", self.next_id), "New", &self.now(rng));
        let message = task_message("task.created", &task, None);
        self.tasks.insert(task.id.clone(), task);
        self.send(message);
    }
}

/// The app's rule: a list replaces, one task is merged.
fn apply(list: &mut Vec<Task>, event: DashboardEvent) {
    match event {
        DashboardEvent::Tasks(tasks) => *list = tasks,
        DashboardEvent::TaskChanged(task) => crate::events::task_body::merge_task(list, *task),
        _ => {}
    }
}

/// One turn of `live::follow`: take `count` messages, then `Stale::read` sends the passed events
/// and then the list, read at this moment. Gives the tasks passed on and whether it read the list.
fn follow(
    stale: &mut Stale,
    daemon: &mut Daemon,
    count: usize,
    list: &mut Vec<Task>,
) -> (usize, bool) {
    for _ in 0..count {
        if let Some(text) = daemon.socket.pop_front() {
            stale.note(&text, &shown(&[]));
        }
    }
    let passed = std::mem::take(&mut stale.passed);
    let bodies = passed
        .iter()
        .filter(|event| matches!(event, DashboardEvent::TaskChanged(_)))
        .count();
    for event in passed {
        apply(list, event);
    }
    let read = std::mem::take(&mut stale.tasks);
    if read {
        apply(list, DashboardEvent::Tasks(daemon.list()));
    }
    (bodies, read)
}

#[test]
fn task_bodies_keep_the_list_equal_to_a_full_read() {
    for seed in 1..=20u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut daemon = Daemon::default();
        let mut list = Vec::new();
        let mut stale = Stale {
            tasks: true,
            ..Stale::default()
        };
        let (mut bodies, mut reads) = (0, 0);
        for round in 0..400 {
            for _ in 0..=rng.below(3) {
                daemon.change(&mut rng);
            }
            // Some turns leave messages in the socket while the list is read.
            let count = match round % 2 {
                0 => daemon.socket.len(),
                _ => rng.below(4) as usize,
            };
            let (passed, read) = follow(&mut stale, &mut daemon, count, &mut list);
            bodies += passed;
            reads += usize::from(read);
            if daemon.socket.is_empty() {
                assert_eq!(list, daemon.list(), "seed {seed}, round {round}");
            }
        }
        // The stream goes both ways: tasks passed on, and lists read.
        assert!(bodies > 200 && reads > 10, "seed {seed}");
    }
}
