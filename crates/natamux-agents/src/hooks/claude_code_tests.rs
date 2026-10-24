use super::*;
use natamux_core::js::utf16_len;

fn run(raw: Value) -> HookResult {
    normalize(&raw, Path::new("/home/me"))
}

fn named(name: &str, extra: Value) -> Value {
    let mut body = json!({ "hook_event_name": name });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }
    body
}

fn hook(name: &str, extra: Value) -> Option<NormalizedHook> {
    run(named(name, extra)).unwrap()
}

fn signal_of(name: &str, extra: Value) -> Option<&'static str> {
    hook(name, extra).map(|hook| hook.signal.as_str())
}

fn waiting_for(hook: &Option<NormalizedHook>) -> Option<Value> {
    hook.as_ref()?.details.get("waitingFor").cloned()
}

#[test]
fn turns_each_event_into_its_signal() {
    let cases = [
        ("UserPromptSubmit", "prompt"),
        ("PreToolUse", "activity"),
        ("PostToolUse", "activity"),
        ("PermissionRequest", "asking"),
        ("Stop", "turn-ended"),
        ("StopFailure", "failed"),
    ];
    for (name, signal) in cases {
        assert_eq!(signal_of(name, json!({})), Some(signal), "{name}");
    }
}

#[test]
fn turns_a_notification_into_its_signal() {
    let cases = [
        ("permission_prompt", Some("asking")),
        ("elicitation_dialog", Some("asking")),
        ("agent_needs_input", Some("asking")),
        ("idle_prompt", None),
        ("auth_success", None),
    ];
    for (kind, signal) in cases {
        let extra = json!({ "notification_type": kind });
        assert_eq!(signal_of("Notification", extra), signal, "{kind}");
    }
}

#[test]
fn ignores_a_notification_without_a_type() {
    assert_eq!(signal_of("Notification", json!({})), None);
}

#[test]
fn ignores_hooks_it_does_not_use() {
    assert_eq!(signal_of("SessionEnd", json!({})), None);
}

#[test]
fn passes_the_working_directory_on() {
    let found = hook("Stop", json!({ "cwd": "/work/T-0001" }));
    assert_eq!(
        found.and_then(|hook| hook.cwd).as_deref(),
        Some("/work/T-0001")
    );
}

#[test]
fn keeps_the_details_of_the_hook_and_cuts_a_long_message() {
    let found = hook(
        "Notification",
        json!({
            "notification_type": "permission_prompt",
            "session_id": "s1",
            "message": "x".repeat(900),
        }),
    )
    .unwrap();
    assert_eq!(found.details["hook"], json!("Notification"));
    assert_eq!(
        found.details["notificationType"],
        json!("permission_prompt")
    );
    assert_eq!(found.details["sessionId"], json!("s1"));
    let message = found.details["message"].as_str().unwrap();
    assert_eq!(utf16_len(message), 500);
    let keys: Vec<_> = found.details.keys().map(String::as_str).collect();
    assert_eq!(keys, ["hook", "sessionId", "notificationType", "message"]);
}

mod background_work {
    use super::*;

    fn shell(command: &str, status: &str) -> Value {
        json!({
            "id": "t1",
            "type": "shell",
            "status": status,
            "description": "Run tests",
            "command": command,
        })
    }

    fn stop(extra: Value) -> Option<NormalizedHook> {
        hook("Stop", extra)
    }

