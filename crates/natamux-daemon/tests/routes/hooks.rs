//! The groups 'Claude Code hooks', 'agent activity' and 'GET /usage' of `server.test.ts`.
//!
//! Differences from the Node tests: a task gets its workspace through `give_workspace`, as the
//! workspace routes come with part 5; and the usage test reads its transcript through the home
//! folder of a daemon that was started with one (`bare_with_home`), where Node gave its
//! `UsageService` a `home`.

use hyper::Method;
use serde_json::{Value, json};

use crate::hook_client::bare_with_home;
use crate::server::{TOKEN, TestDaemon, bare};

const HEADER: [(&str, &str); 1] = [("x-natamux-task", "T-0001")];

async fn daemon_with_task(name: &str) -> TestDaemon {
    let d = bare(name).await;
    d.make_task("Fix the bug").await;
    d
}

async fn state_of(d: &TestDaemon, id: &str) -> String {
    let (_, body) = d
        .call(Method::GET, &format!("/tasks/{id}"), Some(TOKEN))
        .await;
    body["task"]["state"].as_str().unwrap().to_string()
}

async fn post(d: &TestDaemon, payload: Value, headers: &[(&str, &str)]) -> (u16, Value) {
    d.hook("claude-code", "", headers, Some(TOKEN), &payload)
        .await
}

