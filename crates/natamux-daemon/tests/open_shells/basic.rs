//! describe 'OpenShells'.

use natamux_daemon::shared::event_bus::DaemonMessage;
use natamux_daemon::terminals::open_shells::{
    EXIT_CONFIRM_MS, RESTORE_POLL_MS, RESTORE_SPINNER_MS,
};
use natamux_daemon::terminals::shell_files::{
    ended_shells_path, read_ended_shells, save_ended_shells, save_open_shells,
};
use natamux_proto::terminals::ended::EndedShell;

use super::fixture::{TempDir, changed, copilot, open, plain, restoring_of, setup, setup_with};

const T: &str = "T-0076";

#[test]
fn writes_down_each_shell_that_starts_as_a_plain_shell_and_drops_one_that_stays_ended() {
    let dir = TempDir::new();
    let s = setup(dir.path());

    s.terminals.start_shell(T, "1");
    s.terminals.start_shell(T, "6");
    assert_eq!(s.read(T), vec![plain("1"), plain("6")]);

    s.terminals.end_shell(T, "6");
    assert_eq!(s.read(T).len(), 2);
    s.advance(EXIT_CONFIRM_MS);
    assert_eq!(s.read(T), vec![plain("1")]);

    s.terminals.end_shell(T, "1");
    s.advance(EXIT_CONFIRM_MS);
    assert!(!s.file_exists(T));
}

#[test]
fn writes_down_the_agent_of_a_launch_and_the_session_of_a_hook() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");

    s.shells.note_launch(T, "6", "copilot", None);
    assert_eq!(s.read(T), vec![open("6", Some("copilot"), None)]);

    s.shells.note_session(T, "6", "copilot", "c-1");
    assert_eq!(s.read(T), vec![copilot()]);

    // A launch that goes on with a session keeps it, so a second restart still resumes.
    s.shells.note_launch(T, "6", "copilot", Some("c-1"));
    assert_eq!(s.read(T), vec![copilot()]);
}

// The only test of this binary that sets the custom agents (they are the process's).
#[test]
fn keeps_a_custom_agent_when_its_bases_hook_names_the_session() {
    use natamux_proto::agents::custom::CustomAgent;
    natamux_agents::custom::set_custom_agents(vec![CustomAgent {
        id: "claude-qwen".into(),
        base: "claude-code".into(),
        command: "cq".into(),
    }]);
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");
    s.shells.note_launch(T, "6", "claude-qwen", None);

    s.shells.note_session(T, "6", "claude-code", "q-1");

    assert_eq!(s.read(T), vec![open("6", Some("claude-qwen"), Some("q-1"))]);
    assert_eq!(
        s.shells.session_of(T, "6", "claude-qwen").as_deref(),
        Some("q-1")
    );
}

#[test]
fn writes_only_when_a_shell_changes_not_on_every_hook() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");
    s.shells.note_session(T, "6", "copilot", "c-1");
    let before = s.writes.lock().unwrap().len();

    for _ in 0..20 {
        s.shells.note_session(T, "6", "copilot", "c-1");
        s.publish(changed(T));
    }

    assert_eq!(s.writes.lock().unwrap().len(), before);
}

#[test]
fn ignores_a_launch_or_a_session_in_a_shell_that_is_not_open() {
    let dir = TempDir::new();
    let s = setup(dir.path());

    s.shells.note_launch(T, "6", "copilot", None);
    s.shells.note_session(T, "7", "cursor", "u-1");

    assert!(!s.file_exists(T));
}

#[test]
fn keeps_the_list_through_the_shutdown_with_the_agent_the_person_quit_cleared() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    for (name, agent, session) in [
        ("1", "claude-code", "a-1"),
        ("6", "copilot", "c-1"),
        ("7", "cursor", "u-1"),
    ] {
        s.terminals.start_shell(T, name);
        s.terminals.set_program(T, name, Some(agent));
        s.shells.note_session(T, name, agent, session);
    }

    // The person quit Cursor in shell 7 and kept the shell.
    s.terminals.set_program(T, "7", None);
    s.advance(EXIT_CONFIRM_MS);
    s.shells.stop();
    s.terminals.end_shell(T, "1");
    s.terminals.end_shell(T, "6");
    s.terminals.end_shell(T, "7");
    s.advance(EXIT_CONFIRM_MS);

    assert_eq!(
        s.read(T),
        vec![
            open("1", Some("claude-code"), Some("a-1")),
            copilot(),
            plain("7")
        ]
    );
}

#[test]
fn lets_the_shells_wait_after_a_restart_except_the_ones_a_stage_starts_again() {
    let dir = TempDir::new();
    save_open_shells(
        dir.path(),
        T,
        &[
            open("1", Some("claude-code"), Some("a-1")),
            copilot(),
            open("7", Some("cursor"), None),
            open("8", Some("copilot"), Some("c-8")),
            plain("9"),
        ],
    )
    .unwrap();
    let s = setup(dir.path());

    s.shells.load(T, &["8".to_string()]);

    assert_eq!(s.shells.waiting(T), ["1", "6", "7", "9"]);
    // Shell 7 has no session: Cursor goes on with the newest of the folder.
    assert_eq!(
        s.restoring(T),
        restoring_of(&[
            ("1", Some("claude-code")),
            ("6", Some("copilot")),
            ("7", Some("cursor")),
            ("9", None),
        ])
    );
}

