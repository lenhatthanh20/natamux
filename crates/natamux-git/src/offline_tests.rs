use super::*;

#[test]
fn the_network_commands_are_found() {
    for args in [
        &["fetch", "--quiet", "origin", "main"][..],
        &["pull", "--ff-only"],
        &["push", "origin", "t-1"],
        &["ls-remote", "--heads", "origin"],
        &["clone", "https://example.test/r.git"],
        &["remote", "update"],
        &["submodule", "update", "--init"],
    ] {
        assert!(uses_network(args), "{args:?}");
    }
}

#[test]
fn local_commands_are_not_network_commands() {
    for args in [
        &["status", "--porcelain"][..],
        &["diff", "--stat"],
        &["rev-parse", "--verify", "HEAD"],
        &["worktree", "add", "-b", "t-1", "../t-1"],
        &["commit", "-m", "push the fetch"],
        &["remote", "get-url", "origin"],
        &["submodule", "status"],
        &["config", "--get", "remote.origin.push"],
        &["--version"],
        &[],
    ] {
        assert!(!uses_network(args), "{args:?}");
    }
}

#[test]
fn global_options_come_before_the_subcommand() {
    assert!(uses_network(&["-C", "/repo", "fetch"]));
    assert!(uses_network(&["--no-pager", "-c", "a.b=c", "pull"]));
    assert!(uses_network(&["--git-dir=/repo/.git", "push"]));
    assert!(uses_network(&["--git-dir", "/repo/.git", "push"]));
    // The value of an option is not the subcommand.
    assert!(!uses_network(&["-C", "push", "status"]));
    assert!(!uses_network(&["-c", "alias.x=fetch", "log"]));
}

#[test]
fn the_credential_helper_form_before_push_is_found() {
    let args = [
        "-c",
        "credential.https://github.com.helper=!gh auth git-credential",
        "push",
        "--quiet",
        "origin",
    ];
    assert!(uses_network(&args));
}

#[test]
fn nothing_is_refused_while_the_switch_is_off() {
    // The switch is off in this process (the integration test has its own process).
    assert_eq!(refusal(&["push", "origin"]), None);
}
