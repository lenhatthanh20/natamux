//! `GET /tasks/:id/blame`: who last changed each line, of the working tree or the last commit.

use hyper::Method;
use natamux_git::test_support::run_git;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::TestDaemon;

const ZERO: &str = "0000000000000000000000000000000000000000";

async fn blame(d: &TestDaemon, query: &str) -> (u16, Value) {
    d.get(&format!("/tasks/T-0001/blame?{query}")).await
}

/// The (start, count, summary) of each run.
fn runs(answer: &Value) -> Vec<(u64, u64, String)> {
    let commits = answer["commits"].as_array().unwrap();
    answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| {
            let commit = &commits[run["commit"].as_u64().unwrap() as usize];
            let summary = if commit["sha"] == ZERO {
                "-".to_string()
            } else {
                commit["summary"].as_str().unwrap().to_string()
            };
            (
                run["start"].as_u64().unwrap(),
                run["count"].as_u64().unwrap(),
                summary,
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn blames_the_working_tree_and_the_last_commit() {
    let d = git_daemon("blame-sides").await;
    let workspace = d.task_with_workspace().await;
    std::fs::write(workspace.join("README.md"), "# Example\nmore\n").unwrap();

    let (status, now) = blame(&d, "path=README.md").await;
    assert_eq!(status, 200, "{now}");
    assert_eq!(now["path"], "README.md");
    assert_eq!(now["tracked"], true);
    let first = (0, 1, "init".to_string());
    assert_eq!(runs(&now), vec![first.clone(), (1, 1, "-".to_string())]);
    assert_eq!(now["commits"][0]["author"], "Test");
    assert!(now["commits"][0]["time"].as_i64().unwrap() > 0);

    let (status, head) = blame(&d, "path=README.md&side=original").await;
    assert_eq!(status, 200, "{head}");
    assert_eq!(runs(&head), vec![first]);
}

#[tokio::test(flavor = "multi_thread")]
async fn blames_the_old_path_of_a_renamed_file() {
    let d = git_daemon("blame-rename").await;
    let workspace = d.task_with_workspace().await;
    run_git(&workspace, &["mv", "README.md", "DOC.md"]);

    let (status, head) = blame(&d, "path=DOC.md&side=original").await;

    assert_eq!(status, 200, "{head}");
    assert_eq!(head["path"], "DOC.md");
    assert_eq!(runs(&head), vec![(0, 1, "init".to_string())]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_untracked_file_is_not_tracked() {
    let d = git_daemon("blame-untracked").await;
    let workspace = d.task_with_workspace().await;
    std::fs::write(workspace.join("new.txt"), "x\n").unwrap();

    let (status, answer) = blame(&d, "path=new.txt").await;

    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["tracked"], false);
    assert_eq!(answer["runs"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_bad_paths_and_sides() {
    let d = git_daemon("blame-refuse").await;
    let workspace = d.task_with_workspace().await;
    let big = "x".repeat(1_000_001);
    std::fs::write(workspace.join("big.txt"), big).unwrap();

    assert_eq!(blame(&d, "path=README.md&side=original").await.0, 404);
    assert_eq!(blame(&d, "path=README.md&side=left").await.0, 400);
    assert_eq!(blame(&d, "side=modified").await.0, 400);
    assert_eq!(blame(&d, "path=.git").await.0, 409);
    assert_eq!(blame(&d, "path=..%2F..%2Fetc%2Fpasswd").await.0, 409);
    assert_eq!(blame(&d, "path=nope.txt").await.0, 404);
    assert_eq!(blame(&d, "path=big.txt").await.0, 413);
    let (status, _) = d
        .call(Method::GET, "/tasks/T-0001/blame?path=README.md", None)
        .await;
    assert_eq!(status, 401);
}