#[test]
fn hands_a_waiting_shell_out_once() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[copilot(), open("7", Some("cursor"), None)]).unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    assert_eq!(s.shells.take(T, "6"), Some(copilot()));
    assert_eq!(s.shells.take(T, "6"), None);
    assert_eq!(s.shells.take(T, "7"), Some(open("7", Some("cursor"), None)));
    assert_eq!(s.shells.take(T, "9"), None);
    assert!(s.shells.waiting(T).is_empty());
}

#[test]
fn keeps_a_waiting_shell_in_the_file_and_keeps_its_agent_once_it_started_again() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[plain("1"), copilot()]).unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    s.terminals.start_shell(T, "1");
    assert_eq!(s.read(T), vec![plain("1"), copilot()]);

    s.shells.take(T, "6");
    s.terminals.start_shell(T, "6");
    assert_eq!(s.read(T), vec![plain("1"), copilot()]);
}

#[test]
fn drops_a_waiting_shell_when_another_shell_of_that_name_starts() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[copilot()]).unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    // A stage command, for example, takes the name before a dashboard attached.
    s.terminals.start_shell(T, "6");

    assert!(s.shells.waiting(T).is_empty());
    assert_eq!(s.shells.take(T, "6"), None);
    assert_eq!(s.read(T), vec![plain("6")]);
}

#[test]
fn forgets_the_shells_of_a_task_that_ended_or_was_deleted() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");
    s.terminals.start_shell("T-0077", "1");

    s.ended.lock().unwrap().push(T.to_string());
    s.publish(changed(T));
    s.publish(DaemonMessage::TaskDeleted {
        task_id: "T-0077".into(),
    });

    assert!(!s.file_exists(T));
    assert!(!s.file_exists("T-0077"));
    assert!(s.shells.restoring(T).is_empty());
}

#[test]
fn keeps_the_session_of_an_agent_reopened_in_a_finished_task_with_its_ended_shell() {
    let dir = TempDir::new();
    let s = setup_with(dir.path(), &[T], None);
    let ended = EndedShell {
        name: "1".into(),
        agent: Some("claude-code".into()),
        stage: Some("spec".into()),
        session: Some("s-1".into()),
        ended_at: "2026-10-20T00:00:00.000Z".into(),
        text: "Done.".into(),
    };
    save_ended_shells(dir.path(), T, std::slice::from_ref(&ended)).unwrap();
    s.terminals.start_shell(T, "1");

    s.shells.note_launch(T, "1", "claude-code", Some("s-1"));
    s.shells.note_session(T, "1", "claude-code", "s-2");

    assert_eq!(
        read_ended_shells(dir.path(), T),
        vec![EndedShell {
            session: Some("s-2".into()),
            ..ended
        }]
    );
    assert!(ended_shells_path(dir.path(), T).exists());
    assert!(!s.file_exists(T));
    assert_eq!(s.shells.session_of(T, "1", "claude-code"), None);
}

#[test]
fn forgets_the_shells_of_a_task_whose_workspace_was_removed() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[copilot()]).unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    let event = serde_json::from_value(serde_json::json!({
        "taskId": T, "source": "system", "type": "workspace.removed",
        "at": "2026-10-08T09:00:00.000Z", "payload": {}
    }))
    .unwrap();
    let task = natamux_proto::tasks::task::Task {
        id: T.into(),
        ..Default::default()
    };
    s.publish(DaemonMessage::TaskUpdated {
        task,
        event,
        accepted: true,
        moved: None,
    });

    assert!(s.shells.waiting(T).is_empty());
    assert!(!s.file_exists(T));
}

#[test]
fn shows_a_starting_agent_as_restoring_until_it_is_in_the_foreground() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");
    s.messages.lock().unwrap().clear();

    s.shells.mark_starting(T, "6", "copilot");
    assert_eq!(s.restoring(T), restoring_of(&[("6", Some("copilot"))]));
    assert_eq!(*s.messages.lock().unwrap(), [changed(T)]);

    s.advance(RESTORE_POLL_MS * 2);
    assert_eq!(s.restoring(T), restoring_of(&[("6", Some("copilot"))]));

    s.terminals.set_program_quiet(T, "6", Some("copilot"));
    s.advance(RESTORE_POLL_MS);
    assert!(s.restoring(T).is_empty());
    assert_eq!(s.messages.lock().unwrap().last(), Some(&changed(T)));
}

#[test]
fn stops_the_spinner_of_an_agent_that_never_shows_after_a_while() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");

    s.shells.mark_starting(T, "6", "copilot");
    s.advance(RESTORE_SPINNER_MS - RESTORE_POLL_MS);
    assert_eq!(s.restoring(T), restoring_of(&[("6", Some("copilot"))]));

    s.advance(RESTORE_POLL_MS);
    assert!(s.restoring(T).is_empty());
}
