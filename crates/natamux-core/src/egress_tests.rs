use serde_json::json;

use super::*;
use crate::security::network_of;

fn allowlist() -> NetworkSettings {
    network_of(
        &json!({ "network": {
            "egress": "allowlist",
            "git": [
                { "host": "github.com", "owner": "acme", "repo": "*", "actions": ["fetch", "push"] },
                { "host": "*.corp.example", "actions": ["fetch"] },
            ],
            "gh": [{ "host": "github.com", "actions": ["read"] }],
        }}),
        false,
    )
}

fn with_egress(egress: &str) -> NetworkSettings {
    let mut settings = allowlist();
    settings.egress = Egress::parse(egress).unwrap();
    settings
}

const GIT: Target<'static> = Target::Git {
    url: "https://github.com/acme/app.git",
    action: "fetch",
};
const GH: Target<'static> = Target::Gh {
    host: "github.com",
    action: "read",
};
const UPDATE: Target<'static> = Target::Update { host: "github.com" };

#[test]
fn each_egress_mode_against_git_gh_and_update() {
    for target in [GIT, GH, UPDATE] {
        assert_eq!(check(&with_egress("open"), &target), Ok(()), "{target:?}");
        assert_eq!(
            check(&with_egress("allowlist"), &target),
            Ok(()),
            "{target:?}"
        );
        let loopback = check(&with_egress("loopback"), &target).unwrap_err();
        assert!(loopback.starts_with(PREFIX), "{loopback}");
    }
    let other_host = Target::Update {
        host: "evil.example",
    };
    assert!(check(&with_egress("open"), &other_host).is_ok());
    assert!(check(&with_egress("allowlist"), &other_host).is_err());
    let mut off = with_egress("open");
    off.updater = false;
    assert!(check(&off, &UPDATE).unwrap_err().starts_with(PREFIX));
}

#[test]
fn owner_and_host_rules() {
    let settings = allowlist();
    let git = |url, action| check(&settings, &Target::Git { url, action });
    assert!(git("https://github.com/acme/app", "push").is_ok());
    let other = git("https://github.com/other/app", "fetch").unwrap_err();
    assert!(other.starts_with("egress policy:") && other.contains("github.com/other/app"));
    assert!(git("https://github.com/acme/app", "delete").is_err());
    assert!(git("https://github.com/acme/app", "push-force").is_err());
    assert!(git("https://git.corp.example/team/x.git", "fetch").is_ok());
    assert!(git("https://a.b.corp.example/team/x.git", "fetch").is_ok());
    assert!(git("https://corp.example/team/x.git", "fetch").is_err());
    assert!(git("https://evilcorp.example/team/x.git", "fetch").is_err());
    assert!(git("https://git.corp.example/team/x.git", "push").is_err());
    let gh = |host, action| check(&settings, &Target::Gh { host, action });
    assert!(gh("github.com", "read").is_ok());
    assert!(gh("github.com", "pr-create").is_err());
    assert!(gh("ghe.corp.example", "read").is_err());
}

#[test]
fn remote_forms() {
    let settings = allowlist();
    let fetch = |url| {
        check(
            &settings,
            &Target::Git {
                url,
                action: "fetch",
            },
        )
    };
    assert!(fetch("ssh://git@github.com:22/acme/app.git").is_ok());
    assert!(fetch("git@github.com:acme/app.git").is_ok());
    assert!(fetch("git@github.com:other/app.git").is_err());
    assert!(fetch("file:///srv/repos/app.git").is_ok());
    assert!(fetch("/srv/repos/app.git").is_ok());
    assert!(fetch("../app.git").is_ok());
    assert!(fetch("C:\\repos\\app.git").is_ok());
    for refused in [
        "ext::sh -c touch% /tmp/pwned",
        "fd::7",
        "http://github.com/acme/app.git",
        "git://github.com/acme/app.git",
        "https://user:FAKE-PASS@github.com/acme/app.git",
    ] {
        let reason = fetch(refused).unwrap_err();
        assert!(reason.starts_with(PREFIX), "{refused}: {reason}");
        assert!(!reason.contains("FAKE-PASS"), "{reason}");
    }
    assert!(fetch("https://token@github.com/acme/app.git").is_ok());
    assert_eq!(
        parse_remote("https://[::1]:8080/a/b.git"),
        Remote::Network {
            host: "::1".into(),
            owner: "a".into(),
            repo: "b".into(),
            password: false
        }
    );
}

#[test]
fn an_empty_list_or_a_bad_entry_denies() {
    let empty = network_of(&json!({ "network": { "egress": "allowlist" } }), false);
    assert!(check(&empty, &GIT).is_err());
    assert!(check(&empty, &GH).is_err());
    let bad = network_of(
        &json!({ "network": { "egress": "allowlist", "git": [
            { "host": "github.com", "owner": "acme", "actions": ["fetch"] },
            { "host": "github.com", "actions": [] },
        ]}}),
        false,
    );
    let reason = check(&bad, &GIT).unwrap_err();
    assert!(
        reason.starts_with(PREFIX) && reason.contains("not valid"),
        "{reason}"
    );
}
