//! The Claude Code adapter and background work: tasks that still run, scheduled prompts, and
//! prompts that background work sends (T-0390).

use super::*;
use crate::hooks::SCHEDULED_KEY;
use natamux_core::js::utf16_len;

fn run(raw: Value) -> HookResult {
    normalize(&raw, Path::new("/home/me"))
}

fn hook(name: &str, extra: Value) -> Option<NormalizedHook> {
    let mut body = json!({ "hook_event_name": name });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        body.extend(extra.clone());
    }
    run(body).unwrap()
}

fn signal_of(name: &str, extra: Value) -> Option<&'static str> {
    hook(name, extra).map(|hook| hook.signal.as_str())
}

fn waiting_for(hook: &Option<NormalizedHook>) -> Option<Value> {
    hook.as_ref()?.details.get("waitingFor").cloned()
}

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

fn scheduled(found: &Option<NormalizedHook>) -> Option<Value> {
    found.as_ref()?.details.get(SCHEDULED_KEY).cloned()
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

// T-0390: a cron may wait for days, so a turn with only scheduled prompts left ends; the cron is
// kept and named.
#[test]
fn ends_the_turn_when_only_a_scheduled_prompt_is_left() {
    let found = stop(json!({
        "background_tasks": [],
        "session_crons": [
            { "id": "c1", "schedule": "*/5 * * * *", "recurring": true, "prompt": "check CI" },
        ],
    }));
    assert_eq!(signal(&found), Some("turn-ended"));
    assert_eq!(waiting_for(&found), None);
    assert_eq!(scheduled(&found), Some(json!(["check CI"])));
    assert_eq!(summary(&found), None);
}

#[test]
fn ends_the_turn_for_a_one_shot_prompt_with_no_background_task_list_at_all() {
    let found = stop(json!({ "session_crons": [
        { "id": "c1", "recurring": false, "prompt": "check CI" },
    ] }));
    assert_eq!(signal(&found), Some("turn-ended"));
    assert_eq!(scheduled(&found), Some(json!(["check CI"])));
}

#[test]
fn names_a_scheduled_prompt_without_its_text_by_its_schedule() {
    let found = stop(json!({ "session_crons": [
        { "id": "c1", "schedule": "0 9 * * 1-5" },
        { "id": "c2" },
    ] }));
    assert_eq!(
        scheduled(&found),
        Some(json!(["0 9 * * 1-5", "scheduled prompt"]))
    );
}

#[test]
fn a_question_wins_over_background_work_and_keeps_its_list() {
    let found = stop(json!({
        "last_assistant_message": "Wait for the tests?",
        "background_tasks": [shell("pnpm test", "running")],
    }));
    assert_eq!(signal(&found), Some("turn-asked"));
    assert_eq!(waiting_for(&found), Some(json!(["Bash: pnpm test"])));
    let message = found.as_ref().and_then(|hook| hook.details.get("message"));
    assert_eq!(message, Some(&json!("Wait for the tests?")));
}

#[test]
fn a_question_with_only_a_scheduled_prompt_left_asks_too() {
    let found = stop(json!({
        "last_assistant_message": "Merge it?",
        "session_crons": [{ "prompt": "check CI" }],
    }));
    assert_eq!(signal(&found), Some("turn-asked"));
    assert_eq!(scheduled(&found), Some(json!(["check CI"])));
}

#[test]
fn marks_a_prompt_that_background_work_sent() {
    // The shape of the prompt seen in a Claude Code 2.1.322 transcript (docs/claude-code-adapter.md).
    let prompt = "<task-notification>\n<task-id>brq24mb42</task-id>\n\
        <tool-use-id>toolu_01</tool-use-id>\n<output-file>/tmp/tasks/brq24mb42.output</output-file>\n\
        <status>completed</status>\n\
        <summary>Background command \"Start the site\" completed (exit code 0)</summary>\n\
        </task-notification>";
    let found = hook("UserPromptSubmit", json!({ "prompt": prompt })).unwrap();
    assert_eq!(found.details.get(WAKE_KEY), Some(&json!(BACKGROUND)));
    assert_eq!(found.details.get(PROMPT_HEAD_KEY), None);
}

#[test]
fn keeps_only_the_start_of_a_prompt_of_the_user() {
    let found = hook("UserPromptSubmit", json!({ "prompt": "p".repeat(300) })).unwrap();
    assert_eq!(found.details.get(WAKE_KEY), None);
    let head = found.details.get(PROMPT_HEAD_KEY).and_then(Value::as_str);
    assert_eq!(head.map(utf16_len), Some(200));
    let none = hook("UserPromptSubmit", json!({ "prompt": 7 })).unwrap();
    assert_eq!(none.details.get(PROMPT_HEAD_KEY), None);
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
fn ends_the_turn_for_a_cron_when_the_task_list_cannot_be_read() {
    let found = stop(json!({
        "background_tasks": "x",
        "session_crons": [{ "prompt": "check CI" }],
    }));
    assert_eq!(signal(&found), Some("turn-ended"));
    assert_eq!(scheduled(&found), Some(json!(["check CI"])));
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
