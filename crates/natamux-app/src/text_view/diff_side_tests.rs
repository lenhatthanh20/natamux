//! One side of the own diff view (S6.2 part 2), drawn: its boxes, strips, gaps, marker and line
//! numbers, the colours of each line's own file, the shared scroll and the diff case of T-SB-1.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use gpui::{Entity, Hsla, Pixels, Point, TestAppContext, VisualTestContext, point};
use theme::ActiveTheme;
use ui::prelude::*;

use super::super::diff_paint::DiffPaint;
use super::*;
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_model::{self as model, Status, diff, inline_rows};

const MAC: bool = cfg!(target_os = "macos");

/// Two sides 400 x 200 px, one above the other.
struct Host {
    left: DiffSide,
    right: DiffSide,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .child(div().w(px(400.)).h(px(200.)).child(self.left.text_box()))
            .child(div().w(px(400.)).h(px(200.)).child(self.right.text_box()))
    }
}

fn open(cx: &mut TestAppContext) -> (DiffSide, DiffSide, &mut VisualTestContext) {
    let (host, _, cx) = test_views::init_app(cx, |_, window, cx| {
        let left = DiffSide::new(true, window, cx);
        let right = DiffSide::new(true, window, cx);
        cx.new(|_| Host { left, right })
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, MAC));
    cx.run_until_parked();
    let (left, right) = host.read_with(cx, |host, _| (host.left.clone(), host.right.clone()));
    (left, right, cx)
}

fn show(
    side: &DiffSide,
    path: &str,
    diff: &FileDiff,
    rows: &[Row],
    new_side: bool,
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| side.show(path, diff, rows, new_side, cx));
    cx.run_until_parked();
}

/// What the side paints, as drawn now: the layout, the diff's part, the origin of the text and
/// the gutter's numbers with whether each is an added line.
struct Drawn {
    line_height: Pixels,
    origin: gpui::Point<Pixels>,
    paint: DiffPaint,
    numbers: Vec<(usize, bool)>,
    rows: usize,
    first_rows: Vec<usize>,
}

fn drawn(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Drawn {
    input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the side was drawn");
        let laid = input.layout(window, cx);
        let origin = bounds.origin + point(laid.inset, px(0.)) - input.scroll;
        let mut paint = input.code_paint(&laid, bounds, origin, cx);
        let gutter = paint.gutter.take().expect("a diff side has a gutter");
        Drawn {
            line_height: laid.line_height,
            origin,
            paint: paint.diff.take().expect("a diff side paints its hunks"),
            numbers: gutter
                .numbers
                .iter()
                .zip(gutter.added.iter())
                .map(|((_, number, _), added)| (*number, *added))
                .collect(),
            rows: laid.rows,
            first_rows: laid.lines.iter().map(|line| line.first_row).collect(),
        }
    })
}

/// The theme colours the tests compare with.
struct Look {
    editor_diff_hunk_deleted_hollow_background: Hsla,
    editor_diff_hunk_added_hollow_background: Hsla,
    editor_diff_hunk_deleted_hollow_border: Hsla,
    editor_diff_hunk_added_hollow_border: Hsla,
    version_control_modified: Hsla,
    version_control_deleted: Hsla,
    border_variant: Hsla,
}

fn colors(cx: &mut VisualTestContext) -> Look {
    cx.update(|_, cx| {
        let colors = cx.theme().colors();
        Look {
            editor_diff_hunk_deleted_hollow_background: colors
                .editor_diff_hunk_deleted_hollow_background,
            editor_diff_hunk_added_hollow_background: colors
                .editor_diff_hunk_added_hollow_background,
            editor_diff_hunk_deleted_hollow_border: colors.editor_diff_hunk_deleted_hollow_border,
            editor_diff_hunk_added_hollow_border: colors.editor_diff_hunk_added_hollow_border,
            version_control_modified: colors.version_control_modified,
            version_control_deleted: colors.version_control_deleted,
            border_variant: colors.border_variant,
        }
    })
}

