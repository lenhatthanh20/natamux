//! The reads and writes of screen 5 against a real daemon. Ignored by default; the `live-daemon`
//! job of .github/workflows/rust.yml runs it with `--ignored`.
//!
//! It writes the settings file of the daemon's first project, then puts the first content back
//! (`{}` when the project had no file), also when a step fails.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod live;

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use live::runtime;
use natamux_client::{Client, ClientError, Config};
use serde_json::{Map, Value};

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_5() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let repos = client.list_repos().await.unwrap();
        let repo = repos
            .repos
            .first()
            .expect("the daemon has a project")
            .id
            .clone();
        let loaded = client.load_settings(&repo).await.unwrap();
        let original: Map<String, Value> = match &loaded.text {
            Some(text) => serde_json::from_str::<Value>(text)
                .unwrap()
                .as_object()
                .cloned()
                .expect("the settings file is an object"),
            None => Map::new(),
        };
        let result = AssertUnwindSafe(screen_5(&client, &repo, &original))
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

fn status_of<T: std::fmt::Debug>(answer: Result<T, ClientError>) -> ClientError {
    match answer {
        Err(error @ ClientError::Status { .. }) => error,
        other => panic!("expected a status error, got {other:?}"),
    }
}

async fn screen_5(client: &Client, repo: &str, original: &Map<String, Value>) {
    let loaded = client.load_settings(repo).await.unwrap();
    assert!(loaded.version.is_some(), "the daemon sends a version");
    client.list_skills(repo).await.unwrap();

    // The same content with the version read: taken, and a version comes back.
    let same = client
        .save_settings(repo, original.clone(), loaded.version.clone())
        .await
        .unwrap();
    let first = same.version.expect("a version after a save");

    // Another content with that version: taken again, with another version.
    let mut tracker = original
        .get("tracker")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let flipped = !loaded.settings.tracker.link_in_pr;
    tracker.insert("linkInPr".into(), Value::Bool(flipped));
    let mut changed = original.clone();
    changed.insert("tracker".into(), Value::Object(tracker));
    let saved = client
        .save_settings(repo, changed.clone(), Some(first.clone()))
        .await
        .unwrap();
    assert_eq!(saved.settings.tracker.link_in_pr, flipped);
    let second = saved.version.expect("a version after a save");
    assert_ne!(second, first, "the content changed, so the version did");

    // The old version is stale now.
    let stale = client
        .save_settings(repo, changed.clone(), Some(first))
        .await;
    assert!(status_of(stale).is_conflict());

    // Settings that break the rules: the daemon refuses with its message, not as a conflict.
    let mut broken = Map::new();
    broken.insert("stages".into(), Value::Array(Vec::new()));
    let refused = status_of(client.save_settings(repo, broken, Some(second)).await);
    assert!(!refused.is_conflict());
    match refused {
        ClientError::Status { status, message } => {
            assert_eq!(status, 400);
            assert!(!message.is_empty());
        }
        other => panic!("{other:?}"),
    }
}
