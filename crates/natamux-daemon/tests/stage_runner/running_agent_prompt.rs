//! `describe('a prompt for a running agent')` of `stage-runner.test.ts`, with its nested
//! `describe('on a stage whose agent failed')`.

use std::sync::Arc;

use natamux_agents::hook_files::WorkspaceHooks;
use natamux_agents::trust::trust_paths;
use natamux_daemon::agents::runner::{AgentRunner, AgentRunnerOptions};
use natamux_daemon::stages::launcher::GO_ON_PROMPT;
use natamux_daemon::stages::run::StageRunOptions;
use natamux_daemon::stages::runner::StageRunner;
use natamux_daemon::stages::shells::LAUNCH_GRACE_MS;
use natamux_daemon::terminals::shell_files::read_ended_shells;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

use crate::context::act_to;
use crate::support::SIZE;
use crate::{FakePty, Fixture, expect_submitted, wait_for};

/// Spec runs in Shell 1 where `program` is in the foreground.
fn at_spec_with_shell(f: &Fixture, program: &str) -> (Task, FakePty) {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    // Held at the context gate, so the spec stage starts only when the shell is set up.
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "context" } }),
    );
    f.with_workspace(&task.id);
    // The dashboard opened the first shell; the program in it is up to the test.
    f.terminals.run(&task.id, "echo ready", "1", SIZE).unwrap();
    let pty = f.ptys().pop().unwrap();
    pty.set_program(Some(program));
    act_to(f, &task.id, StageAction::Approve, None, None);
    (f.task(&task.id), pty)
}

fn typed(pty: &FakePty) -> String {
    pty.written().concat()
}

#[test]
fn starts_an_agent_when_only_a_shell_runs_and_never_pastes_the_prompt_into_it() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "bash");
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Running)
    );
    let written = typed(&pty);
    assert!(written.contains(&format!("agent-launch-{}.sh", task.id)));
    assert!(!written.contains("\x1b[200~"));
}

#[test]
fn pastes_the_prompt_when_an_agent_runs_in_the_foreground() {
    let f = Fixture::new();
    let (_, pty) = at_spec_with_shell(&f, "node");
    assert!(typed(&pty).contains("\x1b[200~Task: Export tasks"));
    expect_submitted(&pty, "Task: Export tasks");
}

#[test]
fn starts_the_agent_again_when_it_ended_and_only_the_shell_is_left() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "bash");
    f.advance(LAUNCH_GRACE_MS + 1);

    act_to(&f, &task.id, StageAction::Reopen, Some("spec"), None);

    let written = typed(&pty);
    assert_eq!(
        written
            .matches(&format!("agent-launch-{}.sh", task.id))
            .count(),
        2
    );
    assert!(!written.contains("\x1b[200~"));
}

/// A runner and agent runner on the Windows platform, in place of the fixture's.
fn windows_runner(f: &mut Fixture) {
    f.runner.stop();
    let modes = Arc::clone(&f.settings);
    let home = f.home.clone();
    let agents = Arc::new(AgentRunner::new(AgentRunnerOptions {
        tasks: f.tasks.clone(),
        terminals: f.terminals.clone(),
        claude_settings_path: f.data_dir.join("settings.json"),
        gemini_settings_path: None,
        workspace_hooks: WorkspaceHooks::default(),
        programs: Default::default(),
        extra_args: Default::default(),
        windows: true,
        launch_dir: None,
        bin_dir: None,
        on_launch: None,
        trust_paths: trust_paths(|_| None, &home),
        permission_mode_of: Arc::new(move |_| modes.lock().unwrap().agents.permission_mode.clone()),
        trusted_of: Arc::new(|_| true),
    }));
    let repo = f.repo.to_string_lossy().into_owned();
    let settings = Arc::clone(&f.settings);
    let mut options = StageRunOptions::new(
        f.tasks.clone(),
        f.terminals.clone(),
        agents,
        Arc::clone(&f.bus),
        f.data_dir.clone(),
        Arc::new(move |_| Ok(repo.clone())),
        Arc::new(move |_| settings.lock().unwrap().clone()),
    );
    let clock = Arc::clone(&f.clock);
    options.now = Arc::new(move || clock.load(Ordering::SeqCst));
    options.windows = true;
    options.home = f.home.clone();
    options.jobs = Arc::clone(&f.jobs);
    f.runner = StageRunner::new(options);
    f.runner.start();
}

#[test]
fn on_windows_starts_the_agent_again_only_once_the_shell_says_the_agent_ended() {
    let mut f = Fixture::new();
    windows_runner(&mut f);
    let exited = "$Host.UI.RawUI.WindowTitle = 'natamux-agent-exited'";
    let (task, pty) = at_spec_with_shell(&f, "powershell.exe");
    f.advance(LAUNCH_GRACE_MS + 1);
    assert_eq!(typed(&pty).matches(exited).count(), 1);

    // The shell echoes the typed line, which is no sign that the agent ended.
    f.runner.run.on_output(&task.id, "1", exited.as_bytes());
    act_to(&f, &task.id, StageAction::Reopen, Some("spec"), None);
    assert!(typed(&pty).contains("\x1b[200~"));

    f.runner.run.on_output(&task.id, "1", b"\x1b]0;natamux-ag");
    f.runner
        .run
        .on_output(&task.id, "1", "ent-exited\x07PS C:\\work> ".as_bytes());
    act_to(&f, &task.id, StageAction::Reopen, Some("spec"), None);
    // The launch line waits for the Enter of the prompt pasted before.
    wait_for(|| typed(&pty).matches(exited).count() == 2);
}

