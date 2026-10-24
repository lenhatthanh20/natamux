use super::transport::{curl, release_url};

#[test]
fn only_github_https_release_addresses_are_requested() {
    assert!(release_url("https://github.com/lenhatthanh20/natamux/releases/latest"));
    assert!(release_url("https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/SHA256SUMS.txt"));
    assert!(!release_url("http://github.com/lenhatthanh20/natamux/releases/latest"));
    assert!(!release_url("https://github.com.evil.test/x/releases/latest"));
    assert!(!release_url("https://github.com/x/y/issues"));
    assert!(!release_url("https://github.com/x/y/releases/latest"));
    assert!(!release_url("https://github.com/x/y/releases/download/v1.2.3/SHA256SUMS.txt"));
    assert!(!release_url("https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/../secret"));
}

#[test]
fn curl_options_are_before_the_url_separator() {
    let command = curl("https://github.com/lenhatthanh20/natamux/releases/latest", 30, true).expect("command");
    let args: Vec<String> = command.get_args().map(|part| part.to_string_lossy().into_owned()).collect();
    assert_eq!(args.last().map(String::as_str), Some("https://github.com/lenhatthanh20/natamux/releases/latest"));
    assert_eq!(args.get(args.len() - 2).map(String::as_str), Some("--"));
    assert!(args.iter().position(|arg| arg == "--head") < args.iter().position(|arg| arg == "--"));
}
