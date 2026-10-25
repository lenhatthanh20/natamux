use ChangeStatus::{Added, Conflict, Deleted, Modified, Renamed, Untracked};

use super::*;

fn change(modified_start: u32, modified_end: u32) -> LineChange {
    // Only the new side matters for navigation.
    LineChange {
        original_start: 1,
        original_end: 1,
        modified_start,
        modified_end,
    }
}

/// Lines 3 to 5 changed, then lines were deleted after line 10, then line 20 changed.
fn changes() -> Vec<LineChange> {
    vec![change(3, 5), change(10, 0), change(20, 20)]
}

fn counts(added: u64, removed: u64) -> LineCounts {
    LineCounts { added, removed }
}

#[test]
fn shows_new_files_as_added_deleted_files_as_removed_and_the_rest_as_a_diff() {
    let kinds: Vec<_> = [Added, Untracked, Deleted, Modified, Renamed, Conflict]
        .into_iter()
        .map(whole_file_kind)
        .collect();
    assert_eq!(
        kinds,
        [
            Some(WholeKind::Added),
            Some(WholeKind::Added),
            Some(WholeKind::Removed),
            None,
            None,
            None
        ]
    );
}

#[test]
fn drops_one_final_line_break_so_no_empty_last_line_shows() {
    assert_eq!(trim_trailing_newline("a\nb\n"), "a\nb");
    assert_eq!(trim_trailing_newline("a\r\n"), "a");
    assert_eq!(trim_trailing_newline("a\n\n"), "a\n");
    assert_eq!(trim_trailing_newline("a"), "a");
}

#[test]
fn counts_the_lines_of_a_file() {
    assert_eq!(count_lines(""), 0);
    assert_eq!(count_lines("a\n"), 1);
    assert_eq!(count_lines("a\nb\nc\n"), 3);
}

#[test]
fn adds_up_lines_on_each_side_and_treats_an_end_of_0_as_no_lines() {
    let list = [
        LineChange {
            original_start: 3,
            original_end: 4,
            modified_start: 3,
            modified_end: 3,
        },
        LineChange {
            original_start: 9,
            original_end: 0,
            modified_start: 10,
            modified_end: 12,
        },
    ];
    assert_eq!(count_line_changes(&list), counts(4, 2));
    assert_eq!(count_line_changes(&[]), counts(0, 0));
}

#[test]
fn covers_the_new_lines_of_a_change() {
    assert_eq!(change_anchor(&change(3, 5)), (3, 5));
    assert_eq!(change_anchor(&change(20, 20)), (20, 20));
}

#[test]
fn puts_a_deletion_on_the_line_it_follows_and_a_deletion_at_the_top_on_line_1() {
    assert_eq!(change_anchor(&change(10, 0)), (10, 10));
    assert_eq!(change_anchor(&change(0, 0)), (1, 1));
}

#[test]
fn finds_the_change_that_holds_a_line_and_none_between_changes() {
    let list = changes();
    assert_eq!(change_at(&list, 3), Some(0));
    assert_eq!(change_at(&list, 4), Some(0));
    assert_eq!(change_at(&list, 5), Some(0));
    assert_eq!(change_at(&list, 6), None);
    assert_eq!(change_at(&list, 10), Some(1));
    assert_eq!(change_at(&list, 11), None);
    assert_eq!(change_at(&[], 1), None);
}

#[test]
fn goes_to_the_first_or_the_last_change_when_no_line_is_known_yet() {
    assert_eq!(step_change(&changes(), None, Step::Next), Some(0));
    assert_eq!(step_change(&changes(), None, Step::Prev), Some(2));
}

#[test]
fn goes_to_the_next_change_that_starts_after_the_line() {
    let list = changes();
    assert_eq!(step_change(&list, Some(1), Step::Next), Some(0));
    assert_eq!(step_change(&list, Some(3), Step::Next), Some(1));
    assert_eq!(step_change(&list, Some(4), Step::Next), Some(1));
    assert_eq!(step_change(&list, Some(10), Step::Next), Some(2));
}

