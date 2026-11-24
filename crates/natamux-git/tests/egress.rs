//! T-0373 Part 8 (F3): git remote calls under egress `allowlist`. The URL git would use is
//! resolved and checked before any network process starts, and the call goes to that URL even
//! when the remote changes after the check. The spawn counter is process-wide, so the tests here
//! run one at a time.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use natamux_core::security::{NetworkSettings, force_network, network_of};
use natamux_git::branches::list_remote_branches;
use natamux_git::commit_push::{delete_remote_ref, push_branch, push_ref};
use natamux_git::egress::{RemoteOp, resolve_remote, run_pinned};
use natamux_git::exec::{git, spawn_count};
use natamux_git::graph::{fetch_origin_branch, fetch_upstream};
use serde_json::json;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let at = COUNTER.fetch_add(1, Ordering::SeqCst);
        let name = format!("natamux-egress-{label}-{}-{at}", std::process::id());
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_git(cwd: &Path, args: &[&str]) -> String {
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

/// A repository with one empty commit on main, inside its own temporary folder.
fn test_repo() -> (TempDir, PathBuf) {
    let root = TempDir::new("repo");
    let repo = root.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["config", "user.name", "Test"]);
    run_git(&repo, &["config", "user.email", "test@example.com"]);
    run_git(&repo, &["config", "commit.gpgsign", "false"]);
    run_git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    (root, repo)
}

fn allowlist() -> NetworkSettings {
    network_of(
        &json!({ "network": {
            "egress": "allowlist",
            "git": [{ "host": "github.com", "owner": "acme", "repo": "app", "actions": ["push"] }],
        }}),
        false,
    )
}

fn bare(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    run_git(
        root,
        &["init", "--bare", "-q", "-b", "main", path.to_str().unwrap()],
    );
    path
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn has_ref(bare: &Path, name: &str) -> bool {
    git(bare, &["rev-parse", "--verify", "--quiet", name], None, &[]).is_ok()
}

/// Adds `text` to the repository's config: `git config` reads a value that starts with `-` as an
/// option.
fn append_config(repo: &Path, text: &str) {
    use std::io::Write;
    let config = repo.join(".git").join("config");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(config)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

/// The spawns of `call` beyond what resolving the remote alone costs: a refused call must add none.
fn extra_spawns(repo: &Path, op: RemoteOp, call: impl FnOnce()) -> u64 {
    let before = spawn_count();
    let _ = resolve_remote(repo, op, "origin");
    let resolve = spawn_count() - before;
    let before = spawn_count();
    call();
    (spawn_count() - before).saturating_sub(resolve)
}

#[test]
fn a_local_remote_fetches_and_pushes() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (root, repo) = test_repo();
    let origin = bare(root.path(), "origin.git");
    run_git(&repo, &["remote", "add", "origin", &file_url(&origin)]);
    let _net = force_network(allowlist());
    push_branch(&repo, "main").unwrap();
    assert!(has_ref(&origin, "refs/heads/main"));
    // The pinned push set the upstream and the remote-tracking ref as `--set-upstream` would.
    assert!(has_ref(&repo, "refs/remotes/origin/main"));
    assert_eq!(
        run_git(&repo, &["rev-parse", "--abbrev-ref", "main@{upstream}"]).trim(),
        "origin/main"
    );
    assert!(fetch_origin_branch(&repo, "main"));
    assert_eq!(list_remote_branches(&repo).unwrap(), ["origin/main"]);
    // A plain network call that skips the check is refused.
    let error = git(&repo, &["fetch", "origin"], None, &[]).unwrap_err();
    assert!(error.0.contains("egress policy:"), "{}", error.0);
}

#[test]
fn another_remote_is_refused_before_any_network_process() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (_root, repo) = test_repo();
    run_git(
        &repo,
        &["remote", "add", "origin", "https://evil.example/x/y.git"],
    );
    let _net = force_network(allowlist());
    let mut error = None;
    let extra = extra_spawns(&repo, RemoteOp::Push, || {
        error = push_branch(&repo, "main").err();
    });
    let text = error.unwrap().0;
    assert!(text.starts_with("egress policy:"), "{text}");
    assert!(
        text.contains("evil.example") && !text.contains("https://"),
        "{text}"
    );
    assert_eq!(extra, 0);
}

