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
    bus.subscribe_client(move |text| sink.lock().unwrap().push(text.to_string()));
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
    let first = bus.subscribe_client(|_| {});
    let second = bus.subscribe_client(|_| {});
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
    bus.subscribe_client(|_| panic!("closed socket"));
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
    bus.subscribe_client(|_| panic!("closed socket"));
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
