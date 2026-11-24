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

fn hardened() -> PathBuf {
    // Process-wide: every test here installs the same folder.
    let hooks = std::env::temp_dir().join("natamux-git-harden-tests-hooks");
    install_git_hardening(hooks.clone());
    hooks
}

#[test]
fn hardened_git_allows_no_remote_helper_and_a_narrower_list_of_the_caller_stays() {
    // T-0375 R7-8: `foo::url` or `remote.<n>.vcs` would start the program `git-remote-foo`.
    hardened();
    let mut command = Command::new("git");
    harden_git(&mut command);
    let list = env_of(&command, "GIT_ALLOW_PROTOCOL");
    assert_eq!(list.as_deref(), Some("https:ssh:file:git"));

    let mut pinned = Command::new("git");
    pinned.env("GIT_ALLOW_PROTOCOL", "https:ssh:file");
    harden_git(&mut pinned);
    let list = env_of(&pinned, "GIT_ALLOW_PROTOCOL");
    assert_eq!(list.as_deref(), Some("https:ssh:file"));
}

#[test]
fn a_pinned_worktree_gets_its_checked_git_folders_also_from_a_subfolder() {
    // T-0375 R7-1: the agent can rewrite `.git`, so git gets the folders the plan checked.
    let hooks = hardened();
    let root = std::env::temp_dir().join(format!("natamux-pins-{}", std::process::id()));
    let (worktree, other) = (root.join("wt"), root.join("other"));
    for dir in [worktree.join("sub"), other.clone()] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let worktree = worktree.canonicalize().unwrap();
    let common = root.join("repo/.git");
    let git_dir = common.join("worktrees/wt");
    pin_git_dirs(&worktree, Some((git_dir.clone(), common.clone())));
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let pins = [
        ("GIT_DIR".to_string(), text(&git_dir)),
        ("GIT_COMMON_DIR".to_string(), text(&common)),
        ("GIT_WORK_TREE".to_string(), text(&worktree)),
    ];
    assert_eq!(git_pins(&worktree.join("sub")), pins);
    // A later failed check keeps the pair checked before.
    pin_git_dirs(&worktree, None);
    assert_eq!(git_pins(&worktree), pins);
    // Not isolated: nothing, as before.
    assert!(git_pins(&root).is_empty());

    pin_git_dirs(&other, None);
    let refused = [("GIT_DIR".to_string(), text(&hooks.join("refused")))];
    assert_eq!(git_pins(&other), refused);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_checked_pair_is_never_replaced_by_another_pair() {
    // T-0375 review 1: the agent can build a repo that passes the check in a folder it writes.
    hardened();
    let root = std::env::temp_dir().join(format!("natamux-pins-keep-{}", std::process::id()));
    let worktree = root.join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let common = root.join("repo/.git");
    let checked = (common.join("worktrees/wt"), common.clone());
    let fake = common.join("objects/x");
    assert!(pin_git_dirs(&worktree, Some(checked.clone())));
    assert!(pin_git_dirs(&worktree, Some(checked.clone())));
    assert!(!pin_git_dirs(
        &worktree,
        Some((fake.join("worktrees/wt"), fake.clone()))
    ));
    let (_, kept) = pinned_git_dirs(&worktree).unwrap();
    assert_eq!(kept, Some(checked));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_removed_worktree_loses_its_refused_pin() {
    // T-0375 review 2: one made again at that path must not stay refused until a restart.
    hardened();
    let root = std::env::temp_dir().join(format!("natamux-pins-gone-{}", std::process::id()));
    let worktree = root.join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    pin_git_dirs(&worktree, None);
    assert!(!git_pins(&worktree).is_empty());
    std::fs::remove_dir_all(&worktree).unwrap();
    unpin_git_dirs(&worktree);
    std::fs::create_dir_all(&worktree).unwrap();
    assert!(git_pins(&worktree).is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}
