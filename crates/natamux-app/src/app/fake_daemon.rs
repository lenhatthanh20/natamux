//! `--fake-daemon`: a fixed task list and terminal, so CI can take screenshots with no daemon.

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use natamux_client::{LinkStatus, TaskListEvent, TerminalEvent, TerminalStatus};
use natamux_proto::tasks::task::{Task, TaskState};

const TASKS: &[(&str, &str, TaskState, &str, &str)] = &[
    (
        "T-0143",
        "Rust migration phase 1",
        TaskState::Running,
        "implement",
        "2026-10-22T09:30:00.000Z",
    ),
    (
        "T-0142",
        "Rust migration phase 0",
        TaskState::Review,
        "agent-review",
        "2026-10-22T08:10:00.000Z",
    ),
    (
        "T-0141",
        "Count tokens and cost of each task",
        TaskState::NeedsInput,
        "implement",
        "2026-10-21T17:45:00.000Z",
    ),
    (
        "T-0139",
        "Own terminal toggle on the Stages tab",
        TaskState::Done,
        "release",
        "2026-10-21T11:00:00.000Z",
    ),
    (
        "T-0137",
        "Stable terminals for stage agents",
        TaskState::Failed,
        "check",
        "2026-10-21T09:20:00.000Z",
    ),
    (
        "T-0135",
        "Background jobs in the footer",
        TaskState::Ready,
        "spec",
        "2026-10-20T15:05:00.000Z",
    ),
    (
        "T-0131",
        "Gõ tiếng Việt trong ô tìm kiếm",
        TaskState::Draft,
        "spec",
        "2026-10-19T10:00:00.000Z",
    ),
];

pub fn tasks() -> UnboundedReceiver<TaskListEvent> {
    let tasks = TASKS
        .iter()
        .map(|&(id, title, state, stage, updated_at)| Task {
            id: id.into(),
            title: title.into(),
            state,
            stage: stage.into(),
            workspace: Some(format!("/work/{id}")),
            agent: Some("claude-code".into()),
            parent_id: None,
            repo: None,
            updated_at: updated_at.into(),
            ..Task::default()
        })
        .collect();
    events([
        TaskListEvent::Status(LinkStatus::Live),
        TaskListEvent::Tasks(tasks),
    ])
}

/// An agent's screen, with colours, wide lines and Vietnamese text.
const SCREEN: &str = concat!(
    "\x1b[1;38;5;75m● Natamux agent\x1b[0m \x1b[2m· claude-code · implement\x1b[0m\r\n\r\n",
    "\x1b[38;5;245m> Read crates/natamux-client/src/terminals/session.rs\x1b[0m\r\n",
    "\x1b[38;5;245m> Read crates/natamux-client/src/terminals/protocol.rs\x1b[0m\r\n\r\n",
    "The terminal socket resumes from the last offset the screen parsed. A snapshot\r\n",
    "comes only when the ring no longer has that offset.\r\n\r\n",
    "\x1b[32m✓\x1b[0m cargo test -p natamux-client   \x1b[32m44 passed\x1b[0m\r\n",
    "\x1b[33m!\x1b[0m cargo deny check               \x1b[33m15 advisories ignored, with reasons\x1b[0m\r\n",
    "\x1b[31m✗\x1b[0m live-daemon                    \x1b[31mwaiting for the image of main\x1b[0m\r\n\r\n",
    "Đã sửa xong lỗi kết nối lại; gõ tiếng Việt: người, được, khuỷu, thuở.\r\n\r\n",
    "\x1b[1m❯\x1b[0m ",
);

pub fn terminal() -> UnboundedReceiver<TerminalEvent> {
    events([
        TerminalEvent::Status(TerminalStatus::Open),
        TerminalEvent::Output {
            bytes: SCREEN.as_bytes().to_vec(),
            end: Some(SCREEN.len() as u64),
        },
    ])
}

fn events<T>(items: impl IntoIterator<Item = T>) -> UnboundedReceiver<T> {
    let (sender, receiver) = unbounded();
    for item in items {
        // The receiver is right here: a send cannot fail.
        sender.unbounded_send(item).ok();
    }
    receiver
}
