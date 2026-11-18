//! Links in terminal text (docs/plan-mit-license.md 6.2.6 and D5): a URL under the pointer is
//! found with a regex on its logical line, and only web and mail links open.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

/// A URL: one of these schemes, then everything up to a space, a control character, a quote,
/// a backtick, `< > { | } ^` or `⟨ ⟩` (the set of alacritty's default hint regex, plus `'`).
/// Plain paths never match.
static URL: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r#"(?:ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file://|git://|ssh:|ftp://|zed://)[^\x00-\x1F\x7F-\x9F\s<>"'`{|}\^⟨⟩]+"#,
    )
    .ok()
});

/// The URL of `line` that covers the character at `column` (a char index), with its char range.
/// Trailing `.` `,` `:` `;` `(` and a `)` with no `(` before it are left out.
pub fn url_at(line: &str, column: usize) -> Option<(Range<usize>, String)> {
    let regex = URL.as_ref()?;
    // Chars before byte `counted`, counted once while the matches go right.
    let (mut counted, mut chars) = (0, 0);
    for found in regex.find_iter(line) {
        chars += line[counted..found.start()].chars().count();
        counted = found.start();
        if chars > column {
            // Matches do not overlap, so no later one covers the column.
            return None;
        }
        let url = trim(found.as_str());
        let range = chars..chars + url.chars().count();
        if range.contains(&column) {
            return Some((range, url.to_owned()));
        }
    }
    None
}

fn trim(mut url: &str) -> &str {
    loop {
        let Some(last) = url.chars().last() else {
            return url;
        };
        let unbalanced = last == ')' && url.matches('(').count() < url.matches(')').count();
        if !(matches!(last, '.' | ',' | ':' | ';' | '(') || unbalanced) {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

/// The text shown for a link: a `file://` link shows its path.
pub fn shown(url: &str) -> &str {
    url.strip_prefix("file://").unwrap_or(url)
}

/// Only web and mail links open (D5); any other scheme does nothing. No case counts.
pub fn may_open(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_the_end_and_keeps_a_balanced_bracket() {
        let url = |line: &str, column| url_at(line, column).map(|(_, url)| url);
        assert_eq!(
            url("see https://example.com/a). and more", 8).as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(
            url("paren http://example.com/(p) end", 10).as_deref(),
            Some("http://example.com/(p)")
        );
        assert_eq!(url("path src/main.rs:12 no link", 6), None);
        // The trimmed dot is not part of the link.
        assert_eq!(url("a http://x.y. b", 12), None);
        // Quotes, brackets and braces end a link.
        for line in [
            r#""url":"https://x.y/z","#,
            "<https://x.y/z>",
            "`https://x.y/z`",
            "'https://x.y/z'",
            "{https://x.y/z|^}",
            "⟨https://x.y/z⟩",
        ] {
            assert_eq!(url(line, 9).as_deref(), Some("https://x.y/z"), "{line}");
        }
    }

    /// The old quadratic scan, kept to check that `url_at` gives the same results.
    fn url_at_slow(line: &str, column: usize) -> Option<(Range<usize>, String)> {
        let regex = URL.as_ref()?;
        let char_of = |byte: usize| line[..byte].chars().count();
        regex.find_iter(line).find_map(|found| {
            let url = trim(found.as_str());
            let start = char_of(found.start());
            let range = start..start + url.chars().count();
            range.contains(&column).then(|| (range, url.to_owned()))
        })
    }

    #[test]
    fn url_at_matches_the_plain_scan_on_every_column() {
        let line = "é https://a.b/x. ü http://c.d/(p) ⟨https://e.f⟩ end";
        for column in 0..line.chars().count() + 2 {
            assert_eq!(url_at(line, column), url_at_slow(line, column), "{column}");
        }
    }

    #[test]
    fn url_at_finds_the_last_of_many_links() {
        let line: String = (0..20_000).map(|n| format!("https://x.y/{n} ")).collect();
        let last = "https://x.y/19999";
        let start = line.chars().count() - last.len() - 1;
        assert_eq!(
            url_at(&line, start),
            Some((start..start + last.len(), last.to_owned()))
        );
        assert_eq!(url_at(&line, start + last.len()), None);
    }

    #[test]
    fn opens_only_web_and_mail_links() {
        assert!(may_open("HTTPS://EXAMPLE.COM/UP"));
        assert!(may_open("mailto:me@example.com"));
        assert!(!may_open("file:///etc/passwd"));
        assert!(!may_open("ssh://host"));
        assert_eq!(shown("file:///etc/hosts"), "/etc/hosts");
    }
}
