//! The reads and writes of screen 7 against a real daemon. Ignored by default; the `live-daemon`
//! job of .github/workflows/rust.yml runs it with `--ignored`.
//!
//! It sets `tracker.createIssue` of the daemon's first project, then puts the first settings text
//! back (`{}` when the project had no file), also when a step fails.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod live;

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use live::runtime;
use natamux_client::{Client, ClientError, Config};
use natamux_proto::projects::repo::AddRepo;
use serde_json::{Map, Value};

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_7() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config.clone()).unwrap();
    // Made and dropped outside the test's runtime: a client owns a runtime of its own, and tokio
    // refuses to drop one inside another.
    let wrong = Client::new(config.with_token("not-the-token")).unwrap();
    runtime().block_on(async {
        assert_eq!(
            wrong.list_tasks().await.unwrap_err(),
            ClientError::Unauthorized
        );
        client.list_tasks().await.unwrap();

        let repos = client.list_repos().await.unwrap();
        let first = repos.repos.first().expect("the daemon has a project");
        let (repo, path) = (first.id.clone(), first.path.clone());
        let loaded = client.load_settings(&repo).await.unwrap();
        let original: Map<String, Value> = match &loaded.text {
            Some(text) => serde_json::from_str::<Value>(text)
                .unwrap()
                .as_object()
                .cloned()
                .expect("the settings file is an object"),
            None => Map::new(),
        };
        let result = AssertUnwindSafe(screen_7(&client, &repo, &path, &original))
            .catch_unwind()
            .await;
        restore(&client, &repo, &original).await;
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    });
}

/// Saves the first content over whatever is there now. Errors are left to the flow's own result.
async fn restore(client: &Client, repo: &str, original: &Map<String, Value>) {
    let Ok(current) = client.load_settings(repo).await else {
        return;
    };
    let _ = client
        .save_settings(repo, original.clone(), current.version)
        .await;
}

fn refused<T: std::fmt::Debug>(answer: Result<T, ClientError>, status: u16) -> String {
    match answer {
        Err(ClientError::Status {
            status: seen,
            message,
        }) if seen == status => {
            assert!(!message.is_empty());
            message
        }
        other => panic!("expected a {status} error, got {other:?}"),
    }
}

async fn screen_7(client: &Client, repo: &str, path: &str, original: &Map<String, Value>) {
    let relative = AddRepo {
        path: "relative/folder".into(),
        name: None,
        worktrees_dir: None,
    };
    refused(client.add_repo(&relative).await, 400);
    let same = AddRepo {
        path: path.into(),
        name: None,
        worktrees_dir: None,
    };
    let message = refused(client.add_repo(&same).await, 400);
    assert!(message.contains("added already"), "{message}");

    // No `gh`, or not signed in, in the image: either sentence starts the same way.
    let message = refused(client.list_issues(None).await, 409);
    assert!(message.contains("GitHub CLI failed"), "{message}");
    refused(client.import_issues(&[1], None).await, 409);

    let loaded = client.load_settings(repo).await.unwrap();
    let mut tracker = original
        .get("tracker")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tracker.insert("createIssue".into(), Value::String("on-pr".into()));
    let mut changed = original.clone();
    changed.insert("tracker".into(), Value::Object(tracker));
    let saved = client
        .save_settings(repo, changed, loaded.version)
        .await
        .unwrap();
    assert_eq!(saved.settings.tracker.create_issue, "on-pr");
    let read = client.load_settings(repo).await.unwrap();
    assert_eq!(read.settings.tracker.create_issue, "on-pr");
    assert_eq!(read.version, saved.version);
}
