//! `describe('a resume after a restart')` of `stage-runner.test.ts`, without its nested groups
//! (`resume_daemon_start.rs`, `resume_command_ran.rs`). The helpers of the whole group are here.
//!
//! Node mocks `terminals.run` to make an agent fail to start; these tests make the spawn of the
//! shell fail instead (`Fixture::fail_spawn`), which gives the same error text.

use natamux_daemon::stages::act::StageActionInput;
use natamux_daemon::stages::launcher::GO_ON_PROMPT;
use natamux_proto::tasks::task::TaskState;
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::Fixture;
pub use crate::resume_helpers::*;

pub const OTHER_AGENTS: [&str; 4] = ["codex", "gemini", "copilot", "cursor"];

fn reopen_spec(f: &Fixture, id: &str) {
    let input = StageActionInput {
        to: Some("spec".into()),
        ..StageActionInput::default()
    };
    f.runner.run.act(id, StageAction::Reopen, &input).unwrap();
    f.settle();
}

#[test]
fn keeps_the_claude_code_session_of_a_stage_after_another_stage_started_its_own_agent() {
    let f = Fixture::new();
    with_own_agent(&f);
    let task = build_after_restart(&f);
    resume(&f, &task.id, json!({ "stage": "test-spec" }));
    assert!(f.launch_script(&task.id, "9").starts_with("codex "));
    f.terminals.close(&task.id);

    resume(&f, &task.id, json!({ "stage": "implement" }));

    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}

#[test]
fn keeps_the_claude_code_session_of_a_stage_after_an_agent_came_back_in_another_shell() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    f.tasks
        .record_agent_launch(&task.id, "copilot", Some("copilot"), None, Some("6"))
        .unwrap();

    resume(&f, &task.id, json!({}));

    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}

#[test]
fn goes_on_with_the_last_claude_code_session_of_the_stage_with_no_new_prompt() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    assert_eq!(task.state, TaskState::NeedsInput);

    resume(&f, &task.id, json!({}));

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(typed_launch(&f, &task.id));
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(!script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn picks_the_newest_session_of_the_stage() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    let payload = json!({ "stage": "spec", "sessionId": "s-2" });
    ev(&f, &task.id, "agent", "agent.resumed", payload.clone());
    ev(&f, &task.id, "agent", "agent.finished", payload);

    resume(&f, &task.id, json!({}));

    assert!(script_has(&f, &task.id, "1", "--resume 's-2'"));
}

#[test]
fn never_takes_a_session_of_another_task() {
    let f = Fixture::new();
    let other = waiting_after_restart(&f, json!({ "sessionId": "other" }), "claude-code");
    let task = waiting_after_restart(&f, json!({}), "claude-code");

    resume(&f, &task.id, json!({}));

    assert_ne!(other.id, task.id);
    assert!(!script_has(&f, &task.id, "1", "--resume"));
}

#[test]
fn goes_on_with_the_session_of_a_stage_that_waits_for_approval() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    let to = json!({ "to": "review", "stage": "spec" });
    ev(&f, &task.id, "human", "state.overridden", to);
    let now = f.task(&task.id);
    assert_eq!((now.stage.as_str(), now.state), ("spec", TaskState::Review));

    resume(&f, &task.id, json!({}));

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}

#[test]
fn starts_the_agent_with_the_stage_prompt_with_no_session_an_empty_one_or_one_that_is_not_text() {
    for payload in [
        json!({}),
        json!({ "sessionId": "" }),
        json!({ "sessionId": 42 }),
    ] {
        let f = Fixture::new();
        let task = waiting_after_restart(&f, payload, "claude-code");

        resume(&f, &task.id, json!({}));

        assert_eq!(f.task(&task.id).state, TaskState::Running);
        assert!(typed_launch(&f, &task.id));
        assert!(script_has(&f, &task.id, "1", "Task: Export tasks"));
        assert!(!script_has(&f, &task.id, "1", "--resume"));
    }
}

// Only Claude Code goes on with a session.
#[test]
fn starts_each_other_agent_with_the_stage_prompt() {
    for agent in OTHER_AGENTS {
        let f = Fixture::new();
        let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), agent);

        resume(&f, &task.id, json!({}));

        assert_eq!(f.task(&task.id).state, TaskState::Running, "{agent}");
        assert!(typed_launch(&f, &task.id), "{agent}");
        assert!(
            script_has(&f, &task.id, "1", "Task: Export tasks"),
            "{agent}"
        );
        assert!(!script_has(&f, &task.id, "1", "--resume"), "{agent}");
    }
}