#[test]
fn goes_to_the_previous_change_that_starts_before_the_line() {
    let list = changes();
    assert_eq!(step_change(&list, Some(20), Step::Prev), Some(1));
    assert_eq!(step_change(&list, Some(15), Step::Prev), Some(1));
    assert_eq!(step_change(&list, Some(4), Step::Prev), Some(0));
}

#[test]
fn wraps_at_both_ends() {
    let list = changes();
    assert_eq!(step_change(&list, Some(20), Step::Next), Some(0));
    assert_eq!(step_change(&list, Some(30), Step::Next), Some(0));
    assert_eq!(step_change(&list, Some(3), Step::Prev), Some(2));
    assert_eq!(step_change(&list, Some(1), Step::Prev), Some(2));
}

#[test]
fn has_nothing_to_go_to_without_changes() {
    assert_eq!(step_change(&[], None, Step::Next), None);
    assert_eq!(step_change(&[], Some(5), Step::Prev), None);
}

#[test]
fn names_the_change_the_cursor_is_in_or_counts_them_all() {
    assert_eq!(change_count_label(Some(1), 7), "2 of 7");
    assert_eq!(change_count_label(None, 7), "7 changes");
    assert_eq!(change_count_label(None, 1), "1 change");
    assert_eq!(change_count_label(None, 0), "0 changes");
}

fn paths() -> Vec<String> {
    ["a", "b", "c"].map(String::from).to_vec()
}

#[test]
fn moves_to_the_next_and_previous_file_and_stops_at_the_ends() {
    let list = paths();
    assert_eq!(step_file(&list, Some("b"), Step::Next), Some("c"));
    assert_eq!(step_file(&list, Some("b"), Step::Prev), Some("a"));
    assert_eq!(step_file(&list, Some("c"), Step::Next), Some("c"));
    assert_eq!(step_file(&list, Some("a"), Step::Prev), Some("a"));
}

#[test]
fn starts_at_the_first_file_when_none_is_chosen_and_has_none_for_an_empty_list() {
    assert_eq!(step_file(&paths(), None, Step::Next), Some("a"));
    assert_eq!(step_file(&[], None, Step::Next), None);
}

#[test]
fn wraps_prose_and_files_without_an_extension_not_code() {
    assert!(wraps_by_default("docs/notes.md"));
    assert!(wraps_by_default("README.TXT"));
    assert!(wraps_by_default("Makefile"));
    assert!(wraps_by_default(".gitignore"));
    assert!(!wraps_by_default("src/a.ts"));
    assert!(!wraps_by_default("v1.2/app.json"));
}

#[test]
fn covers_every_line_the_selection_touches() {
    assert_eq!(whole_lines(4, 4, 9), (4, 4));
    assert_eq!(whole_lines(4, 7, 3), (4, 7));
}

#[test]
fn leaves_out_a_last_line_that_the_selection_only_reaches_at_column_1() {
    assert_eq!(whole_lines(4, 7, 1), (4, 6));
    assert_eq!(whole_lines(4, 5, 1), (4, 4));
}

#[test]
fn keeps_a_one_line_selection_that_starts_at_column_1() {
    assert_eq!(whole_lines(4, 4, 1), (4, 4));
}

#[test]
fn splits_a_path_into_folder_and_name() {
    assert_eq!(split_path("src/lib/a.ts"), ("src/lib/", "a.ts"));
    assert_eq!(split_path("a.ts"), ("", "a.ts"));
}

#[test]
fn counts_changed_files() {
    assert_eq!(count_label(1), "1 changed file");
    assert_eq!(count_label(0), "0 changed files");
    assert_eq!(count_label(3), "3 changed files");
}

#[test]
fn names_lines_with_the_dash_it_is_given() {
    assert_eq!(lines_label(12, 12, "–"), "line 12");
    assert_eq!(lines_label(12, 14, "–"), "lines 12–14");
    assert_eq!(lines_label(12, 14, "-"), "lines 12-14");
}

