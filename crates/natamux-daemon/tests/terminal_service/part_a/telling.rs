//! `describe('telling the dashboards')`.

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::limits::WORKSPACE_QUIET_MS;

use super::util::attach_at;
use crate::support::{Harness, SIZE, SetupOptions, programs_changed, setup_with, shells_changed};

fn watched() -> (Harness, Arc<Mutex<Vec<String>>>) {
    let quiet = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&quiet);
    let h = setup_with(SetupOptions {
        on_output_idle: Some(Arc::new(move |task: &str| {
            sink.lock().unwrap().push(task.into())
        })),
        ..SetupOptions::default()
    });
    (h, quiet)
}

fn changes(h: &Harness) -> usize {
    h.messages()
        .iter()
        .filter(|message| message.kind() == "shells.changed")
        .count()
}

#[test]
fn sends_shells_changed_when_a_shell_starts_is_closed_or_ends() {
    let (h, _) = watched();
    let changed = shells_changed("T-0001");

    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "2");
    assert_eq!(h.messages(), [changed.clone(), changed.clone()]);

    h.service.close_one("T-0001", "2");
    h.pump();
    assert_eq!(h.messages().len(), 3);

    // A shell that crashed ended too.
    h.pty(0).exit(Some(1));
    assert_eq!(
        h.messages(),
        [changed.clone(), changed.clone(), changed.clone(), changed]
    );
}

#[test]
fn tells_once_when_the_output_of_a_task_stopped_for_a_moment() {
    let (h, quiet) = watched();
    attach_at(&h, "T-0001", SIZE, "1");

    for _ in 0..10 {
        h.pty(0).emit("working\r\n");
        h.advance(WORKSPACE_QUIET_MS / 2);
    }
    assert!(quiet.lock().unwrap().is_empty());

    h.advance(WORKSPACE_QUIET_MS * 2);
    assert_eq!(*quiet.lock().unwrap(), ["T-0001"]);
    h.advance(WORKSPACE_QUIET_MS * 4);
    assert_eq!(*quiet.lock().unwrap(), ["T-0001"]);
}

#[test]
fn sends_shells_changed_when_the_output_stops_and_the_program_in_front_is_another() {
    let (h, _) = watched();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "2");
    let started = changes(&h);

    h.pty(1).emit("ls\r\n");
    h.advance(WORKSPACE_QUIET_MS * 2);
    assert_eq!(changes(&h), started);

    h.pty(1).set_program(Some("vim"));
    h.pty(1).emit("vim notes.txt\r\n");
    h.advance(WORKSPACE_QUIET_MS * 2);
    assert_eq!(changes(&h), started + 1);
    assert_eq!(
        h.messages().last(),
        Some(&programs_changed("T-0001", &["2"]))
    );
}

#[test]
fn names_only_the_shell_whose_program_changed_of_three() {
    let (h, _) = watched();
    for name in ["1", "2", "3"] {
        attach_at(&h, "T-0001", SIZE, name);
    }
    let started = changes(&h);

    h.pty(1).set_program(Some("vim"));
    for index in 0..3 {
        h.pty(index).emit("working\r\n");
    }
    h.advance(WORKSPACE_QUIET_MS * 2);
    assert_eq!(changes(&h), started + 1);
    assert_eq!(
        h.messages().last(),
        Some(&programs_changed("T-0001", &["2"]))
    );

    // Nothing changed: nothing is sent.
    h.pty(0).emit("more\r\n");
    h.advance(WORKSPACE_QUIET_MS * 2);
    assert_eq!(changes(&h), started + 1);
}
