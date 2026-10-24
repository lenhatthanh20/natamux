//! The group 'AgentHookService' (without its nested groups, which have a file each).

use std::sync::atomic::{AtomicBool, Ordering};

use natamux_daemon::agents::errors::HookError;

use super::*;

#[test]
fn keeps_what_the_agent_does_now_and_forgets_it_when_the_turn_ends() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    step(&service, &task.id, "UserPromptSubmit");
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Reading the prompt".into())
    );

    let tool = hook(
        "PreToolUse",
        json!({ "tool_name": "Bash", "tool_input": { "command": "pnpm test" } }),
    );
    claude(&service, &task.id, &tool, None);
    assert_eq!(
        line(&service, &task.id, "1"),
        Some("Bash: pnpm test".into())
    );

    step(&service, &task.id, "Stop");
    assert!(service.activity().is_empty());
}

fn edit(path: &str) -> Value {
    hook(
        "PreToolUse",
        json!({ "tool_name": "Edit", "tool_input": { "file_path": path } }),
    )
}

#[test]
fn keeps_what_the_agent_of_each_terminal_does_apart() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    // Hooks from a terminal count only while the task has its workspace.
    fx.workspace(&task.id, Some("/work/T-0001"));

    claude(&service, &task.id, &edit("src/a.ts"), Some("1"));
    claude(&service, &task.id, &edit("test-spec.md"), Some("2"));
    assert_eq!(line(&service, &task.id, "1"), Some("Edit src/a.ts".into()));
    assert_eq!(
        line(&service, &task.id, "2"),
        Some("Edit test-spec.md".into())
    );

    // The turn end in Shell 2 clears only its own line.
    claude(&service, &task.id, &hook("Stop", json!({})), Some("2"));
    assert_eq!(line_terminals(&service, &task.id), ["1"]);
    assert_eq!(line(&service, &task.id, "1"), Some("Edit src/a.ts".into()));

    // A hook without a terminal comes from the first one.
    step(&service, &task.id, "Stop");
    assert!(service.activity().is_empty());
}

#[test]
fn finds_the_task_by_the_header() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    let found = service
        .find_task(Some("/anywhere"), Some(&task.id))
        .unwrap();
    assert_eq!(found.map(|found| found.id), Some(task.id));
}

#[test]
fn finds_the_task_by_the_workspace_that_holds_the_working_directory() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));

    for cwd in ["/work/T-0001", "/work/T-0001/packages/web"] {
        let found = service.find_task(Some(cwd), None).unwrap();
        assert_eq!(found.map(|found| found.id), Some(task.id.clone()));
    }
}

#[test]
fn does_not_mix_up_workspaces_whose_paths_start_the_same_way() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));

    assert!(
        service
            .find_task(Some("/work/T-00010"), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn falls_back_to_the_working_directory_when_the_header_is_empty_invalid_or_unknown() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));

    for header in [None, Some("nope"), Some("T-0099")] {
        let found = service.find_task(Some("/work/T-0001"), header).unwrap();
        assert_eq!(found.map(|found| found.id), Some(task.id.clone()));
    }
}

#[test]
fn finds_nothing_for_a_directory_that_belongs_to_no_task() {
    let fx = Fx::new();
    let service = fx.service();
    fx.make("Fix it");
    assert!(
        service
            .find_task(Some("/elsewhere"), None)
            .unwrap()
            .is_none()
    );
    assert!(service.find_task(None, None).unwrap().is_none());
}

#[test]
fn moves_a_task_through_a_run_prompt_question_answer_end_of_turn_without_a_report() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");

    assert_eq!(
        step(&service, &task.id, "UserPromptSubmit").applied,
        ["task.readied", "agent.started"]
    );
    assert_eq!(fx.state(&task.id), TaskState::Running);

    step(&service, &task.id, "PermissionRequest");
    assert_eq!(fx.state(&task.id), TaskState::NeedsInput);

    step(&service, &task.id, "PostToolUse");
    assert_eq!(fx.state(&task.id), TaskState::Running);

    // The agent did not run `natamux done`, so the turn end waits for you.
    step(&service, &task.id, "Stop");
    assert_eq!(fx.state(&task.id), TaskState::NeedsInput);
}

