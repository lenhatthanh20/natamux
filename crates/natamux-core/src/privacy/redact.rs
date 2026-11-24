//! Removes secrets from text before it reaches a log or an error answer: the user name and
//! password of a URL, secret query and fragment values, and tokens with a known prefix.

use url::Url;

/// What a secret is replaced with. Query encoding keeps `*` as it is.
pub const MASK: &str = "***";

const SECRET_KEYS: [&str; 10] = [
    "token",
    "access_token",
    "key",
    "password",
    "secret",
    "sig",
    "api_key",
    "client_secret",
    "private_token",
    "x-amz-signature",
];

// The longest prefix first, so `github_pat_` is not read as a shorter one.
const TOKEN_PREFIXES: [&str; 4] = ["github_pat_", "ghp_", "gho_", "sk-"];

// Fewer characters after a prefix is a normal word, such as `sk-learn`.
const MIN_TOKEN_CHARS: usize = 8;

/// `text` with the user info of the URL dropped and secret query and fragment values masked
/// (an OAuth redirect carries `#access_token=`). Text that holds
/// no secret comes back unchanged, so a URL is not rewritten for nothing.
pub fn redact_url(text: &str) -> String {
    let Ok(mut url) = Url::parse(text) else {
        return strip_user_info(text);
    };
    let mut changed = false;
    if !url.username().is_empty() || url.password().is_some() {
        if url.set_username("").is_err() || url.set_password(None).is_err() {
            return MASK.to_string();
        }
        changed = true;
    }
    if url.query_pairs().any(|(key, _)| is_secret_key(&key)) {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| {
                let value = if is_secret_key(&key) {
                    MASK.to_string()
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        url.query_pairs_mut().clear().extend_pairs(pairs);
        changed = true;
    }
    if let Some(fragment) = url.fragment().and_then(masked_pairs) {
        url.set_fragment(Some(&fragment));
        changed = true;
    }
    if changed {
        url.to_string()
    } else {
        text.to_string()
    }
}

/// `text` with every URL passed through `redact_url` and known token prefixes masked. Plain text
/// comes back unchanged.
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for char in text.chars() {
        if is_separator(char) {
            out.push_str(&redact_word(&word));
            word.clear();
            out.push(char);
        } else {
            word.push(char);
        }
    }
    out.push_str(&redact_word(&word));
    out
}

fn is_separator(char: char) -> bool {
    char.is_whitespace() || matches!(char, '\'' | '"' | '`' | '<' | '>')
}

fn is_secret_key(key: &str) -> bool {
    SECRET_KEYS
        .iter()
        .any(|secret| key.eq_ignore_ascii_case(secret))
}

/// `key=value&...` with the values of secret keys masked; None when there is none.
fn masked_pairs(text: &str) -> Option<String> {
    let mut hit = false;
    let parts: Vec<String> = text
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, _)) if is_secret_key(key) => {
                hit = true;
                format!("{key}={MASK}")
            }
            _ => part.to_string(),
        })
        .collect();
    hit.then(|| parts.join("&"))
}

/// Each URL of the word, up to the next one, goes through `redact_url`: a word can hold a list
/// such as `https://a:b@x,https://c:d@y`.
fn redact_word(word: &str) -> String {
    let starts: Vec<usize> = word
        .match_indices("://")
        .map(|(at, _)| scheme_start(word, at))
        .collect();
    let first = starts.first().copied().unwrap_or(word.len());
    let mut out = redact_tokens(word.get(..first).unwrap_or_default());
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(word.len());
        let url = word.get(*start..end).unwrap_or_default();
        out.push_str(&redact_tokens(&redact_url(url)));
    }
    out
}

/// Where the scheme before the `://` at `at` begins.
fn scheme_start(word: &str, at: usize) -> usize {
    let scheme = word
        .get(..at)
        .unwrap_or_default()
        .bytes()
        .rev()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
        .count();
    at - scheme
}

// A URL the parser refuses can still carry `user:password@` before its host. A password with an
// unencoded `/ ? #` ends the authority early, so after `user:` the user info runs to the last `@`.
// A URL with no user info parses, so it does not come here and keeps an `@` of its path.
fn strip_user_info(text: &str) -> String {
    let Some(at) = text.find("://") else {
        return text.to_string();
    };
    let host_start = at + 3;
    let rest = text.get(host_start..).unwrap_or_default();
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or_default();
    let user_end = if authority.contains('@') || !authority.contains(':') {
        authority.rfind('@')
    } else {
        rest.rfind('@')
    };
    match user_end {
        Some(user_end) => format!(
            "{}{}",
            text.get(..host_start).unwrap_or_default(),
            rest.get(user_end + 1..).unwrap_or_default()
        ),
        None => text.to_string(),
    }
}

fn is_token_char(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_' || char == '-'
}

fn redact_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut after_token_char = false;
    while let Some(rest) = text.get(index..).filter(|rest| !rest.is_empty()) {
        if !after_token_char && let Some(length) = token_length(rest) {
            out.push_str(MASK);
            index += length;
            continue;
        }
        let Some(char) = rest.chars().next() else {
            break;
        };
        out.push(char);
        after_token_char = is_token_char(char);
        index += char.len_utf8();
    }
    out
}

// Token characters are ASCII, so their count is their length in bytes.
fn token_length(text: &str) -> Option<usize> {
    let prefix = TOKEN_PREFIXES
        .iter()
        .find(|prefix| text.starts_with(**prefix))?;
    let body = text
        .get(prefix.len()..)
        .unwrap_or_default()
        .chars()
        .take_while(|char| is_token_char(*char))
        .count();
    (body >= MIN_TOKEN_CHARS).then_some(prefix.len() + body)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
