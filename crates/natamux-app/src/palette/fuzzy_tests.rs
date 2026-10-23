use super::*;

fn ranges(query: &str, text: &str) -> Option<Vec<Range>> {
    fuzzy_match(query, text).map(|found| found.ranges)
}

fn score(query: &str, text: &str) -> f64 {
    fuzzy_match(query, text).map_or(0.0, |found| found.score)
}

#[test]
fn matches_the_characters_in_order_and_says_where() {
    assert_eq!(ranges("fix", "Fix the tests"), Some(vec![(0, 3)]));
}

#[test]
fn finds_nothing_when_a_character_is_missing_or_out_of_order() {
    assert!(fuzzy_match("xyz", "Fix the tests").is_none());
    assert!(fuzzy_match("tf", "fix").is_none());
}

#[test]
fn ignores_case() {
    assert!(fuzzy_match("FIX", "fix it").is_some());
}

#[test]
fn ranks_the_start_of_words_and_neighbouring_characters_higher() {
    assert!(fuzzy_match("ft", "Fix tests").is_some());
    assert!(fuzzy_match("ft", "Draft").is_some());
    assert!(score("ft", "Fix tests") > score("ft", "Draft"));
    assert!(score("test", "xtestx") > score("test", "xtxexsxtx"));
}

#[test]
fn treats_the_capitals_of_a_name_as_word_starts() {
    assert_eq!(ranges("tb", "toggleBoard"), Some(vec![(0, 1), (6, 7)]));
}

#[test]
fn matches_several_words_in_any_order() {
    assert_eq!(
        ranges("tests fix", "Fix the tests"),
        Some(vec![(0, 3), (8, 13)])
    );
}

#[test]
fn does_not_find_a_long_query_by_letters_that_are_far_apart() {
    assert!(fuzzy_match("stage", "Show the tab, toggle sidebar, change editor").is_none());
    assert!(fuzzy_match("stage", "Skip the stage").is_some());
    assert!(fuzzy_match("ft", "Draft").is_some());
}

#[test]
fn matches_everything_for_an_empty_query() {
    assert_eq!(
        fuzzy_match("  ", "anything"),
        Some(FuzzyMatch {
            score: 0.0,
            ranges: Vec::new()
        })
    );
}

#[test]
fn cuts_a_text_into_plain_and_matched_pieces() {
    let piece = |text: &str, matched| Piece {
        text: text.into(),
        matched,
    };
    assert_eq!(
        split_by_ranges("Fix the tests", &[(0, 3), (8, 13)]),
        vec![
            piece("Fix", true),
            piece(" the ", false),
            piece("tests", true)
        ]
    );
    assert_eq!(split_by_ranges("abc", &[]), vec![piece("abc", false)]);
}

#[test]
fn ranges_of_a_text_with_wide_chars_land_on_char_boundaries() {
    let text = "Café été";
    let found = ranges("été", text).unwrap_or_default();
    for (start, end) in &found {
        assert!(text.is_char_boundary(*start) && text.is_char_boundary(*end));
    }
    let pieces = split_by_ranges(text, &found);
    assert!(
        pieces
            .iter()
            .any(|piece| piece.matched && piece.text == "été")
    );
}