#[test]
fn makes_a_failed_check_final_when_no_agent_is_left_to_fix_it() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "bash");
    f.advance(LAUNCH_GRACE_MS + 1);

    let task_id = task.id.clone();
    f.runner
        .run
        .act(&task_id, StageAction::Report, &Default::default())
        .unwrap();

    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    assert!(!typed(&pty).contains("The checks of this stage failed"));
}

#[test]
fn starts_one_agent_when_a_person_starts_it_while_the_new_workspace_starts_the_stage() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks" }));
    // `with_workspace` waits for the stage start, so the person's start comes after it.
    f.with_workspace(&task.id);

    let after = f
        .runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.settle();

    assert_eq!(
        (after.stage.as_str(), after.state),
        ("spec", TaskState::Running)
    );
    assert_eq!(f.ptys().len(), 1);
    assert_eq!(
        typed(&f.pty(0))
            .matches(&format!("agent-launch-{}.sh", task.id))
            .count(),
        1
    );
}

#[test]
fn makes_the_missing_workspace_and_starts_the_agent_the_person_picked_in_it() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    let made = std::cell::Cell::new(0);
    let make = || {
        made.set(made.get() + 1);
        f.with_workspace(&task.id);
        Ok(())
    };

    let after = f
        .runner
        .run
        .start_by_hand(&task.id, "codex", None, Some(&make))
        .unwrap();
    f.settle();

    assert_eq!(
        (after.stage.as_str(), after.state, after.agent.as_deref()),
        ("spec", TaskState::Running, Some("codex"))
    );
    assert!(after.workspace.is_some());
    assert_eq!(f.ptys().len(), 1);
    assert_eq!(
        typed(&f.pty(0))
            .matches(&format!("agent-launch-{}.sh", task.id))
            .count(),
        1
    );
    f.runner
        .run
        .start_by_hand(&task.id, "codex", Some("Also add a test"), Some(&make))
        .unwrap();
    assert_eq!(made.get(), 1);
}

#[test]
fn gives_a_prompt_to_the_agent_that_runs_rather_than_starting_another() {
    let f = Fixture::new();
    let made = f.created(json!({ "title": "Export tasks" }));
    let task = f.with_workspace(&made.id);
    let pty = f.pty(0);
    pty.set_program(Some("node"));

    f.runner
        .run
        .start_by_hand(&task.id, "claude-code", Some("Also add a test"), None)
        .unwrap();

    assert_eq!(f.ptys().len(), 1);
    assert!(typed(&pty).contains("\x1b[200~Also add a test"));
    expect_submitted(&pty, "Also add a test");
}

/// An API error ended the turn of the spec agent, as StopFailure reports it.
fn failed_spec(f: &Fixture) -> Task {
    let made = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    let task = f.with_workspace(&made.id);
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.failed", "payload": { "stage": "spec", "sessionId": "s-1" } }),
    );
    assert_eq!(f.task(&task.id).state, TaskState::Failed);
    f.task(&task.id)
}

/// The events Start agent logged after the failure: one resume per failed stage, by a person.
/// The record of a launch is left out: it only notes that an agent started again.
fn logged_after_failure(f: &Fixture, id: &str) -> Vec<(bool, String, Value)> {
    let events = f.tasks.list_events(id).unwrap();
    let failed = events
        .iter()
        .rposition(|stored| stored.event.kind == "agent.failed")
        .unwrap();
    events[failed + 1..]
        .iter()
        .filter(|stored| stored.event.source == "human" && stored.event.kind != "agent.launched")
        .map(|stored| {
            let stage = stored
                .event
                .payload
                .get("stage")
                .cloned()
                .unwrap_or(Value::Null);
            (stored.accepted, stored.event.kind.clone(), stage)
        })
        .collect()
}

fn resumed(stage: &str) -> Vec<(bool, String, Value)> {
    vec![(true, "agent.resumed".into(), json!(stage))]
}

#[test]
fn asks_the_agent_that_still_runs_to_go_on() {
    let f = Fixture::new();
    let task = failed_spec(&f);
    let pty = f.ptys().pop().unwrap();
    pty.set_program(Some("node"));
    let written = pty.written().len();

    let after = f
        .runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.settle();

    assert_eq!(
        (after.stage.as_str(), after.state),
        ("spec", TaskState::Running)
    );
    assert_eq!(logged_after_failure(&f, &task.id), resumed("spec"));
    assert_eq!(f.ptys().len(), 1);
    let text = pty.written()[written..].concat();
    assert_eq!(text.matches(&format!("\x1b[200~{GO_ON_PROMPT}")).count(), 1);
    assert!(!text.contains("agent-launch-"));
}

