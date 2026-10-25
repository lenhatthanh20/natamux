//! A new program in one shell: `OpenShells` looks at that shell only (spec 5.3).

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::open_shells::AgentIn;

use super::fixture::{TempDir, open, plain, setup, setup_with};

const T: &str = "T-0076";

#[test]
fn a_new_program_in_one_of_three_shells_looks_at_only_that_shell() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    for name in ["1", "2", "3"] {
        s.terminals.start_shell(T, name);
    }
    s.terminals.looks.lock().unwrap().clear();

    s.terminals.set_program_in(T, "2", Some("vim"));
    s.advance(0);

    let looks = s.terminals.looks.lock().unwrap().clone();
    assert!(!looks.is_empty());
    assert!(looks.iter().all(|key| key == "T-0076/2"), "{looks:?}");
    assert_eq!(s.read(T), vec![plain("1"), plain("2"), plain("3")]);
}

#[test]
fn an_agent_typed_by_hand_in_one_shell_is_still_found() {
    let dir = TempDir::new();
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let heard = Arc::clone(&asked);
    let agent_in: Arc<AgentIn> = Arc::new(move |task, terminal| {
        heard.lock().unwrap().push(format!("{task}/{terminal}"));
        Some("copilot".to_string())
    });
    let s = setup_with(dir.path(), &[], Some(agent_in));
    for name in ["1", "2", "3"] {
        s.terminals.start_shell(T, name);
    }
    s.advance(0);
    asked.lock().unwrap().clear();

    s.terminals.set_program_in(T, "3", Some("copilot"));
    s.advance(0);

    assert_eq!(*asked.lock().unwrap(), ["T-0076/3"]);
    let shells = s.read(T);
    assert_eq!(shells[2], open("3", Some("copilot"), None));
}

#[test]
fn a_new_program_in_a_shell_the_list_does_not_have_yet_syncs_the_task() {
    let dir = TempDir::new();
    let s = setup(dir.path());
    s.terminals.start_shell(T, "1");
    s.terminals.start_shell_quiet(T, "2");

    s.terminals.set_program_in(T, "2", Some("vim"));

    assert_eq!(s.read(T), vec![plain("1"), plain("2")]);
}
