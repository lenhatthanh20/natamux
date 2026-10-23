//! The string formats of the schemas: ids, zod's ISO datetime and `z.url`. The regexes are written
//! out as JS prints them, as zod puts them in its issues.

use crate::js::js_trim;
use crate::zod::{Pattern, Rule};

/// `/^[A-Z]+-\d+$/`.
pub fn is_task_id_shape(text: &str) -> bool {
    let Some((letters, digits)) = text.split_once('-') else {
        return false;
    };
    !letters.is_empty()
        && letters.bytes().all(|byte| byte.is_ascii_uppercase())
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// `/^[a-z0-9]+(-[a-z0-9]+)*$/`: stage, template and repository ids.
pub fn is_slug(text: &str) -> bool {
    !text.is_empty()
        && text.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

const SLUG_SOURCE: &str = "/^[a-z0-9]+(-[a-z0-9]+)*$/";

/// `taskIdSchema`.
pub const TASK_ID: [Rule; 2] = [
    Rule::Max(32),
    Rule::Regex(Pattern {
        test: is_task_id_shape,
        source: "/^[A-Z]+-\\d+$/",
        message: Some("Task id looks like T-42 or ABC-42"),
    }),
];

/// `stageIdSchema`.
pub const STAGE_ID: [Rule; 2] = [
    Rule::Regex(Pattern {
        test: is_slug,
        source: SLUG_SOURCE,
        message: Some("A stage id is lower case words joined by \"-\""),
    }),
    Rule::Max(40),
];

/// `repoIdSchema`.
pub const REPO_ID: [Rule; 2] = [
    Rule::Regex(Pattern {
        test: is_slug,
        source: SLUG_SOURCE,
        message: Some("A repository id is lower case words joined by \"-\""),
    }),
    Rule::Max(60),
];

/// `templateIdSchema`.
pub const TEMPLATE_ID: [Rule; 2] = [
    Rule::Regex(Pattern {
        test: is_slug,
        source: SLUG_SOURCE,
        message: Some("A template id is lower case words joined by \"-\""),
    }),
    Rule::Max(60),
];

/// The regex of `z.iso.datetime()` (no offset, any precision), as `RegExp#toString` writes it.
pub const DATETIME_SOURCE: &str = concat!(
    "/^(?:(?:\\d\\d[2468][048]|\\d\\d[13579][26]|\\d\\d0[48]|[02468][048]00|[13579][26]00)-02-29",
    "|\\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\\d|30)",
    "|(?:02)-(?:0[1-9]|1\\d|2[0-8])))T(?:(?:[01]\\d|2[0-3]):[0-5]\\d:[0-5]\\d(?:\\.\\d+)?(?:Z))$/"
);

/// What `DATETIME_SOURCE` matches: `YYYY-MM-DDTHH:MM:SS[.f+]Z` with a real day.
pub fn is_iso_datetime(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digits = |from: usize, len: usize| -> Option<u32> {
        let part = bytes.get(from..from + len)?;
        part.iter().all(u8::is_ascii_digit).then(|| {
            part.iter()
                .fold(0, |sum, byte| sum * 10 + u32::from(byte - b'0'))
        })
    };
    let shape = bytes.len() >= 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[bytes.len() - 1] == b'Z';
    if !shape {
        return false;
    }
    let (Some(year), Some(month), Some(day)) = (digits(0, 4), digits(5, 2), digits(8, 2)) else {
        return false;
    };
    let (Some(hours), Some(minutes), Some(seconds)) = (digits(11, 2), digits(14, 2), digits(17, 2))
    else {
        return false;
    };
    let fraction = &bytes[19..bytes.len() - 1];
    let fraction_ok = fraction.is_empty()
        || (fraction.len() >= 2
            && fraction[0] == b'.'
            && fraction[1..].iter().all(u8::is_ascii_digit));
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    fraction_ok && (1..=days).contains(&day) && hours < 24 && minutes < 60 && seconds < 60
}

/// `z.url({ protocol: /^https?$/ })`: the kept text (trimmed, tabs and line breaks out), or the
/// issue's `note` when it fails.
pub fn check_http_url(text: &str) -> Result<String, Option<&'static str>> {
    let trimmed = js_trim(text);
    let lower = trimmed.get(..8).unwrap_or(trimmed).to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(Some("Invalid URL format"));
    }
    let url = url::Url::parse(trimmed).map_err(|_| None)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Some("Invalid protocol"));
    }
    Ok(trimmed.replace(['\t', '\n', '\r'], ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_ids_apart() {
        assert!(is_task_id_shape("T-0001") && is_task_id_shape("ABC-12"));
        assert!(!is_task_id_shape("t-1") && !is_task_id_shape("T-") && !is_task_id_shape("T-1a"));
        assert!(is_slug("a-b2") && !is_slug("a--b") && !is_slug("-a") && !is_slug("A"));
    }

    #[test]
    fn checks_iso_times_as_zod() {
        assert!(is_iso_datetime("2026-10-23T07:40:34.567Z"));
        assert!(is_iso_datetime("2024-02-29T00:00:00Z"));
        assert!(!is_iso_datetime("2026-02-29T00:00:00Z"));
        assert!(!is_iso_datetime("1900-02-29T00:00:00Z"));
        assert!(is_iso_datetime("2000-02-29T23:59:59.1Z"));
        assert!(!is_iso_datetime("2026-10-23T07:40:34.Z"));
        assert!(!is_iso_datetime("2026-10-23T07:40:34+07:00"));
        assert!(!is_iso_datetime("2026-10-23T24:00:00Z"));
    }

    #[test]
    fn takes_http_and_https_urls_only() {
        assert_eq!(
            check_http_url(" https://github.com/a/b/issues/1\n"),
            Ok("https://github.com/a/b/issues/1".into())
        );
        assert_eq!(check_http_url("HTTP://x.io"), Ok("HTTP://x.io".into()));
        assert_eq!(
            check_http_url("vscode://x"),
            Err(Some("Invalid URL format"))
        );
        assert_eq!(check_http_url("https:/x"), Err(Some("Invalid URL format")));
        assert_eq!(check_http_url("https://"), Err(None));
    }
}
