//! Helpers for the tests of `natamux-git`, `natamux-stages` and `natamux-daemon` (feature
//! `test-support`): a temporary folder that goes away at the end, and git repositories.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test"
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "natamux-git-{label}-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).unwrap();
        // macOS's temp folder is a link that git resolves; Windows' canonical form (`\\?\C:\…`)
        // is one git does not take.
        if cfg!(windows) {
            TempDir(path)
        } else {
            TempDir(path.canonicalize().unwrap())
        }
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A real git repository with one empty commit on main. Returns the folder and the repository.
pub fn test_repo() -> (TempDir, PathBuf) {
    let root = TempDir::new("repo");
    let dir = root.path().join("repo");
    std::fs::create_dir(&dir).unwrap();
    run_git(&dir, &["init", "-b", "main"]);
    run_git(&dir, &["config", "user.name", "Test"]);
    run_git(&dir, &["config", "user.email", "test@example.com"]);
    run_git(&dir, &["config", "commit.gpgsign", "false"]);
    // Git for Windows turns LF into CRLF on checkout, and tests compare the bytes of files.
    run_git(&dir, &["config", "core.autocrlf", "false"]);
    run_git(&dir, &["commit", "--allow-empty", "-m", "init"]);
    (root, dir)
}

/// A bare repository next to the test repository (in `root`), added as its `origin`.
pub fn bare_origin(root: &Path) -> PathBuf {
    let remote = root.join("origin.git");
    let repo = root.join("repo");
    run_git(
        root,
        &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
    );
    run_git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    remote
}
