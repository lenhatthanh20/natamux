//! T-0373 Part 3 (F6): what the agent token of task A reaches with tasks A (T-0001), B (T-0002,
//! another task) and C (T-0003, a subtask of A), each with its own marker. The Strict cases
//! run on one thread, so the security mode a test forces is the one the routes read.

use hyper::Method;
use natamux_core::security::{AgentApi, Forced, SecurityMode, StrictSettings, force};
use natamux_daemon::http::agent_actions::AGENT_DENIED;
use natamux_daemon::privacy::audit::audit_path;
use natamux_daemon::shared::event_bus::DaemonMessage;
use natamux_daemon::shared::jobs::Jobs;
use serde_json::{Value, json};

use crate::server::{TOKEN, TestDaemon, bare, eventually};

async fn three_tasks(name: &str) -> TestDaemon {
    let d = bare(name).await;
    for body in [
        json!({ "title": "A", "description": "A-MARK" }),
        json!({ "title": "B", "description": "B-MARK" }),
        json!({ "title": "C", "description": "C-MARK", "parentId": "T-0001" }),
    ] {
        let (status, made) = d.send(Method::POST, "/tasks", Some(TOKEN), &body).await;
        assert_eq!(status, 201, "{made}");
    }
    d
}

/// The ids of a task list, sorted.
fn ids(body: &Value) -> Vec<String> {
    let tasks = body["tasks"].as_array().unwrap();
    let mut ids: Vec<String> = tasks
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_own_task_and_its_subtask_view_and_only_the_own_project() {
    let d = three_tasks("scope-lists").await;
    let a = d.agent_token("T-0001");

    let (status, listed) = d.call(Method::GET, "/tasks", Some(&a)).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        ids(&listed),
        ["T-0001", "T-0003"].map(String::from).to_vec()
    );
    let text = listed.to_string();
    assert!(
        !text.contains("T-0002") && !text.contains("B-MARK"),
        "{text}"
    );
    assert!(
        text.contains("A-MARK") && !text.contains("C-MARK"),
        "{text}"
    );

    let (status, child) = d.call(Method::GET, "/tasks/T-0003", Some(&a)).await;
    assert_eq!(status, 200, "{child}");
    let keys: Vec<&String> = child["task"].as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 9, "{keys:?}");
    assert_eq!(child["task"]["parentId"], "T-0001");
    assert!(child["task"].get("description").is_none());
    assert_eq!(d.call(Method::GET, "/tasks/T-0002", Some(&a)).await.0, 403);

    let (status, repos) = d.call(Method::GET, "/repos", Some(&a)).await;
    assert_eq!(status, 200, "{repos}");
    assert_eq!(
        repos,
        json!({ "repos": [{ "id": "default", "name": "default" }] })
    );
    // The desktop still gets everything.
    let (_, all) = d.call(Method::GET, "/repos", Some(TOKEN)).await;
    assert!(all["repos"][0]["path"].is_string() && all["worktreesDir"].is_string());
    let (_, all) = d.call(Method::GET, "/tasks", Some(TOKEN)).await;
    assert_eq!(ids(&all).len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn global_reads_carry_no_data_of_another_task() {
    let d = three_tasks("scope-global").await;
    let a = d.agent_token("T-0001");
    let jobs = &d.daemon.state.jobs;
    let _own = Jobs::start(jobs, "push", "Push A", Some("T-0001"));
    let _other = Jobs::start(jobs, "push", "Push B-MARK", Some("T-0002"));

    let (status, listed) = d.call(Method::GET, "/jobs", Some(&a)).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["jobs"].as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed["jobs"][0]["taskId"], "T-0001");
    for path in ["/usage", "/activity", "/diffstats", "/workspaces/preparing"] {
        let (status, body) = d.call(Method::GET, path, Some(&a)).await;
        assert_eq!(
            status,
            d.call(Method::GET, path, Some(TOKEN)).await.0,
            "{path}"
        );
        assert!(!body.to_string().contains("T-0002"), "{path}: {body}");
    }
    for path in [
        "/templates",
        "/limits",
        "/repos/other/settings",
        "/repos/other/skills",
        "/git/branches?repo=other",
        "/git/remote-branches?repo=other",
    ] {
        assert_eq!(d.call(Method::GET, path, Some(&a)).await.0, 403, "{path}");
    }
    for path in [
        "/repos/default/settings",
        "/repos/default/skills",
        "/git/branches",
    ] {
        let user = d.call(Method::GET, path, Some(TOKEN)).await.0;
        assert_eq!(d.call(Method::GET, path, Some(&a)).await.0, user, "{path}");
    }
}