// The Enter of a prompt would answer the menu of an agent that asked.
#[test]
fn types_nothing_when_the_agent_still_runs_and_asked() {
    for agent in ["claude-code", "codex"] {
        let f = Fixture::new();
        let id = f
            .created(json!({ "title": "Export tasks", "agent": agent }))
            .id;
        let task = f.with_workspace(&id);
        let payload = json!({ "stage": "spec", "sessionId": "s-1" });
        ev(&f, &task.id, "agent", "agent.asked", payload);
        let pty = last_pty(&f);
        pty.set_program(Some("node"));
        let written = pty.written().len();

        // Twice, as a quick double click sends it.
        resume(&f, &task.id, json!({}));
        resume(&f, &task.id, json!({}));

        assert_eq!(f.task(&task.id).state, TaskState::Running, "{agent}");
        assert_eq!(f.ptys().len(), 1, "{agent}");
        assert_eq!(pty.written().len(), written, "{agent}");
    }
}

#[test]
fn asks_the_agent_to_go_on_when_it_ended_its_turn() {
    for agent in ["claude-code", "codex"] {
        let f = Fixture::new();
        let id = f
            .created(json!({ "title": "Export tasks", "agent": agent }))
            .id;
        let task = f.with_workspace(&id);
        let payload = json!({ "stage": "spec", "sessionId": "s-1" });
        ev(&f, &task.id, "agent", "agent.finished", payload);
        let pty = last_pty(&f);
        pty.set_program(Some("node"));
        let written = pty.written().len();

        resume(&f, &task.id, json!({}));
        resume(&f, &task.id, json!({}));

        assert_eq!(f.task(&task.id).state, TaskState::Running, "{agent}");
        assert_eq!(f.ptys().len(), 1, "{agent}");
        let typed = pty.written()[written..].concat();
        assert_eq!(
            typed.split(&format!("\x1b[200~{GO_ON_PROMPT}")).count(),
            2,
            "{agent}"
        );
        assert!(!typed.contains("agent-launch-"), "{agent}");
    }
}

// The daemon started the agent again with its session; it is still at the end of that turn.
#[test]
fn asks_the_agent_to_go_on_after_a_restart_started_it_again() {
    let f = Fixture::new();
    let id = f
        .created(json!({ "title": "Export tasks", "agent": "claude-code" }))
        .id;
    let task = f.with_workspace(&id);
    let payload = json!({ "stage": "spec", "sessionId": "s-1" });
    ev(&f, &task.id, "agent", "agent.finished", payload);
    f.tasks
        .record_agent_launch(&task.id, "claude-code", Some("claude"), None, None)
        .unwrap();
    let pty = last_pty(&f);
    pty.set_program(Some("node"));
    let written = pty.written().len();

    resume(&f, &task.id, json!({}));

    let typed = pty.written()[written..].concat();
    assert!(typed.contains(&format!("\x1b[200~{GO_ON_PROMPT}")));
}

// Only what the agent did tells a turn end from a question; a CI result or an edit does not.
#[test]
fn asks_the_agent_to_go_on_after_an_event_that_is_not_from_the_agent() {
    let f = Fixture::new();
    let id = f
        .created(json!({ "title": "Export tasks", "agent": "claude-code" }))
        .id;
    let task = f.with_workspace(&id);
    let payload = json!({ "stage": "spec", "sessionId": "s-1" });
    ev(&f, &task.id, "agent", "agent.finished", payload);
    f.tasks
        .update_task(&task.id, Some(&json!({ "title": "Export all tasks" })))
        .unwrap();
    assert_eq!(last_event(&f, &task.id).1, "task.edited");
    let pty = last_pty(&f);
    pty.set_program(Some("node"));
    let written = pty.written().len();

    resume(&f, &task.id, json!({}));

    let typed = pty.written()[written..].concat();
    assert!(typed.contains(&format!("\x1b[200~{GO_ON_PROMPT}")));
}

