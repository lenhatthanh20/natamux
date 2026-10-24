//! The helpers of `describe('a resume after a restart')` and its nested groups.

use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Value, json};

use crate::{FakePty, Fixture};

pub fn state_of(name: &str) -> TaskState {
    serde_json::from_value(json!(name)).unwrap()
}

pub fn ev(f: &Fixture, id: &str, source: &str, kind: &str, payload: Value) {
    f.apply(
        id,
        json!({ "source": source, "type": kind, "payload": payload }),
    );
}

/// The lanes of a task as JSON, to compare with `json!({...})`.
pub fn lanes_of(f: &Fixture, id: &str) -> Value {
    Value::Object(f.task(id).lanes)
}

/// The shells of a task, in name order.
pub fn shells(f: &Fixture, id: &str) -> Vec<String> {
    let mut list = f.terminals.list(id);
    list.sort();
    list
}

pub fn event_count(f: &Fixture, id: &str) -> usize {
    f.tasks.list_events(id).unwrap().len()
}

pub fn event_types(f: &Fixture, id: &str) -> Vec<String> {
    f.events(id).into_iter().map(|(kind, _)| kind).collect()
}

/// The last event of a task: (accepted, type, payload).
pub fn last_event(f: &Fixture, id: &str) -> (bool, String, Value) {
    let stored = f.tasks.list_events(id).unwrap().pop().unwrap();
    let payload = Value::Object(stored.event.payload);
    (stored.accepted, stored.event.kind, payload)
}

pub fn check_failed_with_no_shell(f: &Fixture, id: &str, stage: &str) {
    let (_, kind, payload) = last_event(f, id);
    assert_eq!(kind, "stage.check-failed");
    assert_eq!(payload["stage"], stage);
    assert_eq!(payload["reasons"], json!(["No shell"]));
}

pub fn script_has(f: &Fixture, id: &str, terminal: &str, text: &str) -> bool {
    f.launch_script(id, terminal).contains(text)
}

/// The script resumes: neither `--continue` nor `--resume`.
pub fn starts_fresh(script: &str) -> bool {
    !script.contains("--continue") && !script.contains("--resume")
}

pub fn typed_launch(f: &Fixture, id: &str) -> bool {
    f.ptys().last().is_some_and(|pty| {
        pty.written()
            .concat()
            .contains(&format!("agent-launch-{id}.sh"))
    })
}

pub fn last_pty(f: &Fixture) -> FakePty {
    f.ptys().last().cloned().unwrap()
}

/// The spec agent ended its turn with this payload, then the app restarted: the agent and its
/// shell are gone, the task still waits for you.
pub fn waiting_after_restart(f: &Fixture, payload: Value, agent: &str) -> Task {
    let id = f
        .created(json!({ "title": "Export tasks", "agent": agent }))
        .id;
    let task = f.with_workspace(&id);
    let mut full = json!({ "stage": "spec" });
    if let (Some(base), Some(more)) = (full.as_object_mut(), payload.as_object()) {
        base.extend(
            more.iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
    }
    ev(f, &task.id, "agent", "agent.finished", full);
    f.terminals.close(&task.id);
    f.task(&task.id)
}

/// As the Resume button sends it: no stage.
pub fn resume(f: &Fixture, id: &str, payload: Value) {
    ev(f, id, "human", "agent.resumed", payload);
}

/// The build step, with the session each stage reported, after a restart.
pub fn build_after_restart(f: &Fixture) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    ev(
        f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "build" }),
    );
    f.with_workspace(&task.id);
    for (stage, session) in [("implement", "s-1"), ("test-spec", "s-2")] {
        let payload = json!({ "stage": stage, "sessionId": session });
        ev(f, &task.id, "agent", "agent.finished", payload);
    }
    f.terminals.close(&task.id);
    f.task(&task.id)
}

/// The default pipeline with a person's review in place of the agent's.
pub fn with_manual_review(f: &Fixture) {
    let mut settings = f.settings.lock().unwrap().clone();
    for step in &mut settings.stages {
        if step.id() == "agent-review" {
            *step = Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual));
        }
    }
    f.set_settings(settings);
}

/// A command runs the tests, not an agent.
pub fn with_test_command(f: &Fixture) {
    let mut settings = f.settings.lock().unwrap().clone();
    for step in &mut settings.stages {
        if step.id() == "testing" {
            let mut command = Stage::new("testing", StageKind::Testing, Gate::Auto);
            command.command = Some("make test".into());
            *step = Step::Stage(command);
        }
    }
    f.set_settings(settings);
}

/// Test spec starts Codex with its own words, beside Claude Code in Shell 1.
pub fn with_own_agent(f: &Fixture) {
    let mut settings = f.settings.lock().unwrap().clone();
    for step in &mut settings.stages {
        if let Step::Group(group) = step {
            for stage in &mut group.parallel {
                if stage.id == "test-spec" {
                    stage.agent = Some("codex".into());
                    stage.args = Some("--model gpt-5".into());
                }
            }
        }
    }
    f.set_settings(settings);
}
