use super::*;

fn numbered(count: u32) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn a_last_newline_adds_no_line_and_cr_is_dropped() {
    assert_eq!(file_lines("a\r\nb\n"), vec!["a", "b"]);
    assert_eq!(file_lines("a\n\nb"), vec!["a", "", "b"]);
    assert!(file_lines("").is_empty());
    assert!(file_lines("\n").is_empty());
}

#[test]
fn quotes_whole_lines_or_nothing() {
    let text = numbered(5);
    assert_eq!(quote_lines(&text, 2, 3).as_deref(), Some("line 2\nline 3"));
    assert_eq!(quote_lines(&text, 5, 5).as_deref(), Some("line 5"));
    assert_eq!(quote_lines(&text, 0, 0).as_deref(), Some(""));
    assert_eq!(quote_lines(&text, 5, 6), None);
    assert_eq!(quote_lines(&text, 3, 2), None);
    assert_eq!(quote_lines(&text, 0, 2), None);
}

#[test]
fn stays_when_the_code_did_not_move() {
    let text = numbered(10);
    let pinned = pin(4, 5, "line 4\nline 5", Some(&text));
    assert_eq!(
        pinned,
        Pinned {
            start: 4,
            end: 5,
            outdated: false
        }
    );
}

#[test]
fn moves_down_with_lines_added_above() {
    let text = format!("{}{}", "new\n".repeat(10), numbered(10));
    let pinned = pin(4, 5, "line 4\nline 5", Some(&text));
    assert_eq!((pinned.start, pinned.end, pinned.outdated), (14, 15, false));
}

#[test]
fn the_match_nearest_the_stored_line_wins_and_a_tie_takes_the_earlier() {
    let text = "x\ny\nx\ny\nx\ny\n";
    assert_eq!(pin(3, 4, "x\ny", Some(text)).start, 3);
    assert_eq!(pin(6, 7, "x\ny", Some(text)).start, 5);
    // Lines 1 and 5 are both 2 away from 3 once line 3 is gone.
    let tie = "x\nq\nq\nq\nx\n";
    assert_eq!(pin(3, 3, "x", Some(tie)).start, 1);
}

#[test]
fn a_rewritten_block_is_outdated_at_its_stored_lines() {
    let text = numbered(10).replace("line 4\n", "changed\n");
    let pinned = pin(4, 5, "line 4\nline 5", Some(&text));
    assert_eq!(
        pinned,
        Pinned {
            start: 4,
            end: 5,
            outdated: true
        }
    );
}

#[test]
fn a_gone_file_is_outdated() {
    assert!(pin(1, 1, "line 1", None).outdated);
    assert!(pin(0, 0, "", None).outdated);
}

#[test]
fn a_whole_file_comment_pins_to_the_file_only() {
    let pinned = pin(0, 0, "", Some("anything\n"));
    assert_eq!(
        pinned,
        Pinned {
            start: 0,
            end: 0,
            outdated: false
        }
    );
}

#[test]
fn a_quote_longer_than_the_file_is_outdated() {
    assert!(pin(1, 3, "a\nb\nc", Some("a\nb\n")).outdated);
}

#[test]
fn crlf_text_matches_a_quote_without_cr() {
    let pinned = pin(1, 2, "a\nb", Some("z\r\na\r\nb\r\n"));
    assert_eq!((pinned.start, pinned.end, pinned.outdated), (2, 3, false));
}

#[test]
fn a_blank_line_is_a_quote_too() {
    let pinned = pin(2, 2, "", Some("a\n\nb\n"));
    assert_eq!((pinned.start, pinned.end, pinned.outdated), (2, 2, false));
    assert!(pin(1, 1, "", Some("a\nb\n")).outdated);
}