// A quick double click sends it twice.
#[test]
fn asks_the_agent_to_go_on_once_when_a_person_starts_it_twice() {
    let f = Fixture::new();
    let task = failed_spec(&f);
    let pty = f.ptys().pop().unwrap();
    pty.set_program(Some("node"));
    let written = pty.written().len();

    f.runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.settle();

    assert_eq!(logged_after_failure(&f, &task.id), resumed("spec"));
    let text = pty.written()[written..].concat();
    assert_eq!(text.matches(&format!("\x1b[200~{GO_ON_PROMPT}")).count(), 1);
    assert_eq!(f.ptys().len(), 1);
}

#[test]
fn starts_the_agent_again_on_the_session_of_the_stage_when_none_runs() {
    let f = Fixture::new();
    let task = failed_spec(&f);
    f.terminals.close(&task.id);

    let after = f
        .runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.settle();

    assert_eq!(
        (after.stage.as_str(), after.state),
        ("spec", TaskState::Running)
    );
    assert_eq!(logged_after_failure(&f, &task.id), resumed("spec"));
    assert_eq!(f.terminals.list(&task.id), ["1"]);
    assert!(f.launch_script(&task.id, "1").contains("--resume 's-1'"));
}

// As Resume does: the stage goes on where it stopped, with its own agent.
#[test]
fn goes_on_with_the_agent_of_the_stage_whichever_agent_the_person_picked() {
    let f = Fixture::new();
    let task = failed_spec(&f);
    f.terminals.close(&task.id);

    f.runner
        .run
        .start_by_hand(&task.id, "codex", None, None)
        .unwrap();
    f.settle();

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(f.launch_script(&task.id, "1").contains("--resume 's-1'"));
}

#[test]
fn starts_only_the_failed_stage_of_a_group_in_its_own_terminal() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    f.with_workspace(&task.id);
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.finished", "payload": { "stage": "test-spec", "sessionId": "s-2" } }),
    );
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.failed", "payload": { "stage": "implement", "sessionId": "s-1" } }),
    );
    f.terminals.close(&task.id);

    f.runner
        .run
        .start_by_hand(&task.id, "claude-code", None, None)
        .unwrap();
    f.settle();

    assert_eq!(logged_after_failure(&f, &task.id), resumed("implement"));
    let lanes = Value::Object(f.task(&task.id).lanes);
    assert_eq!(
        lanes,
        json!({ "test-spec": "needs-input", "implement": "running" })
    );
    assert_eq!(f.terminals.list(&task.id), ["1"]);
    assert!(f.launch_script(&task.id, "1").contains("--resume 's-1'"));
}

#[test]
fn ends_the_shells_of_a_task_that_is_cancelled() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "node");

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );

    assert!(pty.killed());
    wait_for(|| f.terminals.list(&task.id).is_empty());
}

#[test]
fn keeps_the_last_screen_of_each_shell_of_a_task_that_is_cancelled() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "node");
    // The end of the screen is kept; the prompt pasted into shell 1 may be longer than that.
    pty.emit("last line of shell 1\r\n");
    f.terminals.settled(&task.id, "1");
    f.terminals.run(&task.id, "ls", "2", SIZE).unwrap();
    // The fake pty echoes what was typed only when told to.
    f.ptys().pop().unwrap().deliver();
    f.terminals.settled(&task.id, "2");

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );

    let shells = read_ended_shells(&f.data_dir, &task.id);
    let at = "2026-10-09T09:00:00.000Z";
    // Shell 1 ran the spec stage last; no stage used shell 2.
    let named: Vec<_> = shells
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.agent.as_deref(),
                s.stage.as_deref(),
                s.session.as_deref(),
                s.ended_at.as_str(),
            )
        })
        .collect();
    assert_eq!(
        named,
        [
            ("1", None, Some("spec"), None, at),
            ("2", None, None, None, at)
        ]
    );
    assert!(shells[0].text.ends_with("last line of shell 1"));
    assert!(shells[1].text.contains("ls"));
}

#[test]
fn keeps_a_shell_where_a_program_that_is_no_agent_runs_such_as_a_dev_server() {
    let f = Fixture::new();
    let (task, pty) = at_spec_with_shell(&f, "node");
    pty.state().command_lines = Some(vec![
        "node /work/node_modules/.bin/claude --settings x.json".into(),
    ]);
    f.terminals.run(&task.id, "pnpm dev", "2", SIZE).unwrap();
    let server = f.ptys().pop().unwrap();
    server.set_program(Some("node"));
    server.state().command_lines = Some(vec!["node /work/node_modules/.bin/vite".into()]);

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );

    assert!(pty.killed());
    assert!(!server.killed());
    assert_eq!(f.terminals.list(&task.id), ["2"]);
    let agents: Vec<Option<String>> = read_ended_shells(&f.data_dir, &task.id)
        .into_iter()
        .map(|shell| shell.agent)
        .collect();
    assert_eq!(agents, [Some("claude-code".to_string()), None]);
}
