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

// T-0008: the end of the Bash call that ran `natamux ask` resumed the task the ask had stopped.
#[test]
fn ignores_the_end_of_the_call_that_ran_natamux_ask() {
    let ask = json!({ "tool_name": "Bash", "tool_input": { "command": "natamux ask \"Why?\"" } });
    assert_eq!(signal_of("PostToolUse", ask.clone()), None);
    assert_eq!(signal_of("PreToolUse", ask), Some("activity"));
    let other = json!({ "tool_name": "Bash", "tool_input": { "command": "natamux done" } });
    assert_eq!(signal_of("PostToolUse", other), Some("activity"));
}

#[test]
fn waits_for_the_user_while_the_options_dialog_is_open() {
    let options = json!({ "tool_name": "AskUserQuestion", "tool_input": {} });
    assert_eq!(signal_of("PreToolUse", options.clone()), Some("asking"));
    assert_eq!(signal_of("PostToolUse", options), Some("activity"));
}

#[test]
fn a_turn_that_ends_on_a_question_asks_and_shows_the_question() {
    let found = hook(
        "Stop",
        json!({ "last_assistant_message": "Tests pass.\n\n**Open the PR now?**" }),
    )
    .unwrap();
    assert_eq!(found.signal, Signal::TurnAsked);
    assert_eq!(
        found.details.get("message"),
        Some(&json!("Open the PR now?"))
    );
}

#[test]
fn a_turn_that_ends_on_a_report_or_without_a_message_just_ends() {
    let report = json!({ "last_assistant_message": "Done. Why? The test was flaky.\nFixed." });
    assert_eq!(signal_of("Stop", report), Some("turn-ended"));
    assert_eq!(signal_of("Stop", json!({})), Some("turn-ended"));
}

// Review rev 1: an odd last message must not lose the turn end.
#[test]
fn a_null_or_odd_last_message_still_ends_the_turn() {
    for value in [Value::Null, json!(7), json!({ "text": "Why?" })] {
        let extra = json!({ "last_assistant_message": value });
        assert_eq!(signal_of("Stop", extra), Some("turn-ended"), "{value}");
    }
}
