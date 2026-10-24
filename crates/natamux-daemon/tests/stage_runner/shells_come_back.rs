//! `describe('shells that come back after a restart')` of `stage-runner.test.ts`.
//!
//! Node mocks `agents.start` to make an agent fail to start; these tests make the spawn of the
//! shell fail instead. Node also counts the `console.warn` of such a failure; Rust logs it with
//! `eprintln!`, which a test cannot count.

use std::sync::Arc;

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::tasks::service::is_task_ended;
use natamux_daemon::terminals::open_shells::{OpenShells, OpenShellsOptions};
use natamux_daemon::terminals::shell_files::{
    OpenShell, open_shells_path, read_open_shells, save_open_shells,
};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::json;

use crate::Fixture;
use crate::resume_restart::{ev, script_has};

fn shell(name: &str, agent: Option<&str>, session: Option<&str>) -> OpenShell {
    OpenShell {
        name: name.into(),
        agent: agent.map(String::from),
        session: session.map(String::from),
    }
}

fn shells_of_t0075() -> Vec<OpenShell> {
    vec![
        shell("1", Some("claude-code"), Some("s-1")),
        shell("6", Some("copilot"), Some("c-1")),
        shell("7", None, None),
    ]
}

/// The daemon starts again, with this list of the task's shells. The old shells are gone.
fn restart(f: &mut Fixture, task_id: &str, list: &[OpenShell]) -> Arc<OpenShells> {
    f.terminals.close(task_id);
    f.runner.stop();
    save_open_shells(&f.data_dir, task_id, list).unwrap();
    let tasks = f.tasks.clone();
    let shells = OpenShells::new(OpenShellsOptions {
        data_dir: f.data_dir.clone(),
        terminals: Arc::new(f.terminals.clone()),
        bus: Arc::clone(&f.bus),
        scheduler: Scheduler::manual(),
        is_ended: Arc::new(move |id| is_task_ended(&tasks, id)),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    shells.start();
    f.runner = f.make_runner(Some(Arc::clone(&shells)));
    f.runner.start();
    f.runner.run.load_open_shells();
    shells
}

/// The spec runs in Shell 1.
fn spec_running(f: &Fixture, agent: &str) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": agent }));
    f.with_workspace(&task.id);
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("spec", TaskState::Running)
    );
    now
}

/// What is coming back: (shell, agent), in name order.
fn restoring(open: &OpenShells, id: &str) -> Vec<(String, Option<String>)> {
    let mut list = open.restoring(id);
    list.sort();
    list
}

fn pair(name: &str, agent: Option<&str>) -> (String, Option<String>) {
    (name.to_string(), agent.map(String::from))
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list
}

fn resume_agents(f: &Fixture) {
    f.runner.run.resume_agents();
    f.settle();
}

#[test]
fn lets_every_shell_wait_for_a_dashboard_except_the_shell_of_a_working_stage() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");

    let open = restart(&mut f, &task.id, &shells_of_t0075());

    assert_eq!(open.waiting(&task.id), ["6", "7"]);
    assert_eq!(
        restoring(&open, &task.id),
        [pair("6", Some("copilot")), pair("7", None)]
    );
}

#[test]
fn brings_an_agent_back_on_its_session_once_and_keeps_the_task_agent() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let open = restart(&mut f, &task.id, &shells_of_t0075());
    let before = f.ptys().len();

    f.runner.run.restore_task(&task.id, "6");
    f.runner.run.restore_task(&task.id, "6");

    assert_eq!(f.ptys().len(), before + 1);
    let script = f.launch_script(&task.id, "6");
    assert!(script.starts_with("copilot "));
    assert!(script.contains("--resume 'c-1'"));
    assert!(!script.contains("Export tasks"));
    assert_eq!(f.task(&task.id).agent.as_deref(), Some("claude-code"));
    assert_eq!(
        restoring(&open, &task.id),
        [pair("6", Some("copilot")), pair("7", None)]
    );
    assert!(f.runner.run.agent_runs(&task.id, "6"));
}