#[test]
fn types_nothing_for_a_prompt_typed_to_an_agent_that_runs() {
    let f = Fixture::new();
    let id = f
        .created(json!({ "title": "Export tasks", "agent": "claude-code" }))
        .id;
    let task = f.with_workspace(&id);
    let payload = json!({ "stage": "spec", "sessionId": "s-1" });
    ev(&f, &task.id, "agent", "agent.finished", payload.clone());
    let pty = last_pty(&f);
    pty.set_program(Some("node"));
    let written = pty.written().len();

    ev(&f, &task.id, "agent", "agent.resumed", payload);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert_eq!(pty.written().len(), written);
}

#[test]
fn starts_no_agent_for_a_prompt_typed_to_the_agent() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");

    let payload = json!({ "stage": "spec", "sessionId": "s-1" });
    ev(&f, &task.id, "agent", "agent.resumed", payload);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(shells(&f, &task.id).is_empty());
}

#[test]
fn goes_on_with_the_session_of_the_stage_in_the_terminal_of_that_stage() {
    let f = Fixture::new();
    let task = build_after_restart(&f);

    resume(&f, &task.id, json!({ "stage": "test-spec" }));

    let lanes = json!({ "test-spec": "running", "implement": "needs-input" });
    assert_eq!(lanes_of(&f, &task.id), lanes);
    assert_eq!(shells(&f, &task.id), ["2"]);
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
}

#[test]
fn never_takes_the_session_of_another_stage_of_the_group() {
    let f = Fixture::new();
    let task = build_after_restart(&f);

    resume(&f, &task.id, json!({ "stage": "implement" }));

    assert_eq!(shells(&f, &task.id), ["1"]);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}

#[test]
fn leaves_the_agent_of_the_other_terminal_alone() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "build" }),
    );
    f.with_workspace(&task.id);
    let payload = json!({ "stage": "test-spec", "sessionId": "s-2" });
    ev(&f, &task.id, "agent", "agent.finished", payload);
    // The agent of terminal 1 runs on; the one of terminal 2 is gone.
    let running = f.ptys();
    for pty in &running {
        pty.set_program(Some("node"));
    }
    let written: Vec<usize> = running.iter().map(|pty| pty.written().len()).collect();
    f.terminals.close_one(&task.id, "2");

    resume(&f, &task.id, json!({ "stage": "test-spec" }));

    assert_eq!(shells(&f, &task.id), ["1", "2"]);
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
    let after: Vec<usize> = running.iter().map(|pty| pty.written().len()).collect();
    assert_eq!(after, written);
}

#[test]
fn starts_the_stage_prompt_on_a_new_start_of_the_stage_even_with_a_session() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");

    reopen_spec(&f, &task.id);

    assert!(typed_launch(&f, &task.id));
    assert!(script_has(&f, &task.id, "1", "Task: Export tasks"));
    assert!(!script_has(&f, &task.id, "1", "--resume"));
}

#[test]
fn waits_for_you_with_the_reason_when_the_agent_cannot_start() {
    for agent in ["claude-code", "codex"] {
        let f = Fixture::new();
        let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), agent);
        f.fail_spawn(|_, _| true);

        resume(&f, &task.id, json!({}));

        assert_eq!(f.task(&task.id).state, TaskState::NeedsInput, "{agent}");
        let (_, kind, payload) = last_event(&f, &task.id);
        assert_eq!(kind, "stage.check-failed", "{agent}");
        assert_eq!(payload["stage"], "spec", "{agent}");
        assert_eq!(payload["reasons"], json!(["No shell"]), "{agent}");
    }
}

#[test]
fn in_a_group_puts_only_the_resumed_stage_back_when_its_agent_cannot_start() {
    let f = Fixture::new();
    let task = build_after_restart(&f);
    f.fail_spawn(|_, _| true);

    resume(&f, &task.id, json!({ "stage": "test-spec" }));

    let lanes = json!({ "test-spec": "needs-input", "implement": "needs-input" });
    assert_eq!(lanes_of(&f, &task.id), lanes);
    let (_, kind, payload) = last_event(&f, &task.id);
    assert_eq!(kind, "stage.check-failed");
    assert_eq!(payload["stage"], "test-spec");
    assert_eq!(payload["reasons"], json!(["No shell"]));
}
