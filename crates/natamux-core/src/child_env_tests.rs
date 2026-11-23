use std::path::Path;

use super::*;

fn env(pairs: &[(&str, &str)]) -> Env {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn daemon_env() -> Env {
    env(&[
        ("PATH", "/usr/bin"),
        ("GH_TOKEN", "FAKE-TOKEN-MARK"),
        ("GITHUB_TOKEN", "FAKE-TOKEN-MARK"),
        ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
        ("GIT_ASKPASS", "/bin/askpass"),
        ("GIT_CREDENTIAL_HELPER_X", "x"),
        ("NPM_TOKEN", "npm-secret"),
        ("ANTHROPIC_API_KEY", "model-key"),
        ("GIT_CONFIG_COUNT", "2"),
        ("NO_PROXY", ".old"),
    ])
}

fn strip() -> NetworkSettings {
    NetworkSettings {
        strip: true,
        strip_extra: vec!["NPM_TOKEN".into()],
        ..NetworkSettings::defaults(false)
    }
}

#[test]
fn strip_removes_the_credentials_and_keeps_the_model_keys() {
    let out = scrub_with(&strip(), &daemon_env(), Path::new("/data/gh-none"));
    for gone in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "SSH_AUTH_SOCK",
        "GIT_ASKPASS",
        "GIT_CREDENTIAL_HELPER_X",
        "NPM_TOKEN",
    ] {
        assert!(!out.contains_key(gone), "{gone}");
    }
    assert!(!out.values().any(|value| value.contains("FAKE-TOKEN-MARK")));
    assert_eq!(out["ANTHROPIC_API_KEY"], "model-key");
    assert_eq!(out["GH_CONFIG_DIR"], "/data/gh-none");
    assert_eq!(out["GIT_TERMINAL_PROMPT"], "0");
    assert_eq!(out["GCM_INTERACTIVE"], "never");
    // The reset goes after the two entries the environment had.
    assert_eq!(out["GIT_CONFIG_COUNT"], "3");
    assert_eq!(out["GIT_CONFIG_KEY_2"], "credential.helper");
    assert_eq!(out["GIT_CONFIG_VALUE_2"], "");
    assert_eq!(out["NO_PROXY"], ".old");
}

#[test]
fn inherit_and_no_proxy_is_todays_env() {
    let settings = NetworkSettings::defaults(false);
    assert_eq!(
        scrub_with(&settings, &daemon_env(), Path::new("/x")),
        daemon_env()
    );
}

#[test]
fn a_proxy_is_set_and_no_proxy_is_replaced_by_loopback_and_the_list() {
    let settings = NetworkSettings {
        proxy: Some("http://proxy.corp:3128".into()),
        no_proxy: vec![".corp".into()],
        ..NetworkSettings::defaults(false)
    };
    let out = scrub_with(&settings, &daemon_env(), Path::new("/x"));
    assert_eq!(out["HTTPS_PROXY"], "http://proxy.corp:3128");
    assert_eq!(out["all_proxy"], "http://proxy.corp:3128");
    assert_eq!(out["NO_PROXY"], "127.0.0.1,localhost,::1,.corp");
    assert_eq!(out["no_proxy"], out["NO_PROXY"]);
    assert_eq!(out["GH_TOKEN"], "FAKE-TOKEN-MARK");
}

#[test]
fn a_command_loses_the_credentials_set_on_it() {
    let _strip = crate::security::force_network(strip());
    let mut command = Command::new("env");
    command
        .env("GH_TOKEN", "FAKE-TOKEN-MARK")
        .env("ANTHROPIC_API_KEY", "k");
    scrub_command(&mut command);
    let envs: Vec<(String, Option<String>)> = command
        .get_envs()
        .map(|(name, value)| {
            let value = value.map(|value| value.to_string_lossy().into_owned());
            (name.to_string_lossy().into_owned(), value)
        })
        .collect();
    assert!(envs.contains(&("GH_TOKEN".into(), None)));
    assert!(envs.contains(&("ANTHROPIC_API_KEY".into(), Some("k".into()))));
    assert!(envs.contains(&("GIT_TERMINAL_PROMPT".into(), Some("0".into()))));
}
