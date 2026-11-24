use serde_json::json;

use super::*;
use crate::egress::{Target, check};
use crate::security::{Source, WRITTEN_NAME};

#[test]
fn no_key_is_open_and_strict_strips_credentials() {
    let personal = network_of(&json!({}), false);
    assert_eq!(personal, NetworkSettings::defaults(false));
    assert_eq!(personal.egress, Egress::Open);
    assert!(!personal.strip && personal.updater);
    assert!(network_of(&json!({}), true).strip);
}

#[test]
fn reads_every_key() {
    let settings = network_of(
        &json!({ "network": {
            "egress": "allowlist",
            "git": [{ "host": "GitHub.com", "owner": "acme", "repo": "*", "actions": ["fetch", "push"] }],
            "gh": [{ "host": "github.com", "actions": ["read"] }],
            "updater": ["github.com"],
            "proxy": { "url": " http://proxy.corp:3128 ", "noProxy": [".corp", ""] },
            "credentials": { "agentEnv": "strip", "stripExtra": ["NPM_TOKEN"] },
        }}),
        false,
    );
    assert_eq!(settings.egress, Egress::Allowlist);
    let git = settings.git.unwrap();
    assert_eq!(git[0].host, "github.com");
    assert_eq!((git[0].owner.as_str(), git[0].repo.as_str()), ("acme", "*"));
    assert_eq!(settings.gh.unwrap()[0].actions, ["read"]);
    assert_eq!(settings.update_hosts, ["github.com"]);
    assert_eq!(settings.proxy.as_deref(), Some("http://proxy.corp:3128"));
    assert_eq!(settings.no_proxy, [".corp"]);
    assert!(settings.strip);
    assert_eq!(settings.strip_extra, ["NPM_TOKEN"]);
}

#[test]
fn bad_values_fail_closed() {
    let settings = network_of(
        &json!({ "network": {
            "egress": "alowlist",
            "git": [{ "host": "github.com", "actions": ["fetch"] }, { "host": "x", "actions": ["fly"] }],
            "gh": [{ "actions": ["read"] }],
            "updater": "yes",
            "credentials": { "agentEnv": "keep" },
        }}),
        false,
    );
    assert_eq!(settings.egress, Egress::Loopback);
    assert!(settings.git.is_err() && settings.gh.is_err());
    assert!(!settings.updater && settings.strip);
    let wild = network_of(
        &json!({ "network": { "git": [{ "host": "a*.b", "actions": ["fetch"] }] }}),
        false,
    );
    assert!(wild.git.is_err());
}

#[test]
fn inherit_is_ignored_under_strict() {
    let value = json!({ "network": { "credentials": { "agentEnv": "inherit" } } });
    assert!(!network_of(&value, false).strip);
    assert!(network_of(&value, true).strip);
}