fn mentions(frame: &Value, id: &str) -> bool {
    frame.to_string().contains(id)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_event_socket_of_an_agent_gets_its_own_and_its_subtasks_frames_only() {
    let d = three_tasks("scope-events").await;
    let a = d.agent_token("T-0001");
    let mut socket = d.dashboard(&a).await;
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 1).await;

    for id in ["T-0002", "T-0001", "T-0003"] {
        if id == "T-0001" {
            bus.publish(DaemonMessage::ReposChanged);
            bus.publish(DaemonMessage::JobsChanged { jobs: Vec::new() });
        }
        let path = format!("/tasks/{id}");
        let body = json!({ "title": format!("Moved {id}") });
        assert_eq!(
            d.send(Method::PATCH, &path, Some(TOKEN), &body).await.0,
            200
        );
    }

    socket
        .until(|socket| {
            socket
                .controls()
                .iter()
                .any(|frame| mentions(frame, "Moved T-0003"))
        })
        .await;
    let frames = socket.controls();
    assert!(
        frames.iter().any(|frame| mentions(frame, "Moved T-0001")),
        "{frames:?}"
    );
    assert!(
        frames.iter().all(|frame| !mentions(frame, "T-0002")),
        "{frames:?}"
    );
    for kind in ["jobs.changed", "repos.changed"] {
        assert!(
            frames.iter().all(|frame| frame["kind"] != kind),
            "{frames:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_cannot_reach_past_its_task_when_it_makes_or_edits_one() {
    let d = three_tasks("scope-fields").await;
    let a = d.agent_token("T-0001");
    for refused in [
        json!({ "title": "D", "parentId": "T-0002" }),
        json!({ "title": "D", "parentId": "T-0001", "dependsOn": ["T-0002"] }),
        json!({ "title": "D", "parentId": "T-0001", "repo": "other" }),
    ] {
        let (status, _) = d.send(Method::POST, "/tasks", Some(&a), &refused).await;
        assert_eq!(status, 403, "{refused}");
    }
    let (_, all) = d.call(Method::GET, "/tasks", Some(TOKEN)).await;
    assert_eq!(ids(&all).len(), 3, "a refused create made a task");
    let inside = json!({ "title": "D", "parentId": "T-0001", "dependsOn": ["T-0003"] });
    assert_eq!(
        d.send(Method::POST, "/tasks", Some(&a), &inside).await.0,
        201
    );

    for refused in [
        json!({ "dependsOn": ["T-0002"] }),
        json!({ "repo": "default" }),
        json!({ "parentId": "T-0002" }),
    ] {
        let (status, _) = d
            .send(Method::PATCH, "/tasks/T-0001", Some(&a), &refused)
            .await;
        assert_eq!(status, 403, "{refused}");
    }
    let (_, own) = d.call(Method::GET, "/tasks/T-0001", Some(TOKEN)).await;
    assert_eq!(own["task"]["dependsOn"], json!([]));
    let inside = json!({ "dependsOn": ["T-0003"] });
    let (status, edited) = d
        .send(Method::PATCH, "/tasks/T-0001", Some(&a), &inside)
        .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["task"]["dependsOn"], json!(["T-0003"]));
}

/// `METHOD path` of an `AGENT_DENIED` entry, with ids for its `*` segments.
fn denied_routes() -> Vec<(Method, String)> {
    AGENT_DENIED
        .iter()
        .map(|(method, pattern)| {
            let path = pattern.replace('*', "T-0001");
            (
                Method::from_bytes(method.as_bytes()).unwrap(),
                format!("/{path}"),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unclassified_or_denied_route_answers_403_to_agents() {
    let d = three_tasks("scope-unknown").await;
    let a = d.agent_token("T-0001");
    let unknown = [
        (Method::GET, "/nothing".to_string()),
        (Method::GET, "/tasks/T-0001/nothing".to_string()),
        (Method::POST, "/tasks/T-0001/terminals".to_string()),
    ];
    for (method, path) in unknown.into_iter().chain(denied_routes()) {
        if path == "/health" {
            continue;
        }
        let (status, body) = d.send(method.clone(), &path, Some(&a), &json!({})).await;
        assert_eq!(status, 403, "{method} {path}: {body}");
    }
    assert_eq!(d.call(Method::GET, "/nothing", Some(TOKEN)).await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_reads_its_subtask_files_in_personal_mode() {
    let d = three_tasks("scope-parent").await;
    let folder = d.give_workspace("T-0003");
    std::fs::write(folder.join("notes.txt"), "C-MARK").unwrap();
    let a = d.agent_token("T-0001");
    for path in ["/tasks/T-0003/tree", "/tasks/T-0003/file?path=notes.txt"] {
        let user = d.call(Method::GET, path, Some(TOKEN)).await.0;
        let agent = d.call(Method::GET, path, Some(&a)).await.0;
        assert_ne!(agent, 403, "{path}");
        assert_eq!(agent, user, "{path}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_token_answers_401_and_its_tickets_stop_working() {
    let d = three_tasks("scope-revoke").await;
    d.give_workspace("T-0001");
    let a = d.agent_token("T-0001");
    assert_eq!(d.call(Method::GET, "/tasks/T-0001", Some(&a)).await.0, 200);
    let (status, body) = d
        .call(Method::POST, "/tasks/T-0001/terminal/ticket", Some(&a))
        .await;
    assert_eq!(status, 200, "{body}");
    let ticket = body["ticket"].as_str().unwrap().to_string();

    d.daemon.state.terminals.revoke_agent("T-0001");

    assert_eq!(d.call(Method::GET, "/tasks/T-0001", Some(&a)).await.0, 401);
    let mut socket = d
        .open_with(&format!("ticket={ticket}&cols=80&rows=24"))
        .await;
    assert_eq!(socket.close_code().await, 4401);
    assert!(d.ptys().is_empty());
    let renewed = d.agent_token("T-0001");
    assert_ne!(renewed, a);
    assert_eq!(
        d.call(Method::GET, "/tasks/T-0001", Some(&renewed)).await.0,
        200
    );
}

fn strict(grants: &[&str]) -> Forced {
    force(SecurityMode::Strict(StrictSettings {
        agent_api: AgentApi {
            grants: grants.iter().map(|name| name.to_string()).collect(),
            require_human_for_push: false,
            ..AgentApi::default()
        },
        ..StrictSettings::default()
    }))
}

const GRANTED: [&str; 4] = [
    "git.push.own",
    "pr.create.own",
    "issue.create.own",
    "subtask.read.workspace",
];

const NEEDS_GRANT: [(&str, &str); 4] = [
    ("POST", "/tasks/T-0001/push"),
    ("POST", "/tasks/T-0001/pull-request"),
    ("POST", "/tasks/T-0001/issue"),
    ("GET", "/tasks/T-0003/tree"),
];

#[tokio::test]
async fn strict_refuses_global_reads_and_ungranted_actions_until_granted() {
    let d = three_tasks("scope-strict").await;
    d.give_workspace("T-0003");
    let a = d.agent_token("T-0001");
    {
        let _strict = strict(&[]);
        for path in ["/tasks", "/jobs", "/usage", "/repos", "/diffstats"] {
            assert_eq!(d.call(Method::GET, path, Some(&a)).await.0, 403, "{path}");
        }
        for (method, path) in NEEDS_GRANT {
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            let (status, _) = d.send(method, path, Some(&a), &json!({})).await;
            assert_eq!(status, 403, "{path}");
        }
        // The minimum still works.
        assert_eq!(d.call(Method::GET, "/tasks/T-0001", Some(&a)).await.0, 200);
        assert_eq!(d.call(Method::GET, "/tasks/T-0003", Some(&a)).await.0, 200);
        // The desktop is not scoped.
        assert_eq!(d.call(Method::GET, "/tasks", Some(TOKEN)).await.0, 200);
    }
    let _strict = strict(&GRANTED);
    for (method, path) in NEEDS_GRANT {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        let (status, body) = d.send(method, path, Some(&a), &json!({})).await;
        assert_ne!(status, 403, "{path}: {body}");
    }
}

#[tokio::test]
async fn strict_takes_the_agent_token_on_events_in_the_header_only() {
    let d = three_tasks("scope-strict-events").await;
    let a = d.agent_token("T-0001");
    let in_query = format!("/events?token={a}");
    // Without the WebSocket upgrade a known caller gets 404, an unknown one 401.
    assert_eq!(d.call(Method::GET, &in_query, None).await.0, 404);
    let _strict = strict(&["events.subscribe"]);
    assert_eq!(d.call(Method::GET, &in_query, None).await.0, 401);
    assert_eq!(d.call(Method::GET, "/events", Some(&a)).await.0, 404);
    let desktop = format!("/events?token={TOKEN}");
    assert_eq!(d.call(Method::GET, &desktop, None).await.0, 404);
    // T-0375 (R3-4): the refused `?token=` is one audit line.
    assert_eq!(denied_lines(&d), 1);
}

/// The `agent-api.denied` lines of the audit log.
fn denied_lines(d: &TestDaemon) -> usize {
    let path = audit_path(&d.root.join("data"));
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| line.contains("\"agent-api.denied\""))
        .count()
}

// T-0375 (R3-4): a refusal in a handler writes its audit line, as one in the router does.
#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_in_a_handler_adds_one_audit_line() {
    let d = three_tasks("scope-audit").await;
    let on = json!({ "strict": false, "data": { "auditLog": true } });
    let (status, _) = d.send(Method::PUT, "/security", Some(TOKEN), &on).await;
    assert_eq!(status, 200);
    let a = d.agent_token("T-0001");

    let body = json!({ "title": "D", "parentId": "T-0002" });
    let (status, _) = d.send(Method::POST, "/tasks", Some(&a), &body).await;
    assert_eq!(status, 403);
    assert_eq!(denied_lines(&d), 1);
    let ci = json!({ "stage": "implement", "on": true });
    let (status, _) = d.send(Method::PUT, "/tasks/T-0001/ci", Some(&a), &ci).await;
    assert_eq!(status, 403);
    assert_eq!(denied_lines(&d), 2);
    // The router lets an agent read its own task's comments; the handler refuses.
    let comments = "/tasks/T-0001/review-comments";
    let (status, _) = d.call(Method::GET, comments, Some(&a)).await;
    assert_eq!(status, 403);
    assert_eq!(denied_lines(&d), 3);
}

// T-0375 (R3-2): the revoke ends the sockets the token opened; the desktop's stays.
#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_token_closes_its_event_and_terminal_sockets() {
    let d = three_tasks("scope-revoke-sockets").await;
    d.give_workspace("T-0001");
    let a = d.agent_token("T-0001");
    let mut events = d.dashboard(&a).await;
    let mut desktop = d.dashboard(TOKEN).await;
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 2).await;
    let (status, body) = d
        .call(Method::POST, "/tasks/T-0001/terminal/ticket", Some(&a))
        .await;
    assert_eq!(status, 200, "{body}");
    let ticket = body["ticket"].as_str().unwrap().to_string();
    let mut terminal = d
        .open_with(&format!("ticket={ticket}&cols=80&rows=24"))
        .await;
    eventually(|| !d.ptys().is_empty()).await;

    d.daemon.state.terminals.revoke_agent("T-0001");

    assert_eq!(events.close_code().await, 4401);
    assert_eq!(terminal.close_code().await, 4401);
    assert_eq!(bus.size(), 1);
    let body = json!({ "title": "Moved T-0001" });
    let path = "/tasks/T-0001";
    assert_eq!(d.send(Method::PATCH, path, Some(TOKEN), &body).await.0, 200);
    desktop
        .until(|socket| {
            socket
                .controls()
                .iter()
                .any(|frame| mentions(frame, "Moved T-0001"))
        })
        .await;
    let frames = events.controls();
    assert!(
        frames.iter().all(|frame| !mentions(frame, "Moved T-0001")),
        "{frames:?}"
    );
}
