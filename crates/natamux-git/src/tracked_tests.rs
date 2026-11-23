#![allow(clippy::unwrap_used)]

use super::*;
use crate::test_support::{run_git, test_repo};

#[test]
fn gives_the_tracked_files_only() {
    let (_root, repo) = test_repo();
    std::fs::create_dir_all(repo.join(".claude/agents")).unwrap();
    std::fs::write(repo.join(".mcp.json"), "{}").unwrap();
    std::fs::write(repo.join(".claude/agents/x.md"), "x").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-m", "config"]);
    std::fs::write(repo.join(".cursor-hooks.json"), "{}").unwrap();

    let paths = [
        ".mcp.json",
        ".claude/agents",
        ".cursor-hooks.json",
        "missing",
    ]
    .map(String::from)
    .to_vec();
    let tracked = tracked_paths(&repo, &paths).unwrap();

    let want: HashSet<String> = [".mcp.json", ".claude/agents/x.md"]
        .map(String::from)
        .into_iter()
        .collect();
    assert_eq!(tracked, want);
}

#[test]
fn fails_outside_a_repository() {
    let dir = crate::test_support::TempDir::new("not-a-repo");
    assert!(tracked_paths(dir.path(), &[".mcp.json".into()]).is_err());
}
