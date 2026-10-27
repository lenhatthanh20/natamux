//! Paused shells: the daemon ends the idle agent of a shell, which then waits with its agent and
//! session as after a restart.

use natamux_daemon::terminals::open_shells::EXIT_CONFIRM_MS;
use natamux_daemon::terminals::shell_files::save_open_shells;

use super::fixture::{Setup, TempDir, changed, copilot, open, paused, restoring_of, setup};

const T: &str = "T-0080";

/// Copilot runs in shell 6 on session c-1.
fn running(s: &Setup) {
    s.terminals.start_shell(T, "6");
    s.terminals.set_program(T, "6", Some("copilot"));
    s.shells.note_session(T, "6", "copilot", "c-1");
}

/// The daemon pauses shell 6: marked, then ended.
fn pause(s: &Setup) {
    assert!(s.shells.pause(T, "6"));
    s.terminals.end_shell(T, "6");
    s.shells.pause_done(T, "6");
}

#[test]
fn keeps_the_agent_and_session_of_a_paused_shell_through_its_end_and_the_quit_watch() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);

    pause(&s);
    s.advance(EXIT_CONFIRM_MS * 2);

    assert_eq!(s.read(T), vec![paused("6", "copilot", "c-1")]);
    assert_eq!(s.shells.waiting(T), ["6"]);
    assert_eq!(s.restoring(T), restoring_of(&[("6", Some("copilot"))]));
    assert!(s.shells.is_paused(T, "6"));
    assert_eq!(s.shells.agent_of(T, "6").as_deref(), Some("copilot"));
}

#[test]
fn does_not_take_a_paused_shell_that_still_closes_for_a_new_shell() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);

    assert!(s.shells.pause(T, "6"));
    // Another shell of the task starts before shell 6 is gone.
    s.terminals.start_shell(T, "7");
    s.publish(changed(T));

    assert_eq!(s.shells.waiting(T), ["6"]);
    assert_eq!(s.read(T)[0], paused("6", "copilot", "c-1"));
}

#[test]
fn stops_a_quit_watch_that_started_before_the_pause() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);
    // The agent left the foreground a moment before the pause.
    s.terminals.set_program(T, "6", None);

    pause(&s);
    s.advance(EXIT_CONFIRM_MS * 2);

    assert_eq!(s.read(T), vec![paused("6", "copilot", "c-1")]);
}

#[test]
fn does_not_pause_a_shell_with_no_session_or_one_that_is_not_open() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "6");
    s.shells.note_launch(T, "6", "copilot", None);

    assert!(!s.shells.pause(T, "6"));
    assert!(!s.shells.pause(T, "7"));
    assert_eq!(s.read(T), vec![open("6", Some("copilot"), None)]);
    assert!(!s.shells.is_paused(T, "6"));
}

#[test]
fn clears_the_pause_once_the_shell_runs_again() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);
    pause(&s);

    assert_eq!(s.shells.take(T, "6"), Some(paused("6", "copilot", "c-1")));
    s.terminals.start_shell(T, "6");

    assert_eq!(s.read(T), vec![copilot()]);
    assert!(!s.shells.is_paused(T, "6"));
}

#[test]
fn lets_a_shell_run_on_when_its_pause_failed() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);

    assert!(s.shells.pause(T, "6"));
    s.shells.unpause(T, "6");

    assert_eq!(s.read(T), vec![copilot()]);
    assert!(s.shells.waiting(T).is_empty());
    assert!(!s.shells.is_paused(T, "6"));
}

#[test]
fn lets_a_paused_shell_wait_after_a_restart_even_when_its_stage_starts_again() {
    let dir = TempDir::new();
    let list = [paused("1", "claude-code", "a-1"), copilot()];
    save_open_shells(dir.path(), T, &list).unwrap();
    let s = setup(dir.path());

    s.shells.load(T, &["1".to_string()]);

    assert_eq!(s.shells.waiting(T), ["1", "6"]);
    assert!(s.shells.is_paused(T, "1"));
    assert!(!s.shells.is_paused(T, "6"));
    // A write for another shell keeps the mark.
    s.terminals.start_shell(T, "7");
    assert_eq!(s.read(T)[0], paused("1", "claude-code", "a-1"));
}

#[test]
fn forgets_the_pause_with_the_shells_of_a_task_that_ended() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    running(&s);
    pause(&s);

    s.ended.lock().unwrap().push(T.to_string());
    s.publish(changed(T));

    assert!(!s.shells.is_paused(T, "6"));
    assert!(!s.file_exists(T));
}
