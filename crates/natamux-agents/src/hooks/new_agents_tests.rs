//! The adapters of OpenCode, Qwen Code and Droid.

use super::*;
use serde_json::json;

fn try_normalize(agent: &str, raw: Value) -> HookResult {
    find_adapter(agent)
        .unwrap()
        .normalize(&raw, Path::new("home"))
}

fn normalize(agent: &str, raw: Value) -> Option<NormalizedHook> {
    try_normalize(agent, raw).unwrap()
}

fn signal_of(agent: &str, raw: Value) -> Option<&'static str> {
    normalize(agent, raw).map(|hook| hook.signal.as_str())
}

mod qwen {
    use super::*;

    fn event(name: &str) -> Value {
        json!({ "hook_event_name": name, "cwd": "/w", "session_id": "s1" })
    }

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            ("UserPromptSubmit", "prompt"),
            ("PreToolUse", "activity"),
            ("PostToolUse", "activity"),
            ("PostToolUseFailure", "activity"),
            ("PermissionRequest", "asking"),
            ("Stop", "turn-ended"),
            ("StopFailure", "failed"),
        ];
        for (name, signal) in cases {
            assert_eq!(signal_of("qwen", event(name)), Some(signal), "{name}");
        }
    }

    #[test]
    fn takes_only_a_permission_notification_as_a_wait() {
        let mut raw = event("Notification");
        raw["notification_type"] = json!("permission_prompt");
        assert_eq!(signal_of("qwen", raw.clone()), Some("asking"));
        raw["notification_type"] = json!("idle_prompt");
        assert_eq!(signal_of("qwen", raw), None);
    }

    #[test]
    fn names_the_tool_and_the_session_and_the_failure() {
        let mut raw = event("PreToolUse");
        raw["tool_name"] = json!("run_shell_command");
        raw["tool_input"] = json!({ "command": "ls" });
        let found = normalize("qwen", raw).unwrap();
        assert_eq!(found.summary.as_deref(), Some("run_shell_command: ls"));
        assert_eq!(found.details["sessionId"], json!("s1"));
        let mut failed = event("StopFailure");
        failed["error"] = json!("rate_limit");
        failed["error_details"] = json!("Too many requests");
        let found = normalize("qwen", failed).unwrap();
        assert_eq!(found.details["message"], json!("Too many requests"));
    }

    #[test]
    fn ignores_other_events_and_the_end_of_natamux_ask_and_fails_on_bad_input() {
        assert_eq!(signal_of("qwen", event("SessionStart")), None);
        let mut ask = event("PostToolUse");
        ask["tool_input"] = json!({ "command": "natamux ask \"Why?\"" });
        assert_eq!(signal_of("qwen", ask), None);
        assert!(try_normalize("qwen", json!({ "cwd": "/w" })).is_err());
        assert!(try_normalize("qwen", json!("text")).is_err());
    }
}

mod droid {
    use super::*;

    fn event(name: &str) -> Value {
        json!({ "hook_event_name": name, "cwd": "/w", "session_id": "d1" })
    }

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            ("UserPromptSubmit", "prompt"),
            ("PreToolUse", "activity"),
            ("PostToolUse", "activity"),
            ("Stop", "turn-ended"),
        ];
        for (name, signal) in cases {
            assert_eq!(signal_of("droid", event(name)), Some(signal), "{name}");
        }
    }

    #[test]
    fn takes_a_permission_or_question_notification_as_a_wait() {
        for kind in ["permission_prompt", "elicitation_dialog"] {
            let mut raw = event("Notification");
            raw["notification_type"] = json!(kind);
            assert_eq!(signal_of("droid", raw), Some("asking"), "{kind}");
        }
        let mut raw = event("Notification");
        raw["notification_type"] = json!("idle_prompt");
        assert_eq!(signal_of("droid", raw), None);
    }

    #[test]
    fn ignores_other_events_and_fails_on_bad_input() {
        assert_eq!(signal_of("droid", event("SessionEnd")), None);
        let found = normalize("droid", event("Stop")).unwrap();
        assert_eq!(found.details["sessionId"], json!("d1"));
        assert!(try_normalize("droid", json!({ "tool_name": 1 })).is_err());
    }
}

mod opencode {
    use super::*;

    fn event(name: &str) -> Value {
        json!({ COPILOT_EVENT_FIELD: name, "cwd": "/w", "sessionID": "o1" })
    }

    #[test]
    fn turns_each_event_into_its_signal() {
        let cases = [
            ("chatMessage", "prompt"),
            ("toolBefore", "activity"),
            ("toolAfter", "activity"),
            ("permissionAsked", "asking"),
            ("permissionV2Asked", "asking"),
            ("sessionIdle", "turn-ended"),
            ("sessionError", "failed"),
        ];
        for (name, signal) in cases {
            assert_eq!(signal_of("opencode", event(name)), Some(signal), "{name}");
        }
    }

    #[test]
    fn names_the_tool_the_session_and_the_error() {
        let mut raw = event("toolBefore");
        raw["tool"] = json!("edit");
        raw["args"] = json!({ "filePath": "/w/src/a.ts" });
        let found = normalize("opencode", raw).unwrap();
        assert_eq!(found.summary.as_deref(), Some("edit src/a.ts"));
        assert_eq!(found.details["sessionId"], json!("o1"));
        let mut failed = event("sessionError");
        failed["error"] = json!("boom");
        let found = normalize("opencode", failed).unwrap();
        assert_eq!(found.details["message"], json!("boom"));
    }

    #[test]
    fn ignores_other_events_and_the_end_of_natamux_ask_and_fails_on_bad_input() {
        assert_eq!(signal_of("opencode", event("somethingElse")), None);
        let mut ask = event("toolAfter");
        ask["args"] = json!({ "command": "natamux ask x" });
        assert_eq!(signal_of("opencode", ask), None);
        assert!(try_normalize("opencode", json!({ "cwd": "/w" })).is_err());
    }
}
