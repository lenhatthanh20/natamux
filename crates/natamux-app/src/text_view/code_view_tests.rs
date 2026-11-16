//! The own code view (plan 6.4, S4.3b): the wrap width keeps room for the scrollbar, a click does
//! not scroll, the matching brackets, the other places of the selected text after an edit, and
//! T-SB-1, the table of when the scrollbar shows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use gpui::{
    Entity, FocusHandle, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, UpdateGlobal as _, VisualTestContext, point,
};
use ui::prelude::*;

use super::code_bar::BAR_WIDTH;
use super::code_view::CodeView;
use super::text_box_tests::focus_handle;
use super::text_input::TextInput;
use crate::app::appearance::Appearance;
use crate::app::test_driver::press;
use crate::app::test_views;

const MAC: bool = cfg!(target_os = "macos");

/// A code view 400 x 200 px, and a stand-in that can take the focus from it.
struct Host {
    view: CodeView,
    other: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .child(div().w(px(400.)).h(px(200.)).child(self.view.text_box()))
            .child(div().size(px(10.)).track_focus(&self.other))
    }
}

fn open<'a>(
    path: &'static str,
    text: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<TextInput>, FocusHandle, &'a mut VisualTestContext) {
    let text = text.to_owned();
    let (host, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        let view = CodeView::new(path, &text, false, window, cx);
        cx.new(|cx| Host {
            view,
            other: cx.focus_handle(),
        })
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, MAC));
    cx.run_until_parked();
    let (input, other) = host.read_with(cx, |host, _| {
        (host.view.input().clone(), host.other.clone())
    });
    (input, other, cx)
}

fn lines(count: usize) -> String {
    (0..count).map(|n| format!("line {n}\n")).collect()
}

fn set_look(cx: &mut VisualTestContext, change: impl FnOnce(&mut Appearance)) {
    cx.update(|_, cx| Appearance::update_global(cx, |look, _| change(look)));
    cx.run_until_parked();
}

fn line_height(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Pixels {
    input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height)
}

fn scroll_to(input: &Entity<TextInput>, y: Pixels, cx: &mut VisualTestContext) {
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), y);
        cx.notify();
    });
    cx.run_until_parked();
}

fn scroll_y(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Pixels {
    input.read_with(cx, |input, _| input.scroll.y)
}

fn select(input: &Entity<TextInput>, from: usize, to: usize, cx: &mut VisualTestContext) {
    input.update(cx, |input, cx| {
        input.model.select(from, to);
        input.changed(false, cx);
    });
    cx.run_until_parked();
}

/// Lets the 100 ms wait for the other places of the selected text pass.
fn wait(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
}

fn click(at: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

/// The scrollbar as drawn now: its thumb, if it shows.
fn thumb(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> Option<gpui::Bounds<Pixels>> {
    input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the view was drawn");
        let laid = input.layout(window, cx);
        input.bar_bounds(&laid, bounds, cx).map(|bar| bar.thumb)
    })
}

/// The painted backgrounds (matching brackets, other places) and scrollbar markers.
fn painted(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> (usize, usize) {
    input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the view was drawn");
        let laid = input.layout(window, cx);
        let paint = input.code_paint(&laid, bounds, bounds.origin, cx);
        (paint.backgrounds.len(), paint.markers.len())
    })
}

#[gpui::test]
fn the_wrap_width_keeps_room_for_the_scrollbar_unless_it_never_shows(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.txt", &"word ".repeat(200), cx);
    let width = |cx: &mut VisualTestContext| {
        input.update_in(cx, |input, window, cx| {
            input
                .layout(window, cx)
                .key
                .width
                .expect("a full view wraps")
        })
    };
    let shown = width(cx);
    set_look(cx, |look| look.hide_scrollbars = true);
    let never = width(cx);
    let room = f32::from(never - shown);
    assert!((room - f32::from(BAR_WIDTH)).abs() < 0.01, "room {room}");
}

