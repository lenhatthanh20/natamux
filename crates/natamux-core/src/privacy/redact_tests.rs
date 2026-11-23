use super::*;

#[test]
fn a_url_loses_its_user_name_and_password() {
    let url = redact_url("https://user:FAKE-PASS-123@github.com/acme/app.git");
    assert_eq!(url, "https://github.com/acme/app.git");
    assert_eq!(
        redact_url("https://x-access-token@github.com/a.git"),
        "https://github.com/a.git"
    );
}

#[test]
fn secret_query_values_are_masked_and_the_others_stay() {
    for key in [
        "token",
        "access_token",
        "key",
        "password",
        "secret",
        "sig",
        "Token",
    ] {
        let url = redact_url(&format!("https://h.test/p?{key}=FAKE-PASS-123&page=2"));
        assert!(!url.contains("FAKE-PASS-123"), "{key}: {url}");
        assert!(url.contains("page=2"), "{key}: {url}");
        assert!(url.contains(&format!("{key}=***")), "{key}: {url}");
    }
}

#[test]
fn a_url_without_secrets_is_not_rewritten() {
    for url in [
        "https://github.com",
        "https://h.test/a?page=2&q=a+b",
        "file:///tmp/x",
    ] {
        assert_eq!(redact_url(url), url);
    }
}

#[test]
fn a_url_the_parser_refuses_still_loses_its_user_info() {
    let url = redact_url("https://user:FAKE-PASS-123@bad host/x");
    assert!(!url.contains("FAKE-PASS-123"), "{url}");
    assert!(url.starts_with("https://"), "{url}");
}

#[test]
fn urls_in_free_text_are_found() {
    let text = redact_text(
        "fatal: unable to access 'https://user:FAKE-PASS-123@host.test/x.git/': failed\n\
         see url=https://h.test/cb?access_token=FAKE-PASS-123 now",
    );
    assert!(!text.contains("FAKE-PASS-123"), "{text}");
    assert!(text.starts_with("fatal: unable to access 'https://host.test/x.git/': failed\n"));
    assert!(text.ends_with("see url=https://h.test/cb?access_token=*** now"));
}

#[test]
fn known_token_prefixes_are_masked() {
    for token in [
        "ghp_FAKEFAKE1234",
        "github_pat_FAKE_FAKE_1234",
        "gho_FAKEFAKE1234",
        "sk-FAKE-FAKE-1234",
    ] {
        let text = redact_text(&format!("auth failed for {token}: denied GH_TOKEN={token}"));
        assert!(!text.contains("FAKE"), "{text}");
        assert_eq!(text, "auth failed for ***: denied GH_TOKEN=***");
    }
}

#[test]
fn plain_text_is_unchanged() {
    for text in [
        "",
        "git push failed: rejected (non-fast-forward)",
        "risk-free task-0001 uses sk-learn and ghp_short",
        "Cannot reach the daemon at http://127.0.0.1:4317: refused",
        "ünïcödé  tabs\tand\nlines",
    ] {
        assert_eq!(redact_text(text), text);
    }
}
