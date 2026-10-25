//! The group 'a turn that ends with background work'.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

fn background() -> Value {
    json!({
        "background_tasks": [
            { "id": "t1", "type": "shell", "status": "running", "command": "pnpm test" }
        ]
    })
}

fn stop_with(extra: Value) -> Value {
    hook("Stop", extra)
}

fn crons() -> Value {
    json!({ "session_crons": [{ "id": "c1", "prompt": "check CI" }] })
}

fn running_task(fx: &Fx, service: &HookService) -> Task {
    let task = fx.make("Fix it");
    step(service, &task.id, "UserPromptSubmit");
    task
}

fn edit(file: &str) -> Value {
    hook(
        "PreToolUse",
        json!({ "tool_name": "Edit", "tool_input": { "file_path": file } }),
    )
}

/// A lookup that reports a turn when `reported` says so.
fn reporting(reported: impl Fn(&str) -> bool + Send + Sync + 'static) -> Arc<dyn StageLookup> {
    mock(
        |task, _| own_lane(task),
        move |_, terminal| reported(terminal),
    )
}

#[test]
fn keeps_the_task_running_and_says_what_the_agent_waits_for() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);

    let result = claude(&service, &task.id, &stop_with(background()), None);

    assert_eq!(result.applied, ["agent.waiting"]);
    assert_eq!(fx.state(&task.id), TaskState::Running);
    assert!(fx.task(&task.id).background_since.is_some());
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Waiting for Bash: pnpm test".into())
    );
    let payload = fx.last_payload(&task.id);
    assert_eq!(payload["waitingFor"], json!(["Bash: pnpm test"]));
    assert_eq!(payload["stage"], "implement");
}

#[test]
fn goes_on_as_before_when_the_agent_comes_back_and_then_really_stops() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    claude(&service, &task.id, &stop_with(background()), None);

    let tool = hook(
        "PreToolUse",
        json!({ "tool_name": "Bash", "tool_input": { "command": "gh pr checks" } }),
    );
    claude(&service, &task.id, &tool, None);
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Bash: gh pr checks".into())
    );

    assert_eq!(step(&service, &task.id, "Stop").applied, ["agent.finished"]);
    assert_eq!(fx.state(&task.id), TaskState::NeedsInput);
    assert!(fx.task(&task.id).background_since.is_none());
    assert!(service.activity().is_empty());
}

#[test]
fn takes_a_task_that_waits_for_the_user_back_to_running() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    step(&service, &task.id, "PermissionRequest");

    let result = claude(&service, &task.id, &stop_with(background()), None);

    assert_eq!(result.applied, ["agent.resumed", "agent.waiting"]);
    assert_eq!(fx.state(&task.id), TaskState::Running);
    assert!(fx.task(&task.id).background_since.is_some());
}

#[test]
fn still_drops_the_turn_end_that_follows_a_report() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    let service = fx.service_with(reporting(|_| true), fx.options(false));
    step(&service, &task.id, "UserPromptSubmit");

    let result = claude(&service, &task.id, &stop_with(background()), None);

    assert!(result.applied.is_empty());
    assert!(!fx.kinds(&task.id).contains(&"agent.waiting".to_string()));
    assert!(fx.task(&task.id).background_since.is_none());
    assert!(service.activity().is_empty());
}

#[test]
fn forgets_only_the_line_of_the_terminal_that_reported() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some(&format!("/work/{}", task.id)));
    let service = fx.service_with(reporting(|terminal| terminal == "2"), fx.options(false));
    claude(&service, &task.id, &edit("src/a.ts"), Some("1"));
    claude(&service, &task.id, &edit("test-spec.md"), Some("2"));

    claude(&service, &task.id, &stop_with(background()), Some("2"));

    assert_eq!(line_terminals(&service, &task.id), ["1"]);
}

#[test]
fn logs_each_paused_turn_and_shows_the_newest_list_of_work() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    claude(&service, &task.id, &stop_with(background()), None);

    let next = claude(&service, &task.id, &stop_with(crons()), None);

    assert_eq!(next.applied, ["agent.waiting"]);
    let kinds = fx.kinds(&task.id);
    assert_eq!(kinds[kinds.len() - 2..], ["agent.waiting", "agent.waiting"]);
    assert_eq!(fx.state(&task.id), TaskState::Running);
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Waiting for scheduled prompt: check CI".into())
    );
}

#[test]
fn takes_a_task_that_waits_for_the_user_back_to_running_for_a_scheduled_prompt_too() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    step(&service, &task.id, "PermissionRequest");

    let result = claude(&service, &task.id, &stop_with(crons()), None);

    assert_eq!(result.applied, ["agent.resumed", "agent.waiting"]);
    assert_eq!(fx.state(&task.id), TaskState::Running);
}

#[test]
fn ends_the_turn_as_before_for_a_task_that_waits_for_the_user_with_no_background_work() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    step(&service, &task.id, "PermissionRequest");

    let result = claude(
        &service,
        &task.id,
        &stop_with(json!({ "background_tasks": [] })),
        None,
    );

    assert_eq!(result.applied, ["agent.resumed", "agent.finished"]);
    assert_eq!(fx.state(&task.id), TaskState::NeedsInput);
    assert!(!fx.kinds(&task.id).contains(&"agent.waiting".to_string()));
}

#[test]
fn pauses_a_later_turn_as_usual_once_the_report_was_used() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    let reported = Arc::new(AtomicBool::new(true));
    let take = Arc::clone(&reported);
    let service = fx.service_with(
        reporting(move |_| take.swap(false, Ordering::SeqCst)),
        fx.options(false),
    );
    step(&service, &task.id, "UserPromptSubmit");
    let stop = stop_with(background());
    assert!(claude(&service, &task.id, &stop, None).applied.is_empty());

    assert_eq!(
        claude(&service, &task.id, &stop, None).applied,
        ["agent.waiting"]
    );
}

#[test]
fn shows_the_new_work_when_a_scheduled_prompt_wakes_the_agent() {
    let fx = Fx::new();
    let service = fx.service();
    let task = running_task(&fx, &service);
    claude(&service, &task.id, &stop_with(background()), None);

    step(&service, &task.id, "UserPromptSubmit");
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Reading the prompt".into())
    );
    claude(&service, &task.id, &edit("src/a.ts"), None);

    assert_eq!(line(&service, &task.id, "1"), Some("Edit src/a.ts".into()));
    assert_eq!(fx.state(&task.id), TaskState::Running);
    assert!(fx.task(&task.id).background_since.is_none());
}

#[test]
fn clears_the_wait_on_a_failure_or_a_question_and_acts_as_before() {
    let cases = [
        ("StopFailure", json!({}), "agent.failed", TaskState::Failed),
        (
            "PermissionRequest",
            json!({}),
            "agent.asked",
            TaskState::NeedsInput,
        ),
        (
            "Notification",
            json!({ "notification_type": "agent_needs_input" }),
            "agent.asked",
            TaskState::NeedsInput,
        ),
    ];
    for (name, extra, event, state) in cases {
        let fx = Fx::new();
        let service = fx.service();
        let task = running_task(&fx, &service);
        claude(&service, &task.id, &stop_with(background()), None);

        let result = claude(&service, &task.id, &hook(name, extra), None);

        assert_eq!(result.applied, [event], "{name}");
        assert_eq!(fx.state(&task.id), state, "{name}");
        assert!(fx.task(&task.id).background_since.is_none(), "{name}");
        assert!(service.activity().is_empty(), "{name}");
    }
}