#[test]
fn brings_back_every_agent_of_the_task_when_any_shell_of_it_is_attached() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let list = [
        shell("6", Some("copilot"), Some("c-6")),
        shell("7", None, None),
        shell("8", Some("codex"), Some("x-8")),
    ];
    let open = restart(&mut f, &task.id, &list);

    // The agents grid attaches Shell 1 only.
    f.runner.run.restore_task(&task.id, "1");

    assert!(script_has(&f, &task.id, "6", "--resume 'c-6'"));
    let codex = f.launch_script(&task.id, "8");
    assert!(codex.starts_with("codex resume ") && codex.contains("'x-8'"));
    // A plain shell starts with its own attach.
    assert!(!f.terminals.list(&task.id).contains(&"7".to_string()));
    assert_eq!(open.waiting(&task.id), ["7"]);
}

#[test]
fn brings_back_an_agent_with_no_session_on_the_newest_session_of_the_folder() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    restart(&mut f, &task.id, &[shell("6", Some("copilot"), None)]);

    f.runner.run.restore_task(&task.id, "6");

    let script = f.launch_script(&task.id, "6");
    assert!(script.starts_with("copilot ") && script.contains("--continue"));
    assert!(!script.contains("Export tasks"));
}

#[test]
fn leaves_the_shell_to_the_attach_when_its_agent_cannot_start() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let open = restart(&mut f, &task.id, &shells_of_t0075());
    f.fail_spawn(|_, _| true);

    f.runner.run.restore_task(&task.id, "6");

    assert_eq!(restoring(&open, &task.id), [pair("7", None)]);
}

#[test]
fn lets_a_shell_that_was_not_attached_wait_again_when_its_agent_cannot_start() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let open = restart(&mut f, &task.id, &shells_of_t0075());
    f.fail_spawn(|_, _| true);

    f.runner.run.restore_task(&task.id, "7");

    assert_eq!(
        restoring(&open, &task.id),
        [pair("6", Some("copilot")), pair("7", None)]
    );
}

#[test]
fn keeps_no_shells_of_a_task_without_a_workspace() {
    let mut f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));

    let open = restart(&mut f, &task.id, &shells_of_t0075());

    assert!(open.waiting(&task.id).is_empty());
    assert!(!open_shells_path(&f.data_dir, &task.id).exists());
}

#[test]
fn forgets_the_shells_of_a_task_that_ends() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    restart(&mut f, &task.id, &shells_of_t0075());

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );

    assert!(!open_shells_path(&f.data_dir, &task.id).exists());
}

#[test]
fn shows_the_agent_of_a_working_stage_as_coming_back_while_the_daemon_starts_it() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let open = restart(&mut f, &task.id, &shells_of_t0075());

    resume_agents(&f);

    // The events have no session of the stage; the shell's list has the last one.
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    let expected = [
        pair("1", Some("claude-code")),
        pair("6", Some("copilot")),
        pair("7", None),
    ];
    assert_eq!(restoring(&open, &task.id), expected);
}

#[test]
fn goes_on_with_the_session_of_the_shell_for_an_agent_other_than_claude_code() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "codex");
    restart(&mut f, &task.id, &[shell("1", Some("codex"), Some("x-1"))]);

    resume_agents(&f);

    let script = f.launch_script(&task.id, "1");
    assert!(script.starts_with("codex resume ") && script.contains("'x-1'"));
    assert!(!script.contains("Export tasks"));
}

#[test]
fn takes_the_session_of_the_shell_over_an_older_one_in_the_events() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let payload = json!({ "stage": "spec", "sessionId": "old" });
    ev(&f, &task.id, "agent", "agent.finished", payload);
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    restart(
        &mut f,
        &task.id,
        &[shell("1", Some("claude-code"), Some("new"))],
    );

    resume_agents(&f);

    assert!(script_has(&f, &task.id, "1", "--resume 'new'"));
}

#[test]
fn leaves_the_agent_of_a_working_stage_quit_when_the_person_quit_it() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    restart(&mut f, &task.id, &[shell("1", None, None)]);
    let before = f.ptys().len();

    resume_agents(&f);

    assert_eq!(f.ptys().len(), before);
    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn lets_the_shell_of_a_stage_wait_when_its_agent_cannot_start_again() {
    let mut f = Fixture::new();
    let task = spec_running(&f, "claude-code");
    let open = restart(&mut f, &task.id, &shells_of_t0075());
    f.fail_spawn(|_, _| true);

    resume_agents(&f);

    assert_eq!(sorted(open.waiting(&task.id)), ["1", "6", "7"]);
    assert!(read_open_shells(&f.data_dir, &task.id).contains(&shells_of_t0075()[0]));
}
