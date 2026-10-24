//! A real git repository for the seeded project, for the route tests of workspaces and git.

use std::path::PathBuf;

use hyper::Method;
use natamux_git::test_support::{bare_origin, run_git};
use serde_json::Value;

use super::server::{TOKEN, TestDaemon, bare};

/// A daemon with no task, whose default project is a git repository.
#[allow(dead_code)]
pub async fn git_daemon(name: &str) -> TestDaemon {
    let d = bare(name).await;
    d.make_git_repo();
    d
}

impl TestDaemon {
    /// Makes the folder of the default project a git repository on `main` with one commit (a
    /// README, and the project settings if a test wrote them). Gives the folder.
    #[allow(dead_code)]
    pub fn make_git_repo(&self) -> PathBuf {
        let repo = self.root.join("repo");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "Test"]);
        run_git(&repo, &["config", "user.email", "test@example.com"]);
        run_git(&repo, &["config", "commit.gpgsign", "false"]);
        // Git for Windows turns LF into CRLF on checkout, and tests compare the bytes of files.
        run_git(&repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("README.md"), "# Example\n").unwrap();
        run_git(&repo, &["add", "-A"]);
        run_git(&repo, &["commit", "-m", "init"]);
        repo
    }

    /// A bare `origin` next to the repository, added as its remote. Call after `make_git_repo`.
    #[allow(dead_code)]
    pub fn add_origin(&self) -> PathBuf {
        bare_origin(&self.root)
    }

    /// The folder of the default project.
    #[allow(dead_code)]
    pub fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    #[allow(dead_code)]
    pub async fn get(&self, path: &str) -> (u16, Value) {
        self.call(Method::GET, path, Some(TOKEN)).await
    }

    /// A POST with no body.
    #[allow(dead_code)]
    pub async fn post_empty(&self, path: &str) -> (u16, Value) {
        self.call(Method::POST, path, Some(TOKEN)).await
    }

    #[allow(dead_code)]
    pub async fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        self.send(Method::POST, path, Some(TOKEN), body).await
    }

    #[allow(dead_code)]
    pub async fn delete(&self, path: &str) -> (u16, Value) {
        self.call(Method::DELETE, path, Some(TOKEN)).await
    }

    /// T-0001, "Fix the bug", with its workspace made by the route. Gives the worktree folder.
    #[allow(dead_code)]
    pub async fn task_with_workspace(&self) -> PathBuf {
        self.make_task("Fix the bug").await;
        let (status, body) = self.post_empty("/tasks/T-0001/workspace").await;
        assert_eq!(status, 201, "{body}");
        PathBuf::from(body["task"]["workspace"].as_str().unwrap())
    }

    /// Waits for the work a new workspace does after its request answered.
    #[allow(dead_code)]
    pub fn settle(&self) {
        self.daemon.state.workspaces.settle();
    }
}
