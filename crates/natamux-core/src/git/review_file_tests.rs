use super::*;

fn comment(id: &str, state: CommentState, start: u32, end: u32, quote: &str) -> ReviewComment {
    ReviewComment {
        id: id.into(),
        path: "src/checkout.ts".into(),
        side: CommentSide::New,
        start,
        end,
        quote: quote.into(),
        body: format!("  Fix {id}  "),
        state,
        created_at: "2026-10-27T00:00:00.000Z".into(),
        sent_at: None,
        resolved_at: None,
        outdated: false,
    }
}

#[test]
fn drafts_go_under_new_and_sent_ones_under_still_open() {
    let comments = [
        comment("c1", CommentState::Sent, 3, 3, "a"),
        comment("c2", CommentState::Draft, 40, 44, "b\nc"),
        comment("c3", CommentState::Resolved, 1, 1, "z"),
    ];
    let text = review_comments_md(&comments);
    assert_eq!(
        text,
        "# Review comments\n\
         \n## New\n\
         \n### c2: src/checkout.ts, lines 40-44\n```\nb\nc\n```\nFix c2\n\
         \n## Still open\n\
         \n### c1: src/checkout.ts, line 3\n```\na\n```\nFix c1\n"
    );
    assert!(!text.contains("c3"));
}

#[test]
fn a_section_with_no_comment_is_left_out() {
    let text = review_comments_md(&[comment("c1", CommentState::Draft, 1, 1, "a")]);
    assert!(text.contains("## New"));
    assert!(!text.contains("## Still open"));
}

#[test]
fn the_fence_is_longer_than_any_backtick_run() {
    let text = review_comments_md(&[comment("c1", CommentState::Draft, 1, 1, "a ```` b")]);
    assert!(text.contains("`````\na ```` b\n`````\n"), "{text}");
}

#[test]
fn the_old_side_says_before_the_change() {
    let mut old = comment("c1", CommentState::Draft, 2, 3, "x\ny");
    old.side = CommentSide::Old;
    let text = review_comments_md(&[old]);
    assert!(text.contains("### c1: src/checkout.ts, lines 2-3 (before the change)\n"));
}

#[test]
fn a_whole_file_comment_has_no_quote() {
    let text = review_comments_md(&[comment("c1", CommentState::Draft, 0, 0, "")]);
    assert!(
        text.contains("### c1: src/checkout.ts, whole file\nFix c1\n"),
        "{text}"
    );
}

#[test]
fn an_outdated_comment_says_so() {
    let mut gone = comment("c1", CommentState::Sent, 1, 1, "a");
    gone.outdated = true;
    let text = review_comments_md(&[gone]);
    assert!(
        text.contains(&format!("line 1\n{OUTDATED_NOTE}\n```\na\n```\n")),
        "{text}"
    );
}

#[test]
fn a_too_long_quote_names_where_to_read() {
    let long = "x".repeat(QUOTE_MAX + 1);
    let text = review_comments_md(&[comment("c1", CommentState::Draft, 1, 1, &long)]);
    assert!(text.contains("(Too long to quote. Read these lines in the file.)\n"));
    assert!(!text.contains(&long));
    let mut old = comment("c2", CommentState::Draft, 1, 1, &long);
    old.side = CommentSide::Old;
    let text = review_comments_md(&[old]);
    assert!(text.contains("in the last commit."));
}

#[test]
fn the_prompt_counts_the_new_ones() {
    assert_eq!(
        send_prompt(3),
        "Review comments: 3 new. Read .natamux-task/review-comments.md. Fix each item under New, \
         then reply with what you changed per id (c3: …)."
    );
}
