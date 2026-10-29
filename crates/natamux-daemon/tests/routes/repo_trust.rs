//! Repo trust (S-02, S-03): the commands of a project, their approval, and the setup script of
//! a new worktree.

use std::time::Duration;

use hyper::Method;
use serde_json::{Value, json};

use crate::server::{EchoPty, TOKEN, TestDaemon, bare};

const TRUST: &str = "/repos/default/trust";
const CHECKED: &str =
    r#"{"stages":[{"id":"test","kind":"agent","gate":"auto","check":"pnpm test"}]}"#;

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

    write_settings(&d, &CHECKED.replace("pnpm test", "pnpm test2")).await;
    assert_eq!(trust(&d).await["trusted"], false);
    let (old, _) = approve(&d, &shown["hash"]).await;
    assert_eq!(old, 409);
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
