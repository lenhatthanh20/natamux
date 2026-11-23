//! T-0373 Part 9 (F5): the audit log, the support bundle, retention, and where test markers may
//! end up in the data folder (the list in `docs/data-inventory.md`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use hyper::Method;
use natamux_daemon::privacy::audit::audit_path;
use natamux_daemon::privacy::retention::{RetentionReport, run_once};
use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{TOKEN, TestDaemon, daemon};

const TITLE_MARK: &str = "TITLE-MARK";
const PROMPT_MARK: &str = "PROMPT-MARK";
const TYPED_MARK: &str = "TYPED-MARK";
const TOKEN_MARK: &str = "ghp_TOKENMARK1234567";
const DAY_MS: i64 = 86_400_000;

/// Turns the audit log on through the route, as a person would.
async fn audit_on(d: &TestDaemon) {
    let body = json!({ "strict": false, "data": { "auditLog": true } });
    let (status, answer) = d.send(Method::PUT, "/security", Some(TOKEN), &body).await;
    assert_eq!(status, 200, "{answer}");
}

fn audit_lines(d: &TestDaemon) -> Vec<Value> {
    let path = audit_path(&d.root.join("data"));
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn events_of(lines: &[Value], event: &str) -> Vec<(String, String)> {
    lines
        .iter()
        .filter(|line| line["event"] == event)
        .map(|line| {
            let id = line["id"].as_str().unwrap_or_default().to_string();
            (id, line["result"].as_str().unwrap_or_default().to_string())
        })
        .collect()
}

fn now_ms() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    i64::try_from(since.as_millis()).unwrap()
}

async fn set_done(d: &TestDaemon, id: &str) {
    let body =
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "done" } });
    let path = format!("/tasks/{id}/events");
    let (status, answer) = d.send(Method::POST, &path, Some(TOKEN), &body).await;
    assert!(status < 300, "{answer}");
}

/// One retention run on a blocking thread, as the daemon's own thread runs it.
async fn retain(d: &TestDaemon, days: u32, now: i64) -> RetentionReport {
    let state = Arc::clone(&d.daemon.state);
    tokio::task::spawn_blocking(move || run_once(&state, days, now))
        .await
        .unwrap()
}

