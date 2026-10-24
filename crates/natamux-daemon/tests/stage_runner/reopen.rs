//! `describe('the agent of a finished task, reopened')` of `stage-runner.test.ts`.

use std::path::Path;
use std::sync::Arc;

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::stages::errors::StageError;
use natamux_daemon::tasks::service::is_task_ended;
use natamux_daemon::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use natamux_daemon::terminals::shell_files::{open_shells_path, read_ended_shells};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Value, json};

use crate::Fixture;
use crate::support::SIZE;

struct Reopening {
    f: Fixture,
    shells: Arc<OpenShells>,
}

impl Drop for Reopening {
    fn drop(&mut self) {
        self.shells.stop();
    }
}

fn setup() -> Reopening {
    let mut f = Fixture::new();
    f.runner.stop();
    let tasks = f.tasks.clone();
    let shells = OpenShells::new(OpenShellsOptions {
        data_dir: f.data_dir.clone(),
        terminals: Arc::new(f.terminals.clone()) as Arc<dyn ShellsView>,
        bus: Arc::clone(&f.bus),
        scheduler: Scheduler::real(),
        is_ended: Arc::new(move |id| is_task_ended(&tasks, id)),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    shells.start();
    f.runner = f.make_runner(Some(Arc::clone(&shells)));
    f.runner.start();
    Reopening { f, shells }
}

/// The spec agent works in Terminal 1.
fn spec_running(f: &Fixture) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.with_workspace(&task.id);
    let task = f.task(&task.id);
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Running)
    );
    task
}

fn end(f: &Fixture, id: &str, done: bool) -> Task {
    let event = if done {
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "done" } })
    } else {
        json!({ "source": "human", "type": "task.cancelled" })
    };
    f.apply(id, event);
    f.task(id)
}

/// The spec agent reported `session` in Terminal 1, then the task ended.
fn ended_with(r: &Reopening, session: &str, done: bool) -> Task {
    let task = spec_running(&r.f);
    r.shells.note_session(&task.id, "1", "claude-code", session);
    end(&r.f, &task.id, done)
}

fn last_launch(f: &Fixture, id: &str) -> Value {
    let launches = f.events(id);
    let (_, payload) = launches
        .iter()
        .rfind(|(kind, _)| kind == "agent.launched")
        .unwrap();
    payload.clone()
}

fn reopen(f: &Fixture, id: &str) -> Result<Task, StageError> {
    f.runner.run.reopen_agent(id)
}

fn script_has(f: &Fixture, id: &str, text: &str) -> bool {
    f.launch_script(id, "1").contains(text)
}

#[test]
fn keeps_the_session_of_the_agent_in_terminal_1_when_the_task_ends() {
    let r = setup();
    let task = ended_with(&r, "s-9", false);

    let shell = &read_ended_shells(&r.f.data_dir, &task.id)[0];
    assert_eq!(shell.name, "1");
    assert_eq!(shell.agent.as_deref(), Some("claude-code"));
    assert_eq!(shell.stage.as_deref(), Some("spec"));
    assert_eq!(shell.session.as_deref(), Some("s-9"));
}

#[test]
fn starts_the_agent_again_in_terminal_1_on_its_session_and_the_task_stays_cancelled() {
    let r = setup();
    let f = &r.f;
    let task = ended_with(&r, "s-9", false);
    assert!(f.terminals.list(&task.id).is_empty());
    let before = f.ptys().len();

    let reopened = reopen(f, &task.id).unwrap();
    f.settle();

    assert_eq!(reopened.state, TaskState::Cancelled);
    assert_eq!(reopened.agent.as_deref(), Some("claude-code"));
    let kept = f.task(&task.id);
    assert_eq!(kept.state, TaskState::Cancelled);
    assert_eq!(kept.agent.as_deref(), Some("claude-code"));
    assert_eq!(f.ptys().len(), before + 1);
    assert_eq!(f.terminals.list(&task.id), ["1"]);
    assert!(script_has(f, &task.id, "--resume 's-9'"));
    assert!(!script_has(f, &task.id, "Export tasks"));
    let launch = last_launch(f, &task.id);
    assert_eq!(launch["agent"], "claude-code");
    assert_eq!(launch["terminal"], "1");
    assert_eq!(launch["restored"], true);
    let restoring = r.shells.restoring(&task.id);
    assert_eq!(
        restoring,
        [("1".to_string(), Some("claude-code".to_string()))]
    );
    assert!(!open_shells_path(&f.data_dir, &task.id).exists());
}

#[test]
fn keeps_a_done_task_done() {
    let r = setup();
    let task = ended_with(&r, "s-9", true);
    assert_eq!(task.state, TaskState::Done);

    assert_eq!(reopen(&r.f, &task.id).unwrap().state, TaskState::Done);
    assert!(script_has(&r.f, &task.id, "--resume 's-9'"));
}

