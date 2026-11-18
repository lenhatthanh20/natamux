//! The gutter widths, the lines of the selection and the sticky rows (plan 6.4, S4.3c).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::app::shots_text::sticky_file;
use crate::git::languages::lang_for;

#[test]
fn the_gutter_has_room_for_four_digits_and_grows_past_them() {
    let (ch, advance) = (px(8.), px(8.));
    // 4 digits, 2 ch left and 4 ch right.
    assert_eq!(gutter_widths(1, ch, advance), (px(80.), px(32.)));
    assert_eq!(gutter_widths(9999, ch, advance), (px(80.), px(32.)));
    assert_eq!(gutter_widths(10000, ch, advance), (px(88.), px(32.)));
    assert_eq!(gutter_widths(0, ch, advance), (px(80.), px(32.)));
}

#[test]
fn the_selected_lines_take_both_ends() {
    let text = "ab\ncd\nef\n";
    assert_eq!(selected_lines(text, 0..0), 0..1);
    assert_eq!(selected_lines(text, 1..4), 0..2);
    // A selection that ends at a line start still takes that line, as Zed's active rows do.
    assert_eq!(selected_lines(text, 0..3), 0..2);
    assert_eq!(selected_lines(text, 9..9), 3..4);
}

fn item(range: Range<usize>, head: usize) -> OutlineItem {
    OutlineItem {
        depth: 0,
        text: String::new(),
        range,
        selection: head..head + 1,
        source: head..head + 1,
        body: None,
        names: Vec::new(),
    }
}

/// The line index of an offset: no wrap, so a row is a line.
fn line_of(text: &str) -> impl Fn(usize) -> usize + '_ {
    move |at| text[..at].matches('\n').count()
}

#[test]
fn a_sticky_row_is_pushed_up_as_its_item_ends() {
    let text: String = (0..10).map(|n| format!("line {n}\n")).collect();
    let start = |line: usize| line * 7;
    // An outer item over lines 0 to 8 and an inner one over lines 2 to 5.
    let items = vec![
        item(start(0)..start(8), 0),
        item(start(2)..start(5), start(2)),
    ];
    let rows = |top: f32| sticky_rows(&items, &text, top, line_of(&text));
    assert!(rows(0.).is_empty(), "the head is still in its place");
    let at_3 = rows(3.);
    assert_eq!(
        at_3,
        vec![
            Sticky {
                row: 0,
                line: 0,
                offset: 0.
            },
            Sticky {
                row: 2,
                line: 2,
                offset: 1.
            },
        ]
    );
    assert_eq!(rows(3.5)[1].offset, 0.5, "half a row up");
    assert_eq!(rows(5.).len(), 1, "the inner item ended");
    assert!(rows(8.).is_empty(), "the outer item ended");
}

#[test]
fn a_one_line_item_and_a_second_item_on_the_same_row_pin_nothing() {
    let text = "a b\nc\nd\ne\n";
    let items = vec![item(0..8, 0), item(0..6, 2), item(4..5, 4)];
    let rows = sticky_rows(&items, text, 1.5, line_of(text));
    assert_eq!(rows.len(), 1, "{rows:?}");
}

/// The `editor-sticky-end` state (S4.3a): the sticky file at top row 125.5. The function ends on
/// row 128, so its head is pushed up half a row under the class and the method.
#[test]
fn the_sticky_end_shot_pushes_the_function_up_half_a_row() {
    let text = sticky_file();
    let lang = lang_for("src/catalog_view.ts").expect("TypeScript");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).unwrap();
    let tree = parser.parse(&text, None).unwrap();
    let items = outline(lang, &tree, &text);
    let rows_at = |top: f32| {
        let top_line = top as usize;
        let near = line_bytes(&text, &(top_line..top_line + STICKY_LINES));
        sticky_rows(&containing(&items, near), &text, top, line_of(&text))
    };
    let end = rows_at(125.5);
    let offsets: Vec<f32> = end.iter().map(|row| row.offset).collect();
    assert_eq!(offsets, vec![0., 1., 1.5], "{end:?}");
    let function = text.lines().nth(end[2].row).unwrap();
    assert!(function.contains("function line"), "{function}");
    let before: Vec<f32> = rows_at(100.).iter().map(|row| row.offset).collect();
    assert_eq!(before, vec![0., 1., 2.]);
}

#[test]
fn line_bytes_take_whole_lines_and_stop_at_the_end() {
    let text = "a\nbc\nd";
    assert_eq!(line_bytes(text, &(1..2)), 2..5);
    assert_eq!(line_bytes(text, &(0..0)), 0..0);
    assert_eq!(line_bytes(text, &(1..10)), 2..6);
    assert_eq!(line_bytes(text, &(5..6)), 6..6);
}