#[test]
fn an_outside_change_keeps_the_stricter_settings() {
    let dir = std::env::temp_dir().join(format!("natamux-net-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let written = json!({ "strict": false, "network": { "egress": "allowlist" } });
    std::fs::write(dir.join(WRITTEN_NAME), written.to_string()).unwrap();
    std::fs::write(dir.join(FILE_NAME), written.to_string()).unwrap();
    assert_eq!(network_in(&dir, false).egress, Egress::Allowlist);
    let looser = json!({ "strict": false, "network": { "egress": "open", "updater": false } });
    std::fs::write(dir.join(FILE_NAME), looser.to_string()).unwrap();
    let kept = network_in(&dir, false);
    assert_eq!(kept.egress, Egress::Allowlist);
    assert!(!kept.updater);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(network_in(&dir, false), NetworkSettings::defaults(false));
}

#[test]
fn a_repo_only_tightens_egress_and_offline_is_loopback() {
    let dir = std::env::temp_dir().join(format!("natamux-net-repo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let open = NetworkSettings::defaults(false);
    let _forced = force_network(NetworkSettings {
        egress: Egress::Allowlist,
        ..open.clone()
    });
    let repo = |egress: &str| json!({ "security": { "network": { "egress": egress } } });
    std::fs::write(dir.join(".natamux.json"), repo("open").to_string()).unwrap();
    assert_eq!(network_for(&dir).egress, Egress::Allowlist);
    std::fs::write(dir.join(".natamux.json"), repo("loopback").to_string()).unwrap();
    assert_eq!(network_for(&dir).egress, Egress::Loopback);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_unparsable_file_is_loopback_without_the_updater() {
    let dir = std::env::temp_dir().join(format!("natamux-net-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE_NAME), "{").unwrap();
    let broken = network_in(&dir, false);
    assert_eq!(broken.egress, Egress::Loopback);
    assert!(!broken.updater && broken.strip);
    std::fs::remove_dir_all(&dir).unwrap();
    // A missing file stays the open default.
    assert_eq!(network_in(&dir, false).egress, Egress::Open);
}

#[test]
fn a_file_that_is_not_utf8_or_cannot_be_read_is_loopback_without_the_updater() {
    let dir = std::env::temp_dir().join(format!("natamux-net-bytes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE_NAME), [0xff_u8, 0xfe]).unwrap();
    let broken = network_in(&dir, false);
    assert_eq!((broken.egress, broken.updater), (Egress::Loopback, false));
    std::fs::remove_file(dir.join(FILE_NAME)).unwrap();
    assert_eq!(network_in(&dir, false), NetworkSettings::defaults(false));
    // A folder where the file should be: reading it fails with an error other than NotFound.
    std::fs::create_dir_all(dir.join(FILE_NAME)).unwrap();
    let unreadable = network_in(&dir, false);
    assert_eq!(
        (unreadable.egress, unreadable.updater),
        (Egress::Loopback, false)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_running_daemons_copy_counts_when_both_files_change() {
    let dir = std::env::temp_dir().join(format!("natamux-net-both-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let network = json!({ "egress": "allowlist", "updater": false });
    let source = Source::open(&dir);
    source
        .write(&json!({ "strict": true, "network": network }).to_string())
        .unwrap();
    let looser = json!({ "strict": false, "network": { "egress": "open" } }).to_string();
    std::fs::write(dir.join(FILE_NAME), &looser).unwrap();
    std::fs::write(dir.join(WRITTEN_NAME), &looser).unwrap();
    // The copies on disk agree, so only the copy in memory shows the outside change.
    assert_eq!(network_in(&dir, false).egress, Egress::Open);
    let kept = network_with(&dir, false, source.written().as_deref());
    assert_eq!((kept.egress, kept.updater), (Egress::Allowlist, false));
    let loopback = json!({ "strict": true, "network": { "egress": "loopback" } });
    source.write(&loopback.to_string()).unwrap();
    std::fs::write(dir.join(FILE_NAME), &looser).unwrap();
    std::fs::write(dir.join(WRITTEN_NAME), &looser).unwrap();
    let kept = network_with(&dir, false, source.written().as_deref());
    assert_eq!(kept.egress, Egress::Loopback);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_subgroup_owner_is_valid_and_matches_its_remote() {
    let settings = network_of(
        &json!({ "network": {
            "egress": "allowlist",
            "git": [
                { "host": "gitlab.com", "owner": "group/sub", "repo": "repo", "actions": ["fetch"] },
            ],
        }}),
        false,
    );
    assert_eq!(settings.git.as_ref().unwrap()[0].owner, "group/sub");
    let fetch = |url| Target::Git {
        url,
        action: "fetch",
    };
    let sub = fetch("https://gitlab.com/group/sub/repo");
    assert_eq!(check(&settings, &sub), Ok(()));
    let other = fetch("https://gitlab.com/group/other/repo");
    assert!(check(&settings, &other).is_err());
    for (owner, repo) in [
        ("/group", "r"),
        ("group/", "r"),
        ("a//b", "r"),
        ("a", "x/y"),
    ] {
        let bad = network_of(
            &json!({ "network": { "git": [
                { "host": "gitlab.com", "owner": owner, "repo": repo, "actions": ["fetch"] },
            ]}}),
            false,
        );
        assert!(bad.git.is_err(), "{owner} {repo}");
    }
}