#[gpui::test]
fn a_click_on_the_last_shown_row_does_not_scroll_but_a_key_does(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.txt", &lines(60), cx);
    let line_height = line_height(&input, cx);
    scroll_to(&input, line_height * 10., cx);
    let bounds = input.read_with(cx, |input, _| input.last_bounds.unwrap());
    click(
        point(bounds.left() + px(20.), bounds.bottom() - line_height / 2.),
        cx,
    );
    assert_eq!(scroll_y(&input, cx), line_height * 10.);
    let head = input.read_with(cx, |input, _| input.model.head());
    assert!(head > 0, "the click moved the caret");
    // Keys keep the scroll margin of 3 lines.
    press(cx, "right");
    assert!(scroll_y(&input, cx) > line_height * 10.);
}

#[gpui::test]
fn a_drag_scrolls_only_once_the_pointer_leaves_the_text(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.txt", &lines(60), cx);
    let line_height = line_height(&input, cx);
    scroll_to(&input, line_height * 10., cx);
    let bounds = input.read_with(cx, |input, _| input.last_bounds.unwrap());
    let x = bounds.left() + px(20.);
    let drag_to = |y: Pixels, cx: &mut VisualTestContext| {
        cx.simulate_mouse_move(point(x, y), MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
    };
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: point(x, bounds.center().y),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    drag_to(bounds.bottom() - line_height * 1.5, cx);
    drag_to(bounds.bottom() - line_height / 2., cx);
    assert_eq!(scroll_y(&input, cx), line_height * 10.);
    let range = input.read_with(cx, |input, _| input.model.selection().range());
    assert!(!range.is_empty(), "the drag selected text");
    drag_to(bounds.bottom() + line_height, cx);
    assert!(scroll_y(&input, cx) > line_height * 10.);
}

#[gpui::test]
fn the_matching_brackets_count_the_edges_and_take_the_smallest_pair(cx: &mut TestAppContext) {
    let text = "{\"a\": [1, 2], \"b\": \"cd\"}";
    let (input, _, cx) = open("x.json", text, cx);
    let at = |at: usize, cx: &mut VisualTestContext| {
        input.update(cx, |input, _| {
            input.model.move_to(at);
            input.matching_brackets()
        })
    };
    let object = vec![0..1, 23..24];
    let array = vec![6..7, 11..12];
    assert_eq!(at(0, cx), object, "|{{");
    assert_eq!(at(24, cx), object, "}}|");
    assert_eq!(at(6, cx), array, "|[");
    assert_eq!(at(7, cx), array, "[|");
    assert_eq!(at(12, cx), array, "]|");
    assert_eq!(at(20, cx), vec![19..20, 22..23], "in a string");
    assert_eq!(at(14, cx), vec![14..15, 16..17], "|\"b\"");
    assert_eq!(at(5, cx), object, "between pairs");
    input.update(cx, |input, _| input.model.select(6, 8));
    assert!(
        input
            .update(cx, |input, _| input.matching_brackets())
            .is_empty(),
        "a selection"
    );
}

#[gpui::test]
fn the_brackets_follow_the_selection_not_the_focus(cx: &mut TestAppContext) {
    let (input, other, cx) = open("x.json", "{}", cx);
    let own = input.read_with(cx, |input, _| input.focus.clone());
    focus_handle(&own, cx);
    assert_eq!(painted(&input, cx).0, 0, "the focus alone");
    select(&input, 1, 1, cx);
    assert_eq!(painted(&input, cx).0, 2, "{{|}}");
    focus_handle(&other, cx);
    assert_eq!(painted(&input, cx).0, 2, "after a blur");
}

#[gpui::test]
fn the_other_places_are_searched_again_after_an_edit(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.txt", "ab x AB", cx);
    select(&input, 0, 2, cx);
    wait(cx);
    let places = |cx: &mut VisualTestContext| {
        input.read_with(cx, |input, _| {
            input.code.as_ref().unwrap().occurrences.clone()
        })
    };
    assert_eq!(places(cx), vec![0..2, 5..7]);
    input.update(cx, |input, cx| input.replace_minimal("zz ab x AB", cx));
    cx.run_until_parked();
    assert!(
        places(cx).is_empty(),
        "no stale places right after the edit"
    );
    wait(cx);
    assert_eq!(places(cx), vec![3..5, 8..10]);
}

/// T-SB-1 (plan 6.4): one row per case of the `show: auto` rule, plus always and never.
#[gpui::test]
fn t_sb_1_when_the_scrollbar_shows(cx: &mut TestAppContext) {
    let text = format!("one\n{}one\n", lines(60));
    let (input, _, cx) = open("x.txt", &text, cx);
    let line_height = line_height(&input, cx);
    let shows = |cx: &mut VisualTestContext| thumb(&input, cx).is_some();

    assert!(!shows(cx), "caret in view, no scroll");

    set_look(cx, |look| look.always_scrollbars = true);
    assert!(shows(cx), "always");
    set_look(cx, |look| look.always_scrollbars = false);

    input.update(cx, |input, cx| input.code_scrolled(cx));
    assert!(shows(cx), "less than 1 s after a scroll");
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    assert!(!shows(cx), "1 s after a scroll");

    scroll_to(&input, line_height * 10., cx);
    assert!(shows(cx), "caret out of view");
    set_look(cx, |look| look.hide_scrollbars = true);
    assert!(!shows(cx), "never, caret out of view");
    set_look(cx, |look| look.hide_scrollbars = false);
    scroll_to(&input, px(0.), cx);
    assert!(!shows(cx), "caret back in view");

    // Taken: it stays while the thumb is held, long after the scroll.
    input.update(cx, |input, cx| input.code_scrolled(cx));
    let held = thumb(&input, cx).expect("shown after a scroll");
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: held.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    assert!(shows(cx), "dragged");
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: held.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    scroll_to(&input, px(0.), cx);
    assert!(!shows(cx), "let go");

    // `one` is on the first and the last line: the other place keeps the bar shown and marked.
    select(&input, 0, 3, cx);
    wait(cx);
    assert!(shows(cx), "selected text with another place");
    // No caret marker for the head of a selection, as Zed's editor drew none (B1 shot
    // `workspace-editor-scrollbar`).
    assert_eq!(
        painted(&input, cx).1,
        1,
        "a place marker and no caret marker"
    );
    select(&input, 4, 10, cx);
    wait(cx);
    assert!(!shows(cx), "selected text with no other place");
}

#[gpui::test]
fn tab_over_a_selection_indents_go_with_a_tab_char(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.go", "a\nb", cx);
    let own = input.read_with(cx, |input, _| input.focus.clone());
    focus_handle(&own, cx);
    select(&input, 0, 3, cx);
    press(cx, "tab");
    cx.run_until_parked();
    let text = input.read_with(cx, |input, _| input.model.text().to_owned());
    assert_eq!(text, "\ta\n\tb");
}

#[gpui::test]
fn find_shows_matches_and_selects_one_in_the_middle(cx: &mut TestAppContext) {
    use crate::git::code_target::FindTarget as _;

    let text = lines(60);
    let (host, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        let view = CodeView::new("x.txt", &text, false, window, cx);
        cx.new(|cx| Host {
            view,
            other: cx.focus_handle(),
        })
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, MAC));
    cx.run_until_parked();
    let (view, input) =
        host.read_with(cx, |host, _| (host.view.clone(), host.view.input().clone()));

    let whole = input.read_with(cx, |input, _| input.text().to_owned());
    let found: Vec<_> = whole
        .match_indices("line ")
        .map(|(at, _)| at..at + 5)
        .collect();
    assert!(found.len() > 2);
    cx.update(|_, cx| view.show_matches(&found, Some(0), cx));
    cx.run_until_parked();
    let shown = input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the view was drawn");
        let laid = input.layout(window, cx);
        input.code_paint(&laid, bounds, bounds.origin, cx).search
    });
    assert!(
        shown.iter().any(|(_, active)| *active),
        "the active match is marked"
    );
    assert!(
        shown.iter().any(|(_, active)| !active),
        "the others are not"
    );

    let last = found[found.len() - 1].clone();
    cx.update(|window, cx| view.select(last.clone(), window, cx));
    cx.run_until_parked();
    let head = input.read_with(cx, |input, _| input.model.head());
    assert_eq!(head, last.end);
    assert!(
        scroll_y(&input, cx) > px(0.),
        "the view scrolled to the match"
    );

    cx.update(|_, cx| view.clear_matches(cx));
    cx.run_until_parked();
    let none = input.read_with(cx, |input, _| input.code.as_ref().map(|c| c.search.len()));
    assert_eq!(none, Some(0));
}

