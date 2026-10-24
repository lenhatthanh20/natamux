//! The `repositories` and `project settings` groups of `server.test.ts`: the project routes of a
//! running Rust daemon. The default project is a real git repository, as `createTestRepo` is.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use hyper::Method;
use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{TOKEN, TestDaemon, start_daemon};

/// A folder `name` in the test root that is a git repository on `trunk`.
fn make_repo(d: &TestDaemon, name: &str, commit: bool) -> PathBuf {
    let folder = d.root.join(name);
    std::fs::create_dir(&folder).unwrap();
    run_git(&folder, &["init", "-b", "trunk"]);
    run_git(&folder, &["config", "user.name", "Test"]);
    run_git(&folder, &["config", "user.email", "test@example.com"]);
    run_git(&folder, &["config", "commit.gpgsign", "false"]);
    if commit {
        run_git(&folder, &["commit", "--allow-empty", "-m", "first"]);
    }
    folder
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

async fn add(d: &TestDaemon, body: Value) -> (u16, Value) {
    d.post("/repos", &body).await
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_default_repository_adds_another_and_makes_its_tasks_work_there() {
    let d = git_daemon("repos-list").await;
    let (_, listed) = d.get("/repos").await;
    assert_eq!(
        listed["repos"],
        json!([{ "id": "default", "name": "default", "path": path_text(&d.repo()) }])
    );
    assert_eq!(
        listed["worktreesDir"],
        path_text(&d.root.join("data/worktrees"))
    );

    let other = make_repo(&d, "other", true);
    let (status, added) = add(
        &d,
        json!({ "path": path_text(&other), "name": "Other app" }),
    )
    .await;
    assert_eq!(status, 201, "{added}");
    assert_eq!(added["repo"]["id"], "other-app");
    assert_eq!(added["repo"]["name"], "Other app");
    assert_eq!(
        std::fs::canonicalize(added["repo"]["path"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&other).unwrap()
    );

    let (_, branches) = d.get("/git/branches?repo=other-app").await;
    assert_eq!(
        branches,
        json!({ "branches": ["trunk"], "current": "trunk", "defaultBranch": "trunk" })
    );

    let (_, created) = d
        .post(
            "/tasks",
            &json!({ "title": "Fix it there", "repo": "other-app" }),
        )
        .await;
    assert_eq!(created["task"]["repo"], "other-app");
    let (_, workspace) = d.post_empty("/tasks/T-0001/workspace").await;
    let folder = PathBuf::from(workspace["task"]["workspace"].as_str().unwrap());
    assert_eq!(
        run_git(&folder, &["log", "-1", "--format=%s"]).trim(),
        "first"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_unknown_repository_a_folder_that_is_not_a_repository_and_a_repeat() {
    let d = git_daemon("repos-refuse").await;
    let (status, _) = d
        .post("/tasks", &json!({ "title": "x", "repo": "nope" }))
        .await;
    assert_eq!(status, 404);

    let plain = d.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(add(&d, json!({ "path": path_text(&plain) })).await.0, 409);
    assert_eq!(
        add(&d, json!({ "path": path_text(&d.repo()) })).await.0,
        400
    );
    assert_eq!(add(&d, json!({ "path": "some/where" })).await.0, 400);

    let other = make_repo(&d, "other", false);
    let (status, relative) = add(
        &d,
        json!({ "path": path_text(&other), "worktreesDir": "trees" }),
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        relative["message"]
            .as_str()
            .unwrap()
            .starts_with("The worktree folder "),
        "{relative}"
    );
    let secret = d.root.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    for worktrees_dir in [json!(path_text(&secret.join("trees"))), json!(42)] {
        let body = json!({ "path": path_text(&other), "worktreesDir": worktrees_dir });
        assert_eq!(add(&d, body).await.0, 400, "{worktrees_dir}");
    }
    let (_, listed) = d.get("/repos").await;
    assert_eq!(listed["repos"].as_array().unwrap().len(), 1);

    let trees = path_text(&d.root.join("other-trees"));
    let (status, added) = add(
        &d,
        json!({ "path": path_text(&other), "worktreesDir": trees }),
    )
    .await;
    assert_eq!(status, 201, "{added}");
    assert_eq!(added["repo"]["worktreesDir"], trees);
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_a_repository_once_it_has_no_workspaces_and_keeps_its_tasks() {
    let d = git_daemon("repos-remove").await;
    let other = make_repo(&d, "other", true);
    let settings_text = "{ \"stages\": [\"implement\"] }\n";
    std::fs::write(other.join(".natamux.json"), settings_text).unwrap();
    add(
        &d,
        json!({ "path": path_text(&other), "name": "Other app" }),
    )
    .await;
    for title in ["With a workspace", "Without one"] {
        d.post("/tasks", &json!({ "title": title, "repo": "other-app" }))
            .await;
    }
    d.post_empty("/tasks/T-0001/workspace").await;

    let (status, busy) = d.delete("/repos/other-app").await;
    assert_eq!(status, 409);
    assert_eq!(busy["error"], "conflict");
    assert_eq!(busy["message"], "Remove the 1 workspace of other-app first");

    d.delete("/tasks/T-0001/workspace").await;
    let task_file = d.root.join("data/tasks/T-0002.md");
    let task_text = std::fs::read_to_string(&task_file).unwrap();
    let (status, _, body) = d
        .exchange(Method::DELETE, "/repos/other-app", Some(TOKEN), None)
        .await;
    assert_eq!(status, 204);
    assert!(body.is_empty());
    let (_, listed) = d.get("/repos").await;
    let ids: Vec<&str> = listed["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["default"]);
    assert!(other.exists());
    assert_eq!(
        std::fs::read_to_string(other.join(".natamux.json")).unwrap(),
        settings_text
    );
    assert!(run_git(&other, &["branch", "--list", "natamux/*"]).contains("natamux/t-0001"));
    assert_eq!(std::fs::read_to_string(&task_file).unwrap(), task_text);

    let (_, kept) = d.get("/tasks/T-0002").await;
    assert_eq!(kept["task"]["repo"], "other-app");
    let (status, workspace) = d.post_empty("/tasks/T-0002/workspace").await;
    assert_eq!(status, 404);
    assert_eq!(
        workspace["message"],
        "Repository other-app is not known. Add it first"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_the_default_repository_like_any_other() {
    let d = git_daemon("repos-remove-default").await;
    d.post(
        "/workspaces",
        &json!({ "title": "In the default repository" }),
    )
    .await;
    d.settle();

    let (status, busy) = d.delete("/repos/default").await;
    assert_eq!(status, 409);
    assert_eq!(busy["message"], "Remove the 1 workspace of default first");

    d.delete("/tasks/T-0001/workspace").await;
    assert_eq!(d.delete("/repos/default").await.0, 204);
    let (_, listed) = d.get("/repos").await;
    assert_eq!(listed["repos"], json!([]));
    assert!(d.repo().exists());

    let (status, again) = d.delete("/repos/default").await;
    assert_eq!(status, 404);
    assert_eq!(again["error"], "not_found");
    let (status, unknown) = d.delete("/repos/missing").await;
    assert_eq!(status, 404);
    assert_eq!(unknown["error"], "not_found");
    assert_eq!(d.delete("/repos/Bad_Id").await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn asks_for_a_project_first_when_a_task_names_none_and_there_is_no_default_one() {
    let d = git_daemon("repos-none").await;
    d.delete("/repos/default").await;

    let (status, task) = d.post("/tasks", &json!({ "title": "Nowhere to go" })).await;
    assert_eq!(status, 404);
    assert!(
        task["message"]
            .as_str()
            .unwrap()
            .contains("Add a project first")
    );
    let (status, workspace) = d
        .post("/workspaces", &json!({ "title": "Nowhere to go" }))
        .await;
    assert_eq!(status, 404);
    assert!(
        workspace["message"]
            .as_str()
            .unwrap()
            .contains("Add a project first")
    );

    for url in ["/tasks", "/workspaces"] {
        let body = json!({ "title": "Nowhere to go", "repo": null });
        let (status, explicit) = d.post(url, &body).await;
        assert_eq!(status, 404, "{url}");
        assert!(
            explicit["message"]
                .as_str()
                .unwrap()
                .contains("Add a project first")
        );
    }

    let (_, listed) = d.get("/tasks").await;
    assert_eq!(listed["tasks"], json!([]));
    assert!(!d.root.join("data/worktrees").exists());
    assert_eq!(d.get("/repos").await.0, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn puts_a_task_that_names_no_repository_or_null_in_the_default_one() {
    let d = git_daemon("repos-default").await;
    for body in [
        json!({ "title": "Missing" }),
        json!({ "title": "Null", "repo": null }),
    ] {
        let (status, created) = d.post("/tasks", &body).await;
        assert_eq!(status, 201, "{created}");
        assert!(created["task"]["repo"].is_null());
    }
    let (status, _) = d
        .post(
            "/workspaces",
            &json!({ "title": "Null workspace", "repo": null }),
        )
        .await;
    assert_eq!(status, 202);
    d.settle();
    let (_, task) = d.get("/tasks/T-0003").await;
    assert!(
        task["task"]["workspace"]
            .as_str()
            .unwrap()
            .contains("T-0003")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tells_the_dashboards_when_a_repository_is_added_or_removed() {
    let d = git_daemon("repos-events").await;
    let messages: Arc<Mutex<Vec<&'static str>>> = Arc::default();
    let sink = Arc::clone(&messages);
    d.daemon
        .state
        .bus
        .subscribe(move |message| sink.lock().unwrap().push(message.kind()));
    let other = make_repo(&d, "other", false);

    add(&d, json!({ "path": path_text(&other) })).await;
    d.delete("/repos/other").await;
    d.delete("/repos/missing").await;

    assert_eq!(
        *messages.lock().unwrap(),
        ["repos.changed", "repos.changed"]
    );
}

const SETTINGS: &str = "/repos/default/settings";

async fn put_settings(d: &TestDaemon, headers: &[(&str, &str)], body: Value) -> (u16, Value) {
    let (status, _, bytes) = d
        .exchange_with(Method::PUT, SETTINGS, Some(TOKEN), headers, Some(&body))
        .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn saves_the_permission_mode_of_the_agents_and_refuses_an_unknown_one() {
    let d = git_daemon("settings-mode").await;
    let (_, before) = d.get(SETTINGS).await;
    assert_eq!(
        before["settings"]["agents"],
        json!({ "permissionMode": "auto" })
    );

    let body = json!({ "agents": { "permissionMode": "ask" } });
    assert_eq!(put_settings(&d, &[], body).await.0, 200);
    let (_, after) = d.get(SETTINGS).await;
    assert_eq!(
        after["settings"]["agents"],
        json!({ "permissionMode": "ask" })
    );

    let body = json!({ "agents": { "permissionMode": "yolo" } });
    let (status, wrong) = put_settings(&d, &[], body).await;
    assert_eq!(status, 400);
    assert!(
        wrong["message"]
            .as_str()
            .unwrap()
            .contains("agents.permissionMode")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_save_over_a_file_that_changed_since_the_editor_read_it() {
    let d = git_daemon("settings-stale").await;
    let (_, before) = d.get(SETTINGS).await;
    assert_eq!(before["found"], false);
    assert!(before["text"].is_null());
    assert_eq!(before["version"], "none");

    let (status, saved) = put_settings(&d, &[("if-match", "none")], json!({ "team": "web" })).await;
    assert_eq!(status, 200, "{saved}");
    let (_, after) = d.get(SETTINGS).await;
    assert!(
        after["text"]
            .as_str()
            .unwrap()
            .contains("\"team\": \"web\"")
    );
    assert_eq!(saved["version"], after["version"]);

    let (status, stale) = put_settings(&d, &[("if-match", "none")], json!({})).await;
    assert_eq!(status, 409);
    assert!(
        stale["message"]
            .as_str()
            .unwrap()
            .contains("changed since you opened it")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_skills_of_the_agents_for_a_repository() {
    let root = crate::server::test_root("settings-skills");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let d = start_daemon(
        root,
        None,
        natamux_proto::terminals::messages::SILENT_CLOSE,
        Some(home),
    )
    .await;
    let dir = d.repo().join(".claude/skills/review");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: review\ndescription: Reviews a diff\n---\n",
    )
    .unwrap();

    let (status, body) = d.get("/repos/default/skills").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["skills"],
        json!([{
            "name": "review",
            "description": "Reviews a diff",
            "origin": "project",
            "agents": ["claude-code", "copilot", "cursor"],
            "path": ".claude/skills/review/SKILL.md",
        }])
    );
}
