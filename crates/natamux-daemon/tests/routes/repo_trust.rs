//! Repo trust (S-02, S-03): the commands of a project, their approval, and the setup script of
//! a new worktree.

use std::time::Duration;

use hyper::Method;
use serde_json::{Value, json};

use crate::server::{EchoPty, TOKEN, TestDaemon, bare};

const TRUST: &str = "/repos/default/trust";
const CHECKED: &str = r#"{"stages":[{"id":"context","kind":"context","gate":"manual"},{"id":"test","kind":"agent","gate":"auto","check":"pnpm test"}]}"#;

/// Writes the settings of the default project. The daemon looks at the file again after a
/// second.
async fn write_settings(d: &TestDaemon, text: &str) {
    std::fs::write(d.repo().join(".natamux.json"), text).unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
}

async fn trust(d: &TestDaemon) -> Value {
    let (status, body) = d.get(TRUST).await;
    assert_eq!(status, 200, "{body}");
    body
}

async fn approve(d: &TestDaemon, hash: &Value) -> (u16, Value) {
    let body = json!({ "hash": hash });
    d.post(TRUST, &body).await
}

/// Each line typed into a terminal of the daemon.
fn typed(d: &TestDaemon) -> Vec<String> {
    d.ptys().iter().flat_map(EchoPty::written).collect()
}

