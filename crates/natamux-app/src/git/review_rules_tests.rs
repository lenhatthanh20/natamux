use super::*;

fn comment(id: &str, path: &str, side: CommentSide, lines: (u32, u32)) -> ReviewComment {
    ReviewComment {
        id: id.into(),
        path: path.into(),
        side,
        start: lines.0,
        end: lines.1,
        quote: "a\nb\nc\nd".into(),
        body: "Rename it".into(),
        state: CommentState::Draft,
        created_at: "2026-10-27T00:00:00.000Z".into(),
        sent_at: None,
        resolved_at: None,
        outdated: false,
    }
}

fn hunk(old: Range<u32>, new: Range<u32>) -> HunkRows {
    HunkRows { old, new }
}

fn with_state(mut comment: ReviewComment, state: CommentState) -> ReviewComment {
    comment.state = state;
    comment
}

#[test]
fn a_block_has_a_header_and_one_row_per_body_line() {
    assert_eq!(block_rows("one"), 2);
    assert_eq!(block_rows("one\ntwo\nthree"), 4);
    assert_eq!(block_rows("1\n2\n3\n4"), 5);
    assert_eq!(block_rows(""), 2);
}

#[test]
fn a_body_of_more_than_4_lines_is_cut_and_gets_one_more_row() {
    let body = "1\n2\n3\n4\n5\n6";
    assert_eq!(body_lines(body), (vec!["1", "2", "3", "4"], true));
    assert_eq!(block_rows(body), 6);
    assert_eq!(body_lines("1\n2\n3\n4"), (vec!["1", "2", "3", "4"], false));
}

#[test]
fn a_new_comment_goes_below_its_last_line_on_the_right() {
    let c = comment("c1", "a.ts", CommentSide::New, (5, 6));
    let place = placement(&c, &[], false);
    assert_eq!(
        place,
        Placement {
            left: false,
            spot: Spot::Below(5),
            other: None
        }
    );
}

#[test]
fn the_other_side_gets_its_rows_below_the_matching_line() {
    // The new side has 2 more lines in the hunk at rows 3..4 (old) and 3..6 (new).
    let hunks = [hunk(3..4, 3..6)];
    let before = comment("c1", "a.ts", CommentSide::New, (2, 2));
    assert_eq!(
        placement(&before, &hunks, true).other,
        Some(Spot::Below(1))
    );
    let first = comment("c2", "a.ts", CommentSide::New, (4, 4));
    assert_eq!(placement(&first, &hunks, true).other, Some(Spot::Below(3)));
    // The 2nd and 3rd new lines of the hunk have no old line: below the empty rows.
    let last = comment("c3", "a.ts", CommentSide::New, (6, 6));
    assert_eq!(placement(&last, &hunks, true).other, Some(Spot::Above(4)));
    let after = comment("c4", "a.ts", CommentSide::New, (9, 9));
    assert_eq!(placement(&after, &hunks, true).other, Some(Spot::Below(6)));
}

#[test]
fn an_old_comment_is_in_the_left_editor_and_maps_to_the_right() {
    let hunks = [hunk(3..4, 3..6)];
    let c = comment("c1", "a.ts", CommentSide::Old, (4, 4));
    assert_eq!(
        placement(&c, &hunks, true),
        Placement {
            left: true,
            spot: Spot::Below(3),
            other: Some(Spot::Below(3))
        }
    );
    let after = comment("c2", "a.ts", CommentSide::Old, (7, 7));
    assert_eq!(placement(&after, &hunks, true).other, Some(Spot::Below(8)));
}

#[test]
fn a_hunk_with_no_lines_on_the_comment_side_is_skipped_by_the_mapping() {
    // Two lines were added at row 2 of the new side only.
    let hunks = [hunk(2..2, 2..4)];
    let c = comment("c1", "a.ts", CommentSide::Old, (5, 5));
    assert_eq!(placement(&c, &hunks, true).other, Some(Spot::Below(6)));
}

#[test]
fn a_whole_file_comment_goes_above_the_first_row_on_both_sides() {
    let c = comment("c1", "a.ts", CommentSide::New, (0, 0));
    assert_eq!(
        placement(&c, &[hunk(1..2, 1..3)], true),
        Placement {
            left: false,
            spot: Spot::Above(0),
            other: Some(Spot::Above(0))
        }
    );
    assert_eq!(placement(&c, &[], false).spot, Spot::Above(0));
}