#[gpui::test]
fn a_hunk_shows_in_one_box_with_a_strip_of_its_kind(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let diff = diff(Some("a\nb\nc"), "a\nx\ny\nc", &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), false);
    show(&right, "x.txt", &diff, &rows, true, cx);
    let drawn = drawn(right.input(), cx);
    let colors = colors(cx);
    let lh = drawn.line_height;
    let row_y = |row: usize| drawn.origin.y + lh * row as f32;
    let strip = (lh * 0.275).floor();

    assert_eq!(
        drawn.numbers,
        vec![(1, false), (2, true), (3, true), (4, false)]
    );
    let backs: Vec<(Pixels, Pixels, Hsla)> = drawn
        .paint
        .backs
        .iter()
        .map(|(rect, color)| (rect.top(), rect.size.height, *color))
        .collect();
    assert_eq!(
        backs,
        vec![
            (
                row_y(1),
                lh,
                colors.editor_diff_hunk_deleted_hollow_background
            ),
            (
                row_y(2),
                lh,
                colors.editor_diff_hunk_added_hollow_background
            ),
            (
                row_y(3),
                lh,
                colors.editor_diff_hunk_added_hollow_background
            ),
            (
                row_y(1),
                px(1.),
                colors.editor_diff_hunk_deleted_hollow_border
            ),
            (
                row_y(4) - px(1.),
                px(1.),
                colors.editor_diff_hunk_added_hollow_border
            ),
        ]
    );
    let left = drawn.paint.backs[0].0.left();
    assert_eq!(drawn.paint.strips.len(), 1);
    let (bounds, color) = drawn.paint.strips[0];
    assert_eq!(color, colors.version_control_modified);
    assert_eq!((bounds.top(), bounds.bottom()), (row_y(1), row_y(4)));
    assert_eq!(bounds.size.width, strip);
    assert_eq!(bounds.right(), left, "the box starts where the strip ends");
}

#[gpui::test]
fn empty_rows_push_the_lines_below_them_down(cx: &mut TestAppContext) {
    let (left, _, cx) = open(cx);
    let diff = diff(Some("a\nb\nc"), "a\nx\ny\nc", &[]);
    let rows = model::side_rows(&diff.hunk_rows(), Side::Old, 3, 4, false);
    show(&left, "x.txt", &diff, &rows, false, cx);
    let drawn = drawn(left.input(), cx);
    assert_eq!(drawn.first_rows, vec![0, 1, 3]);
    assert_eq!(drawn.rows, 4);
    assert_eq!(drawn.numbers, vec![(1, false), (3, false)]);
    assert_eq!(drawn.paint.strips[0].1, colors(cx).version_control_modified);
}

#[gpui::test]
fn a_gap_row_has_a_line_half_a_row_down(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let text = "l0\nl1\nl2\nl3\nl4\nl5\nl6";
    let diff = diff(Some(text), text, &[]);
    let rows = [
        Row::Line {
            row: 0,
            status: Status::Unchanged,
        },
        Row::Gap { hidden: 5 },
        Row::Line {
            row: 6,
            status: Status::Unchanged,
        },
    ];
    show(&right, "x.txt", &diff, &rows, true, cx);
    let drawn = drawn(right.input(), cx);
    let lh = drawn.line_height;
    assert_eq!(drawn.first_rows, vec![0, 2]);
    let lines: Vec<(Pixels, Pixels, Hsla)> = drawn
        .paint
        .backs
        .iter()
        .map(|(rect, color)| (rect.top(), rect.size.height, *color))
        .collect();
    assert_eq!(
        lines,
        vec![(
            drawn.origin.y + lh + (lh / 2.).floor(),
            px(1.),
            colors(cx).border_variant
        )]
    );
    assert!(drawn.paint.strips.is_empty());
}