#[test]
fn goes_on_with_the_newest_session_of_the_folder_when_none_is_known() {
    let r = setup();
    let running = spec_running(&r.f);
    let task = end(&r.f, &running.id, false);
    assert_eq!(read_ended_shells(&r.f.data_dir, &task.id)[0].session, None);

    reopen(&r.f, &task.id).unwrap();

    assert!(script_has(&r.f, &task.id, "--continue"));
    assert!(!script_has(&r.f, &task.id, "Export tasks"));
}

#[test]
fn reopens_the_task_agent_on_its_last_session_of_the_stage_when_no_screen_was_kept() {
    let r = setup();
    let f = &r.f;
    let running = spec_running(f);
    f.tasks
        .apply_event(
            &running.id,
            Some(&json!({
                "source": "agent",
                "type": "agent.finished",
                "payload": { "stage": "spec", "sessionId": "s-4" },
            })),
        )
        .unwrap();
    // As after a restart: no shell was open when the task was cancelled.
    f.terminals.close(&running.id);
    let task = end(f, &running.id, false);
    assert!(read_ended_shells(&f.data_dir, &task.id).is_empty());

    reopen(f, &task.id).unwrap();

    assert!(script_has(f, &task.id, "--resume 's-4'"));
}

#[test]
fn keeps_the_newer_session_of_the_reopened_agent_for_the_next_reopen() {
    let r = setup();
    let f = &r.f;
    let task = ended_with(&r, "s-9", false);
    reopen(f, &task.id).unwrap();

    r.shells.note_session(&task.id, "1", "claude-code", "s-10");
    let shells = read_ended_shells(&f.data_dir, &task.id);
    assert_eq!(shells[0].session.as_deref(), Some("s-10"));
    assert!(!open_shells_path(&f.data_dir, &task.id).exists());

    // The agent quit and its terminal closed.
    f.terminals.close(&task.id);
    reopen(f, &task.id).unwrap();
    assert!(script_has(f, &task.id, "--resume 's-10'"));
}

#[test]
fn types_the_agent_into_a_plain_terminal_1_that_is_open_already() {
    let r = setup();
    let f = &r.f;
    let task = ended_with(&r, "s-9", false);
    // Open a new terminal: the dashboard attached a plain Terminal 1.
    f.terminals.run(&task.id, "ls", "1", SIZE).unwrap();
    let plain = f.ptys().pop().unwrap();
    let before = f.ptys().len();

    reopen(f, &task.id).unwrap();

    assert_eq!(f.ptys().len(), before);
    assert_eq!(f.terminals.list(&task.id), ["1"]);
    let typed = plain.written().concat();
    assert!(typed.contains(&format!("agent-launch-{}.sh", task.id)));
}

fn refused(result: Result<Task, StageError>) -> bool {
    matches!(result, Err(StageError::Action(_)))
}

#[test]
fn refuses_a_task_that_is_not_finished_and_terminal_1_where_an_agent_runs() {
    let r = setup();
    let f = &r.f;
    let running = spec_running(f);
    assert!(refused(reopen(f, &running.id)));

    let task = end(f, &running.id, false);
    reopen(f, &task.id).unwrap();
    let before = f.ptys().len();
    let written = f.ptys().pop().unwrap().written().len();

    assert!(refused(reopen(f, &task.id)));
    assert_eq!(f.ptys().len(), before);
    assert_eq!(f.ptys().pop().unwrap().written().len(), written);
}

#[test]
fn refuses_a_task_with_no_workspace_or_no_agent() {
    let r = setup();
    let f = &r.f;
    let bare = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    end(f, &bare.id, false);
    assert!(refused(reopen(f, &bare.id)));

    // Held at the context gate, so no agent ever started.
    let task = f.created(json!({ "title": "Import tasks" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "context" } }),
    );
    f.with_workspace(&task.id);
    assert_eq!(end(f, &task.id, false).agent, None);
    let before = f.ptys().len();

    assert!(refused(reopen(f, &task.id)));
    assert_eq!(f.ptys().len(), before);
}

#[test]
fn refuses_a_task_that_was_deleted_and_keeps_no_file_of_it() {
    let r = setup();
    let f = &r.f;
    let task = ended_with(&r, "s-9", false);
    f.remove_workspace(&task.id);
    f.tasks.delete_task(&task.id).unwrap();
    f.settle();

    assert!(reopen(f, &task.id).is_err());
    let file = Path::new(&f.data_dir)
        .join("tasks")
        .join(format!("{}.shells.json", task.id));
    assert!(!file.exists());
}
