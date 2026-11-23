//! T-0373 Part 7: while isolation is on, a command that the repo's own config names (a filter,
//! a textconv, an external diff) does not run in the daemon's git. Its own test binary: the
//! hardening is process-wide.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

const COMMIT: [&str; 9] = [
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.com",
    "-c",
    "commit.gpgsign=false",
    "commit",
    "-qm",
    "x",
];

#[test]
fn hardened_git_runs_no_command_of_the_repo_config() {
    let root = std::env::temp_dir().join(format!("natamux-git-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (repo, empty) = (root.join("repo"), root.join("none"));
    for dir in [&repo, &empty] {
        std::fs::create_dir_all(dir).unwrap();
    }
    plain_git(&repo, &["init", "-q", "-b", "main"]);
    let marker = root.join("ran");
    let touch = format!("touch '{}'; cat", marker.display());
    for (key, value) in [
        ("filter.evil.clean", touch.as_str()),
        ("filter.evil.smudge", touch.as_str()),
        ("filter.evil.required", "true"),
        ("diff.evil.textconv", touch.as_str()),
    ] {
        plain_git(&repo, &["config", key, value]);
    }
    std::fs::write(repo.join(".gitattributes"), "* filter=evil diff=evil\n").unwrap();
    std::fs::write(repo.join("a"), "one\n").unwrap();
    // Before the hardening the filter runs, so the test can tell.
    git(&repo, &["add", "a"], None, &[]).unwrap();
    assert!(
        marker.exists(),
        "the filter should run without the hardening"
    );
    std::fs::remove_file(&marker).unwrap();

    install_git_hardening(empty);
    git(&repo, &["add", ".gitattributes", "a"], None, &[]).unwrap();
    git(&repo, &COMMIT, None, &[]).unwrap();
    std::fs::write(repo.join("a"), "two\n").unwrap();
    git(&repo, &["status", "--short"], None, &[]).unwrap();
    let diff = git(&repo, &["diff"], None, &[]).unwrap();
    assert!(diff.contains("+two"), "{diff}");
    git(&repo, &["checkout", "--", "a"], None, &[]).unwrap();
    assert!(!marker.exists(), "a repo command ran under the hardening");
    std::fs::remove_dir_all(&root).unwrap();
}
