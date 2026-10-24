//! 54 `repos_add` and 55 `repos_remove`.

use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon};
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    PROJECT, add_project, both, call, drop_project, get, kept, plain_dir, post, project_dir,
    repos_changed, twin,
};

/// The repositories a daemon lists. The daemon's own worktree folder differs by daemon, so only
/// whether there is one is kept.
fn listed(answer: &natamux_contract::daemon::Answer) -> Value {
    json!([
        answer.status,
        answer.body["repos"],
        answer.body["worktreesDir"].is_string()
    ])
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn repos_add() {
    both(
        "repos_add",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            drop_project(&d).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let (project, plain) = (project_dir(&d), plain_dir(&d));
            t.note("list before", listed(&get(&d, "/repos").await));

            // These fail after the folder is checked and before anything is saved.
            let relative =
                json!({ "path": project, "name": PROJECT, "worktreesDir": "relative/trees" });
            t.note(
                "relative worktreesDir",
                kept(&post(&d, "/repos", relative).await),
            );
            let number = json!({ "path": project, "name": PROJECT, "worktreesDir": 42 });
            t.note("worktreesDir 42", kept(&post(&d, "/repos", number).await));
            t.note(
                "no event yet",
                json!(repos_changed(&d, &mut events, 5411).await),
            );

            let added = post(&d, "/repos", json!({ "path": project, "name": PROJECT })).await;
            t.note("add", kept(&added));
            t.note(
                "add event",
                json!(repos_changed(&d, &mut events, 5412).await),
            );

            t.note(
                "relative path",
                kept(&post(&d, "/repos", json!({ "path": "relative/folder" })).await),
            );
            t.note(
                "plain folder",
                kept(&post(&d, "/repos", json!({ "path": plain })).await),
            );
            let again = json!({ "path": project, "name": "again" });
            t.note("added already", kept(&post(&d, "/repos", again).await));
            let inside = json!({ "path": format!("{project}/.claude") });
            t.note(
                "inside a repository",
                kept(&post(&d, "/repos", inside).await),
            );
            t.note(
                "refused, no event",
                json!(repos_changed(&d, &mut events, 5413).await),
            );

            t.note("list after", listed(&get(&d, "/repos").await));
            drop_project(&d).await;
            t
        },
    )
    .await;
}

/// Waits until the task has a workspace (`present`) or none, at most 120 s.
async fn wait_workspace(d: &Daemon, id: &str, present: bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let task = get(d, &format!("/tasks/{id}")).await.body;
        if task["task"]["workspace"].is_string() == present {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{}: {id} never got its workspace state {present}: {task}",
            d.name
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn repos_remove() {
    both(
        "repos_remove",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let id = twin(&d, 5501);
            t.alias(&id, "<task>");
            t.alias(&id.to_lowercase(), "<task>");
            call(&d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
            drop_project(&d).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let added = add_project(&d).await;
            assert_eq!(added.status, 201, "{}: {}", d.name, added.body);
            repos_changed(&d, &mut events, 5511).await;

            let task = json!({ "title": format!("{id}: In a project"), "repo": PROJECT });
            let made = post(&d, "/tasks", task).await;
            t.note("task", json!([made.status, made.body["task"]["repo"]]));
            let workspace = post(&d, &format!("/tasks/{id}/workspace"), json!({})).await;
            t.note("workspace", json!(workspace.status));
            wait_workspace(&d, &id, true).await;

            let refused = call(&d, Method::DELETE, &format!("/repos/{PROJECT}"), Body::None).await;
            t.note("remove with a workspace", kept(&refused));
            t.note(
                "refused, no event",
                json!(repos_changed(&d, &mut events, 5512).await),
            );
            let listed_now = get(&d, "/repos").await;
            let ids = |answer: &natamux_contract::daemon::Answer| {
                let repos = answer.body["repos"].as_array().cloned().unwrap_or_default();
                repos
                    .iter()
                    .map(|repo| repo["id"].clone())
                    .collect::<Vec<_>>()
            };
            t.note(
                "still listed",
                json!(ids(&listed_now).contains(&json!(PROJECT))),
            );

            let path = format!("/tasks/{id}/workspace?deleteBranch=true");
            let removed = call(&d, Method::DELETE, &path, Body::None).await;
            t.note("remove the workspace", json!(removed.status));
            wait_workspace(&d, &id, false).await;
            let gone = call(&d, Method::DELETE, &format!("/repos/{PROJECT}"), Body::None).await;
            t.note("remove", kept(&gone));
            t.note(
                "remove event",
                json!(repos_changed(&d, &mut events, 5513).await),
            );
            let kept_task = get(&d, &format!("/tasks/{id}")).await;
            let repo = &kept_task.body["task"]["repo"];
            t.note(
                "task stays",
                json!([kept_task.status, repo, kept_task.body["task"]["workspace"]]),
            );
            t.note(
                "list without it",
                json!(!ids(&get(&d, "/repos").await).contains(&json!(PROJECT))),
            );

            let again = add_project(&d).await;
            t.note("add again", kept(&again));
            t.note(
                "add again event",
                json!(repos_changed(&d, &mut events, 5514).await),
            );

            let unknown = call(&d, Method::DELETE, "/repos/nope-project", Body::None).await;
            t.note("unknown", kept(&unknown));
            let bad = call(&d, Method::DELETE, "/repos/No", Body::None).await;
            t.note("bad id", kept(&bad));
            t.note(
                "refused, no event again",
                json!(repos_changed(&d, &mut events, 5515).await),
            );

            call(&d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
            drop_project(&d).await;
            t
        },
    )
    .await;
}
