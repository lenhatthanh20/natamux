//! describe 'OpenShells across restarts'.

use std::path::Path;
use std::sync::Arc;

use natamux_daemon::terminals::open_shells::EXIT_CONFIRM_MS;
use natamux_daemon::terminals::shell_files::{read_open_shells, save_open_shells};

use super::fixture::{Setup, TempDir, changed, open, restoring_of, setup, setup_with};

const T: &str = "T-0128";

// The daemon starts again on the same data folder. The old one was killed or stopped, so its
// timers are gone and its shells with it.
fn restart(old: &Setup, skip: &[&str]) -> Setup {
    old.shells.stop();
    let next = setup(&old.dir);
    let skip: Vec<String> = skip.iter().map(|name| name.to_string()).collect();
    next.shells.load(T, &skip);
    next
}

// Shell 2 runs Codex on session x-2, shell 3 Claude Code on session a-3.
fn two_agents(dir: &Path) -> Setup {
    let first = setup(dir);
    for (name, agent, session) in [("2", "codex", "x-2"), ("3", "claude-code", "a-3")] {
        first.terminals.start_shell(T, name);
        first.terminals.set_program(T, name, Some(agent));
        first.shells.note_session(T, name, agent, session);
    }
    first
}

#[test]
fn writes_down_a_quit_once_the_agent_stayed_gone_so_a_kill_keeps_it_quit() {
    let dir = TempDir::new();
    let first = two_agents(dir.path());

    first.terminals.set_program(T, "2", None);
    first.advance(EXIT_CONFIRM_MS);
    // Killed: no stop, the file is what was written.
    let next = restart(&first, &[]);

    assert_eq!(
        next.restoring(T),
        restoring_of(&[("2", None), ("3", Some("claude-code"))])
    );
}

#[test]
fn brings_back_an_agent_that_ended_just_before_the_kill() {
    let dir = TempDir::new();
    let first = two_agents(dir.path());

    first.terminals.set_program(T, "2", None);
    first.terminals.end_shell(T, "3");
    first.advance(EXIT_CONFIRM_MS - 1000);
    let next = restart(&first, &[]);

    assert_eq!(
        next.restoring(T),
        restoring_of(&[("2", Some("codex")), ("3", Some("claude-code"))])
    );
}

#[test]
fn drops_a_shell_that_stayed_closed() {
    let dir = TempDir::new();
    let first = two_agents(dir.path());

    first.terminals.end_shell(T, "3");
    first.advance(EXIT_CONFIRM_MS);
    let next = restart(&first, &[]);

    assert_eq!(next.restoring(T), restoring_of(&[("2", Some("codex"))]));
}

#[test]
fn gives_the_same_file_after_a_stop_as_after_a_kill() {
    let dir = TempDir::new();
    let killed = two_agents(dir.path());
    killed.terminals.set_program(T, "2", None);
    killed.advance(EXIT_CONFIRM_MS);
    let after_kill = read_open_shells(dir.path(), T);
    killed.shells.stop();
    std::fs::remove_dir_all(dir.path().join("tasks")).unwrap();

    let stopped = two_agents(dir.path());
    stopped.terminals.set_program(T, "2", None);
    stopped.advance(EXIT_CONFIRM_MS);
    stopped.shells.stop();
    stopped.terminals.end_shell(T, "2");
    stopped.terminals.end_shell(T, "3");
    stopped.advance(EXIT_CONFIRM_MS);

    assert_eq!(read_open_shells(dir.path(), T), after_kill);
}

#[test]
fn does_not_count_an_agent_as_quit_while_it_starts() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "2");
    s.shells.note_launch(T, "2", "codex", None);
    // Not in the foreground yet, but agent_runs says it runs (its launch time).
    s.terminals.set_program_quiet(T, "2", Some("zsh"));

    s.advance(EXIT_CONFIRM_MS * 3);

    assert_eq!(s.read(T), vec![open("2", Some("codex"), None)]);
}

#[test]
fn keeps_a_held_shell_through_a_sync_and_lets_it_wait_once_released() {
    let dir = TempDir::new();
    save_open_shells(
        dir.path(),
        T,
        &[
            open("1", Some("claude-code"), Some("a-1")),
            open("2", Some("codex"), Some("x-2")),
        ],
    )
    .unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &["1".to_string()]);
    assert_eq!(s.shells.waiting(T), ["2"]);
    assert!(s.shells.was_running(T, "1"));
    assert_eq!(
        s.shells.session_of(T, "1", "claude-code"),
        Some("a-1".to_string())
    );
    assert_eq!(s.shells.session_of(T, "1", "codex"), None);

    // A dashboard opens shell 2 before resumeAgents ran.
    s.shells.take(T, "2");
    s.terminals.start_shell(T, "2");
    assert!(
        s.read(T)
            .contains(&open("1", Some("claude-code"), Some("a-1")))
    );

    // Its stage could not start the agent.
    s.messages.lock().unwrap().clear();
    s.shells.release_held();
    assert_eq!(s.shells.waiting(T), ["1"]);
    assert_eq!(s.restoring(T), restoring_of(&[("1", Some("claude-code"))]));
    assert!(s.messages.lock().unwrap().contains(&changed(T)));
}

