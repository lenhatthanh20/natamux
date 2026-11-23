use std::io::Read;

use natamux_core::security::{Egress, NetworkSettings};

use super::transport::{Counting, checked_target, curl_with, get_command, host_of, release_url};

const LATEST: &str = "https://github.com/lenhatthanh20/natamux/releases/latest";
const ASSET: &str =
    "https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/SHA256SUMS.txt";

fn args_of(command: &std::process::Command) -> Vec<String> {
    command
        .get_args()
        .map(|part| part.to_string_lossy().into_owned())
        .collect()
}

fn allowlist() -> NetworkSettings {
    NetworkSettings {
        egress: Egress::Allowlist,
        proxy: Some("http://proxy.corp:3128".into()),
        ..NetworkSettings::defaults(false)
    }
}

#[test]
fn only_github_https_release_addresses_are_requested() {
    assert!(release_url(
        "https://github.com/lenhatthanh20/natamux/releases/latest"
    ));
    assert!(release_url(
        "https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/SHA256SUMS.txt"
    ));
    assert!(!release_url(
        "http://github.com/lenhatthanh20/natamux/releases/latest"
    ));
    assert!(!release_url(
        "https://github.com.evil.test/x/releases/latest"
    ));
    assert!(!release_url("https://github.com/x/y/issues"));
    assert!(!release_url("https://github.com/x/y/releases/latest"));
    assert!(!release_url(
        "https://github.com/x/y/releases/download/v1.2.3/SHA256SUMS.txt"
    ));
    assert!(!release_url(
        "https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/../secret"
    ));
}

#[test]
fn curl_options_are_before_the_url_separator() {
    let command = curl_with(&NetworkSettings::defaults(false), LATEST, 30, true).expect("command");
    let args = args_of(&command);
    assert_eq!(args.first().map(String::as_str), Some("--disable"));
    assert!(args.iter().any(|arg| arg == "--location"));
    assert!(!args.iter().any(|arg| arg == "--proxy"));
    assert_eq!(
        args.last().map(String::as_str),
        Some("https://github.com/lenhatthanh20/natamux/releases/latest")
    );
    assert_eq!(args.get(args.len() - 2).map(String::as_str), Some("--"));
    assert!(args.iter().position(|arg| arg == "--head") < args.iter().position(|arg| arg == "--"));
}

#[test]
fn a_download_reports_the_bytes_read_so_far() {
    let mut seen = Vec::new();
    {
        let mut reader = Counting {
            inner: &[7_u8; 10][..],
            total: 0,
            on_bytes: |total| seen.push(total),
        };
        let mut block = [0_u8; 4];
        while reader.read(&mut block).is_ok_and(|size| size > 0) {}
    }
    assert_eq!(seen, vec![4, 8, 10, 10]);
}

#[test]
fn updater_off_starts_no_curl() {
    let off = NetworkSettings {
        updater: false,
        ..NetworkSettings::defaults(false)
    };
    let error = curl_with(&off, LATEST, 30, true).err().unwrap_or_default();
    assert!(error.starts_with("egress policy:"), "{error}");
    let loopback = NetworkSettings {
        egress: Egress::Loopback,
        ..NetworkSettings::defaults(false)
    };
    assert!(curl_with(&loopback, LATEST, 30, true).is_err());
    let mut hops = 0;
    let got = get_command(&off, ASSET, 30, &mut |_| {
        hops += 1;
        Ok(String::new())
    });
    assert!(got.is_err());
    assert_eq!(hops, 0);
}

#[test]
fn a_listed_host_gets_disable_and_the_proxy_and_follows_nothing() {
    let command = curl_with(&allowlist(), LATEST, 30, true).expect("command");
    let args = args_of(&command);
    assert_eq!(args.first().map(String::as_str), Some("--disable"));
    let proxy = args.iter().position(|arg| arg == "--proxy").expect("proxy");
    assert_eq!(args[proxy + 1], "http://proxy.corp:3128");
    assert!(!args.iter().any(|arg| arg == "--location"));
    let redirects = args
        .iter()
        .position(|arg| arg == "--max-redirs")
        .expect("max-redirs");
    assert_eq!(args[redirects + 1], "0");
    assert!(args.iter().any(|arg| arg == "%{redirect_url}"));
    let mut unlisted = allowlist();
    unlisted.update_hosts = vec!["objects.githubusercontent.com".into()];
    assert!(curl_with(&unlisted, LATEST, 30, true).is_err());
}

#[test]
fn a_redirect_to_an_unlisted_host_stops_before_the_second_request() {
    let mut hops = Vec::new();
    let got = get_command(&allowlist(), ASSET, 30, &mut |url| {
        hops.push(url.to_string());
        Ok("https://evil.example/asset".into())
    });
    let error = got.err().unwrap_or_default();
    assert!(
        error.starts_with("egress policy:") && error.contains("evil.example"),
        "{error}"
    );
    assert_eq!(hops, [ASSET]);

    let listed = "https://objects.githubusercontent.com/a?sig=x";
    let command = get_command(&allowlist(), ASSET, 30, &mut |_| Ok(listed.into())).expect("get");
    let args = args_of(&command);
    assert_eq!(args.last().map(String::as_str), Some(listed));
    assert!(!args.iter().any(|arg| arg == "--location"));
    let plain = checked_target(&allowlist(), ASSET, &mut |_| Ok(String::new()));
    assert_eq!(plain.as_deref(), Ok(ASSET));
    let http = checked_target(&allowlist(), ASSET, &mut |_| {
        Ok("http://github.com/x".into())
    });
    assert!(http.is_err());
}

#[test]
fn the_host_ends_at_a_query_a_fragment_or_a_backslash() {
    assert_eq!(
        host_of("https://evil.com?@github.com/x").as_deref(),
        Some("evil.com")
    );
    assert_eq!(
        host_of("https://evil.com#@github.com/x").as_deref(),
        Some("evil.com")
    );
    assert_eq!(host_of("https://evil.com\\@github.com/x"), None);
    assert_eq!(
        host_of("https://user@github.com/x").as_deref(),
        Some("github.com")
    );
}
