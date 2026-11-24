use std::sync::Mutex;

use natamux_proto::tasks::task::TaskState;
use serde_json::json;

use super::*;

pub fn task() -> Task {
    Task {
        id: "T-0001".into(),
        title: "Example".into(),
        state: TaskState::Draft,
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement"), json!("review"), json!("release")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn message() -> DaemonMessage {
    DaemonMessage::TaskCreated { task: task() }
}

fn heard(bus: &EventBus) -> Arc<Mutex<Vec<DaemonMessage>>> {
    let heard: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
    let sink = Arc::clone(&heard);
    bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
    heard
}

fn texts(bus: &EventBus) -> Arc<Mutex<Vec<String>>> {
    let texts: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&texts);
    bus.subscribe_client(ClientScope::All, move |text| {
        sink.lock().unwrap().push(text.to_string())
    });
    texts
}

#[test]
fn delivers_a_message_to_every_subscriber() {
    let bus = EventBus::new();
    let first = heard(&bus);
    let second = heard(&bus);
    bus.publish(message());
    assert_eq!(*first.lock().unwrap(), [message()]);
    assert_eq!(*second.lock().unwrap(), [message()]);
}

#[test]
fn stops_delivering_after_unsubscribe() {
    let bus = EventBus::new();
    let received: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
    let sink = Arc::clone(&received);
    let id = bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
    bus.unsubscribe(id);
    bus.publish(message());
    assert!(received.lock().unwrap().is_empty());
}

// The log line is not checked: it goes to stderr.
#[test]
fn keeps_delivering_when_one_subscriber_throws_and_logs_the_error() {
    let bus = EventBus::new();
    bus.subscribe(|_| panic!("broken listener"));
    let received = heard(&bus);
    bus.publish(message());
    assert_eq!(*received.lock().unwrap(), [message()]);
}

#[test]
fn counts_only_the_dashboards_not_the_services_that_subscribe() {
    let bus = EventBus::new();
    bus.subscribe(|_| {});
    assert_eq!(bus.size(), 0);
    let first = bus.subscribe_client(ClientScope::All, |_| {});
    let second = bus.subscribe_client(ClientScope::All, |_| {});
    assert_eq!(bus.size(), 2);
    bus.unsubscribe(first);
    bus.unsubscribe(second);
    assert_eq!(bus.size(), 0);
}

// One text is made per message (`deliver`); every dashboard gets the same.
#[test]
fn makes_the_json_text_once_for_all_dashboards() {
    let bus = EventBus::new();
    let all: Vec<_> = (0..3).map(|_| texts(&bus)).collect();
    let services = heard(&bus);
    bus.publish(message());
    for each in all {
        assert_eq!(*each.lock().unwrap(), [message().to_json()]);
    }
    assert_eq!(*services.lock().unwrap(), [message()]);
}

#[test]
fn keeps_sending_when_one_dashboard_throws_and_logs_the_error() {
    let bus = EventBus::new();
    bus.subscribe_client(ClientScope::All, |_| panic!("closed socket"));
    let received = texts(&bus);
    bus.publish(message());
    assert_eq!(*received.lock().unwrap(), [message().to_json()]);
}

// Without dashboards `deliver` returns before it makes the text.
#[test]
fn makes_no_text_without_dashboards() {
    let bus = EventBus::new();
    let services = heard(&bus);
    bus.publish(message());
    assert_eq!(services.lock().unwrap().len(), 1);
    assert_eq!(bus.size(), 0);
}