// An API error ends the turn, but the session lives on and the user can type to it again.
#[test]
fn takes_a_failed_task_up_again_on_a_prompt_or_a_tool_then_goes_on_as_before() {
    let cases = [
        ("UserPromptSubmit", json!({})),
        (
            "PreToolUse",
            json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }),
        ),
        ("PostToolUse", json!({ "tool_name": "Bash" })),
    ];
    for (name, extra) in cases {
        let fx = Fx::new();
        let service = fx.service();
        let task = fx.make("Fix it");
        step(&service, &task.id, "UserPromptSubmit");
        step(&service, &task.id, "StopFailure");
        assert_eq!(fx.state(&task.id), TaskState::Failed, "{name}");
        // Not failed twice.
        assert!(
            step(&service, &task.id, "StopFailure").applied.is_empty(),
            "{name}"
        );

        let resumed = claude(&service, &task.id, &hook(name, extra), None);
        assert_eq!(resumed.applied, ["agent.resumed"], "{name}");
        assert_eq!(fx.state(&task.id), TaskState::Running, "{name}");

        step(&service, &task.id, "Stop");
        assert_eq!(fx.state(&task.id), TaskState::NeedsInput, "{name}");
    }
}

fn shell_two_is_no_stage() -> Arc<dyn StageLookup> {
    mock(
        |task, terminal| {
            if terminal == Some("2") {
                None
            } else {
                own_lane(task)
            }
        },
        |_, _| false,
    )
}

#[test]
fn gives_an_agent_in_a_shell_that_no_stage_uses_a_state_of_its_own_and_leaves_the_task_alone() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));
    let shells = fx.service_with(shell_two_is_no_stage(), fx.options(false));
    let events = fx.event_count(&task.id);
    let cursor = |raw: Value| {
        shells
            .handle("cursor", &raw, Some(&task.id), Some("2"))
            .unwrap()
    };

    let prompt = cursor(json!({ "hook_event_name": "beforeSubmitPrompt" }));
    assert_eq!(prompt, nothing_for(&task.id));
    assert_eq!(shells.shell_state(&task.id, "2"), Some(TaskState::Running));
    assert_eq!(
        line(&shells, &task.id, "2"),
        Some("Reading the prompt".into())
    );

    // The turn ends: the agent waits for the user, as the task's own agent does.
    cursor(json!({ "hook_event_name": "stop", "status": "completed" }));
    assert_eq!(
        shells.shell_state(&task.id, "2"),
        Some(TaskState::NeedsInput)
    );
    cursor(json!({ "hook_event_name": "stop", "status": "error" }));
    assert_eq!(shells.shell_state(&task.id, "2"), Some(TaskState::Failed));

    assert_eq!(fx.event_count(&task.id), events);
    assert_eq!(fx.state(&task.id), task.state);
    assert_eq!(shells.shell_state(&task.id, "1"), None);

    // The agent left the shell: a new one there starts with no state.
    shells.forget_shells_except(&task.id, &["1".to_string()]);
    assert_eq!(shells.shell_state(&task.id, "2"), None);
}

#[test]
fn forgets_the_state_of_a_shell_once_its_hooks_reach_a_stage_again() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));
    let on_stage = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&on_stage);
    let shells = fx.service_with(
        mock(
            move |task, _| {
                if flag.load(Ordering::SeqCst) {
                    own_lane(task)
                } else {
                    None
                }
            },
            |_, _| false,
        ),
        fx.options(false),
    );
    let prompt = json!({ "hook_event_name": "beforeSubmitPrompt" });
    shells
        .handle("cursor", &prompt, Some(&task.id), Some("6"))
        .unwrap();
    assert_eq!(shells.shell_state(&task.id, "6"), Some(TaskState::Running));

    // The task came back to the stage of Shell 6.
    on_stage.store(true, Ordering::SeqCst);
    shells
        .handle("cursor", &prompt, Some(&task.id), Some("6"))
        .unwrap();
    assert_eq!(shells.shell_state(&task.id, "6"), None);
}

