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
        "api_key",
        "client_secret",
        "private_token",
        "X-Amz-Signature",
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

#[test]
fn every_url_of_a_word_is_redacted() {
    let text = redact_text("remotes: https://a:FAKE-ONE@x.test,https://c:FAKE-TWO@y.test/r");
    assert!(!text.contains("FAKE-ONE"), "{text}");
    assert!(!text.contains("FAKE-TWO"), "{text}");
    assert!(text.contains("y.test/r"), "{text}");
}

#[test]
fn secret_values_in_the_fragment_are_masked() {
    let text = redact_text("redirect https://h.test/cb#access_token=FAKE-PASS-123&state=ok");
    assert!(!text.contains("FAKE-PASS-123"), "{text}");
    assert!(text.contains("#access_token=***&state=ok"), "{text}");
    let plain = "https://h.test/docs#section=2";
    assert_eq!(redact_text(plain), plain);
}

#[test]
fn a_password_with_a_slash_is_dropped_whole() {
    assert_eq!(
        redact_text("https://u:pa/FAKE@h.test/r"),
        "https://h.test/r"
    );
    assert_eq!(
        redact_text("https://u:pa#FAKE@h.test/r"),
        "https://h.test/r"
    );
}

#[test]
fn an_at_sign_outside_the_user_info_stays() {
    for text in [
        "https://h.test/p@x",
        "https://h.test:8080/users/@me",
        "git@github.com:acme/app.git",
        "see https://h.test/a@b and git@host:path",
    ] {
        assert_eq!(redact_text(text), text);
    }
}

#[test]
fn new_secret_query_keys_are_masked_in_free_text() {
    let text = redact_text("GET https://h.test/v1?api_key=FAKE-PASS-123 failed");
    assert_eq!(text, "GET https://h.test/v1?api_key=*** failed");
}
