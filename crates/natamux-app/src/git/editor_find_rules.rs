//! The match rules of the find bar of the diff and the editor tab (T-0316): byte ranges of a term
//! in a text, plain or as a regular expression, with or without Match case.

use std::ops::Range;

use regex::RegexBuilder;

use crate::terminals::find_rules::{FindResults, find_count_label};

/// More matches than this are not drawn or counted.
pub const MAX_MATCHES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadRegex;

/// The ranges of `term` in `text`, in order. An empty term finds nothing; a match of no width
/// (such as `^`) is skipped, as it has nothing to draw.
pub fn find_ranges(
    text: &str,
    term: &str,
    match_case: bool,
    regex: bool,
) -> Result<Vec<Range<usize>>, BadRegex> {
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = if regex {
        term.to_owned()
    } else {
        regex::escape(term)
    };
    let found = RegexBuilder::new(&pattern)
        .case_insensitive(!match_case)
        .build()
        .map_err(|_| BadRegex)?;
    Ok(found
        .find_iter(text)
        .filter(|found| !found.is_empty())
        .take(MAX_MATCHES)
        .map(|found| found.range())
        .collect())
}

/// "3 of 12", "No results", "Invalid regex", or "10000+" past the cap.
pub fn count_label(term: &str, results: Option<FindResults>, invalid: bool) -> String {
    if term.is_empty() {
        return String::new();
    }
    if invalid {
        return "Invalid regex".to_owned();
    }
    match results {
        Some((None, MAX_MATCHES)) => format!("{MAX_MATCHES}+"),
        Some((Some(index), MAX_MATCHES)) => format!("{} of {MAX_MATCHES}+", index + 1),
        _ => find_count_label(term, results),
    }
}

/// The match Enter (`forward`) or Shift+Enter goes to: from `active`, or from `first` (the first
/// match after the cursor) when none is active yet. Wraps at both ends.
pub fn step_match(
    active: Option<usize>,
    first: usize,
    count: usize,
    forward: bool,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (active, forward) {
        (None, true) => first.min(count - 1),
        (None, false) => (first + count - 1) % count,
        (Some(at), true) => (at + 1) % count,
        (Some(at), false) => (at + count - 1) % count,
    })
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn found(text: &str, term: &str, case: bool, regex: bool) -> Vec<Range<usize>> {
        find_ranges(text, term, case, regex).unwrap_or_default()
    }

    #[test]
    fn ignores_case_unless_asked() {
        assert_eq!(
            found("Foo FOO foo", "foo", false, false),
            [0..3, 4..7, 8..11]
        );
        assert_eq!(found("Foo FOO foo", "foo", true, false), [8..11]);
    }

    #[test]
    fn a_plain_term_matches_its_regex_characters_as_they_are() {
        assert_eq!(found("a.b axb", "a.b", false, false), [0..3]);
        assert_eq!(found("f(x) fx", "(x)", false, false), [1..4]);
    }

    #[test]
    fn a_regex_matches_as_a_regex() {
        assert_eq!(
            found("fo foo fooo", "fo+", false, true),
            [0..2, 3..6, 7..11]
        );
        assert_eq!(found("a.b axb", "a.b", false, true), [0..3, 4..7]);
    }

    #[test]
    fn an_empty_term_finds_nothing() {
        assert_eq!(find_ranges("abc", "", false, false), Ok(Vec::new()));
    }

    #[test]
    fn a_bad_regex_is_an_error_and_says_so() {
        assert_eq!(find_ranges("abc", "(", false, true), Err(BadRegex));
        assert_eq!(count_label("(", None, true), "Invalid regex");
        // A plain term with the same text is fine.
        assert_eq!(found("f(x)", "(", false, false), [1..2]);
    }

    #[test]
    fn skips_matches_of_no_width() {
        assert!(found("bbb", "a*", false, true).is_empty());
        assert!(found("abc\ndef", "^", false, true).is_empty());
    }

    #[test]
    fn stops_at_the_cap() {
        let text = "a".repeat(MAX_MATCHES + 5);
        assert_eq!(found(&text, "a", false, false).len(), MAX_MATCHES);
        assert_eq!(count_label("a", Some((None, MAX_MATCHES)), false), "10000+");
        assert_eq!(
            count_label("a", Some((Some(1), MAX_MATCHES)), false),
            "2 of 10000+"
        );
    }

    #[test]
    fn gives_byte_ranges_in_text_with_accents() {
        let text = "tiếng việt, Việt";
        let ranges = found(text, "việt", false, false);
        assert_eq!(ranges.len(), 2);
        for range in ranges {
            assert_eq!(text[range].to_lowercase(), "việt");
        }
    }

    #[test]
    fn counts_and_steps_like_the_terminal_find() {
        assert_eq!(count_label("", Some((Some(0), 3)), false), "");
        assert_eq!(count_label("foo", Some((Some(0), 3)), false), "1 of 3");
        assert_eq!(count_label("foo", Some((None, 0)), false), "No results");
        assert_eq!(step_match(None, 2, 5, true), Some(2));
        assert_eq!(step_match(None, 0, 5, false), Some(4));
        assert_eq!(step_match(Some(4), 0, 5, true), Some(0));
        assert_eq!(step_match(Some(0), 0, 5, false), Some(4));
        assert_eq!(step_match(None, 0, 0, true), None);
    }
}
