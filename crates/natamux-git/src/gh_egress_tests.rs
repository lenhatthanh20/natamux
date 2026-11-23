use natamux_core::security::{Egress, NetworkSettings, force_network, network_of};
use serde_json::json;

use super::*;
use crate::exec::thread_spawn_count;
use crate::gh::run_gh;
use crate::test_support::{run_git, test_repo};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

fn allow_read() -> NetworkSettings {
    network_of(
        &json!({ "network": {
            "egress": "allowlist",
            "gh": [{ "host": "github.com", "actions": ["read"] }],
        }}),
        false,
    )
}

#[test]
fn calls_are_classified() {
    assert_eq!(
        classify(&args(&["pr", "create", "--head", "b"])),
        Some("pr-create")
    );
    assert_eq!(classify(&args(&["issue", "create"])), Some("issue-create"));
    assert_eq!(classify(&args(&["pr", "view", "1"])), Some("read"));
    assert_eq!(classify(&args(&["run", "list"])), Some("read"));
    assert_eq!(classify(&args(&["api", "repos/a/b"])), Some("read"));
    assert_eq!(
        classify(&args(&["api", "-X", "GET", "-f", "a=b", "x"])),
        Some("read")
    );
    assert_eq!(
        classify(&args(&["api", "repos/a/b/issues", "-f", "title=x"])),
        None
    );
    assert_eq!(
        classify(&args(&["api", "--method=DELETE", "repos/a/b"])),
        None
    );
    assert_eq!(
        classify(&args(&[
            "api",
            "graphql",
            "-f",
            "query=query($o: String!) { x }"
        ])),
        Some("read")
    );
    assert_eq!(
        classify(&args(&["api", "graphql", "-f", "query=mutation { x }"])),
        None
    );
    assert_eq!(classify(&args(&["repo", "delete", "a/b"])), None);
    assert_eq!(classify(&args(&["auth", "token"])), None);
}

#[test]
fn the_repo_and_host_are_pinned_to_origin() {
    let (_root, repo) = test_repo();
    run_git(
        &repo,
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    );
    let _net = force_network(allow_read());
    let env = gh_target(&repo, &args(&["pr", "list"])).unwrap();
    assert!(env.contains(&("GH_HOST".into(), "github.com".into())));
    assert!(env.contains(&("GH_REPO".into(), "github.com/acme/app".into())));
}

#[test]
fn an_unapproved_host_or_action_starts_no_gh() {
    let (_root, repo) = test_repo();
    run_git(
        &repo,
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    );
    let _net = force_network(allow_read());
    let before = thread_spawn_count();
    let url = "https://ghe.evil.example/acme/app/pull/1";
    let error = run_gh(&repo, &args(&["pr", "view", url])).unwrap_err();
    assert!(
        error.message.contains("egress policy:"),
        "{}",
        error.message
    );
    assert_eq!(thread_spawn_count(), before, "neither git nor gh may start");

    let error = run_gh(&repo, &args(&["pr", "create", "--head", "b"])).unwrap_err();
    assert!(
        error.message.contains("egress policy:"),
        "{}",
        error.message
    );
    assert!(error.message.contains("pr-create"), "{}", error.message);
}

#[test]
fn open_egress_adds_nothing_and_loopback_refuses() {
    let (_root, repo) = test_repo();
    let open = force_network(NetworkSettings::defaults(false));
    assert_eq!(
        gh_target(&repo, &args(&["pr", "list"])).unwrap(),
        Vec::new()
    );
    drop(open);
    let _loopback = force_network(NetworkSettings {
        egress: Egress::Loopback,
        ..NetworkSettings::defaults(false)
    });
    let before = thread_spawn_count();
    assert!(run_gh(&repo, &args(&["pr", "list"])).is_err());
    assert_eq!(thread_spawn_count(), before);
}