#[test]
fn inline_an_old_comment_goes_below_the_last_new_row_before_its_hunk() {
    let hunks = [hunk(3..5, 3..4), hunk(10..11, 9..12)];
    let in_hunk = comment("c1", "a.ts", CommentSide::Old, (5, 5));
    assert_eq!(placement(&in_hunk, &hunks, false).spot, Spot::Below(2));
    let in_second = comment("c2", "a.ts", CommentSide::Old, (11, 11));
    assert_eq!(placement(&in_second, &hunks, false).spot, Spot::Below(8));
    let between = comment("c3", "a.ts", CommentSide::Old, (8, 8));
    assert_eq!(placement(&between, &hunks, false).spot, Spot::Below(6));
    let after = comment("c4", "a.ts", CommentSide::Old, (14, 14));
    assert_eq!(placement(&after, &hunks, false).spot, Spot::Below(14));
}

#[test]
fn inline_an_old_comment_in_a_hunk_at_the_top_goes_above_row_1() {
    let hunks = [hunk(0..2, 0..1)];
    let c = comment("c1", "a.ts", CommentSide::Old, (2, 2));
    assert_eq!(placement(&c, &hunks, false).spot, Spot::Above(0));
}

#[test]
fn each_state_has_its_buttons() {
    use CommentAction::*;
    assert_eq!(actions(CommentState::Draft), [Edit, Delete]);
    assert_eq!(actions(CommentState::Sent), [Resolve, Delete]);
    assert_eq!(actions(CommentState::Resolved), [Reopen]);
}

#[test]
fn resolved_comments_show_only_with_show_resolved() {
    let done = with_state(
        comment("c1", "a.ts", CommentSide::New, (1, 1)),
        CommentState::Resolved,
    );
    assert!(!is_visible(&done, false));
    assert!(is_visible(&done, true));
    assert!(is_visible(
        &comment("c2", "a.ts", CommentSide::New, (1, 1)),
        false
    ));
}

#[test]
fn an_outdated_comment_has_no_block() {
    let mut c = comment("c1", "a.ts", CommentSide::New, (1, 1));
    assert!(has_block(&c));
    c.outdated = true;
    assert!(!has_block(&c));
}

#[test]
fn the_list_groups_by_file_and_counts_the_open_ones() {
    let comments = [
        comment("c1", "a.ts", CommentSide::New, (1, 1)),
        with_state(
            comment("c2", "a.ts", CommentSide::New, (4, 5)),
            CommentState::Sent,
        ),
        with_state(
            comment("c3", "b.ts", CommentSide::New, (2, 2)),
            CommentState::Resolved,
        ),
        comment("c4", "c.ts", CommentSide::Old, (0, 0)),
    ];
    assert_eq!(open_count(&comments), 3);
    assert_eq!(draft_count(&comments), 2);
    let groups = group_by_file(&comments, false);
    let shown: Vec<(&str, usize)> = groups
        .iter()
        .map(|group| (group.path, group.comments.len()))
        .collect();
    assert_eq!(shown, [("a.ts", 2), ("c.ts", 1)]);
    let all = group_by_file(&comments, true);
    assert_eq!(all.len(), 3);
    assert_eq!(all[1].comments[0].id, "c3");
}

#[test]
fn a_row_names_the_id_the_lines_and_the_state() {
    let c = comment("c3", "a.ts", CommentSide::New, (40, 44));
    assert_eq!(row_label(&c), "c3 \u{b7} lines 40\u{2013}44 \u{b7} draft");
    let one = with_state(
        comment("c4", "a.ts", CommentSide::New, (7, 7)),
        CommentState::Sent,
    );
    assert_eq!(row_label(&one), "c4 \u{b7} line 7 \u{b7} sent");
    let whole = comment("c5", "a.ts", CommentSide::New, (0, 0));
    assert_eq!(row_label(&whole), "c5 \u{b7} whole file \u{b7} draft");
}

#[test]
fn the_quote_preview_has_3_lines_and_the_row_the_first_line_of_the_body() {
    let c = comment("c1", "a.ts", CommentSide::New, (1, 4));
    assert_eq!(quote_preview(&c), ["a", "b", "c"]);
    assert_eq!(first_line("one\ntwo"), "one");
    assert_eq!(first_line(""), "");
}

#[test]
fn the_send_button_counts_the_drafts() {
    assert_eq!(send_label(0), "Send comments");
    assert_eq!(send_label(1), "Send 1 comment");
    assert_eq!(send_label(3), "Send 3 comments");
}

#[test]
fn add_comment_takes_the_quote_then_the_cursor_then_the_file() {
    let quote = Quote {
        side: Side::Old,
        start: 3,
        end: 4,
        text: "x\ny".into(),
    };
    assert_eq!(
        target(Some(&quote), Some(9)),
        Target {
            side: CommentSide::Old,
            start: 3,
            end: 4
        }
    );
    assert_eq!(
        target(None, Some(9)),
        Target {
            side: CommentSide::New,
            start: 9,
            end: 9
        }
    );
    let file = Target {
        side: CommentSide::New,
        start: 0,
        end: 0,
    };
    assert_eq!(target(None, None), file);
    assert_eq!(target(None, Some(0)), file);
}
