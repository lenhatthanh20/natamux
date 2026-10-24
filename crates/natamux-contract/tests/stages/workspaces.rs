//! The workspace scenarios of part 5 (40 to 53 of docs/rust-phase4.md), each run on the Node
//! daemon and on the Rust daemon and compared. They use the third project,
//! `NATAMUX_CONTRACT_WORKSPACES_REPO` (`/data/workspaces-repo`, origin
//! `/data/workspaces-origin.git`), the pipeline of `tests/fixtures/workspaces.natamux.json`, the fake agent and the fake `gh`.
//!
//! Each daemon makes its own twin task and worktree (`CT-<n>` on Node, `CR-<n>` on Rust), so
//! the steps that touch the shared repository run on Node, then on Rust, never both at once.
//! Twin ids are `number * 100 + 1`; more tasks of a scenario use `+ 2` and on. Ids that a daemon
//! makes itself (imports, fan out) become placeholders as soon as their answer is read.
//! Tests are in `workspaces_flow.rs`, `workspaces_git.rs`, `workspaces_github.rs` and
//! `workspaces_time.rs`.

use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token};
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    both, call, cancel, get, is_sha, kept, launch_script, post, stage_events, task, twin,
    wait_event, wait_value,
};

#[path = "workspaces_flow.rs"]
mod flow;
#[path = "workspaces_git.rs"]
mod git;
#[path = "workspaces_github.rs"]
mod github;
#[path = "workspaces_time.rs"]
mod time;

/// The main checkout of the workspaces project inside the daemon containers.
pub const REPO_DIR: &str = "/data/workspaces-repo";

/// The id of the workspaces project, from `NATAMUX_CONTRACT_WORKSPACES_REPO`.
pub fn repo() -> String {
    std::env::var("NATAMUX_CONTRACT_WORKSPACES_REPO").unwrap_or_else(|_| "workspaces-repo".into())
}

/// A task of the workspaces project without a workspace. Its id is in its title, so it is the
/// twin id of `number`.
pub async fn make_task(d: &Daemon, number: u32, title: &str) -> String {
    let id = twin(d, number);
    let body = json!({
        "title": format!("{id}: {title}"),
        "repo": repo(),
        "agent": "claude-code",
    });
    let made = post(d, "/tasks", body).await;
    assert_eq!(made.status, 201, "{id}: {}", made.body);
    id
}

/// `id` (in both cases, as it is in a branch name) is `label` in what the transcript keeps.
pub fn alias(t: &mut Transcript, id: &str, label: &str) {
    t.alias(id, label);
    t.alias(&id.to_lowercase(), label);
}