#[test]
fn keeps_sending_to_the_other_dashboards_when_one_throws() {
    let bus = EventBus::new();
    bus.subscribe_client(ClientScope::All, |_| panic!("closed socket"));
    let received = texts(&bus);
    bus.publish(DaemonMessage::ShellsChanged {
        task_id: "T-0001".into(),
    });
    assert_eq!(
        *received.lock().unwrap(),
        [r#"{"kind":"shells.changed","taskId":"T-0001"}"#]
    );
}

#[test]
fn writes_each_kind_as_the_node_daemon() {
    let event: TaskEvent = serde_json::from_value(json!({
        "taskId": "T-0001", "source": "human", "type": "task.readied",
        "at": "2026-10-08T09:00:00.000Z", "payload": {}
    }))
    .unwrap();
    let updated = DaemonMessage::TaskUpdated {
        task: task(),
        event: event.clone(),
        accepted: true,
        moved: Some(false),
    };
    let text = updated.to_json();
    assert!(text.starts_with(r#"{"kind":"task.updated","task":{"id":"T-0001","title":"Example","description":"","state":"draft","priority":"normal","labels":[],"agent":null"#));
    assert!(text.ends_with(r#""event":{"taskId":"T-0001","source":"human","type":"task.readied","at":"2026-10-08T09:00:00.000Z","payload":{}},"accepted":true,"moved":false}"#));
    let recorded = DaemonMessage::TaskUpdated {
        task: task(),
        event,
        accepted: true,
        moved: None,
    };
    assert!(recorded.to_json().ends_with(r#""accepted":true}"#));
    for (message, text) in [
        (
            DaemonMessage::TaskDeleted {
                task_id: "T-1".into(),
            },
            r#"{"kind":"task.deleted","taskId":"T-1"}"#,
        ),
        (DaemonMessage::ReposChanged, r#"{"kind":"repos.changed"}"#),
        (DaemonMessage::Resync, r#"{"kind":"resync"}"#),
        (
            DaemonMessage::WorkspaceDirty {
                task_id: "T-1".into(),
            },
            r#"{"kind":"workspace.dirty","taskId":"T-1"}"#,
        ),
        (
            DaemonMessage::ReviewCommentsChanged {
                task_id: "T-1".into(),
            },
            r#"{"kind":"review-comments.changed","taskId":"T-1"}"#,
        ),
        // The dashboards read it as any `shells.changed`; the names stay in the daemon.
        (
            DaemonMessage::ProgramsChanged {
                task_id: "T-1".into(),
                terminals: vec!["2".into()],
            },
            r#"{"kind":"shells.changed","taskId":"T-1"}"#,
        ),
        (
            DaemonMessage::JobsChanged { jobs: Vec::new() },
            r#"{"kind":"jobs.changed","jobs":[]}"#,
        ),
        (
            DaemonMessage::WorkspacePreparing {
                task_id: "T-1".into(),
                step: None,
                error: Some("no".into()),
            },
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":null,"error":"no"}"#,
        ),
        (
            DaemonMessage::WorkspacePreparing {
                task_id: "T-1".into(),
                step: Some("fetch".into()),
                error: None,
            },
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":"fetch"}"#,
        ),
        (
            DaemonMessage::ActivityChanged {
                task_id: "T-1".into(),
                activity: Map::new(),
            },
            r#"{"kind":"activity.changed","taskId":"T-1","activity":{}}"#,
        ),
        (
            DaemonMessage::UsageChanged {
                task_id: "T-1".into(),
                usage: json!({"total": 1.5}),
            },
            r#"{"kind":"usage.changed","taskId":"T-1","usage":{"total":1.5}}"#,
        ),
        (
            DaemonMessage::LimitsChanged {
                limits: json!([{"agent": "codex"}]),
            },
            r#"{"kind":"limits.changed","limits":[{"agent":"codex"}]}"#,
        ),
    ] {
        assert_eq!(message.to_json(), text);
    }
}

#[test]
fn sends_a_message_published_by_a_listener_after_the_one_it_hears() {
    let bus = EventBus::new();
    let inner = Arc::clone(&bus);
    bus.subscribe(move |message| {
        if let DaemonMessage::TaskDeleted { task_id } = message {
            inner.publish(DaemonMessage::ShellsChanged {
                task_id: task_id.clone(),
            });
        }
    });
    let received = texts(&bus);
    bus.publish(DaemonMessage::TaskDeleted {
        task_id: "T-1".into(),
    });
    assert_eq!(
        *received.lock().unwrap(),
        [
            r#"{"kind":"task.deleted","taskId":"T-1"}"#,
            r#"{"kind":"shells.changed","taskId":"T-1"}"#
        ]
    );
}

fn task_of(id: &str, parent: Option<&str>) -> Task {
    Task {
        id: id.into(),
        parent_id: parent.map(str::to_string),
        ..task()
    }
}

// T-0373 (F6): an agent's socket hears its own task and its direct subtasks, nothing else.
#[test]
fn a_task_scoped_client_gets_its_own_and_its_subtasks_messages_only() {
    let bus = EventBus::new();
    let got: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&got);
    let scope = ClientScope::Task("T-0001".into());
    bus.subscribe_client(scope, move |text| {
        sink.lock().unwrap().push(text.to_string())
    });
    let all = texts(&bus);
    let b = task_of("T-0002", None);
    let messages = [
        DaemonMessage::TaskChanged { task: b.clone() },
        DaemonMessage::ShellsChanged {
            task_id: "T-0002".into(),
        },
        DaemonMessage::JobsChanged { jobs: Vec::new() },
        DaemonMessage::ReposChanged,
        DaemonMessage::LimitsChanged { limits: json!({}) },
        DaemonMessage::TaskChanged {
            task: task_of("T-0001", None),
        },
        DaemonMessage::ShellsChanged {
            task_id: "T-0001".into(),
        },
        DaemonMessage::TaskCreated {
            task: task_of("T-0003", Some("T-0001")),
        },
        // Only task messages say who the parent is.
        DaemonMessage::ShellsChanged {
            task_id: "T-0003".into(),
        },
        DaemonMessage::TaskCreated {
            task: task_of("T-0004", Some("T-0002")),
        },
    ];
    for message in messages {
        bus.publish(message);
    }
    let got = got.lock().unwrap().clone();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got[0].contains("\"task.changed\"") && got[0].contains("T-0001"));
    assert!(got[1].contains("shells.changed") && got[1].contains("T-0001"));
    assert!(got[2].contains("T-0003"));
    assert!(
        got.iter()
            .all(|text| !text.contains("T-0002") && !text.contains("T-0004"))
    );
    assert_eq!(all.lock().unwrap().len(), 10);
}

fn scoped(bus: &EventBus, own: &str) -> Arc<Mutex<Vec<String>>> {
    let got: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&got);
    bus.subscribe_client(ClientScope::Task(own.into()), move |text| {
        sink.lock().unwrap().push(text.to_string())
    });
    got
}

// T-0375 (R3-1): the parent's socket gets the same short view of a subtask as its `GET`.
#[test]
fn a_parent_scoped_client_gets_only_the_view_of_a_subtask() {
    let bus = EventBus::new();
    let got = scoped(&bus, "T-0001");
    let all = texts(&bus);
    let child = Task {
        description: "C-MARK".into(),
        workspace: Some("/w/T-0003".into()),
        ..task_of("T-0003", Some("T-0001"))
    };
    let event: TaskEvent = serde_json::from_value(json!({
        "taskId": "T-0003", "source": "agent", "type": "agent.message", "at": "x",
        "payload": { "text": "E-MARK" },
    }))
    .unwrap();
    bus.publish(DaemonMessage::TaskUpdated {
        task: child,
        event,
        accepted: true,
        moved: None,
    });
    let own = DaemonMessage::TaskChanged {
        task: task_of("T-0001", None),
    };
    bus.publish(own.clone());

    let got = got.lock().unwrap().clone();
    let frame: Value = serde_json::from_str(&got[0]).unwrap();
    assert_eq!(frame["kind"], "task.updated");
    assert_eq!(frame["task"], task_view(&task_of("T-0003", Some("T-0001"))));
    assert!(frame.get("event").is_none(), "{frame}");
    assert!(!got[0].contains("C-MARK") && !got[0].contains("E-MARK"));
    // Its own task and the desktop get the whole task.
    assert_eq!(got[1], own.to_json());
    assert!(all.lock().unwrap()[0].contains("C-MARK"));
}

// T-0375 (R3-2): a revoked agent's clients get nothing more and its sockets end; others stay.
#[test]
fn ending_an_agent_drops_its_clients_and_runs_its_ends_once() {
    let bus = EventBus::new();
    let got = scoped(&bus, "T-0001");
    let _other = scoped(&bus, "T-0002");
    let all = texts(&bus);
    let ended: Arc<Mutex<Vec<&str>>> = Arc::default();
    let (one, two) = (Arc::clone(&ended), Arc::clone(&ended));
    bus.on_agent_end("T-0001", move || one.lock().unwrap().push("T-0001"));
    bus.on_agent_end("T-0002", move || two.lock().unwrap().push("T-0002"));

    bus.end_agent("T-0001");
    bus.end_agent("T-0001");
    bus.publish(DaemonMessage::ShellsChanged {
        task_id: "T-0001".into(),
    });

    assert!(got.lock().unwrap().is_empty());
    assert_eq!(*ended.lock().unwrap(), ["T-0001"]);
    assert_eq!(all.lock().unwrap().len(), 1);
    assert_eq!(bus.size(), 2);
}

// T-0375 (R2-6): the listeners run under the mode forced where the message was queued, even
// when another thread sends it.
#[test]
fn a_message_is_delivered_under_the_mode_forced_where_it_was_queued() {
    use natamux_core::security::{StrictSettings, force, mode};

    let bus = EventBus::new();
    let seen: Arc<Mutex<Vec<bool>>> = Arc::default();
    let sink = Arc::clone(&seen);
    bus.subscribe(move |_| sink.lock().unwrap().push(mode().is_strict()));
    {
        let _strict = force(SecurityMode::Strict(StrictSettings::default()));
        bus.enqueue(message());
    }
    let sender = Arc::clone(&bus);
    std::thread::spawn(move || sender.flush()).join().unwrap();

    assert_eq!(*seen.lock().unwrap(), [true]);
}