#[test]
fn gives_each_status_a_colour_family() {
    let tones: Vec<_> = [Modified, Added, Untracked, Deleted, Conflict, Renamed]
        .into_iter()
        .map(status_tone)
        .collect();
    assert_eq!(
        tones,
        [
            StatusTone::Modified,
            StatusTone::Created,
            StatusTone::Created,
            StatusTone::Deleted,
            StatusTone::Deleted,
            StatusTone::Renamed
        ]
    );
}

fn file(
    path: &str,
    status: ChangeStatus,
    original: Option<&str>,
    modified: Option<&str>,
) -> FileDiff {
    FileDiff {
        path: path.into(),
        status,
        original: original.map(String::from),
        modified: modified.map(String::from),
        binary: false,
        too_large: false,
    }
}

#[test]
fn has_no_body_kind_before_the_file_is_read() {
    assert_eq!(body_kind(None, false, false), BodyKind::Pending);
}

#[test]
fn checks_an_image_before_binary_and_too_large() {
    let mut png = file("a.png", Modified, None, None);
    png.binary = true;
    png.too_large = true;
    assert_eq!(body_kind(Some(&png), false, false), BodyKind::Image);
    let mut other = file("a.bin", Modified, None, None);
    other.binary = true;
    assert_eq!(body_kind(Some(&other), false, false), BodyKind::Notice);
}

#[test]
fn shows_an_svg_as_an_image_only_when_the_preview_is_on() {
    let svg = file("a.svg", Modified, Some("<svg/>"), Some("<svg />"));
    assert!(!shows_image(&svg, false));
    assert!(shows_image(&svg, true));
    assert_eq!(body_kind(Some(&svg), false, false), BodyKind::Text);
    assert_eq!(body_kind(Some(&svg), true, false), BodyKind::Image);
}

#[test]
fn checks_binary_then_too_large_then_a_whole_file_then_the_same_text() {
    let mut big = file("a.ts", Added, None, Some("x"));
    big.too_large = true;
    assert_eq!(
        notice_text(&big, false),
        Some("This file is larger than 1 MB, so it is not shown.")
    );
    big.binary = true;
    assert_eq!(
        notice_text(&big, false),
        Some("This file is binary. It changed, but it is not shown.")
    );
    let new = file("a.ts", Untracked, None, Some("x\n"));
    assert_eq!(body_kind(Some(&new), false, false), BodyKind::Whole);
    assert_eq!(notice_text(&new, true), None);
    let gone = file("a.ts", Deleted, Some("x\n"), None);
    assert_eq!(body_kind(Some(&gone), false, false), BodyKind::Whole);
    let same = file("a.ts", Modified, Some("x"), Some("x"));
    assert_eq!(body_kind(Some(&same), false, true), BodyKind::Notice);
    assert_eq!(body_kind(Some(&same), false, false), BodyKind::Text);
}

#[test]
fn says_an_empty_new_or_gone_file_is_empty() {
    let new = file("a.ts", Untracked, None, Some(""));
    assert_eq!(body_kind(Some(&new), false, false), BodyKind::Notice);
    assert_eq!(notice_text(&new, false), Some("Empty file."));
    let gone = file("a.ts", Deleted, None, None);
    assert_eq!(notice_text(&gone, false), Some("Empty file."));
    let missing_side = file("a.ts", Added, Some("old"), None);
    assert_eq!(whole_text(&missing_side, WholeKind::Added), "");
    assert_eq!(whole_text(&missing_side, WholeKind::Removed), "old");
}

#[test]
fn says_why_a_file_with_the_same_text_shows_no_diff() {
    let renamed = file("b.ts", Renamed, Some("x"), Some("x"));
    assert_eq!(
        notice_text(&renamed, true),
        Some("Renamed without changes.")
    );
    let mode = file("a.ts", Modified, Some("x"), Some("x"));
    assert_eq!(
        notice_text(&mode, true),
        Some("The text did not change. Only the file mode may differ.")
    );
    assert_eq!(notice_text(&mode, false), None);
}
