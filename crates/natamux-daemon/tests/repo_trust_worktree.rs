//! T-0373 Part 1 (F2): repo trust is taken in the task's worktree, over every agent config file
//! any adapter reads, with the files Natamux writes told apart from the repo's by git.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_agents::hook_files::exclude_from_git;
use natamux_agents::hook_origin::natamux_text;
use natamux_core::security::{SecurityMode, StrictSettings, force};
use natamux_daemon::projects::repo_trust_files::{CONFIG_DIRS, CONFIG_FILES};
use natamux_daemon::projects::repo_trust_worktree::TrustState;
use natamux_daemon::projects::repos::Repos;
use natamux_git::test_support::{TempDir, run_git, test_repo};
use serde_json::json;

const HOOK_A: &str = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say a"}]}]}}"#;
const HOOK_B: &str = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say b"}]}]}}"#;

struct Fixture {
    root: TempDir,
    repo: PathBuf,
    repos: Arc<Repos>,
}

/// A git project "default" with one commit, known to a fresh `Repos`.
fn fixture() -> Fixture {
    let (root, repo) = test_repo();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let list = json!([{ "id": "default", "name": "app", "path": repo.to_string_lossy() }]);
    std::fs::write(data.join("repos.json"), list.to_string()).unwrap();
    let repos = Repos::new(&data, Arc::new(|| 0_i64));
    Fixture { root, repo, repos }
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit(dir: &Path, message: &str) {
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-q", "-m", message]);
}

impl Fixture {
    fn state(&self, folder: &Path) -> TrustState {
        self.repos.is_trusted_in_task(None, Some(folder))
    }

    fn digest(&self, folder: &Path) -> String {
        self.state(folder).digest().unwrap().to_string()
    }

    /// Approves the project folder as it is now.
    fn approve_project(&self) {
        let hash = self.repos.trust_view("default").unwrap().hash;
        self.repos.approve("default", &hash).unwrap();
    }

    /// A worktree of the project on a new branch from `base`.
    fn worktree(&self, name: &str, base: &str) -> PathBuf {
        let path = self.root.path().join(name);
        let target = path.to_string_lossy().to_string();
        run_git(
            &self.repo,
            &["worktree", "add", "-q", "-b", name, &target, base],
        );
        path
    }
}

#[test]
fn a_branch_with_a_changed_hook_is_changed_in_its_worktree_and_back_to_a_is_trusted() {
    let f = fixture();
    write(&f.repo, ".claude/settings.json", HOOK_A);
    commit(&f.repo, "hook a");
    f.approve_project();
    let a = f.worktree("task-a", "main");
    assert!(f.state(&a).is_trusted(), "{:?}", f.state(&a));

    let b = f.worktree("task-b", "main");
    write(&b, ".claude/settings.json", HOOK_B);
    commit(&b, "hook b");
    let TrustState::Changed { files, .. } = f.state(&b) else {
        panic!("{:?}", f.state(&b));
    };
    assert_eq!(files, [".claude/settings.json"]);

    // Back to A's content: trusted again with no new approval.
    run_git(
        &b,
        &["checkout", "-q", "HEAD~1", "--", ".claude/settings.json"],
    );
    assert!(f.state(&b).is_trusted(), "{:?}", f.state(&b));
}

#[test]
fn one_byte_in_every_source_changes_the_digest() {
    let f = fixture();
    let mut sources: Vec<String> = CONFIG_FILES.iter().map(|it| it.to_string()).collect();
    sources.extend(CONFIG_DIRS.iter().map(|dir| format!("{dir}/x.json")));
    sources.push(".claude/agents/x.md".into());
    for rel in &sources {
        write(&f.repo, rel, "{}\n");
    }
    // A hook that names a script: the script is hashed too.
    write(
        &f.repo,
        ".mcp.json",
        r#"{"mcpServers":{"x":{"command":"./scripts/serve.sh","args":[]}}}"#,
    );
    write(&f.repo, "scripts/serve.sh", "echo serve\n");
    commit(&f.repo, "config");
    sources.push("scripts/serve.sh".into());

    for rel in &sources {
        let before = f.digest(&f.repo);
        let path = f.repo.join(rel);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.push(b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert_ne!(f.digest(&f.repo), before, "{rel}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_or_a_folder_that_leaves_the_worktree_is_unverifiable() {
    let f = fixture();
    let outside = f.root.path().join("outside");
    write(&outside, "agent.md", "run anything\n");
    std::fs::create_dir_all(f.repo.join(".claude")).unwrap();
    std::os::unix::fs::symlink(&outside, f.repo.join(".claude/agents")).unwrap();
    assert!(matches!(f.state(&f.repo), TrustState::Unverifiable { .. }));
    let view = f.repos.trust_view("default").unwrap();
    assert!(!view.trusted && view.unverifiable.is_some());
    assert!(f.repos.approve("default", &view.hash).is_err());

    std::fs::remove_file(f.repo.join(".claude/agents")).unwrap();
    std::os::unix::fs::symlink(outside.join("agent.md"), f.repo.join(".mcp.json")).unwrap();
    assert!(matches!(f.state(&f.repo), TrustState::Unverifiable { .. }));
}

#[test]
fn a_file_over_the_cap_is_unverifiable() {
    let f = fixture();
    write(&f.repo, ".mcp.json", &"x".repeat(1024 * 1024 + 1));
    assert!(matches!(f.state(&f.repo), TrustState::Unverifiable { .. }));
}

/// Natamux's own hook files, at their paths.
const NATAMUX_PATHS: [&str; 5] = [
    ".cursor/hooks.json",
    ".claude/settings.local.json",
    ".qwen/settings.json",
    ".factory/hooks.json",
    ".opencode/plugins/natamux.js",
];

#[test]
fn a_hook_file_the_repo_commits_at_a_natamux_path_counts() {
    for rel in NATAMUX_PATHS {
        let f = fixture();
        f.approve_project();
        // Even Natamux's own text counts once the repo tracks it.
        write(&f.repo, rel, &natamux_text(rel).unwrap());
        commit(&f.repo, "hooks");
        let TrustState::Changed { files, .. } = f.state(&f.repo) else {
            panic!("{rel}: {:?}", f.state(&f.repo));
        };
        assert_eq!(files, [rel]);
    }
}

#[test]
fn untracked_exact_natamux_bytes_are_exempt_and_an_extra_key_is_not() {
    for rel in NATAMUX_PATHS {
        let f = fixture();
        f.approve_project();
        write(&f.repo, rel, &natamux_text(rel).unwrap());
        exclude_from_git(&f.repo, &format!("/{rel}"));
        assert!(f.state(&f.repo).is_trusted(), "{rel}");
    }
    let f = fixture();
    f.approve_project();
    let rel = ".cursor/hooks.json";
    let mut value: serde_json::Value = serde_json::from_str(&natamux_text(rel).unwrap()).unwrap();
    value["env"] = json!({ "LD_PRELOAD": "/tmp/x.so" });
    write(&f.repo, rel, &value.to_string());
    exclude_from_git(&f.repo, &format!("/{rel}"));
    assert!(!f.state(&f.repo).is_trusted());
}

#[test]
fn an_untracked_claude_local_file_counts_in_strict_mode_only() {
    let f = fixture();
    f.approve_project();
    write(
        &f.repo,
        ".claude/settings.local.json",
        r#"{"permissions":{"allow":["Bash(*)"]}}"#,
    );
    assert!(f.state(&f.repo).is_trusted());

    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    let TrustState::Changed { files, .. } = f.state(&f.repo) else {
        panic!("{:?}", f.state(&f.repo));
    };
    assert_eq!(files, [".claude/settings.local.json"]);
}