    fn signal(found: &Option<NormalizedHook>) -> Option<&'static str> {
        found.as_ref().map(|hook| hook.signal.as_str())
    }

    fn summary(found: &Option<NormalizedHook>) -> Option<&str> {
        found.as_ref()?.summary.as_deref()
    }

    #[test]
    fn pauses_the_turn_while_a_background_shell_runs_and_says_what_it_waits_for() {
        let found = stop(json!({
            "background_tasks": [shell("pnpm  test", "running")],
            "session_crons": [],
        }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(summary(&found), Some("Waiting for Bash: pnpm test"));
        assert_eq!(waiting_for(&found), Some(json!(["Bash: pnpm test"])));
    }

    #[test]
    fn names_a_shell_by_its_command_else_by_its_description() {
        let found = stop(json!({ "background_tasks": [
            { "id": "a", "type": "shell", "status": "running", "command": "pnpm build" },
            { "id": "b", "type": "shell", "status": "running", "description": "Watch CI" },
        ] }));
        assert_eq!(
            waiting_for(&found),
            Some(json!(["Bash: pnpm build", "Bash: Watch CI"]))
        );
    }

    #[test]
    fn names_a_task_without_a_command_by_its_description_or_else_by_its_type() {
        let found = stop(json!({ "background_tasks": [
            { "id": "m", "type": "monitor", "status": "running", "description": "watch CI" },
            { "id": "s", "type": "subagent", "status": "running" },
        ] }));
        assert_eq!(
            waiting_for(&found),
            Some(json!(["monitor: watch CI", "subagent"]))
        );
        assert_eq!(
            summary(&found),
            Some("Waiting for monitor: watch CI and 1 more")
        );
    }

    #[test]
    fn pauses_the_turn_for_a_task_that_runs_on_its_own() {
        let kinds = [
            "subagent",
            "monitor",
            "workflow",
            "teammate",
            "cloud session",
            "MCP task",
        ];
        for kind in kinds {
            let found = stop(json!({ "background_tasks": [
                { "id": "x", "type": kind, "status": "running" },
            ] }));
            assert_eq!(signal(&found), Some("turn-paused"), "{kind}");
            assert_eq!(waiting_for(&found), Some(json!([kind])), "{kind}");
        }
    }

    #[test]
    fn pauses_the_turn_for_a_workflow_and_a_background_shell_at_the_same_time() {
        let found = stop(json!({ "background_tasks": [
            { "id": "w", "type": "workflow", "status": "running", "description": "implement" },
            shell("pnpm test", "running"),
        ] }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(
            waiting_for(&found),
            Some(json!(["workflow: implement", "Bash: pnpm test"]))
        );
        assert_eq!(
            summary(&found),
            Some("Waiting for workflow: implement and 1 more")
        );
    }

    #[test]
    fn ends_the_turn_once_the_workflow_is_finished_or_stopped() {
        for status in ["completed", "failed", "killed", "stopped"] {
            let found = stop(json!({ "background_tasks": [
                { "id": "w", "type": "workflow", "status": status },
            ] }));
            assert_eq!(signal(&found), Some("turn-ended"), "{status}");
        }
    }

    #[test]
    fn counts_a_task_with_any_other_status_as_still_running() {
        for status in ["running", "pending", "queued", "starting"] {
            let found = stop(json!({ "background_tasks": [shell("ls", status)] }));
            assert_eq!(signal(&found), Some("turn-paused"), "{status}");
        }
    }

    #[test]
    fn counts_a_task_without_a_status_as_still_running() {
        let found = stop(json!({ "background_tasks": [
            { "id": "x", "type": "shell", "command": "ls" },
        ] }));
        assert_eq!(signal(&found), Some("turn-paused"));
    }

    #[test]
    fn lists_only_the_tasks_that_still_run() {
        let found = stop(json!({ "background_tasks": [
            shell("pnpm lint", "completed"),
            shell("pnpm test", "running"),
        ] }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(waiting_for(&found), Some(json!(["Bash: pnpm test"])));
    }

    #[test]
    fn ignores_fields_of_a_task_it_does_not_read() {
        let mut task = shell("pnpm test", "running");
        if let Some(map) = task.as_object_mut() {
            map.insert("pid".into(), json!(42));
            map.insert("startedAt".into(), json!("2026-10-11T09:00:00Z"));
        }
        let found = stop(json!({ "background_tasks": [task] }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(waiting_for(&found), Some(json!(["Bash: pnpm test"])));
    }

    #[test]
    fn pauses_the_turn_for_a_scheduled_prompt_of_the_session() {
        let found = stop(json!({
            "background_tasks": [],
            "session_crons": [
                { "id": "c1", "schedule": "*/5 * * * *", "recurring": true, "prompt": "check CI" },
            ],
        }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(
            waiting_for(&found),
            Some(json!(["scheduled prompt: check CI"]))
        );
        assert_eq!(
            summary(&found),
            Some("Waiting for scheduled prompt: check CI")
        );
    }

    #[test]
    fn pauses_the_turn_for_a_one_shot_prompt_with_no_background_task_list_at_all() {
        let found = stop(json!({ "session_crons": [
            { "id": "c1", "recurring": false, "prompt": "check CI" },
        ] }));
        assert_eq!(signal(&found), Some("turn-paused"));
    }

    #[test]
    fn names_a_scheduled_prompt_without_its_text_by_its_schedule() {
        let found = stop(json!({ "session_crons": [
            { "id": "c1", "schedule": "0 9 * * 1-5" },
        ] }));
        assert_eq!(
            waiting_for(&found),
            Some(json!(["scheduled prompt: 0 9 * * 1-5"]))
        );
    }

    #[test]
    fn lists_background_tasks_and_scheduled_prompts_together_at_most_five() {
        let found = stop(json!({
            "background_tasks": [
                shell("a", "running"),
                shell("b", "running"),
                shell("c", "running"),
                shell("d", "running"),
            ],
            "session_crons": [{ "prompt": "e" }, { "prompt": "f" }],
        }));
        assert_eq!(
            waiting_for(&found),
            Some(json!([
                "Bash: a",
                "Bash: b",
                "Bash: c",
                "Bash: d",
                "scheduled prompt: e",
            ]))
        );
        assert_eq!(summary(&found), Some("Waiting for Bash: a and 5 more"));
    }

    #[test]
    fn ends_the_turn_as_before_when_the_only_task_is_finished() {
        for status in ["completed", "failed", "killed", "stopped"] {
            let found = stop(json!({
                "background_tasks": [shell("ls", status)],
                "session_crons": [],
            }));
            assert_eq!(signal(&found), Some("turn-ended"), "{status}");
            assert_eq!(summary(&found), None);
            assert_eq!(waiting_for(&found), None);
        }
    }

    #[test]
    fn ends_the_turn_as_before_when_there_is_nothing_to_wait_for() {
        let mut cases = vec![
            ("no fields", json!({})),
            (
                "empty lists",
                json!({ "background_tasks": [], "session_crons": [] }),
            ),
            ("no task list and no crons", json!({ "session_crons": [] })),
            (
                "no tasks and no cron list",
                json!({ "background_tasks": [] }),
            ),
            (
                "entries without a type",
                json!({ "background_tasks": [{ "status": "running" }, null, "x", 7] }),
            ),
            (
                "a type that is not text",
                json!({ "background_tasks": [{ "type": 3, "status": "running" }] }),
            ),
            (
                "crons that are not objects",
                json!({ "session_crons": [null, "daily", 5] }),
            ),
        ];
        for value in [json!(null), json!("x"), json!(42), json!({})] {
            cases.push(("a task list", json!({ "background_tasks": value })));
            cases.push(("a cron list", json!({ "session_crons": value })));
        }
        for (name, extra) in cases {
            let found = stop(extra.clone());
            assert_eq!(signal(&found), Some("turn-ended"), "{name} {extra}");
            assert_eq!(summary(&found), None, "{name} {extra}");
            assert_eq!(waiting_for(&found), None, "{name} {extra}");
        }
    }

    #[test]
    fn skips_a_bad_entry_and_keeps_the_good_one() {
        let found = stop(json!({ "background_tasks": [
            { "status": "running" },
            shell("pnpm test", "running"),
        ] }));
        assert_eq!(signal(&found), Some("turn-paused"));
        assert_eq!(waiting_for(&found), Some(json!(["Bash: pnpm test"])));
    }

    #[test]
    fn pauses_the_turn_for_a_cron_when_the_task_list_cannot_be_read() {
        let found = stop(json!({
            "background_tasks": "x",
            "session_crons": [{ "prompt": "check CI" }],
        }));
        assert_eq!(signal(&found), Some("turn-paused"));
    }

    #[test]
    fn keeps_at_most_five_entries_of_200_characters_and_a_short_summary() {
        let tasks: Vec<Value> = (0..7).map(|_| shell(&"x".repeat(300), "running")).collect();
        let found = stop(json!({ "background_tasks": tasks }));
        let waiting = waiting_for(&found).unwrap();
        let waiting = waiting.as_array().unwrap();
        assert_eq!(waiting.len(), 5);
        for entry in waiting {
            assert_eq!(utf16_len(entry.as_str().unwrap()), 200);
        }
        let line = summary(&found).unwrap();
        assert!(line.ends_with(" and 6 more"));
        assert_eq!(utf16_len(line), 80);
    }

    #[test]
    fn cuts_a_long_description_and_a_long_scheduled_prompt_to_200_characters() {
        let found = stop(json!({
            "background_tasks": [{ "id": "m", "type": "monitor", "description": "d".repeat(300) }],
            "session_crons": [{ "prompt": "p".repeat(300) }],
        }));
        let waiting = waiting_for(&found).unwrap();
        let waiting = waiting.as_array().unwrap();
        let lengths: Vec<_> = waiting
            .iter()
            .map(|entry| utf16_len(entry.as_str().unwrap()))
            .collect();
        assert_eq!(lengths, [200, 200]);
        assert!(
            waiting[1]
                .as_str()
                .unwrap()
                .starts_with("scheduled prompt: ppp")
        );
    }

    #[test]
    fn only_looks_for_background_work_in_a_stop() {
        let extra = json!({ "background_tasks": [shell("ls", "running")] });
        assert_eq!(signal_of("PostToolUse", extra), Some("activity"));
    }
}

#[test]
fn fails_for_input_that_is_not_a_hook() {
    assert!(run(json!({})).is_err());
    assert!(run(json!("Stop")).is_err());
    assert!(run(Value::Null).is_err());
}

#[test]
fn needs_an_event_name_and_drops_the_fields_it_does_not_read() {
    let error = run(json!({})).unwrap_err();
    assert_eq!(
        error.describe(),
        "hook_event_name: Invalid input: expected string, received undefined"
    );
    let error = run(json!({ "hook_event_name": "" })).unwrap_err();
    assert_eq!(
        error.describe(),
        "hook_event_name: Too small: expected string to have >=1 characters"
    );
    let found = hook("Stop", json!({ "permission_mode": "default" })).unwrap();
    assert_eq!(found.details.len(), 1);
}

#[test]
fn gives_the_summary_for_the_tool_about_to_run_and_for_a_new_prompt() {
    let before = hook(
        "PreToolUse",
        json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }),
    );
    assert_eq!(
        before.and_then(|hook| hook.summary).as_deref(),
        Some("Bash: ls")
    );
    let after = hook("PostToolUse", json!({ "tool_name": "Bash" }));
    assert_eq!(after.and_then(|hook| hook.summary), None);
    let prompt = hook("UserPromptSubmit", json!({}));
    assert_eq!(
        prompt.and_then(|hook| hook.summary).as_deref(),
        Some("Reading the prompt")
    );
}
