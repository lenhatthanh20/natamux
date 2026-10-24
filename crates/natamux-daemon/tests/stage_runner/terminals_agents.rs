//! `describe('the agent in each terminal')` of `stage-runner.test.ts`.

use natamux_daemon::stages::shells::LAUNCH_GRACE_MS;
use serde_json::json;

use crate::Fixture;
use crate::support::SIZE;

fn agent_in(f: &Fixture, id: &str, terminal: &str) -> Option<String> {
    f.runner.run.agent_in(id, terminal)
}

fn typed(
    f: &Fixture,
    id: &str,
    line: &str,
    program: Option<&str>,
    lines: &[&str],
) -> crate::FakePty {
    f.terminals.run(id, line, "2", SIZE).unwrap();
    let pty = f.ptys().pop().unwrap();
    pty.set_program(program);
    pty.state().command_lines = Some(lines.iter().map(|line| line.to_string()).collect());
    pty
}

fn with_task(f: &Fixture, input: serde_json::Value) -> String {
    let task = f.created(input);
    f.with_workspace(&task.id);
    task.id
}

#[test]
fn names_the_agent_a_person_typed_into_an_extra_shell_not_the_agent_of_the_task() {
    let f = Fixture::new();
    let id = with_task(
        &f,
        json!({ "title": "Export tasks", "agent": "claude-code" }),
    );
    let lines = [
        "node /usr/local/bin/codex",
        "/usr/local/lib/codex/vendor/codex",
    ];
    typed(&f, &id, "codex", Some("node"), &lines);

    assert_eq!(agent_in(&f, &id, "2").as_deref(), Some("codex"));
}

#[test]
fn names_an_agent_typed_by_hand_when_the_task_has_another_agent() {
    let f = Fixture::new();
    let id = with_task(&f, json!({ "title": "Export tasks", "agent": "gemini" }));
    typed(&f, &id, "claude", Some("claude"), &["claude"]);

    assert_eq!(agent_in(&f, &id, "2").as_deref(), Some("claude-code"));
    assert_eq!(f.task(&id).agent.as_deref(), Some("gemini"));
}

#[test]
fn names_an_agent_that_a_wrapper_such_as_npx_starts() {
    let f = Fixture::new();
    let id = with_task(&f, json!({ "title": "Export tasks" }));
    let lines = [
        "node /usr/local/bin/npx @openai/codex",
        "node /home/u/.npm/_npx/1a2b/node_modules/.bin/codex",
    ];
    typed(&f, &id, "npx @openai/codex", Some("node"), &lines);

    assert_eq!(agent_in(&f, &id, "2").as_deref(), Some("codex"));
}

#[test]
fn follows_the_agents_a_person_starts_and_quits_in_an_extra_shell() {
    let f = Fixture::new();
    let id = with_task(
        &f,
        json!({ "title": "Export tasks", "agent": "claude-code" }),
    );
    f.terminals.run(&id, "claude", "2", SIZE).unwrap();
    let pty = f.ptys().pop().unwrap();
    let set = |lines: &[&str]| {
        pty.state().command_lines = Some(lines.iter().map(|line| line.to_string()).collect());
    };

    set(&["claude"]);
    assert_eq!(agent_in(&f, &id, "2").as_deref(), Some("claude-code"));
    set(&[]);
    assert_eq!(agent_in(&f, &id, "2"), None);
    set(&["node /usr/local/bin/codex"]);
    assert_eq!(agent_in(&f, &id, "2").as_deref(), Some("codex"));
    // The shell itself ended.
    f.terminals.close_one(&id, "2");
    assert_eq!(agent_in(&f, &id, "2"), None);
}

#[test]
fn names_no_agent_when_the_processes_cannot_be_read() {
    let f = Fixture::new();
    let id = with_task(
        &f,
        json!({ "title": "Export tasks", "agent": "claude-code" }),
    );
    f.terminals.run(&id, "codex", "2", SIZE).unwrap();
    // The fake pty has no list of processes (None), as when `ps` fails.
    assert!(f.ptys().pop().unwrap().state().command_lines.is_none());

    assert_eq!(agent_in(&f, &id, "2"), None);
}

#[test]
fn names_no_agent_for_a_program_that_is_not_one() {
    let f = Fixture::new();
    let id = with_task(
        &f,
        json!({ "title": "Export tasks", "agent": "claude-code" }),
    );
    typed(&f, &id, "vim notes.md", Some("vim"), &["vim notes.md"]);

    assert_eq!(agent_in(&f, &id, "2"), None);
    assert_eq!(agent_in(&f, &id, "3"), None);
}

#[test]
fn names_the_agent_it_started_where_the_processes_cannot_be_listed() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks" }));
    // The new workspace starts the stage with the agent picked here, not the default one. Rust's
    // `start_by_hand` has no `makeWorkspace` (part 5), so the pick is set around the workspace.
    let picked = &f.runner.run;
    picked
        .state()
        .chosen_agents
        .insert(task.id.clone(), "gemini".into());
    f.with_workspace(&task.id);
    picked.start_by_hand(&task.id, "gemini", None).unwrap();
    picked.state().chosen_agents.remove(&task.id);
    f.settle();
    let pty = f.pty(0);
    pty.set_program(Some("node"));
    let set = |lines: Option<&[&str]>| {
        pty.state().command_lines =
            lines.map(|lines| lines.iter().map(|line| line.to_string()).collect());
    };

    set(None);
    assert_eq!(agent_in(&f, &task.id, "1").as_deref(), Some("gemini"));
    // Just after the launch the agent may not be up yet.
    set(Some(&[]));
    assert_eq!(agent_in(&f, &task.id, "1").as_deref(), Some("gemini"));
    // Later, the processes tell: the agent ended and another program runs.
    f.advance(LAUNCH_GRACE_MS + 1);
    set(Some(&["vim notes.md"]));
    assert_eq!(agent_in(&f, &task.id, "1"), None);
    set(Some(&["codex"]));
    assert_eq!(agent_in(&f, &task.id, "1").as_deref(), Some("codex"));
}