fn hook(name: &str) -> Value {
    json!({ "hook_event_name": name })
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token() {
    let d = bare("hooks-token").await;
    let (status, _) = d.hook("claude-code", "", &[], None, &hook("Stop")).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn moves_the_task_named_in_the_header_and_answers_with_an_empty_object() {
    let d = daemon_with_task("hooks-header").await;

    let (status, body) = post(&d, hook("UserPromptSubmit"), &HEADER).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({}));
    assert_eq!(state_of(&d, "T-0001").await, "running");

    post(&d, hook("Stop"), &HEADER).await;
    assert_eq!(state_of(&d, "T-0001").await, "needs-input");
}

#[tokio::test(flavor = "multi_thread")]
async fn moves_a_task_with_a_chosen_id_but_not_for_that_id_in_lower_case() {
    let d = bare("hooks-chosen").await;
    let id = d.make_task("Task-0005: Fix the bug").await;
    assert_eq!(id, "TASK-0005");

    post(
        &d,
        hook("UserPromptSubmit"),
        &[("x-natamux-task", "task-0005")],
    )
    .await;
    assert_eq!(state_of(&d, "TASK-0005").await, "draft");

    post(
        &d,
        hook("UserPromptSubmit"),
        &[("x-natamux-task", "TASK-0005")],
    )
    .await;
    assert_eq!(state_of(&d, "TASK-0005").await, "running");
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_task_running_when_the_agent_stops_with_background_work() {
    let d = daemon_with_task("hooks-background").await;
    post(&d, hook("UserPromptSubmit"), &HEADER).await;

    let stop = json!({
        "hook_event_name": "Stop",
        "background_tasks": [{ "id": "b1", "type": "shell", "status": "running", "command": "pnpm test" }],
        "session_crons": [],
    });
    let (status, _) = post(&d, stop, &HEADER).await;

    assert_eq!(status, 200);
    assert_eq!(state_of(&d, "T-0001").await, "running");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_200_and_ends_the_turn_as_before_for_odd_background_work_and_crons() {
    let cases = [
        (Value::Null, Value::Null),
        (json!("x"), json!("x")),
        (json!(42), json!(42)),
        (json!({}), json!({})),
        (
            json!([null, 7, { "status": "running" }]),
            json!([null, "daily"]),
        ),
    ];
    for (background_tasks, session_crons) in cases {
        let d = daemon_with_task("hooks-odd").await;
        post(&d, hook("UserPromptSubmit"), &HEADER).await;

        let stop = json!({
            "hook_event_name": "Stop",
            "background_tasks": background_tasks,
            "session_crons": session_crons,
        });
        let (status, _) = post(&d, stop, &HEADER).await;

        assert_eq!(status, 200, "{background_tasks} {session_crons}");
        assert_eq!(
            state_of(&d, "T-0001").await,
            "needs-input",
            "{background_tasks} {session_crons}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn finds_the_task_by_the_workspace_of_the_working_directory() {
    let d = daemon_with_task("hooks-cwd").await;
    let workspace = d.give_workspace("T-0001");
    let cwd = workspace.join("src").to_string_lossy().into_owned();

    post(
        &d,
        json!({ "hook_event_name": "UserPromptSubmit", "cwd": cwd }),
        &[],
    )
    .await;

    assert_eq!(state_of(&d, "T-0001").await, "running");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_200_and_changes_nothing_for_a_directory_that_belongs_to_no_task() {
    let d = daemon_with_task("hooks-elsewhere").await;

    let (status, _) = post(
        &d,
        json!({ "hook_event_name": "UserPromptSubmit", "cwd": "/elsewhere" }),
        &[],
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(state_of(&d, "T-0001").await, "draft");
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_when_the_event_name_is_missing() {
    let d = bare("hooks-400").await;
    let (status, body) = post(&d, json!({ "cwd": "/elsewhere" }), &[]).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"], "invalid_request");
}

// Not in Node's tests: `hookQuerySchema` limits `?event=` to 1 to 64 characters.
#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_an_empty_or_too_long_event_in_the_query() {
    let d = bare("hooks-event").await;
    let long = format!("?event={}", "e".repeat(65));
    for query in ["?event=", long.as_str()] {
        let (status, _) = d
            .hook("claude-code", query, &[], Some(TOKEN), &hook("Stop"))
            .await;
        assert_eq!(status, 400, "{query}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_naming_the_known_agents_for_an_agent_it_has_no_adapter_for() {
    let d = bare("hooks-404").await;

    let (status, body) = d.hook("nope", "", &[], Some(TOKEN), &hook("Stop")).await;

    assert_eq!(status, 404);
    assert_eq!(body["error"], "unknown_agent");
    assert!(body["message"].as_str().unwrap().contains("claude-code"));
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_200_and_changes_nothing_for_a_hook_the_adapter_does_not_use() {
    let d = daemon_with_task("hooks-unused").await;

    let (status, _) = post(&d, hook("SessionEnd"), &HEADER).await;

    assert_eq!(status, 200);
    assert_eq!(state_of(&d, "T-0001").await, "draft");
}

fn edit(path: &str) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": { "file_path": path },
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_what_the_agents_do_now() {
    let d = daemon_with_task("activity").await;
    // Hooks from a terminal count only while the task has its workspace.
    d.give_workspace("T-0001");
    post(&d, edit("a.ts"), &HEADER).await;
    let second = [("x-natamux-task", "T-0001"), ("x-natamux-terminal", "2")];
    post(&d, edit("test-spec.md"), &second).await;

    let (_, body) = d.call(Method::GET, "/activity", Some(TOKEN)).await;

    let activity = &body["activity"]["T-0001"];
    assert_eq!(activity["1"]["text"], "Edit a.ts");
    assert_eq!(activity["2"]["text"], "Edit test-spec.md");
}

/// One message of a Claude Code session.
fn line(id: &str, model: &str, input: u64, output: u64) -> String {
    let counts = json!({ "input_tokens": input, "output_tokens": output });
    format!(
        "{}\n",
        json!({ "message": { "id": id, "model": model, "usage": counts } })
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_the_tokens_and_cost_of_each_task_read_after_a_turn_of_its_agent() {
    let d = bare_with_home("usage").await;
    let home = d.root.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let transcript = home.join(".claude").join("session.jsonl");
    std::fs::write(
        &transcript,
        line("m1", "claude-opus-5-5", 1000, 1000) + &line("m2", "mystery-model", 10, 5),
    )
    .unwrap();
    d.make_task("Fix the bug").await;
    let stop = json!({
        "hook_event_name": "Stop",
        "transcript_path": transcript.to_string_lossy(),
    });
    post(&d, stop, &HEADER).await;
    d.daemon.state.usage.settle();

    let (status, body) = d.call(Method::GET, "/usage", Some(TOKEN)).await;

    assert_eq!(status, 200);
    let task = &body["usage"]["T-0001"];
    assert_eq!(task["tokens"], 2015);
    assert_eq!(task["unpricedModels"], json!(["mystery-model"]));
    assert!((task["costUsd"].as_f64().unwrap() - 0.024).abs() < 1e-9);
    let rows = task["stages"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["model"], "claude-opus-5-5");
    assert_eq!(rows[0]["input"], 1000);
    assert_eq!(rows[0]["output"], 1000);
    assert_eq!(rows[1]["model"], "mystery-model");
    assert_eq!(rows[1]["input"], 10);
    assert_eq!(rows[1]["output"], 5);
    assert_eq!(rows[1]["costUsd"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token_and_answers_an_empty_object_without_usage() {
    let d = bare("usage-empty").await;

    assert_eq!(d.call(Method::GET, "/usage", None).await.0, 401);
    let (_, body) = d.call(Method::GET, "/usage", Some(TOKEN)).await;
    assert_eq!(body, json!({ "usage": {} }));
}
