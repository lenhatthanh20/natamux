//! T-0373 Part 6: the Strict rules of Parts 1–5 through the real setting, not the test hook.
//! `install` changes the mode of the whole process, so this file is its own test binary and
//! all of it is one test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_agents::launch_ask::allowed_now;
use natamux_agents::trust::trust_paths;
use natamux_core::security::{
    CiMode, OUTSIDE_CHANGE_WARNING, SecurityMode, Source, auto_allowed, ci_mode, forced,
    input_trace_allowed, install, mode,
};
use natamux_daemon::agents::errors::AgentError;
use natamux_daemon::agents::launch_guard::guard;
use natamux_daemon::http::agent_actions::AgentAction;
use natamux_daemon::http::agent_grants::AgentGrants;
use natamux_daemon::projects::errors::RepoError;
use natamux_daemon::projects::repo_trust_worktree::trust_always;
use natamux_daemon::projects::repos::Repos;
use natamux_proto::tasks::task::Task;
use serde_json::json;

fn folder() -> PathBuf {
    let root = std::env::temp_dir().join(format!("natamux-strict-setting-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::create_dir_all(root.join("repo")).unwrap();
    root
}

fn task_in(workspace: &Path) -> Task {
    serde_json::from_value(json!({
        "id": "T-0001",
        "title": "Example",
        "state": "running",
        "workspace": workspace,
        "stage": "implement",
        "stages": ["implement"],
        "createdAt": "2026-11-23T00:00:00.000Z",
        "updatedAt": "2026-11-23T00:00:00.000Z",
    }))
    .unwrap()
}

fn repos_in(root: &Path) -> Arc<Repos> {
    let list = json!([{ "id": "default", "name": "default", "path": root.join("repo") }]);
    std::fs::write(root.join("data/repos.json"), list.to_string()).unwrap();
    Repos::new(&root.join("data"), Arc::new(|| 0_i64))
}

fn strict_rules_hold(root: &Path, repos: &Repos) {
    assert!(forced().is_none(), "no test hook in this binary");
    assert!(mode().is_strict());
    // Part 1: an unapproved worktree and an agent that cannot be held to asking never start.
    let task = task_in(&root.join("repo"));
    let paths = trust_paths(|_| None, &root.join("data"));
    let refused = guard(&trust_always(false), &task, "claude-code", &paths);
    assert!(matches!(refused, Err(AgentError::Untrusted(_))));
    assert!(guard(&trust_always(true), &task, "claude-code", &paths).is_ok());
    assert!(!allowed_now("cursor"));
    // Part 2: CI gets only the approved last commit.
    assert_eq!(ci_mode(), CiMode::ApprovedCommit);
    // Part 3: an agent never pushes and gets no global reads.
    let grants = AgentGrants::of(&mode());
    assert!(!grants.allows(AgentAction::GitPushOwn));
    assert!(!grants.allows(AgentAction::GlobalRead));
    assert!(grants.allows(AgentAction::TaskReadOwn));
    // Part 5: no auto mode and no input trace, whatever the env says.
    assert!(!auto_allowed() && !input_trace_allowed());
    let auto = json!({ "agents": { "permissionMode": "auto" } });
    assert!(matches!(
        repos.save("default", &auto, None),
        Err(RepoError::SettingsInput(_))
    ));
}

#[test]
fn the_real_setting_turns_the_strict_rules_on_and_only_a_save_turns_them_off() {
    let root = folder();
    let repos = repos_in(&root);
    let source = Arc::new(Source::open(&root.join("data")));
    install(Arc::clone(&source));
    assert_eq!(mode(), SecurityMode::Personal);
    assert!(auto_allowed() && input_trace_allowed());

    // SAFETY: this binary has one test, so no other thread reads the env meanwhile.
    unsafe {
        std::env::set_var("NATAMUX_INPUT_TRACE", "1");
        std::env::set_var("NATAMUX_STRICT", "0");
        std::env::set_var("NATAMUX_SECURITY", "off");
    }
    source.write(r#"{"strict": true}"#).unwrap();
    strict_rules_hold(&root, &repos);

    // An edit outside the daemon cannot turn Strict off: the stricter state stays, with a warning.
    std::fs::write(root.join("data/security.json"), r#"{"strict": false}"#).unwrap();
    source.check_now();
    assert_eq!(
        source.current().warning.as_deref(),
        Some(OUTSIDE_CHANGE_WARNING)
    );
    strict_rules_hold(&root, &repos);

    // A worktree's `.natamux.json` cannot loosen the gate either.
    std::fs::write(
        root.join("repo/.natamux.json"),
        r#"{"security":{"strict":false}}"#,
    )
    .unwrap();
    let task = task_in(&root.join("repo"));
    let paths = trust_paths(|_| None, &root.join("data"));
    assert!(guard(&trust_always(false), &task, "claude-code", &paths).is_err());

    // The save of `PUT /security` turns it off at once.
    source.write(r#"{"strict": false}"#).unwrap();
    assert_eq!(mode(), SecurityMode::Personal);
    assert!(auto_allowed() && allowed_now("cursor"));
    assert_eq!(ci_mode(), CiMode::Snapshot);

    // In Personal mode a worktree may turn Strict on for its own launches.
    std::fs::write(
        root.join("repo/.natamux.json"),
        r#"{"security":{"strict":true}}"#,
    )
    .unwrap();
    assert!(guard(&trust_always(false), &task, "claude-code", &paths).is_err());
    let _ = std::fs::remove_dir_all(&root);
}
