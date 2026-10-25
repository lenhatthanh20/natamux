//! The reads and writes of screen 4 against a real daemon. Ignored by default; the `live-daemon`
//! job of .github/workflows/rust.yml runs it with `--ignored`.
//!
//! The daemon runs in a container and keeps its worktrees in its own volume, so this test cannot
//! edit a file there: the commit, rollback and image reads that need a change are not run.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod live;

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use live::{Made, finish, runtime};
use natamux_client::{Client, ClientError, Config};
use natamux_proto::tasks::task::NewTaskInput;
use natamux_proto::workspaces::changes::ImageSide;

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_4() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let made = Made::default();
        let result = AssertUnwindSafe(screen_4(&client, &made))
            .catch_unwind()
            .await;
        finish(&client, &made, result).await;
    });
}

fn status_of<T: std::fmt::Debug>(answer: Result<T, ClientError>) -> u16 {
    match answer {
        Err(ClientError::Status { status, .. }) => status,
        other => panic!("expected a status error, got {other:?}"),
    }
}

async fn screen_4(client: &Client, made: &Made) {
    let repos = client.list_repos().await.unwrap();
    let repo = repos.repos.first().map(|repo| repo.id.clone());
    let task = client
        .create_task(NewTaskInput {
            title: "Rust screen 4 test".into(),
            parent_id: None,
            repo,
        })
        .await
        .unwrap();
    made.add(&task.id);
    client.create_workspace(&task.id, None).await.unwrap();
    let id = task.id.as_str();

    assert!(client.list_changes(id, false).await.unwrap().is_empty());
    let graph = client.graph(id, true).await.unwrap();
    assert!(!graph.head.is_empty());
    assert!(!graph.commits.is_empty());

    // A clean workspace has nothing to commit, to undo or to type into.
    assert_eq!(status_of(client.commit_message(id).await), 409);
    assert_eq!(status_of(client.commit(id, "x".into()).await), 409);
    assert_eq!(status_of(client.undo_rollback(id).await), 409);
    assert_eq!(
        status_of(client.file_diff(id, "no/such/file.ts").await),
        404
    );
    // The daemon checks that the file changed before it checks that it is an image.
    assert_eq!(
        status_of(
            client
                .file_image(id, "README.md", ImageSide::Modified)
                .await
        ),
        404
    );
    // The workspace starts the task's agent in Terminal 1, so the message is either taken or,
    // before the agent runs, refused.
    match client.send_to_agent(id, "hi".into()).await {
        Ok(task) => assert_eq!(task.id, id),
        Err(ClientError::Status { status, .. }) => assert_eq!(status, 409),
        Err(other) => panic!("the message failed: {other}"),
    }

    client.remove_workspace(id, true).await.unwrap();
    client.delete_task(id).await.unwrap();
}
