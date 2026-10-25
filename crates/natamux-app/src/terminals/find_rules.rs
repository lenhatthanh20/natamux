//! Port of `findCountLabel` in `packages/web/src/features/terminals/terminal-find.ts`, and the
//! pattern the find bar gives to the terminal search.

/// The matches of a search: the index of the active one (None when the matches are counted but
/// none is the active one), and how many there are.
pub type FindResults = (Option<usize>, usize);

/// "3 of 12", "No results", or nothing before the user has typed.
pub fn find_count_label(term: &str, results: Option<FindResults>) -> String {
    if term.is_empty() {
        return String::new();
    }
    match results {
        None | Some((_, 0)) => "No results".into(),
        Some((None, count)) => format!("{count} found"),
        Some((Some(index), count)) => format!("{} of {count}", index + 1),
    }
}

const REGEX_META: &str = "\\.+*?()|[]{}^$";

/// The regex for the terminal search (alacritty syntax). A plain term is escaped; a regex is given
/// as typed. Alacritty ignores case unless the term has a capital, so both choices get a flag:
/// `(?i)` with match case off, `(?-i)` with it on. An empty term gives an empty pattern, so the
/// search finds nothing.
pub fn find_pattern(term: &str, match_case: bool, regex: bool) -> String {
    if term.is_empty() {
        return String::new();
    }
    let mut pattern = String::from(if match_case { "(?-i)" } else { "(?i)" });
    for ch in term.chars() {
        if !regex && REGEX_META.contains(ch) {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_nothing_before_a_search() {
        assert_eq!(find_count_label("", None), "");
        assert_eq!(find_count_label("", Some((Some(0), 3))), "");
    }

    #[test]
    fn counts_the_matches_from_one() {
        assert_eq!(find_count_label("foo", Some((Some(0), 12))), "1 of 12");
        assert_eq!(find_count_label("foo", Some((Some(11), 12))), "12 of 12");
        assert_eq!(find_count_label("foo", Some((None, 4))), "4 found");
    }

    #[test]
    fn says_when_there_is_no_match() {
        assert_eq!(find_count_label("foo", Some((None, 0))), "No results");
        assert_eq!(find_count_label("foo", None), "No results");
    }

    #[test]
    fn puts_the_case_flag_in_front() {
        assert_eq!(find_pattern("foo", false, false), "(?i)foo");
        assert_eq!(find_pattern("foo", true, false), "(?-i)foo");
    }

    #[test]
    fn escapes_the_meta_characters_of_a_plain_term() {
        assert_eq!(find_pattern("a.b*c", false, false), r"(?i)a\.b\*c");
        assert_eq!(
            find_pattern("(x)[y]{z}", false, false),
            r"(?i)\(x\)\[y\]\{z\}"
        );
        assert_eq!(
            find_pattern(r"a\b|c+d?^$", false, false),
            r"(?i)a\\b\|c\+d\?\^\$"
        );
        assert_eq!(find_pattern("tiếng việt", false, false), "(?i)tiếng việt");
    }

    #[test]
    fn gives_a_regex_as_typed() {
        assert_eq!(
            find_pattern(r"err(or)?\d+", true, true),
            r"(?-i)err(or)?\d+"
        );
        assert_eq!(find_pattern("a.b", false, true), "(?i)a.b");
    }

    #[test]
    fn gives_an_empty_pattern_for_an_empty_term() {
        assert_eq!(find_pattern("", false, false), "");
        assert_eq!(find_pattern("", true, true), "");
    }
}
