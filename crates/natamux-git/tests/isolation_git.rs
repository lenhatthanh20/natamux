//! T-0373 Part 7 (F1): while isolation is on, the daemon's own git runs no hook a worktree
//! set up (`core.hooksPath`), so an agent cannot get a command run outside its sandbox. Its own
//! test binary: the hardening is process-wide.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use natamux_core::security::install_git_hardening;
use natamux_git::exec::git;

fn plain_git(cwd: &Path, args: &[&str]) {
    let done = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(done.success(), "git {args:?}");
}

#[test]
fn hardened_git_runs_no_hook_of_the_worktree() {
    let root = std::env::temp_dir().join(format!("natamux-git-harden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (repo, hooks, empty) = (
        root.join("repo"),
        root.join("evil-hooks"),
        root.join("none"),
    );
    for dir in [&repo, &hooks, &empty] {
        std::fs::create_dir_all(dir).unwrap();
    }
    plain_git(&repo, &["init", "-q", "-b", "main"]);
    let marker = root.join("hook-ran");
    let hook = hooks.join("post-commit");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    plain_git(
        &repo,
        &["config", "core.hooksPath", &hooks.to_string_lossy()],
    );
    let commit = [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--allow-empty",
        "-q",
        "-m",
        "x",
    ];
    // Before the hardening the hook runs, so the test can tell.
    git(&repo, &commit, None, &[]).unwrap();
    assert!(marker.exists(), "the hook should run without the hardening");
    std::fs::remove_file(&marker).unwrap();
    install_git_hardening(empty);
    git(&repo, &commit, None, &[]).unwrap();
    assert!(!marker.exists(), "a worktree hook ran under the hardening");
    std::fs::remove_dir_all(&root).unwrap();
}