#[test]
fn leaves_a_held_shell_that_its_stage_started_alone() {
    let dir = TempDir::new();
    save_open_shells(
        dir.path(),
        T,
        &[open("1", Some("claude-code"), Some("a-1"))],
    )
    .unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &["1".to_string()]);

    s.terminals.start_shell(T, "1");
    s.shells.release_held();

    assert!(s.shells.waiting(T).is_empty());
}

#[test]
fn lets_a_shell_wait_again_when_its_agent_could_not_come_back() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[open("2", Some("codex"), Some("x-2"))]).unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    s.shells.take(T, "2");
    s.shells.put_back(T, "2");

    assert_eq!(s.restoring(T), restoring_of(&[("2", Some("codex"))]));
}

#[test]
fn keeps_the_agent_of_a_shell_that_came_back_plain_for_the_next_start() {
    let dir = TempDir::new();
    save_open_shells(dir.path(), T, &[open("2", Some("codex"), Some("x-2"))]).unwrap();
    let first = setup(dir.path());
    first.shells.load(T, &[]);

    // The agent could not start, so the attach started a plain shell.
    first.shells.take(T, "2");
    first.terminals.start_shell(T, "2");
    first.advance(EXIT_CONFIRM_MS);
    let next = restart(&first, &[]);

    assert_eq!(next.restoring(T), restoring_of(&[("2", Some("codex"))]));
}

#[test]
fn writes_down_an_agent_typed_by_hand_from_its_process() {
    let dir = TempDir::new();
    let agent_in = Arc::new(|_task: &str, terminal: &str| {
        (terminal == "2").then(|| "claude-code".to_string())
    });
    let s = setup_with(dir.path(), &[], Some(agent_in));
    s.terminals.start_shell(T, "2");

    s.terminals.set_program(T, "2", Some("claude"));
    // Rust: the process lookup runs on a scheduler job, not in an awaited promise.
    s.advance(0);
    assert_eq!(s.read(T), vec![open("2", Some("claude-code"), None)]);
    // Its hooks, if any, name the session later.
    s.shells.note_session(T, "2", "claude-code", "a-2");
    assert_eq!(s.read(T), vec![open("2", Some("claude-code"), Some("a-2"))]);
}

#[test]
fn brings_back_one_agent_of_a_kind_with_no_session_the_others_plain() {
    let dir = TempDir::new();
    save_open_shells(
        dir.path(),
        T,
        &[
            open("2", Some("claude-code"), None),
            open("3", Some("claude-code"), None),
            open("4", Some("claude-code"), Some("a-4")),
            open("5", Some("codex"), None),
        ],
    )
    .unwrap();
    let s = setup(dir.path());
    s.shells.load(T, &[]);

    assert_eq!(
        s.restoring(T),
        restoring_of(&[
            ("2", Some("claude-code")),
            ("3", None),
            ("4", Some("claude-code")),
            ("5", Some("codex")),
        ])
    );
}

#[test]
fn keeps_the_agent_of_a_held_stage_shell_with_no_session_beside_a_hand_typed_one() {
    let dir = TempDir::new();
    save_open_shells(
        dir.path(),
        T,
        &[
            open("2", Some("codex"), None),
            open("3", Some("codex"), None),
        ],
    )
    .unwrap();
    let s = setup(dir.path());
    // Shell 3 belongs to a working stage; Codex there has not reported a session yet.
    s.shells.load(T, &["3".to_string()]);

    assert!(s.shells.was_running(T, "3"));
    assert_eq!(s.restoring(T), restoring_of(&[("2", Some("codex"))]));
    s.shells.release_held();
    assert_eq!(
        s.restoring(T),
        restoring_of(&[("2", Some("codex")), ("3", Some("codex"))])
    );
}

#[test]
fn keeps_the_same_agents_through_many_restarts_killed_or_stopped_opened_or_not() {
    let dir = TempDir::new();
    let mut current = two_agents(dir.path());
    let expected = restoring_of(&[("2", Some("codex")), ("3", Some("claude-code"))]);
    for round in 0..5 {
        let next = restart(&current, &[]);
        assert_eq!(next.restoring(T), expected);
        if round % 2 == 0 {
            // Opened: both agents come back on their sessions and report them again.
            for name in ["2", "3"] {
                let shell = next.shells.take(T, name);
                next.terminals.start_shell(T, name);
                let agent = shell.as_ref().and_then(|shell| shell.agent.as_deref());
                next.terminals.set_program(T, name, agent);
                let session = shell.as_ref().and_then(|shell| shell.session.as_deref());
                next.shells
                    .note_launch(T, name, agent.unwrap_or(""), session);
            }
            next.advance(EXIT_CONFIRM_MS);
        }
        current = next;
    }
    assert_eq!(
        read_open_shells(dir.path(), T),
        vec![
            open("2", Some("codex"), Some("x-2")),
            open("3", Some("claude-code"), Some("a-3"))
        ]
    );
}
