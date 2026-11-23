#![allow(clippy::unwrap_used)]

use super::*;
use crate::hook_files::{
    cursor_hook_file, exclude_from_git, hook_file_text_of, is_natamux_hook, natamux_hook_json,
    prepare_workspace,
};
use crate::test_support::git_repo;

const CURSOR: &str = ".cursor/hooks.json";

fn git(dir: &Path, args: &[&str]) {
    let done = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(done.success(), "git {args:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let target = dir.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

#[test]
fn knows_the_text_natamux_writes_at_each_of_its_paths() {
    assert_eq!(
        natamux_text(CURSOR),
        Some(hook_file_text_of(&cursor_hook_file()))
    );
    assert!(natamux_text(".claude/settings.local.json").is_some());
    assert_eq!(natamux_text(".mcp.json"), None);
}

#[test]
fn only_exact_excluded_natamux_bytes_are_natamuxs_own() {
    let Some(repo) = git_repo("origin") else {
        return;
    };
    let dir = repo.path();
    let text = hook_file_text_of(&cursor_hook_file());
    assert!(
        !is_natamux_file(dir, CURSOR, text.as_bytes()),
        "not excluded"
    );

    exclude_from_git(dir, &format!("/{CURSOR}"));
    assert!(is_excluded(dir, CURSOR));
    assert!(is_natamux_file(dir, CURSOR, text.as_bytes()));

    // One more top-level key, such as `env`, makes it someone else's file.
    let mut value = cursor_hook_file();
    value["env"] = serde_json::json!({ "X": "1" });
    let other = hook_file_text_of(&value);
    assert!(!is_natamux_file(dir, CURSOR, other.as_bytes()));
}

#[test]
fn exact_bytes_are_enough_outside_any_repository() {
    let dir = crate::test_support::TempDir::new("origin-plain");
    let text = hook_file_text_of(&cursor_hook_file());
    if exclude_file(dir.path()).is_some() {
        return;
    }
    assert!(is_natamux_file(dir.path(), CURSOR, text.as_bytes()));
    assert!(!is_natamux_file(dir.path(), CURSOR, b"{}\n"));
}

#[test]
fn a_committed_file_is_tracked_and_a_new_one_is_not() {
    let Some(repo) = git_repo("origin-tracked") else {
        return;
    };
    let dir = repo.path();
    write(dir, CURSOR, "{}\n");
    assert!(!is_tracked(dir, CURSOR));
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "hooks"]);
    assert!(is_tracked(dir, CURSOR));
    assert!(!is_tracked(dir, ".mcp.json"));
}

const OLD_CURSOR: &str = r#"{"version":1,"hooks":{"stop":[{"command":"natamux hook cursor"}]}}"#;

#[test]
fn a_hook_command_is_exactly_natamux_hook_and_an_agent() {
    let hook = |command: &str| serde_json::json!({ "command": command });
    assert!(is_natamux_hook(&hook("natamux hook cursor")));
    assert!(is_natamux_hook(&hook("natamux hook copilot agentStop")));
    for bad in [
        "natamux hook x; evil",
        "natamux hook cursor; evil",
        "natamux hook cursor && curl x",
        "natamux hook cursor extra",
        "natamux hook copilot evil",
        "natamux hook nobody",
        "natamux hook",
    ] {
        assert!(!is_natamux_hook(&hook(bad)), "{bad}");
    }
    let dir = crate::test_support::TempDir::new("origin-evil");
    let text = r#"{"version":1,"hooks":{"stop":[{"command":"natamux hook x; evil"}]}}"#;
    write(dir.path(), CURSOR, text);
    assert!(!prepare_workspace(
        "cursor",
        dir.path(),
        WorkspaceHooks::default()
    ));
    assert_eq!(fs::read_to_string(dir.path().join(CURSOR)).unwrap(), text);
}

#[test]
fn refuses_a_hook_file_with_a_top_level_key_natamux_never_writes() {
    assert!(natamux_hook_json(OLD_CURSOR).is_some());
    for key in ["env", "apiKeyHelper", "mcpServers", "permissions"] {
        let mut value: serde_json::Value = serde_json::from_str(OLD_CURSOR).unwrap();
        value[key] = serde_json::json!({});
        assert!(natamux_hook_json(&value.to_string()).is_none(), "{key}");
    }
}

#[test]
fn never_merges_into_a_file_the_repo_tracks() {
    let Some(repo) = git_repo("origin-merge") else {
        return;
    };
    let dir = repo.path();
    write(dir, CURSOR, OLD_CURSOR);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "hooks"]);

    assert!(!prepare_workspace("cursor", dir, WorkspaceHooks::default()));

    assert_eq!(fs::read_to_string(dir.join(CURSOR)).unwrap(), OLD_CURSOR);
}
