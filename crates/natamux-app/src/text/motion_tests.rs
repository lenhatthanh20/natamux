use super::*;

const TEXT: &str = "foo_bar  baz.qux";

#[test]
fn word_moves_skip_spaces_then_one_run() {
    let back: Vec<usize> = [16, 13, 12, 9, 0]
        .iter()
        .map(|&at| previous_word_start(TEXT, at))
        .collect();
    assert_eq!(back, [13, 12, 9, 0, 0]);
    let forward: Vec<usize> = [0, 7, 12, 13, 16]
        .iter()
        .map(|&at| next_word_end(TEXT, at))
        .collect();
    assert_eq!(forward, [7, 12, 13, 16, 16]);
}

#[test]
fn a_newline_stops_a_word_move() {
    let text = "ab\n  cd";
    // From the start of the spaces: over the newline only.
    assert_eq!(previous_word_start(text, 3), 2);
    // From inside the spaces: to their start.
    assert_eq!(previous_word_start(text, 5), 3);
    assert_eq!(next_word_end(text, 2), 3);
    assert_eq!(next_word_end(text, 0), 2);
}

#[test]
fn a_double_click_takes_the_run_around_it() {
    let text = "hello world foo";
    assert_eq!(word_around(text, 7), (6, 11));
    assert_eq!(word_around(text, 6), (6, 11));
    assert_eq!(word_around(text, 15), (12, 15));
    assert_eq!(word_around("a  b", 2), (1, 3));
}

#[test]
fn graphemes_keep_combining_marks_with_their_letter() {
    let nfd = "Vie\u{323}\u{302}t";
    assert_eq!(previous_grapheme(nfd, nfd.len()), nfd.len() - 1);
    assert_eq!(previous_grapheme(nfd, nfd.len() - 1), 2);
    assert_eq!(next_grapheme(nfd, 2), nfd.len() - 1);
    assert_eq!(previous_grapheme(nfd, 0), 0);
    assert_eq!(next_grapheme(nfd, nfd.len()), nfd.len());
}

#[test]
fn utf16_offsets_round_trip_and_clip_inside_a_char() {
    let text = "đ👍日本";
    assert_eq!(to_utf16(text, text.len()), 5);
    assert_eq!(from_utf16(text, 1), 2);
    assert_eq!(from_utf16(text, 3), 6);
    // Unit 2 is inside the emoji: back to its start.
    assert_eq!(from_utf16(text, 2), 2);
    assert_eq!(from_utf16(text, 99), text.len());
}

#[test]
fn line_edges() {
    let text = "one\ntwo";
    assert_eq!((line_start(text, 5), line_end(text, 5)), (4, 7));
    assert_eq!((line_start(text, 2), line_end(text, 2)), (0, 3));
}