/// A git project with both setup scripts committed, so a new worktree has them.
async fn with_setup_script() -> TestDaemon {
    let d = bare("trust-setup").await;
    std::fs::write(d.repo().join(".natamux-setup"), "touch setup-ran\n").unwrap();
    std::fs::write(d.repo().join(".natamux-setup.ps1"), "New-Item setup-ran\n").unwrap();
    d.make_git_repo();
    d
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_the_agent_token_on_the_trust_routes() {
    let d = bare("trust-agent").await;
    let agent_token: &str = &d.agent_token("T-0001");
    let (read, _) = d.call(Method::GET, TRUST, Some(agent_token)).await;
    assert_eq!(read, 403);
    let hash = trust(&d).await["hash"].clone();
    let body = json!({ "hash": hash });
    let (approved, _) = d.send(Method::POST, TRUST, Some(agent_token), &body).await;
    assert_eq!(approved, 403);
}

#[tokio::test(flavor = "multi_thread")]
async fn shows_the_commands_and_approves_only_the_hash_that_was_shown() {
    let d = bare("trust-approve").await;
    write_settings(&d, CHECKED).await;

    let shown = trust(&d).await;
    assert_eq!(shown["trusted"], false);
    assert_eq!(
        shown["commands"],
        json!([{ "where": "Stage test: check", "text": "pnpm test" }])
    );
    let (_, listed) = d.get("/repos").await;
    assert_eq!(listed["repos"][0]["trusted"], false);

    let (wrong, _) = approve(&d, &json!("0000")).await;
    assert_eq!(wrong, 409);
    let (status, approved) = approve(&d, &shown["hash"]).await;
    assert_eq!(status, 200, "{approved}");
    assert_eq!(approved["trusted"], true);
    let (_, listed) = d.get("/repos").await;
    assert_eq!(listed["repos"][0]["trusted"], true);

    // Personal mode: the approval covers later changes; the old hash is still refused.
    write_settings(&d, &CHECKED.replace("pnpm test", "pnpm test2")).await;
    let after = trust(&d).await;
    assert_eq!(after["trusted"], true);
    assert_eq!(after["covered"], true);
    assert_eq!(after["changed"], Value::Null);
    let (old, _) = approve(&d, &shown["hash"]).await;
    assert_eq!(old, 409);

    d.make_task("A").await;
    let worktree = d.give_workspace("T-0001");
    std::fs::write(worktree.join(".mcp.json"), "{}\n").unwrap();
    let (status, in_task) = d.get(TASK_TRUST).await;
    assert_eq!(status, 200, "{in_task}");
    assert_eq!(in_task["trusted"], true);
    assert_eq!(in_task["covered"], true);
    assert_eq!(in_task["changed"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settings_save_by_the_user_approves_the_saved_commands() {
    let d = bare("trust-save").await;
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    let (saved, answer) = d
        .send(Method::PUT, "/repos/default/settings", Some(TOKEN), &body)
        .await;
    assert_eq!(saved, 200, "{answer}");

    let view = trust(&d).await;

    assert_eq!(view["trusted"], true);
    assert_eq!(view["commands"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn skips_the_setup_script_of_a_project_that_is_not_approved_and_logs_why() {
    let d = with_setup_script().await;
    assert_eq!(trust(&d).await["trusted"], false);

    let worktree = d.task_with_workspace().await;
    d.settle();

    assert!(worktree.join(".natamux-setup").exists());
    let lines = typed(&d);
    assert!(
        !lines.iter().any(|line| line.contains(".natamux-setup")),
        "{lines:?}"
    );
    let skipped = ("workspace.warning".to_string(), "system".to_string());
    let events = d.events_of("T-0001").await;
    assert!(events.contains(&skipped), "{events:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_the_setup_script_once_its_project_is_approved() {
    let d = with_setup_script().await;
    let shown = trust(&d).await;
    assert_eq!(shown["setup"].as_array().unwrap().len(), 2);
    let (status, _) = approve(&d, &shown["hash"]).await;
    assert_eq!(status, 200);

    d.task_with_workspace().await;
    d.settle();

    let lines = typed(&d);
    assert!(
        lines.iter().any(|line| line.contains(".natamux-setup")),
        "{lines:?}"
    );
}

/// T-0001 with a worktree that holds an MCP server config: not approved yet. Its own
/// `.natamux.json` makes it Strict, where only the approved digest is trusted.
async fn task_with_config(name: &str) -> (TestDaemon, std::path::PathBuf) {
    let d = bare(name).await;
    d.make_task("A").await;
    let worktree = d.give_workspace("T-0001");
    std::fs::write(worktree.join(".mcp.json"), "{}\n").unwrap();
    let strict = r#"{"security":{"strict":true}}"#;
    std::fs::write(worktree.join(".natamux.json"), strict).unwrap();
    (d, worktree)
}

const TASK_TRUST: &str = "/repos/default/trust?task=T-0001";

async fn launch_check(d: &TestDaemon, task: &str, token: &str, digest: &Value) -> u16 {
    let path = format!("/tasks/{task}/launch-check");
    let body = json!({ "digest": digest });
    d.send(Method::POST, &path, Some(token), &body).await.0
}

// T-0373: the approval of a task's worktree digest, and the pre-exec check of a launch.
#[tokio::test(flavor = "multi_thread")]
async fn approves_a_worktree_digest_and_checks_it_before_the_agent_runs() {
    let (d, worktree) = task_with_config("trust-launch-check").await;
    d.make_task("B").await;
    let (status, shown) = d.get(TASK_TRUST).await;
    assert_eq!(status, 200, "{shown}");
    assert_eq!(shown["trusted"], false);
    assert_eq!(shown["changed"], json!([".mcp.json"]));
    let digest = shown["hash"].clone();
    let own = d.agent_token("T-0001");
    assert_eq!(launch_check(&d, "T-0001", &own, &digest).await, 409);

    let (approved, body) = d.post(TASK_TRUST, &json!({ "hash": digest })).await;
    assert_eq!(approved, 200, "{body}");
    assert_eq!(launch_check(&d, "T-0001", &own, &digest).await, 200);
    let other = d.agent_token("T-0002");
    assert_eq!(launch_check(&d, "T-0001", &other, &digest).await, 403);

    std::fs::write(worktree.join(".mcp.json"), "{ }\n").unwrap();
    assert_eq!(launch_check(&d, "T-0001", &own, &digest).await, 409);
    let (stale, _) = d.post(TASK_TRUST, &json!({ "hash": digest })).await;
    assert_eq!(stale, 409);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_the_agent_token_on_the_trust_routes_of_a_task() {
    let (d, _) = task_with_config("trust-task-agent").await;
    let own: &str = &d.agent_token("T-0001");
    let (read, _) = d.call(Method::GET, TASK_TRUST, Some(own)).await;
    assert_eq!(read, 403);
    let hash = d.get(TASK_TRUST).await.1["hash"].clone();
    let body = json!({ "hash": hash });
    let (approved, _) = d.send(Method::POST, TASK_TRUST, Some(own), &body).await;
    assert_eq!(approved, 403);
    let (_, view) = d.get(TASK_TRUST).await;
    assert_eq!(view["trusted"], false);
}