#[gpui::test]
fn a_hunk_with_no_lines_on_the_side_has_a_marker(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let diff = diff(Some("a\nb\nc"), "a\nc", &[]);
    let rows = model::side_rows(&diff.hunk_rows(), Side::New, 3, 2, false);
    show(&right, "x.txt", &diff, &rows, true, cx);
    let drawn = drawn(right.input(), cx);
    let lh = drawn.line_height;
    let size = (lh * 0.35).floor();
    assert_eq!(drawn.paint.strips.len(), 1);
    let (bounds, color) = drawn.paint.strips[0];
    assert_eq!(color, colors(cx).version_control_deleted);
    assert_eq!(bounds.size, gpui::size(size, size));
    // Line `c` is below the empty row of the deleted line.
    assert_eq!(bounds.top(), drawn.origin.y + lh * 2. - (size / 2.).floor());
    assert!(
        drawn.paint.backs.is_empty(),
        "no box: the side has no line of it"
    );
}

/// The colour of each byte of a side's text, as its runs give it, and the plain colour.
fn colours_of(side: &DiffSide, cx: &mut VisualTestContext) -> (String, Vec<Hsla>, Hsla) {
    let input = side.input().clone();
    input.update_in(cx, |input, _, cx| {
        let style = input.text_style(cx);
        let text = input.model.text().to_owned();
        let query = (input.query_lines(), input.sticky_heads());
        let runs = input.runs(&style, text.len(), false, &[], (query.0, &query.1), cx);
        let colours = runs
            .iter()
            .flat_map(|run| std::iter::repeat_n(run.color, run.len))
            .collect();
        (text, colours, style.color)
    })
}

/// Each line takes the colours of its own file: the last commit's side of side by side from the
/// last commit's parse, the working tree's lines from theirs. The deleted lines of an inline diff
/// stay plain, as the B2 shots show them (`changes-diff-focused`, a `.ts` file).
#[gpui::test]
fn every_line_takes_the_colours_of_its_own_file(cx: &mut TestAppContext) {
    let (left, right, cx) = open(cx);
    let diff = diff(Some("fn a() {}\n"), "fn b() {}\n", &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), false);
    show(&right, "x.rs", &diff, &rows, true, cx);
    let old_rows = model::side_rows(&diff.hunk_rows(), Side::Old, 2, 2, false);
    show(&left, "x.rs", &diff, &old_rows, false, cx);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| right.parsed(cx) && left.parsed(cx)));

    let (text, colours, plain) = colours_of(&right, cx);
    assert_eq!(text, "fn a() {}\nfn b() {}\n");
    // `fn` starts both lines.
    assert_eq!(
        colours[0], plain,
        "the deleted line of an inline diff is plain"
    );
    assert_ne!(colours[10], plain, "the added line is coloured");

    let (text, colours, plain) = colours_of(&left, cx);
    assert_eq!(text, "fn a() {}\n");
    assert_ne!(colours[0], plain, "the last commit's line is coloured");
    assert_eq!(colours[0], colours_of(&right, cx).1[10]);
}

/// The two sides of `old` to `new`, paired.
fn pair_of<'a>(
    old: &str,
    new: &str,
    cx: &'a mut TestAppContext,
) -> (DiffSide, DiffSide, &'a mut VisualTestContext) {
    let (left, right, cx) = open(cx);
    let diff = diff(Some(old), new, &[]);
    let hunks = diff.hunk_rows();
    let (old_rows, new_rows) = (diff.old_rows(), diff.new_rows());
    let left_rows = model::side_rows(&hunks, Side::Old, old_rows, new_rows, false);
    let right_rows = model::side_rows(&hunks, Side::New, old_rows, new_rows, false);
    show(&left, "x.txt", &diff, &left_rows, false, cx);
    show(&right, "x.txt", &diff, &right_rows, true, cx);
    cx.update(|_, cx| left.pair(&right, cx));
    cx.run_until_parked();
    (left, right, cx)
}

