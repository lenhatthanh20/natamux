//! The group 'after the workspace was removed': Natamux terminals run in the task's workspace
//! only.

use super::*;

fn late_hooks() -> Vec<Value> {
    vec![
        hook("UserPromptSubmit", json!({})),
        hook(
            "PreToolUse",
            json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }),
        ),
        hook("PermissionRequest", json!({})),
        hook("Stop", json!({})),
        hook("StopFailure", json!({})),
    ]
}

/// A task whose agent ran in terminal 1 until its workspace was removed.
fn removed_task(fx: &Fx, service: &HookService, state: TaskState) -> Task {
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some(&format!("/work/{}", task.id)));
    let prompt = hook("UserPromptSubmit", json!({}));
    claude(service, &task.id, &prompt, Some("1"));
    if state == TaskState::NeedsInput {
        let ask = hook("PermissionRequest", json!({}));
        claude(service, &task.id, &ask, Some("1"));
    }
    fx.workspace(&task.id, None);
    fx.task(&task.id)
}

#[test]
fn ignores_every_late_hook_from_its_terminal() {
    for state in [TaskState::Running, TaskState::NeedsInput] {
        let fx = Fx::new();
        let service = fx.service();
        let before = removed_task(&fx, &service, state);
        let events = fx.event_count(&before.id);

        for late in late_hooks() {
            let result = claude(&service, &before.id, &late, Some("1"));
            assert_eq!(result, no_task(), "agent was {state:?}");
        }

        assert_eq!(fx.task(&before.id), before);
        assert_eq!(fx.event_count(&before.id), events);
        assert!(service.activity().is_empty());
    }
}

#[test]
fn does_not_touch_another_task() {
    let fx = Fx::new();
    let service = fx.service();
    let removed = removed_task(&fx, &service, TaskState::Running);
    let other = fx.make("Other");
    fx.workspace(&other.id, Some(&format!("/work/{}", other.id)));
    let prompt = hook("UserPromptSubmit", json!({}));
    claude(&service, &other.id, &prompt, Some("1"));

    claude(&service, &removed.id, &hook("Stop", json!({})), Some("1"));

    assert_eq!(fx.state(&other.id), TaskState::Running);
    let stop = claude(&service, &other.id, &hook("Stop", json!({})), Some("1"));
    assert_eq!(stop.applied, ["agent.finished"]);
}

#[test]
fn still_ignores_them_after_a_restart() {
    let fx = Fx::new();
    let service = fx.service();
    let before = removed_task(&fx, &service, TaskState::Running);
    let restarted = fx.service();

    let result = claude(&restarted, &before.id, &hook("Stop", json!({})), Some("1"));

    assert!(result.applied.is_empty());
    assert_eq!(fx.task(&before.id), before);
}

#[test]
fn follows_the_agent_in_a_new_workspace_as_usual() {
    let fx = Fx::new();
    let service = fx.service();
    let before = removed_task(&fx, &service, TaskState::Running);
    fx.apply(
        &before.id,
        json!({ "source": "system", "type": "agent.stopped", "payload": { "stage": before.stage } }),
    );
    fx.workspace(&before.id, Some(&format!("/work/{}", before.id)));

    let started = claude(
        &service,
        &before.id,
        &hook("UserPromptSubmit", json!({})),
        Some("1"),
    );
    assert_eq!(started.applied, ["agent.started"]);
    let stopped = claude(&service, &before.id, &hook("Stop", json!({})), Some("1"));
    assert_eq!(stopped.applied, ["agent.finished"]);
    assert_eq!(fx.state(&before.id), TaskState::NeedsInput);
}

#[test]
fn answers_a_hook_with_an_unknown_task_as_before() {
    let fx = Fx::new();
    let service = fx.service();
    let result = claude(&service, "T-0099", &hook("Stop", json!({})), Some("1"));
    assert_eq!(result, no_task());
}
