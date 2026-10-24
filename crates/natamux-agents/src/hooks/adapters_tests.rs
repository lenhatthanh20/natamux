use super::*;
use serde_json::json;
use std::path::PathBuf;

fn home() -> PathBuf {
    Path::new("home").join("me")
}

fn try_normalize(agent: &str, raw: Value) -> HookResult {
    find_adapter(agent).unwrap().normalize(&raw, &home())
}

fn normalize(agent: &str, raw: Value) -> Option<NormalizedHook> {
    try_normalize(agent, raw).unwrap()
}

fn signal_of(agent: &str, raw: Value) -> Option<&'static str> {
    normalize(agent, raw).map(|hook| hook.signal.as_str())
}

fn summary_of(hook: Option<NormalizedHook>) -> Option<String> {
    hook.and_then(|hook| hook.summary)
}

#[test]
fn the_adapters_cover_every_agent_natamux_starts() {
    assert_eq!(
        known_agent_ids(),
        ["claude-code", "codex", "gemini", "copilot", "cursor"]
    );
    assert!(find_adapter("nope").is_none());
    assert_eq!(
        find_adapter("codex").map(|adapter| adapter.id()),
        Some("codex")
    );
}

#[test]
fn the_usage_file_is_the_transcript_path_that_the_agent_sends() {
    let cases = [
        ("claude-code", "Stop"),
        ("codex", "Stop"),
        ("gemini", "AfterAgent"),
    ];
    for (agent, event) in cases {
        let found = normalize(
            agent,
            json!({ "hook_event_name": event, "transcript_path": "/home/me/s.jsonl" }),
        )
        .unwrap();
        assert_eq!(found.transcript.as_deref(), Some("/home/me/s.jsonl"));
        // Not in the details, so the path never goes into the event log.
        assert!(!Value::Object(found.details).to_string().contains("s.jsonl"));
    }
}

#[test]
fn the_usage_file_is_copilots_events_file_of_the_session() {
    let stop = |extra: Value| {
        let mut body = json!({ COPILOT_EVENT_FIELD: "agentStop" });
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            body.extend(extra.clone());
        }
        normalize("copilot", body).and_then(|hook| hook.transcript)
    };
    let expected = home()
        .join(".copilot")
        .join("session-state")
        .join("c-1")
        .join("events.jsonl");
    assert_eq!(
        stop(json!({ "sessionId": "c-1" })),
        Some(expected.to_string_lossy().into_owned())
    );
    assert_eq!(stop(json!({ "sessionId": "../c-1" })), None);
    assert_eq!(stop(json!({})), None);
}

#[test]
fn the_usage_file_is_none_for_cursor() {
    let found = normalize("cursor", json!({ "hook_event_name": "stop" }));
    assert_eq!(found.and_then(|hook| hook.transcript), None);
}

mod codex {
    use super::*;

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            ("UserPromptSubmit", "prompt"),
            ("PreToolUse", "activity"),
            ("PostToolUse", "activity"),
            ("PermissionRequest", "asking"),
            ("Stop", "turn-ended"),
            ("Interrupt", "turn-ended"),
        ];
        for (event, signal) in cases {
            let raw = json!({ "hook_event_name": event, "cwd": "/w" });
            assert_eq!(signal_of("codex", raw), Some(signal), "{event}");
        }
    }

    #[test]
    fn ignores_the_events_natamux_does_not_use_and_shows_the_shell_command_it_runs() {
        let ignored = normalize("codex", json!({ "hook_event_name": "SessionStart" }));
        assert_eq!(ignored, None);
        let found = normalize(
            "codex",
            json!({
                "hook_event_name": "PreToolUse",
                "cwd": "/w",
                "session_id": "s1",
                "tool_name": "shell",
                "tool_input": { "command": ["bash", "-lc", "pnpm test"] },
            }),
        )
        .unwrap();
        assert_eq!(found.cwd.as_deref(), Some("/w"));
        assert_eq!(found.summary.as_deref(), Some("shell: pnpm test"));
        assert_eq!(
            Value::Object(found.details),
            json!({ "hook": "PreToolUse", "sessionId": "s1" })
        );
    }
}