#[test]
fn an_instead_of_rule_to_an_unapproved_host_is_refused() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (root, repo) = test_repo();
    let origin = file_url(&bare(root.path(), "origin.git"));
    run_git(&repo, &["remote", "add", "origin", &origin]);
    let rule = "url.https://evil.example/x/y.git.insteadOf".to_string();
    run_git(&repo, &["config", &rule, &origin]);
    let _net = force_network(allowlist());
    let error = push_branch(&repo, "main").unwrap_err().0;
    assert!(
        error.contains("egress policy:") && error.contains("evil.example"),
        "{error}"
    );
    assert!(!fetch_origin_branch(&repo, "main"));
}

#[test]
fn a_remote_changed_after_the_check_does_not_move_the_call() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (root, repo) = test_repo();
    let (first, second) = (bare(root.path(), "a.git"), bare(root.path(), "b.git"));
    run_git(&repo, &["remote", "add", "origin", &file_url(&first)]);
    let _net = force_network(allowlist());
    let url = resolve_remote(&repo, RemoteOp::Push, "origin").unwrap();
    run_git(&repo, &["remote", "set-url", "origin", &file_url(&second)]);
    run_pinned(&repo, &url, &["push", &url, "main:refs/heads/main"], None).unwrap();
    assert!(has_ref(&first, "refs/heads/main"));
    assert!(!has_ref(&second, "refs/heads/main"));
    // Args that do not name the checked URL are refused.
    let other = file_url(&second);
    assert!(run_pinned(&repo, &url, &["push", &other, "main"], None).is_err());
}

#[test]
fn force_push_and_delete_are_refused_when_only_push_is_allowed() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (_root, repo) = test_repo();
    run_git(
        &repo,
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    );
    let _net = force_network(allowlist());
    let head = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    let mut errors = Vec::new();
    let extra = extra_spawns(&repo, RemoteOp::Push, || {
        let url = "https://github.com/acme/app.git";
        let pushed = push_ref(&repo, &head, "natamux-ci/t-1", url);
        errors.push(pushed.unwrap_err().0);
    });
    assert_eq!(extra, 0);
    let extra = extra_spawns(&repo, RemoteOp::Push, || {
        errors.push(delete_remote_ref(&repo, "natamux-ci/t-1").unwrap_err().0);
    });
    assert_eq!(extra, 0);
    assert!(errors[0].contains("push-force"), "{}", errors[0]);
    assert!(errors[1].contains("delete"), "{}", errors[1]);
}

#[test]
fn open_egress_runs_the_old_call() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let root = TempDir::new("egress-open");
    let (_repo_root, repo) = test_repo();
    let origin = bare(root.path(), "origin.git");
    run_git(
        &repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    let _net = force_network(NetworkSettings::defaults(false));
    push_branch(&repo, "main").unwrap();
    assert!(has_ref(&origin, "refs/heads/main"));
}

#[test]
fn a_remote_that_starts_with_a_dash_runs_no_command() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (root, repo) = test_repo();
    let marker = root.path().join("x");
    let option = format!("--upload-pack=touch {}", marker.display());
    append_config(
        &repo,
        &format!(
            "[remote \"origin\"]\n\turl = {option}\n\
             [branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n"
        ),
    );
    let _net = force_network(allowlist());
    let error = list_remote_branches(&repo).unwrap_err().0;
    assert!(error.starts_with("egress policy:"), "{error}");
    let error = push_branch(&repo, "main").unwrap_err().0;
    assert!(error.starts_with("egress policy:"), "{error}");
    fetch_upstream(&repo, "main");
    assert!(!marker.exists());
}

#[test]
fn an_upstream_remote_that_starts_with_a_dash_runs_no_command_with_open_egress() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|it| it.into_inner());
    let (root, repo) = test_repo();
    let marker = root.path().join("y");
    append_config(
        &repo,
        &format!(
            "[branch \"main\"]\n\tremote = --upload-pack=touch {}\n\
             \tmerge = refs/heads/main\n",
            marker.display()
        ),
    );
    let _net = force_network(NetworkSettings::defaults(false));
    fetch_upstream(&repo, "main");
    assert!(!marker.exists());
}
