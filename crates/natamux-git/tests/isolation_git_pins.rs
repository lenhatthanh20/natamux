//! T-0375 R7-1, R7-4, R7-8: while isolation is on, the daemon's git in an isolated worktree uses
//! the git folders the isolation plan checked, never the `.git` file the agent can rewrite, and
//! starts no remote helper. Its own test binary: the hardening is process-wide, so every test
//! here turns it on first.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use natamux_core::security::{install_git_hardening, pin_git_dirs};
use natamux_git::exec::{current_branch, git, linked_git_dirs, tool_git_env};

fn plain_git(cwd: &Path, args: &[&str]) {
    let done = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(done.success(), "git {args:?}");
}

fn hardened() {
    let hooks = std::env::temp_dir().join(format!("natamux-pins-hooks-{}", std::process::id()));
    install_git_hardening(hooks);
}

/// A repository with a linked worktree `wt`, pinned as an isolated shell start pins it.
fn isolated(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("natamux-pins-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let root = root.canonicalize().unwrap();
    let repo = root.join("repo");
    plain_git(&repo, &["init", "-q", "-b", "main"]);
    plain_git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let worktree = root.join("wt");
    let path = worktree.to_string_lossy().into_owned();
    plain_git(&repo, &["worktree", "add", "-q", "-b", "t-1", &path]);
    pin_git_dirs(&worktree, linked_git_dirs(&worktree));
    (root, worktree)
}

/// Points the worktree's `.git` at a new git folder `fake` that the agent wrote.
fn fake_git_folder(root: &Path, worktree: &Path, fake: &Path) {
    let path = fake.to_string_lossy().into_owned();
    plain_git(root, &["init", "-q", "--bare", &path]);
    std::fs::write(worktree.join(".git"), format!("gitdir: {path}\n")).unwrap();
}

#[test]
fn git_in_an_isolated_worktree_never_follows_a_rewritten_git_file() {
    hardened();
    let (root, worktree) = isolated("fake");
    std::fs::create_dir_all(worktree.join("sub")).unwrap();
    let real = git(&worktree, &["rev-parse", "--absolute-git-dir"], None, &[]).unwrap();
    let marker = root.join("ran");
    let touch = format!("touch '{}'; cat", marker.display());
    // A fake git folder in the worktree, then one in the task's tmp folder.
    for fake in [worktree.join("fake"), root.join("tmp/fake")] {
        fake_git_folder(&root, &worktree, &fake);
        for (key, value) in [
            ("core.fsmonitor", touch.as_str()),
            ("filter.x.process", touch.as_str()),
            ("filter.x.required", "true"),
        ] {
            plain_git(&fake, &["config", key, value]);
        }
        std::fs::write(worktree.join(".gitattributes"), "* filter=x\n").unwrap();
        std::fs::write(worktree.join("a"), "one\n").unwrap();
        git(&worktree, &["status", "--porcelain"], None, &[]).unwrap();
        git(&worktree, &["add", "a"], None, &[]).unwrap();
        assert!(!marker.exists(), "a command of the fake config ran");
        let read = git(&worktree, &["rev-parse", "--absolute-git-dir"], None, &[]).unwrap();
        assert_eq!(read, real);
        assert_eq!(current_branch(&worktree).as_deref(), Some("t-1"));
        // From a subfolder, the top is still the worktree.
        let sub = worktree.join("sub");
        let top = git(&sub, &["rev-parse", "--show-toplevel"], None, &[]).unwrap();
        assert_eq!(top.trim_end(), worktree.to_string_lossy());
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_fifo_in_the_fake_config_does_not_hang_the_call() {
    hardened();
    let (root, worktree) = isolated("fifo");
    let fake = root.join("fake");
    fake_git_folder(&root, &worktree, &fake);
    let fifo = root.join("fifo");
    let made = Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(made.success());
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(fake.join("config"))
        .unwrap();
    writeln!(config, "[include]\n\tpath = {}", fifo.display()).unwrap();
    let (done, finished) = mpsc::channel();
    let cwd = worktree.clone();
    std::thread::spawn(move || {
        let _ = done.send(git(&cwd, &["status", "--porcelain"], None, &[]));
    });
    let status = finished.recv_timeout(Duration::from_secs(20));
    assert!(matches!(status, Ok(Ok(_))), "{status:?}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn gh_in_an_isolated_worktree_gets_the_pins_and_the_hardening() {
    hardened();
    let (root, worktree) = isolated("gh");
    let (git_dir, common) = linked_git_dirs(&worktree).unwrap();
    let env = tool_git_env(&worktree);
    let has = |key: &str, value: &Path| {
        let pair = (key.to_string(), value.display().to_string());
        env.contains(&pair)
    };
    assert!(has("GIT_DIR", &git_dir), "{env:?}");
    assert!(has("GIT_COMMON_DIR", &common), "{env:?}");
    assert!(has("GIT_WORK_TREE", &worktree), "{env:?}");
    assert!(env.contains(&("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string())));
    let value_of = |name: &str| {
        let (key, _) = env.iter().find(|(_, value)| value == name)?;
        let wanted = key.replace("GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_");
        let (_, value) = env.iter().find(|(key, _)| *key == wanted)?;
        Some(value.clone())
    };
    assert_eq!(value_of("core.fsmonitor").as_deref(), Some("false"));
    assert!(value_of("core.hooksPath").is_some(), "{env:?}");
    // A folder in no isolated worktree gets the hardening and no pin.
    let plain = tool_git_env(&root);
    assert!(!plain.iter().any(|(key, _)| key == "GIT_DIR"), "{plain:?}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_remote_helper_does_not_start_under_the_hardening() {
    hardened();
    let (root, worktree) = isolated("helper");
    let error = git(&worktree, &["ls-remote", "foo::bar"], None, &[]).unwrap_err();
    assert!(error.0.contains("not allowed"), "{error}");
    std::fs::remove_dir_all(&root).unwrap();
}