mod gemini {
    use super::*;

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            (json!({ "hook_event_name": "BeforeAgent" }), "prompt"),
            (
                json!({ "hook_event_name": "BeforeTool", "tool_name": "read_file" }),
                "activity",
            ),
            (
                json!({ "hook_event_name": "AfterTool", "tool_name": "read_file" }),
                "activity",
            ),
            (
                json!({ "hook_event_name": "Notification", "notification_type": "ToolPermission" }),
                "asking",
            ),
            (json!({ "hook_event_name": "AfterAgent" }), "turn-ended"),
        ];
        for (raw, signal) in cases {
            assert_eq!(signal_of("gemini", raw.clone()), Some(signal), "{raw}");
        }
    }

    #[test]
    fn takes_only_a_permission_notification_as_a_wait_and_names_the_file_a_tool_reads() {
        let other = json!({ "hook_event_name": "Notification", "notification_type": "Other" });
        assert_eq!(normalize("gemini", other), None);
        let read = normalize(
            "gemini",
            json!({
                "hook_event_name": "BeforeTool",
                "cwd": "/w",
                "tool_name": "read_file",
                "tool_input": { "absolute_path": "/w/src/a.ts" },
            }),
        );
        assert_eq!(summary_of(read).as_deref(), Some("read_file src/a.ts"));
    }
}

mod copilot {
    use super::*;

    fn hook(event: &str, rest: Value) -> Option<NormalizedHook> {
        let mut body = json!({ COPILOT_EVENT_FIELD: event, "cwd": "/w" });
        if let (Some(body), Some(rest)) = (body.as_object_mut(), rest.as_object()) {
            body.extend(rest.clone());
        }
        normalize("copilot", body)
    }

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            ("userPromptSubmitted", "prompt"),
            ("preToolUse", "activity"),
            ("postToolUseFailure", "activity"),
            ("agentStop", "turn-ended"),
        ];
        for (event, signal) in cases {
            let found = hook(event, json!({}));
            assert_eq!(found.map(|hook| hook.signal.as_str()), Some(signal));
        }
    }

    // permissionRequest fires before the permission rules, also for tools --allow-all-tools
    // approves.
    #[test]
    fn does_not_take_permission_request_as_asking() {
        assert_eq!(hook("permissionRequest", json!({})), None);
    }

    #[test]
    fn turns_a_notification_into_its_signal_and_summary() {
        let cases = [
            (
                "permission_prompt",
                Some(("asking", "Waiting for permission")),
            ),
            ("elicitation_dialog", Some(("asking", "Asking a question"))),
            ("shell_completed", None),
            ("shell_detached_completed", None),
            ("agent_completed", None),
            ("agent_idle", None),
        ];
        for (kind, expected) in cases {
            let found = hook("notification", json!({ "notification_type": kind }));
            let got = found.map(|hook| (hook.signal.as_str(), hook.summary.unwrap_or_default()));
            let expected = expected.map(|(signal, text)| (signal, text.to_string()));
            assert_eq!(got, expected, "{kind}");
        }
    }

    #[test]
    fn reads_a_notification_in_camel_case_too() {
        let cases = [
            ("permission_prompt", "Waiting for permission"),
            ("elicitation_dialog", "Asking a question"),
        ];
        for (kind, summary) in cases {
            let found = hook("notification", json!({ "notificationType": kind })).unwrap();
            assert_eq!(found.signal, Signal::Asking);
            assert_eq!(found.summary.as_deref(), Some(summary));
        }
    }

    #[test]
    fn takes_the_snake_case_type_when_both_spellings_are_there() {
        let snake = json!({
            "notification_type": "permission_prompt",
            "notificationType": "agent_idle",
        });
        let camel = json!({
            "notification_type": "shell_completed",
            "notificationType": "permission_prompt",
        });
        assert_eq!(
            hook("notification", snake).map(|hook| hook.signal),
            Some(Signal::Asking)
        );
        assert_eq!(hook("notification", camel), None);
    }

    #[test]
    fn gives_no_signal_and_does_not_fail_for_a_type_it_cannot_read() {
        let cases = [
            json!({ "notification_type": null }),
            json!({ "notification_type": 7 }),
            json!({ "notification_type": "" }),
            json!({}),
            json!({ "notification_type": "toString" }),
            json!({ "notificationType": null }),
        ];
        for rest in cases {
            assert_eq!(hook("notification", rest.clone()), None, "{rest}");
        }
    }

    #[test]
    fn reads_the_notification_of_a_real_copilot_cli_1_0_98_session() {
        // Recorded on 2026-10-17: the stdin of the notification hook when Copilot asked to run a
        // command. Only cwd is made shorter.
        let session = "7fa32534-1e09-4549-90df-d1a12eb4d059";
        let found = normalize(
            "copilot",
            json!({
                "sessionId": session,
                "timestamp": 1792242222365_u64,
                "cwd": "/tmp/cp-record/repo",
                "message": "Run command: touch made-by-copilot.txt && ls -l made-by-copilot.txt",
                "title": "Permission needed",
                "hook_event_name": "Notification",
                "notification_type": "permission_prompt",
                COPILOT_EVENT_FIELD: "notification",
            }),
        )
        .unwrap();
        assert_eq!(found.signal, Signal::Asking);
        assert_eq!(found.cwd.as_deref(), Some("/tmp/cp-record/repo"));
        assert_eq!(
            Value::Object(found.details),
            json!({ "hook": "notification", "sessionId": session })
        );
        assert_eq!(found.summary.as_deref(), Some("Waiting for permission"));
    }

    #[test]
    fn fails_only_on_an_error_copilot_cannot_get_over() {
        assert_eq!(hook("errorOccurred", json!({ "recoverable": true })), None);
        let found = hook("errorOccurred", json!({ "recoverable": false }));
        assert_eq!(found.map(|hook| hook.signal), Some(Signal::Failed));
    }

    #[test]
    fn reads_the_tool_arguments_as_json_text() {
        let found = hook(
            "preToolUse",
            json!({ "toolName": "bash", "toolArgs": "{\"command\":\"ls -la\"}" }),
        );
        assert_eq!(summary_of(found).as_deref(), Some("bash: ls -la"));
    }

    #[test]
    fn reads_the_json_of_a_real_copilot_cli_1_0_94_session() {
        // Recorded on 2026-10-10: the stdin of the agentStop hook, as Copilot sent it.
        let session = "8e832061-c62c-438e-ad8b-f399d51a200c";
        let found = normalize(
            "copilot",
            json!({
                "sessionId": session,
                "timestamp": 1791570302333_u64,
                "cwd": "/tmp/cp-test",
                "transcriptPath": "/home/vscode/.copilot/session-state/8e832061/events.jsonl",
                "stopReason": "end_turn",
                "stop_hook_active": false,
                COPILOT_EVENT_FIELD: "agentStop",
            }),
        )
        .unwrap();
        assert_eq!(found.signal, Signal::TurnEnded);
        assert_eq!(found.cwd.as_deref(), Some("/tmp/cp-test"));
        assert_eq!(
            Value::Object(found.details),
            json!({ "hook": "agentStop", "sessionId": session })
        );
    }

    #[test]
    fn needs_the_event_that_the_hook_command_names() {
        let error = try_normalize("copilot", json!({ "cwd": "/w" })).unwrap_err();
        assert_eq!(
            error.describe(),
            "natamux_event: Invalid input: expected string, received undefined"
        );
    }
}