#[test]
fn drops_the_turn_end_that_follows_a_report_and_names_the_stage_of_the_terminal() {
    let fx = Fx::new();
    let task = fx.make("Fix it");
    fx.workspace(&task.id, Some("/work/T-0001"));
    let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let record = Arc::clone(&seen);
    let reporting = fx.service_with(
        mock(
            move |task, terminal| {
                record.lock().unwrap().push(terminal.map(String::from));
                own_lane(task)
            },
            |_, terminal| terminal == "2",
        ),
        fx.options(false),
    );
    let prompt = hook("UserPromptSubmit", json!({}));
    claude(&reporting, &task.id, &prompt, Some("2"));
    assert_eq!(fx.last_payload(&task.id)["stage"], "implement");

    let stop = hook("Stop", json!({}));
    assert!(
        claude(&reporting, &task.id, &stop, Some("2"))
            .applied
            .is_empty()
    );
    assert_eq!(fx.state(&task.id), TaskState::Running);
    assert_eq!(
        *seen.lock().unwrap(),
        [Some("2".to_string()), Some("2".to_string())]
    );
}

#[test]
fn still_follows_an_agent_outside_natamux_terminals_that_names_a_task_with_no_workspace() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");

    assert_eq!(
        step(&service, &task.id, "UserPromptSubmit").applied,
        ["task.readied", "agent.started"]
    );
    assert_eq!(
        step(&service, &task.id, "PermissionRequest").applied,
        ["agent.asked"]
    );
    assert_eq!(fx.state(&task.id), TaskState::NeedsInput);
}

#[test]
fn records_the_events_as_coming_from_the_agent() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    step(&service, &task.id, "UserPromptSubmit");

    let sources: Vec<String> = fx
        .tasks
        .list_events(&task.id)
        .unwrap()
        .into_iter()
        .map(|stored| stored.event.source)
        .collect();
    assert_eq!(sources, ["system", "agent", "agent"]);
}

#[test]
fn does_nothing_and_logs_nothing_when_the_hook_changes_nothing() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");

    assert_eq!(step(&service, &task.id, "Stop"), nothing_for(&task.id));
    assert_eq!(fx.event_count(&task.id), 1);
}

#[test]
fn ignores_a_hook_the_adapter_does_not_use_even_when_a_task_matches() {
    let fx = Fx::new();
    let service = fx.service();
    let task = fx.make("Fix it");
    assert_eq!(step(&service, &task.id, "SessionEnd"), no_task());
}

#[test]
fn reports_that_no_task_was_found() {
    let fx = Fx::new();
    let service = fx.service();
    let raw = hook("Stop", json!({ "cwd": "/elsewhere" }));
    let result = service.handle("claude-code", &raw, None, None).unwrap();
    assert_eq!(result, no_task());
}

#[test]
fn throws_unknown_agent_naming_the_known_agents_for_an_agent_without_an_adapter() {
    let fx = Fx::new();
    let service = fx.service();
    let error = service
        .handle("nope", &hook("Stop", json!({})), None, None)
        .unwrap_err();
    assert!(matches!(error, HookError::UnknownAgent(_)));
    assert!(error.to_string().contains("claude-code"));
}

#[test]
fn throws_for_input_that_is_not_a_hook_of_the_agent() {
    let fx = Fx::new();
    let service = fx.service();
    let error = service
        .handle("claude-code", &json!({ "cwd": "/x" }), None, None)
        .unwrap_err();
    assert!(matches!(error, HookError::Invalid(_)));
}