/// A wrap point inside `[**` falls between the two `*`, by the widths of the chars, as Zed's
/// editor wrapped, and the row after it is shaped alone: its glyphs sit where that row's text
/// shaped by itself puts them (B1 shot `code-markdown-inline` at 1024 x 720).
#[gpui::test]
fn the_row_after_a_wrap_inside_a_bold_mark_is_shaped_alone(cx: &mut TestAppContext) {
    let (input, _, cx) = open("x.txt", "x", cx);
    let fit = input.update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        let width = laid.key.width.expect("a full view wraps");
        let text_system = window.text_system();
        let font = text_system.resolve_font(&laid.key.font);
        let char_width = text_system.layout_width(font, laid.key.font_size, 'a');
        (width / char_width).floor() as usize
    });
    // The last `*` that fits is the first of the two; the second starts the next row.
    let text = format!(
        "{} [**bold link**](https://example.com).",
        "a".repeat(fit - 3)
    );
    input.update(cx, |input, cx| input.set_text(&text, cx));
    cx.run_until_parked();
    input.update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        assert_eq!(laid.row_range(0), 0..fit, "the wrap point");
        assert_eq!(&text[fit - 1..fit + 1], "**");
        let rest = SharedString::from(text[fit..].to_owned());
        let run = gpui::TextRun {
            len: rest.len(),
            font: laid.key.font.clone(),
            color: laid.key.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let alone = window
            .text_system()
            .shape_line(rest.clone(), laid.key.font_size, &[run], None);
        for at in 0..rest.len() {
            let position = laid.position(fit + at, false);
            assert_eq!(
                position.y, laid.line_height,
                "char {at} is on the second row"
            );
            assert_eq!(position.x, alone.x_for_index(at), "char {at}");
        }
        // A click finds the char by the same row.
        let x = alone.x_for_index(2) + px(1.);
        assert_eq!(laid.index_in_row(1, x), (fit + 2, false));
    });
}

/// A row after a wrap starts as far in as its line, and a selection that goes on from the row
/// above covers that indent too, as in Zed's editor (S3.3 shot `workspace-docs-edit-selection`
/// at 1024 x 720).
#[gpui::test]
fn a_row_after_a_wrap_keeps_the_indent_of_its_line(cx: &mut TestAppContext) {
    let text = format!("    {}", "word ".repeat(200));
    let (input, _, cx) = open("x.txt", &text, cx);
    input.update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        let start = laid.row_range(1).start;
        assert!(start > 4, "the line wraps");
        let text_system = window.text_system();
        let font = text_system.resolve_font(&laid.key.font);
        let indent = text_system.layout_width(font, laid.key.font_size, ' ') * 4.;
        assert_eq!(laid.position(start, false).x, indent);
        let rects = laid.selection_rects(0..text.len());
        assert_eq!(rects[1].left(), px(0.), "the selection covers the indent");
        // Its first rect is the bit past the end of the row above.
        let rects = laid.selection_rects(start..text.len());
        assert_eq!(
            rects[1].left(),
            indent,
            "a selection from the row start does not"
        );
    });
}
