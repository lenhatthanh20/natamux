use super::*;

fn listing(entries: &[(&str, &str, &str)]) -> String {
    entries
        .iter()
        .map(|(scope, key, value)| format!("{scope}\0{key}\n{value}\0"))
        .collect()
}

#[test]
fn repo_command_settings_get_a_value_that_runs_nothing_of_the_repo() {
    // T-0373 Part 7: the agent can write the repo's config, the daemon's git reads it.
    let text = listing(&[
        ("local", "filter.evil.clean", "touch x"),
        ("local", "filter.evil.process", "touch x"),
        ("local", "diff.evil.textconv", "touch x"),
        ("local", "diff.evil.command", "touch x"),
        ("local", "diff.external", "touch x"),
        ("worktree", "merge.evil.driver", "touch x"),
        ("local", "core.sshcommand", "touch x"),
        ("local", "core.askpass", "touch x"),
        ("local", "gpg.program", "touch x"),
        ("local", "gpg.ssh.program", "touch x"),
        ("local", "remote.origin.uploadpack", "touch x"),
        ("local", "core.gitproxy", "touch x"),
    ]);
    assert_eq!(
        overrides_of(&text).unwrap(),
        [
            "filter.evil.clean=",
            "filter.evil.required=false",
            "filter.evil.process=",
            "filter.evil.required=false",
            "diff.evil.textconv=cat",
            "diff.evil.command=",
            "diff.external=",
            "merge.evil.driver=",
            "core.sshcommand=ssh",
            "core.askpass=",
            "gpg.program=gpg",
            "gpg.ssh.program=ssh-keygen",
            "remote.origin.uploadpack=git-upload-pack",
            "protocol.git.allow=never",
        ]
    );
}

#[test]
fn the_users_own_value_wins_and_trusted_settings_are_left_alone() {
    let text = listing(&[
        ("global", "filter.lfs.process", "git-lfs filter-process"),
        ("global", "core.sshcommand", "ssh -i key"),
        ("local", "filter.lfs.process", "touch x"),
        ("command", "diff.external", "difft"),
    ]);
    assert_eq!(
        overrides_of(&text).unwrap(),
        ["filter.lfs.process=git-lfs filter-process"]
    );
    assert_eq!(overrides_of("").unwrap(), Vec::<String>::new());
}

#[test]
fn a_repo_credential_helper_clears_the_list_and_keeps_the_trusted_ones_in_order() {
    let text = listing(&[
        ("global", "credential.helper", "osxkeychain"),
        ("global", "credential.https://github.com.helper", ""),
        ("local", "credential.helper", "!touch x"),
        ("local", "credential.https://evil.test.helper", "!touch x"),
        (
            "command",
            "credential.https://github.com.helper",
            "!gh auth git-credential",
        ),
    ]);
    assert_eq!(
        overrides_of(&text).unwrap(),
        [
            "credential.helper=",
            "credential.helper=osxkeychain",
            "credential.https://github.com.helper=",
            "credential.https://github.com.helper=!gh auth git-credential",
        ]
    );
}

#[test]
fn a_key_with_an_equals_sign_or_a_broken_listing_fails_closed() {
    let text = listing(&[("local", "filter.a=b.clean", "touch x")]);
    assert_eq!(overrides_of(&text), None);
    assert_eq!(overrides_of("local\0filter.a.clean\ntouch x"), None);
    assert_eq!(overrides_of("local\0"), None);
}
