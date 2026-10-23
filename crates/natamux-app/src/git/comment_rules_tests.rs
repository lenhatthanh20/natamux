use super::*;

fn quote() -> Quote {
    Quote {
        side: Side::New,
        start: 12,
        end: 14,
        text: "const a = 1\nconst b = 2\nconst c = 3".into(),
    }
}

fn quote_with(side: Side, start: u32, end: u32, text: &str) -> Quote {
    Quote {
        side,
        start,
        end,
        text: text.into(),
    }
}

#[test]
fn names_the_line_when_there_is_one() {
    assert_eq!(
        comment_for_agent("src/a.ts", "Too long.", Some(12), None),
        "Review comment on src/a.ts, line 12:\nToo long."
    );
}

#[test]
fn names_the_file_then_the_comment() {
    assert_eq!(
        comment_for_agent("src/a.ts", "  Rename x.\n", None, None),
        "Review comment on src/a.ts:\nRename x."
    );
}

#[test]
fn quotes_the_selected_lines_between_the_heading_and_the_comment() {
    assert_eq!(
        comment_for_agent("src/a.ts", "Why three?", Some(3), Some(&quote())),
        "Review comment on src/a.ts, lines 12-14:\n```\nconst a = 1\nconst b = 2\nconst c = 3\n```\nWhy three?"
    );
}

#[test]
fn names_one_line_and_the_old_side() {
    let old = quote_with(Side::Old, 5, 5, "x");
    assert_eq!(
        comment_for_agent("src/a.ts", "Why?", None, Some(&old)),
        "Review comment on src/a.ts, line 5 before the change:\n```\nx\n```\nWhy?"
    );
}

#[test]
fn uses_a_longer_fence_than_any_backticks_in_the_quote() {
    let text = "a\n```js\nb\n````\nc";
    let long = Quote {
        text: text.into(),
        ..quote()
    };
    assert_eq!(
        comment_for_agent("a.md", "Ok?", None, Some(&long)),
        format!("Review comment on a.md, lines 12-14:\n`````\n{text}\n`````\nOk?")
    );
}

#[test]
fn sends_only_the_line_numbers_for_a_quote_that_is_too_long() {
    let text = "x".repeat(QUOTE_MAX + 1);
    let new = Quote {
        text: text.clone(),
        ..quote()
    };
    assert_eq!(
        comment_for_agent("src/a.ts", "Split it.", None, Some(&new)),
        "Review comment on src/a.ts, lines 12-14:\n(Too long to quote. Read these lines in the file.)\nSplit it."
    );
    let old = Quote {
        text,
        side: Side::Old,
        ..quote()
    };
    assert_eq!(
        comment_for_agent("src/a.ts", "Hm.", None, Some(&old)),
        "Review comment on src/a.ts, lines 12-14 before the change:\n(Too long to quote. Read these lines in the last commit.)\nHm."
    );
}

#[test]
fn quotes_a_text_of_exactly_the_most_characters() {
    let text = "x".repeat(QUOTE_MAX);
    let exact = Quote { text, ..quote() };
    assert!(!exact.too_long());
    assert!(comment_for_agent("a", "c", None, Some(&exact)).contains("```\nxxx"));
}

#[test]
fn names_the_lines_of_the_quote_with_an_en_dash() {
    let old = quote_with(Side::Old, 12, 14, "x");
    assert_eq!(
        comment_label(Some(3), Some(&old)),
        "Comment on lines 12–14 (before the change) for the agent"
    );
    assert_eq!(chip_text(&old), "Lines 12–14 (before the change)");
    let one = quote_with(Side::New, 7, 7, "x");
    assert_eq!(
        comment_label(None, Some(&one)),
        "Comment on line 7 for the agent"
    );
    assert_eq!(chip_text(&one), "Line 7");
}

#[test]
fn labels_the_cursor_line_or_the_file() {
    assert_eq!(
        comment_label(Some(3), None),
        "Comment on line 3 for the agent"
    );
    assert_eq!(
        comment_label(None, None),
        "Comment on this file for the agent"
    );
    assert_eq!(
        comment_label(Some(0), None),
        "Comment on this file for the agent"
    );
}

#[test]
fn says_when_the_quote_is_too_long_to_send() {
    let long = Quote {
        text: "x".repeat(QUOTE_MAX + 1),
        ..quote()
    };
    assert_eq!(
        chip_text(&long),
        "Lines 12–14 · too long to quote, only the line numbers are sent"
    );
    assert_eq!(chip_text(&quote()), "Lines 12–14");
}

#[test]
fn asks_a_different_question_when_lines_are_quoted() {
    assert_eq!(placeholder(None), "What should change, and why");
    assert_eq!(
        placeholder(Some(&quote())),
        "Ask about these lines, or say what should change"
    );
}

#[test]
fn turns_send_off_until_something_is_written() {
    assert_eq!(send_reason(""), Some("Write a comment first"));
    assert_eq!(send_reason("  \n"), Some("Write a comment first"));
    assert_eq!(send_reason(" Why? "), None);
}