mod cursor {
    use super::*;

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            (json!({ "hook_event_name": "beforeSubmitPrompt" }), "prompt"),
            (
                json!({ "hook_event_name": "beforeShellExecution", "command": "ls" }),
                "activity",
            ),
            (
                json!({ "hook_event_name": "afterFileEdit", "file_path": "/w/a.ts" }),
                "activity",
            ),
            (
                json!({ "hook_event_name": "stop", "status": "completed" }),
                "turn-ended",
            ),
            (
                json!({ "hook_event_name": "stop", "status": "aborted" }),
                "turn-ended",
            ),
            (
                json!({ "hook_event_name": "stop", "status": "error" }),
                "failed",
            ),
        ];
        for (raw, signal) in cases {
            assert_eq!(signal_of("cursor", raw.clone()), Some(signal), "{raw}");
        }
    }

    #[test]
    fn finds_the_folder_in_workspace_roots_when_there_is_no_cwd() {
        let found = normalize(
            "cursor",
            json!({
                "hook_event_name": "beforeShellExecution",
                "workspace_roots": ["/w"],
                "conversation_id": "c1",
                "command": "pnpm test",
            }),
        )
        .unwrap();
        assert_eq!(found.cwd.as_deref(), Some("/w"));
        assert_eq!(found.summary.as_deref(), Some("Shell: pnpm test"));
        assert_eq!(found.details["sessionId"], json!("c1"));
    }
}