/// Two long sides, paired. The first line is long, so the sides scroll to the right with wrap
/// off.
fn long_pair(cx: &mut TestAppContext) -> (DiffSide, DiffSide, &mut VisualTestContext) {
    let lines: String = (0..60).map(|n| format!("line {n}\n")).collect();
    let old = format!("{}\n{lines}", "x".repeat(300));
    let new = format!("one\n{old}one\n");
    pair_of(&old, &new, cx)
}

fn scroll_y(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Pixels {
    input.read_with(cx, |input, _| input.scroll.y)
}

/// The scroll a side was last laid out at.
fn drawn_at(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Option<Point<Pixels>> {
    input.read_with(cx, |input, _| input.code.as_ref()?.diff.as_ref()?.drawn)
}

fn drawn_y(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Option<Pixels> {
    drawn_at(input, cx).map(|at| at.y)
}

/// The line height and the row count of a side, as laid out now.
fn rows_of(side: &DiffSide, cx: &mut VisualTestContext) -> (Pixels, usize) {
    side.input().update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        (laid.line_height, laid.rows)
    })
}

#[gpui::test]
fn the_two_sides_scroll_as_one(cx: &mut TestAppContext) {
    let (left, right, cx) = long_pair(cx);
    let (lh, _) = rows_of(&right, cx);
    right.input().update(cx, |input, cx| {
        input.scroll = point(px(0.), lh * 5.);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(scroll_y(left.input(), cx), lh * 5.);
    // The left side is drawn first, before the right one takes the scroll: it is drawn again,
    // with no other notify.
    assert_eq!(drawn_y(left.input(), cx), Some(lh * 5.));
    left.input().update(cx, |input, cx| {
        input.scroll = point(px(0.), lh * 2.);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(scroll_y(right.input(), cx), lh * 2.);
    assert_eq!(drawn_y(right.input(), cx), Some(lh * 2.));

    // Wrap off: a scroll to the right moves the other side too.
    for side in [&left, &right] {
        side.input().update(cx, |input, cx| {
            if let Some(code) = input.code.as_mut() {
                code.wrap = false;
            }
            cx.notify();
        });
    }
    cx.run_until_parked();
    let at = point(px(100.), lh * 2.);
    left.input().update(cx, |input, cx| {
        input.scroll = at;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(left.input().read_with(cx, |input, _| input.scroll), at);
    assert_eq!(right.input().read_with(cx, |input, _| input.scroll), at);
    assert_eq!(drawn_at(right.input(), cx), Some(at));
}

/// Wrap on, the sides can have a different row count (D17). The taller side still scrolls to its
/// end: the shorter side stops at its own end and does not pull the taller one back.
#[gpui::test]
fn the_taller_side_scrolls_to_its_end(cx: &mut TestAppContext) {
    let lines: String = (0..20).map(|n| format!("line {n}\n")).collect();
    let old = format!("{}\n{lines}", "word ".repeat(600));
    let (left, right, cx) = pair_of(&old, &lines, cx);
    let (lh, left_rows) = rows_of(&left, cx);
    let (_, right_rows) = rows_of(&right, cx);
    assert!(left_rows > right_rows + 20, "the deleted line wraps");
    let left_end = lh * (left_rows - 1) as f32;
    let right_end = lh * (right_rows - 1) as f32;
    left.input().update(cx, |input, cx| {
        input.scroll = point(px(0.), left_end);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        scroll_y(left.input(), cx),
        left_end,
        "the left side stays at its end"
    );
    assert_eq!(drawn_y(left.input(), cx), Some(left_end));
    assert_eq!(scroll_y(right.input(), cx), right_end);
    assert_eq!(drawn_y(right.input(), cx), Some(right_end));
}

/// T-SB-1, the diff case (plan 6.5): other places of the selected text do not keep the bar of a
/// diff side shown; a scroll and a caret out of view still do.
#[gpui::test]
fn t_sb_1_the_scrollbar_of_a_diff_side(cx: &mut TestAppContext) {
    let (_, right, cx) = long_pair(cx);
    let input = right.input().clone();
    let shows = |cx: &mut VisualTestContext| {
        input.update_in(cx, |input, window, cx| {
            let bounds = input.last_bounds.expect("the side was drawn");
            let laid = input.layout(window, cx);
            input.bar_bounds(&laid, bounds, cx).is_some()
        })
    };
    assert!(!shows(cx), "caret in view, no scroll");

    // `one` is on the first and the last line.
    input.update(cx, |input, cx| {
        input.model.select(0, 3);
        input.changed(false, cx);
    });
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    let places = input.read_with(cx, |input, _| {
        input.code.as_ref().map_or(0, |code| code.occurrences.len())
    });
    assert_eq!(places, 2, "the places are found");
    assert!(!shows(cx), "other places of the selected text");

    input.update(cx, |input, cx| input.code_scrolled(cx));
    assert!(shows(cx), "less than 1 s after a scroll");
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    assert!(!shows(cx), "1 s after a scroll");

    let lh = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), lh * 20.);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shows(cx), "caret out of view");
}

/// With wrap off and a scroll to the right, the selection and the guides reach into the gutter;
/// the whole text layer is clipped at the text's left, the hunk boxes are not.
#[gpui::test]
fn the_text_layer_of_a_side_scrolled_right_keeps_out_of_the_gutter(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let old = format!("fn a() {{\n    let x = \"{}\";\n}}\n", "y".repeat(300));
    let new = old.replace("x =", "z =");
    let diff = diff(Some(&old), &new, &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), false);
    show(&right, "x.rs", &diff, &rows, true, cx);
    right.input().update(cx, |input, cx| {
        if let Some(code) = input.code.as_mut() {
            code.wrap = false;
        }
        input.model.select(0, input.model.text().len());
        input.scroll = point(px(200.), px(0.));
        cx.notify();
    });
    cx.run_until_parked();
    right.input().update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the side was drawn");
        let laid = input.layout(window, cx);
        assert_eq!(
            input.scroll.x,
            px(200.),
            "the line is long enough to scroll"
        );
        let origin = bounds.origin + point(laid.inset, px(0.)) - input.scroll;
        let paint = input.code_paint(&laid, bounds, origin, cx);
        let diff = paint.diff.as_ref().expect("a diff side paints its hunks");
        let text_left = bounds.left() + laid.key.gutter;
        assert_eq!(diff.text_left, text_left);
        let selection_left = laid
            .selection_rects(0..input.model.text().len())
            .iter()
            .map(|rect| rect.left() + origin.x)
            .fold(bounds.right(), Pixels::min);
        assert!(
            selection_left < text_left,
            "the selection reaches into the gutter"
        );
        let guide_left = paint
            .guides
            .iter()
            .map(|(rect, _)| rect.left())
            .fold(bounds.right(), Pixels::min);
        assert!(guide_left < text_left, "a guide reaches into the gutter");
        let mask = diff.text_mask(bounds).bounds;
        assert_eq!(mask.left(), text_left);
        assert_eq!(mask.bottom_right(), bounds.bottom_right());
        assert_eq!(mask.top(), bounds.top());
        assert!(
            diff.backs.iter().all(|(rect, _)| rect.left() < text_left),
            "the hunk boxes start at the strip, left of the mask"
        );
    });
}

/// 30 lines, line 10 changed: inline, rows 0-9 unchanged, row 10 deleted, row 11 added.
fn long_change(cx: &mut TestAppContext) -> (DiffSide, &mut VisualTestContext) {
    let (_, right, cx) = open(cx);
    let old: Vec<String> = (0..30).map(|i| format!("l{i}")).collect();
    let old = old.join("\n");
    let new = old.replace("l10\n", "x\n");
    let diff = diff(Some(&old), &new, &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), false);
    show(&right, "x.txt", &diff, &rows, true, cx);
    (right, cx)
}

#[gpui::test]
fn a_box_scrolled_above_the_top_has_its_top_line_on_the_first_row_in_view(cx: &mut TestAppContext) {
    let (right, cx) = long_change(cx);
    let lh = drawn(right.input(), cx).line_height;
    cx.update(|_, cx| right.set_scroll(point(px(0.), lh * 11.), cx));
    cx.run_until_parked();
    assert_eq!(
        right.input().read_with(cx, |input, _| input.scroll.y),
        lh * 11.
    );
    let drawn = drawn(right.input(), cx);
    let colors = colors(cx);
    let row_y = |row: usize| drawn.origin.y + lh * row as f32;
    let lines: Vec<(Pixels, Hsla)> = drawn
        .paint
        .backs
        .iter()
        .filter(|(rect, _)| rect.size.height == px(1.))
        .map(|(rect, color)| (rect.top(), *color))
        .collect();
    assert_eq!(
        lines,
        vec![
            (row_y(11), colors.editor_diff_hunk_added_hollow_border),
            (
                row_y(12) - px(1.),
                colors.editor_diff_hunk_added_hollow_border
            ),
        ],
        "the deleted row above the top has no line"
    );
    let (strip, _) = drawn.paint.strips[0];
    assert_eq!((strip.top(), strip.bottom()), (row_y(11), row_y(12)));
}

#[gpui::test]
fn a_side_pins_its_header_once_its_top_row_is_at_or_below_a_gap(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let text: Vec<String> = (0..40).map(|i| format!("l{i}")).collect();
    let text = text.join("\n");
    let diff = diff(Some(&text), &text, &[]);
    let line = |row: u32| Row::Line {
        row,
        status: Status::Unchanged,
    };
    // Rows 0-19 lines, row 20 the gap, then 19 more lines.
    let mut rows: Vec<Row> = (0..20).map(line).collect();
    rows.push(Row::Gap { hidden: 1 });
    rows.extend((21..40).map(line));
    show(&right, "x.txt", &diff, &rows, true, cx);
    let lh = drawn(right.input(), cx).line_height;
    let pins = |scroll: Pixels, cx: &mut VisualTestContext| {
        cx.update(|_, cx| right.set_scroll(point(px(0.), scroll), cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            let place = right.place(window, cx).expect("the side was drawn");
            right
                .rows(cx, |rows| place.pins_header(rows))
                .expect("a diff side has rows")
        })
    };
    assert!(!pins(px(0.), cx), "at the top");
    assert!(!pins(lh * 19.5, cx), "the top row is above the gap");
    assert!(pins(lh * 20., cx), "the gap is the top row");
    assert!(pins(lh * 25., cx), "below the gap");
}

/// P-08: a long hunk paints a back rect only for its lines near the rows in view, not one per
/// line of the hunk.
#[gpui::test]
fn a_long_hunk_paints_boxes_only_for_its_rows_in_view(cx: &mut TestAppContext) {
    let (_, right, cx) = open(cx);
    let added: String = (0..3000).map(|n| format!("line {n}\n")).collect();
    let diff = diff(Some("a\n"), &format!("a\n{added}"), &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), false);
    show(&right, "x.txt", &diff, &rows, true, cx);
    let lh = drawn(right.input(), cx).line_height;
    cx.update(|_, cx| right.set_scroll(point(px(0.), lh * 1000.), cx));
    cx.run_until_parked();
    let bounds = right.input().read_with(cx, |input, _| input.last_bounds);
    let height = bounds.expect("the side was drawn").size.height;
    let in_view = (height / lh).ceil() as usize;
    let backs = drawn(right.input(), cx).paint.backs.len();
    // The rows in view, one line of slack above and below, and the box's top and bottom lines.
    assert!(
        backs <= in_view + 4,
        "{backs} back rects for {in_view} rows in view"
    );
}