/// Polls `script` in the daemon's container until it prints `yes`, at most 120 s.
pub async fn wait_exec(d: &Daemon, script: &str, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while d.exec_in(script).await.trim() != "yes" {
        assert!(Instant::now() < deadline, "{}: never got {what}", d.name);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The worktree of a task, once it has one.
pub async fn workspace_of(d: &Daemon, id: &str) -> String {
    wait_value(d, &format!("/tasks/{id}"), |body| {
        body["task"]["workspace"].as_str().map(str::to_string)
    })
    .await
}

/// The branch of a task.
pub async fn branch_of(d: &Daemon, id: &str) -> String {
    let body = get(d, &format!("/tasks/{id}")).await.body;
    body["task"]["branch"].as_str().unwrap_or_default().into()
}

/// Makes the workspace of `id` and waits until its setup script ran in Terminal 1. The folder
/// holds `setup-ran.txt` from then on, so the changes of a scenario do not move later.
pub async fn open_workspace(d: &Daemon, id: &str) -> String {
    let made = post(d, &format!("/tasks/{id}/workspace"), json!({})).await;
    assert_eq!(made.status, 201, "{id}: {}", made.body);
    let ws = workspace_of(d, id).await;
    let script = format!("test -f '{ws}/setup-ran.txt' && echo yes");
    wait_exec(d, &script, "setup-ran.txt").await;
    ws
}

/// Ends the task and removes its worktree, as far as git lets it. Scenarios do not look at the
/// result: it only keeps the shared repository tidy.
pub async fn retire(d: &Daemon, id: &str) {
    cancel(d, id).await;
    let task = get(d, &format!("/tasks/{id}")).await.body["task"].clone();
    if let Some(ws) = task["workspace"].as_str() {
        d.git_in(ws, "reset --hard -q").await;
        d.git_in(ws, "clean -fdq").await;
    }
    call(d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
    // Both daemons share the repository and give the same default ids (`T-0010`): a branch with
    // commits survives the delete, and would refuse the other daemon's task of that id.
    if let Some(branch) = task["branch"].as_str() {
        d.git_in(REPO_DIR, &format!("branch -D -q '{branch}'"))
            .await;
    }
}

/// Hashes in `text` become `<sha>`.
pub fn shaless(text: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in text.chars().chain(Some('\n')) {
        if c.is_whitespace() {
            out.push_str(if is_sha(&word) { "<sha>" } else { &word });
            word.clear();
            out.push(c);
        } else {
            word.push(c);
        }
    }
    out.pop();
    out
}

/// Saves a file through `PUT /file` with the version the editor read.
pub async fn save_with(d: &Daemon, id: &str, path: &str, text: &str, version: &str) -> Answer {
    let url = format!("/tasks/{id}/file?path={path}");
    let headers = [("if-match", version)];
    let body = Body::Json(json!({ "text": text }));
    d.request_with(Method::PUT, &url, body, Token::Daemon, &headers)
        .await
}

/// Reads the file, then saves `text` over it.
pub async fn save(d: &Daemon, id: &str, path: &str, text: &str) -> Answer {
    let file = get(d, &format!("/tasks/{id}/file?path={path}")).await;
    let version = file.body["version"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    save_with(d, id, path, text, &version).await
}

/// The `step` of each `workspace.preparing` message about the task.
pub fn steps(events: &Events, id: &str) -> Value {
    let steps: Vec<Value> = events
        .messages
        .iter()
        .filter(|m| m["kind"] == "workspace.preparing" && m["taskId"] == id)
        .map(|m| m["step"].clone())
        .collect();
    Value::Array(steps)
}

/// Waits for the `workspace.preparing` message with no step: the workspace is ready.
pub async fn preparing_done(events: &mut Events, id: &str) {
    let ready = |m: &[Value]| {
        m.iter()
            .any(|m| m["kind"] == "workspace.preparing" && m["taskId"] == id && m["step"].is_null())
    };
    assert!(
        events.until(ready).await,
        "{id}: the workspace never got ready"
    );
}

/// How many `workspace.dirty` messages the task has had.
pub fn dirty_count(messages: &[Value], id: &str) -> usize {
    messages
        .iter()
        .filter(|m| m["kind"] == "workspace.dirty" && m["taskId"] == id)
        .count()
}

/// Waits until the log of the task has `count` events of `kind`, whatever their stage.
pub async fn wait_type(d: &Daemon, id: &str, kind: &str, count: usize) {
    wait_value(d, &format!("/tasks/{id}/events"), |body| {
        let found = body["events"].as_array().into_iter().flatten();
        let found = found.filter(|entry| entry["event"]["type"] == kind).count();
        (found >= count).then_some(())
    })
    .await;
}

/// The fake `gh` of this daemon, as it starts a scenario: counters at their start, the answers
/// the scenario does not set back to the defaults. Run before the scenario's first `gh` call.
pub async fn reset_gh(d: &Daemon) {
    d.write_fake_gh("pr-counter", "0").await;
    d.write_fake_gh("issue-counter", "100").await;
    d.write_fake_gh("conclusion", "success").await;
    d.write_fake_gh("log-failed.txt", "fake gh: step failed")
        .await;
    let open = r#"{"state":"OPEN","isDraft":false,"statusCheckRollup":[]}"#;
    d.write_fake_gh("pr-state.json", open).await;
}

/// The `gh` calls that changed something, since `before` (the log at the start): reads are left
/// out, as the pull request watcher asks for them on its own timer.
pub async fn gh_writes(d: &Daemon, before: &str) -> Value {
    let all = d.fake_gh_calls().await;
    let new = all.strip_prefix(before).unwrap_or(&all);
    let reads = [
        "gh pr view",
        "gh pr list",
        "gh api graphql",
        "gh issue list",
    ];
    let writes: Vec<&str> = new
        .lines()
        .filter(|line| !reads.iter().any(|read| line.starts_with(read)))
        .collect();
    json!(shaless(&writes.join("\n")))
}

pub fn median(mut times: Vec<u128>) -> u128 {
    times.sort_unstable();
    times[times.len() / 2]
}

/// One row of the report file, as `hook_time` writes it.
pub fn report_row(line: &str) {
    print!("{line}");
    if let Ok(path) = std::env::var("NATAMUX_CONTRACT_REPORT") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

/// The list of branches as the scenarios compare it: the branches of tasks differ by daemon, so
/// they show as a flag for the task asked about.
fn branch_view(answer: &Answer, id: &str) -> Value {
    let prefix = format!("natamux/{}-", id.to_lowercase());
    let branches = answer.body["branches"].as_array().into_iter().flatten();
    let names: Vec<&str> = branches.filter_map(|b| b.as_str()).collect();
    let others: Vec<&&str> = names
        .iter()
        .filter(|n| !n.starts_with("natamux/"))
        .collect();
    let own = names.iter().any(|name| name.starts_with(&prefix));
    json!([
        answer.status,
        answer.body["current"],
        answer.body["defaultBranch"],
        others,
        own
    ])
}

/// Scenario 40: a workspace is made with and without `base`: the preparing steps, the events, the
/// branch, the setup script, the branch list, and the refusals (second workspace, unknown base,
/// unknown task, bad body).
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn workspace_create() {
    both(
        "workspace_create",
        4001,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let (based, bad) = (twin(&d, 4002), twin(&d, 4003));
            alias(&mut t, &based, "<task 2>");
            alias(&mut t, &bad, "<task 3>");
            d.git_in(REPO_DIR, "branch -f contract-base main").await;
            make_task(&d, 4001, "Create").await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let path = format!("/tasks/{id}/workspace");
            t.note("create", kept(&post(&d, &path, json!({})).await));
            preparing_done(&mut events, &id).await;
            t.note("preparing", steps(&events, &id));
            t.note("events", stage_events(&d, &id).await);
            let ws = workspace_of(&d, &id).await;
            let script = format!("test -f '{ws}/setup-ran.txt' && echo yes");
            wait_exec(&d, &script, "setup-ran.txt").await;
            t.note("setup ran", json!(d.exec_in(&script).await.trim()));
            let branches = get(&d, &format!("/git/branches?repo={}", repo())).await;
            t.note("branches", branch_view(&branches, &id));
            make_task(&d, 4002, "Base").await;
            let body = json!({ "base": "contract-base" });
            let path = format!("/tasks/{based}/workspace");
            t.note("create with base", kept(&post(&d, &path, body).await));
            t.note(
                "second workspace",
                kept(&post(&d, &format!("/tasks/{id}/workspace"), json!({})).await),
            );
            make_task(&d, 4003, "Unknown base").await;
            let path = format!("/tasks/{bad}/workspace");
            t.note(
                "unknown base",
                kept(&post(&d, &path, json!({ "base": "nope" })).await),
            );
            t.note(
                "bad body",
                kept(&post(&d, &path, json!({ "base": "" })).await),
            );
            let missing = post(&d, "/tasks/T-9999/workspace", json!({})).await;
            t.note("unknown task", kept(&missing));
            for each in [&id, &based, &bad] {
                retire(&d, each).await;
            }
            t
        },
    )
    .await;
}

/// Scenario 41: `POST /workspaces` answers 202 before the workspace exists, the workspace
/// comes ready on `/events`, nothing is left preparing; with an agent its stage starts; an
/// attachment is kept; an unknown base is refused and leaves no task.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn new_workspace() {
    both(
        "new_workspace",
        4101,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let (agent, files, bad) = (twin(&d, 4102), twin(&d, 4103), twin(&d, 4104));
            for (each, label) in [
                (&agent, "<task 2>"),
                (&files, "<task 3>"),
                (&bad, "<task 4>"),
            ] {
                alias(&mut t, each, label);
            }
            let mut events = Events::open(&d, Some(&d.token)).await;
            let body = json!({ "title": format!("{id}: New workspace"), "repo": repo() });
            let made = post(&d, "/workspaces", body).await;
            assert_eq!(
                made.body["task"]["id"], id,
                "the title makes the id: {}",
                made.body
            );
            t.note("new workspace", kept(&made));
            preparing_done(&mut events, &id).await;
            workspace_of(&d, &id).await;
            t.note("preparing steps", steps(&events, &id));
            t.note("ready", kept(&get(&d, &format!("/tasks/{id}")).await));
            t.note(
                "preparing after",
                kept(&get(&d, "/workspaces/preparing").await),
            );

            let title = format!("{agent}: With agent");
            let body = json!({ "title": title, "repo": repo(), "agent": "claude-code" });
            t.note("with agent", kept(&post(&d, "/workspaces", body).await));
            wait_event(&d, &agent, "stage.started", "build", 1).await;
            t.note("stage started", task(&d, &agent).await);
            t.note("launch script", launch_script(&d, &agent, "1").await);

            let title = format!("{files}: With a file");
            let file = json!([{ "name": "note.txt", "data": "aGVsbG8=" }]);
            let body = json!({ "title": title, "repo": repo(), "attachments": file });
            t.note(
                "with attachment",
                kept(&post(&d, "/workspaces", body).await),
            );
            workspace_of(&d, &files).await;
            t.note(
                "attachments",
                kept(&get(&d, &format!("/tasks/{files}/attachments")).await),
            );

            let title = format!("{bad}: Unknown base");
            let body = json!({ "title": title, "repo": repo(), "base": "nope" });
            t.note("unknown base", kept(&post(&d, "/workspaces", body).await));
            t.note("no task", kept(&get(&d, &format!("/tasks/{bad}")).await));
            for each in [&id, &agent, &files] {
                retire(&d, each).await;
            }
            t
        },
    )
    .await;
}
