//! The reads and writes of screen 6 against a real daemon. Ignored by default; the `live-daemon`
//! job of .github/workflows/rust.yml runs it with `--ignored`.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod live;

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use live::{Made, finish, runtime};
use natamux_client::{Client, ClientError, Config};
use natamux_proto::tasks::task::NewTaskInput;
use natamux_proto::workspaces::files::EntryKind;

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN"]
fn runs_the_reads_and_writes_of_screen_6() {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let client = Client::new(config).unwrap();
    runtime().block_on(async {
        let made = Made::default();
        let result = AssertUnwindSafe(screen_6(&client, &made))
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

async fn screen_6(client: &Client, made: &Made) {
    let repos = client.list_repos().await.unwrap();
    let repo = repos.repos.first().map(|repo| repo.id.clone());
    let task = client
        .create_task(NewTaskInput {
            title: "Rust screen 6 test".into(),
            parent_id: None,
            repo,
        })
        .await
        .unwrap();
    made.add(&task.id);
    client.create_workspace(&task.id, None).await.unwrap();
    let id = task.id.as_str();

    // The root: folders come before files.
    let root = client.list_folder(id, "").await.unwrap();
    assert!(!root.is_empty());
    let first_file = root.iter().position(|entry| entry.kind == EntryKind::File);
    if let Some(first_file) = first_file {
        assert!(
            root[first_file..]
                .iter()
                .all(|entry| entry.kind == EntryKind::File)
        );
    }
    if let Some(folder) = root.iter().find(|entry| entry.kind == EntryKind::Folder) {
        client.list_folder(id, &folder.name).await.unwrap();
    }

    // The daemon's first project has a README.md, as the screen-4 test relies on.
    let file = client.read_workspace_file(id, "README.md").await.unwrap();
    let old_text = file.text.clone().expect("README.md is text");
    assert!(!file.version.is_empty());
    let new_text = format!("{old_text}\nEdited by the screen 6 test.\n");
    let saved = client
        .write_workspace_file(id, "README.md", new_text.clone(), file.version.clone())
        .await
        .unwrap();
    assert_ne!(saved.version, file.version);
    let again = client.read_workspace_file(id, "README.md").await.unwrap();
    assert_eq!(again.text.as_deref(), Some(new_text.as_str()));
    assert_eq!(again.version, saved.version);

    // The old version no longer matches.
    let stale = client
        .write_workspace_file(id, "README.md", old_text.clone(), file.version)
        .await;
    assert!(stale.unwrap_err().is_conflict());
    // The old text back, so the workspace has no change and can be removed.
    client
        .write_workspace_file(id, "README.md", old_text, saved.version)
        .await
        .unwrap();

    // The daemon answers 409 for a path outside the worktree (OutsideWorkspaceError), not 400.
    assert_eq!(status_of(client.read_workspace_file(id, "../x").await), 409);

    // A stage file lists and reads back.
    let wrote = client
        .write_stage_file(id, "context.md", "# Context\n".into())
        .await
        .unwrap();
    assert_eq!(wrote.text, "# Context\n");
    let files = client.list_stage_files(id).await.unwrap();
    assert!(files.iter().any(|file| file.name == "context.md"));
    let read = client.read_stage_file(id, "context.md").await.unwrap();
    assert_eq!(read.text, "# Context\n");

    assert!(client.list_attachments(id).await.unwrap().is_empty());

    client.remove_workspace(id, true, false).await.unwrap();
    client.delete_task(id, false).await.unwrap();
}