async fn task_with_workspace(d: &TestDaemon, title: &str) -> (String, PathBuf) {
    let id = d.make_task(title).await;
    let (status, body) = d.post_empty(&format!("/tasks/{id}/workspace")).await;
    assert_eq!(status, 201, "{body}");
    d.settle();
    (
        id,
        PathBuf::from(body["task"]["workspace"].as_str().unwrap()),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_bundle_holds_no_task_text_secret_or_typed_input_and_agents_get_403() {
    let d = daemon("privacy-bundle", true).await;
    audit_on(&d).await;
    let task = json!({ "title": TITLE_MARK, "description": PROMPT_MARK });
    let (status, _) = d.send(Method::POST, "/tasks", Some(TOKEN), &task).await;
    assert_eq!(status, 201);
    let log = [
        format!("push failed: https://u:{TOKEN_MARK}@h.test/x.git"),
        format!("input 1/T-1 recv +1 \"{TYPED_MARK}\""),
        "ok".to_string(),
    ]
    .join("\n");
    std::fs::create_dir_all(d.root.join("logs")).unwrap();
    std::fs::write(d.root.join("logs/daemon.log"), log).unwrap();

    let (status, body) = d.call(Method::GET, "/support-bundle", Some(TOKEN)).await;
    assert_eq!(status, 200, "{body}");
    let text = body.to_string();
    for mark in [TITLE_MARK, PROMPT_MARK, TYPED_MARK, TOKEN_MARK, TOKEN] {
        assert!(!text.contains(mark), "{mark} in {text}");
    }
    assert_eq!(body["security"]["auditLog"], true);
    assert!(body["tasks"]["draft"].as_u64().unwrap_or(0) >= 1, "{body}");
    assert_eq!(body["log"].as_array().map(Vec::len), Some(2), "{body}");

    let agent = d.agent_token("T-0001");
    let (status, _) = d.call(Method::GET, "/support-bundle", Some(&agent)).await;
    assert_eq!(status, 403);
    let made = events_of(&audit_lines(&d), "support-bundle.create");
    assert_eq!(made.len(), 1, "only the desktop call made one");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delete_and_a_security_change_write_audit_lines_without_the_title() {
    let d = daemon("privacy-audit", false).await;
    audit_on(&d).await;
    let task = json!({ "title": TITLE_MARK });
    let (_, created) = d.send(Method::POST, "/tasks", Some(TOKEN), &task).await;
    let id = created["task"]["id"].as_str().unwrap().to_string();
    let (status, _) = d
        .call(Method::DELETE, &format!("/tasks/{id}"), Some(TOKEN))
        .await;
    assert_eq!(status, 200);

    let lines = audit_lines(&d);
    let text = std::fs::read_to_string(audit_path(&d.root.join("data"))).unwrap();
    assert!(!text.contains(TITLE_MARK), "{text}");
    assert_eq!(events_of(&lines, "task.delete"), [(id, "ok".to_string())]);
    let changed = events_of(&lines, "security.changed");
    assert_eq!(changed, [("personal".to_string(), "ok".to_string())]);
}

#[tokio::test(flavor = "multi_thread")]
async fn retention_deletes_only_old_finished_tasks_without_work_and_is_safe_to_rerun() {
    let d = git_daemon("privacy-retention").await;
    audit_on(&d).await;
    let (clean, clean_ws) = task_with_workspace(&d, "Clean one").await;
    let (dirty, dirty_ws) = task_with_workspace(&d, "Dirty one").await;
    let (ahead, ahead_ws) = task_with_workspace(&d, "Ahead one").await;
    let (running, _) = task_with_workspace(&d, "Running one").await;
    std::fs::write(dirty_ws.join("notes.txt"), "work in progress").unwrap();
    run_git(&ahead_ws, &["commit", "--allow-empty", "-m", "work"]);
    for id in [&clean, &dirty, &ahead] {
        set_done(&d, id).await;
    }
    let later = now_ms() + 31 * DAY_MS;

    let none = retain(&d, 0, later).await;
    assert!(none.deleted.is_empty() && none.refused.is_empty());
    let young = retain(&d, 30, now_ms()).await;
    assert!(young.deleted.is_empty() && young.refused.is_empty());

    let report = retain(&d, 30, later).await;
    assert_eq!(report.deleted.len(), 1);
    assert_eq!(report.deleted[0], clean);
    let mut refused = report.refused.clone();
    refused.sort();
    assert_eq!(refused, [dirty.clone(), ahead.clone()]);
    assert_eq!(d.get(&format!("/tasks/{clean}")).await.0, 404);
    assert!(!clean_ws.exists());
    assert!(dirty_ws.join("notes.txt").exists());
    assert!(ahead_ws.exists());
    assert_eq!(d.get(&format!("/tasks/{running}")).await.0, 200);

    let again = retain(&d, 30, later).await;
    assert!(again.deleted.is_empty());
    assert_eq!(again.refused.len(), 2);
    let lines = audit_lines(&d);
    let runs = events_of(&lines, "retention.run");
    let wanted = ("deleted:1".to_string(), "refused:2".to_string());
    assert!(runs.contains(&wanted), "{runs:?}");
    let deletes = events_of(&lines, "task.delete");
    assert!(
        deletes.contains(&(dirty, "refused".to_string())),
        "{deletes:?}"
    );
}

/// Every file under `folder` but the worktrees, with its path inside `folder`.
fn data_files(folder: &Path, inside: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        let inner = path.strip_prefix(inside).unwrap();
        let name = inner.to_string_lossy().replace('\\', "/");
        if name == "worktrees" {
            continue;
        }
        if path.is_dir() {
            data_files(&path, inside, out);
        } else {
            out.push((name, std::fs::read(&path).unwrap_or_default()));
        }
    }
}

/// `marker <kind>: <prefix> …` lines of the data inventory.
fn allowed_places() -> Vec<(String, Vec<String>)> {
    include_str!("../../../../docs/data-inventory.md")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("marker "))
        .filter_map(|rest| rest.split_once(':'))
        .map(|(kind, places)| {
            let places: Vec<String> = places.split_whitespace().map(str::to_string).collect();
            (kind.trim().to_string(), places)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn markers_appear_only_where_the_data_inventory_allows() {
    let d = git_daemon("privacy-markers").await;
    audit_on(&d).await;
    let task = json!({ "title": "Marker task", "description": PROMPT_MARK });
    let (status, _) = d.send(Method::POST, "/tasks", Some(TOKEN), &task).await;
    assert_eq!(status, 201);
    let (status, body) = d.post_empty("/tasks/T-0001/workspace").await;
    assert_eq!(status, 201, "{body}");
    d.settle();
    // After the worktree is made, so making it does not try the remote.
    let remote = format!("https://natamux:{TOKEN_MARK}@127.0.0.1:9/acme/app.git");
    run_git(&d.repo(), &["remote", "add", "origin", &remote]);
    let (status, pushed) = d.post_empty("/tasks/T-0001/push").await;
    assert_ne!(status, 200, "the remote is not there: {pushed}");
    assert!(!pushed.to_string().contains(TOKEN_MARK), "{pushed}");

    let mut socket = d.open("cols=80&rows=24").await;
    socket.until(|socket| socket.replayed()).await;
    let input = format!(r#"{{"type":"input","data":"{TYPED_MARK}"}}"#);
    socket.send(&input).await;
    socket
        .until(|socket| socket.output().contains(TYPED_MARK))
        .await;
    d.daemon.state.terminals.save_screens();

    let places = allowed_places();
    let kinds: Vec<&str> = places.iter().map(|(kind, _)| kind.as_str()).collect();
    assert_eq!(kinds, ["prompt", "typed-input", "remote-token"]);
    let marks = [PROMPT_MARK, TYPED_MARK, TOKEN_MARK];
    let data = d.root.join("data");
    let mut files = Vec::new();
    data_files(&data, &data, &mut files);
    let mut seen_prompt = false;
    for (name, bytes) in &files {
        for ((kind, allowed), mark) in places.iter().zip(marks) {
            let holds = bytes.windows(mark.len()).any(|w| w == mark.as_bytes());
            if !holds {
                continue;
            }
            seen_prompt |= mark == PROMPT_MARK;
            let ok = allowed.iter().any(|place| name.starts_with(place));
            assert!(ok, "{kind} marker in {name}, allowed only in {allowed:?}");
        }
    }
    assert!(
        seen_prompt,
        "the prompt is stored somewhere: the scan works"
    );
}
