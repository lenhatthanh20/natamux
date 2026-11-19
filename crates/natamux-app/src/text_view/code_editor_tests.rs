//! The editor tab's parts of the code view (plan 6.4, S4.3c): blame labels and the row menu, the
//! gutter, sideways scroll with its scrollbar, sticky rows, the guide cache and the lines the
//! colours are asked for.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Entity, Hsla, Modifiers, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent,
    TestAppContext, UpdateGlobal as _, VisualTestContext, point,
};
use ui::{ContextMenu, prelude::*};

use super::*;
use crate::app::appearance::Appearance;
use crate::app::shots_text::sticky_file;
use crate::app::test_views;
use crate::git::code_target::RowMenu;
use crate::text::model::TextModel;

const MAC: bool = cfg!(target_os = "macos");

/// The editor tab's code view, 400 x 200 px.
struct Host {
    view: CodeView,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(200.)).child(self.view.text_box())
    }
}

fn open<'a>(
    path: &'static str,
    text: &str,
    cx: &'a mut TestAppContext,
) -> (CodeView, Entity<TextInput>, &'a mut VisualTestContext) {
    let text = text.to_owned();
    let (host, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        let view = CodeView::editor(path, &text, window, cx);
        cx.new(|_| Host { view })
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, MAC));
    cx.run_until_parked();
    let (view, input) =
        host.read_with(cx, |host, _| (host.view.clone(), host.view.input().clone()));
    (view, input, cx)
}

fn lines(count: usize) -> String {
    (0..count).map(|n| format!("line {n}\n")).collect()
}

fn paint(
    input: &Entity<TextInput>,
    cx: &mut VisualTestContext,
) -> super::super::code_paint::CodePaint {
    input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the view was drawn");
        let laid = input.layout(window, cx);
        let origin = bounds.origin + point(laid.inset, px(0.)) - input.scroll;
        input.code_paint(&laid, bounds, origin, cx)
    })
}

fn set_look(cx: &mut VisualTestContext, change: impl FnOnce(&mut Appearance)) {
    cx.update(|_, cx| Appearance::update_global(cx, |look, _| change(look)));
    cx.run_until_parked();
}

#[test]
fn labels_go_before_their_rows_and_map_offsets_both_ways() {
    let text = "ab\ncd\n";
    let mut labels = Labels::default();
    labels.set(
        vec![(1, "L1 ".into()), (0, "L0 ".into()), (9, "gone".into())],
        text,
    );
    assert_eq!(labels.insert(text), "L0 ab\nL1 cd\n");
    assert_eq!(labels.total(), 6);
    assert_eq!(labels.shown_ranges(), vec![0..3, 6..9]);
    // The caret at a row start shows after the label.
    assert_eq!(labels.shown(0), 3);
    assert_eq!(labels.shown(2), 5);
    assert_eq!(labels.shown(3), 9);
    assert_eq!(labels.text_at(1), 0, "inside a label");
    assert_eq!(labels.text_at(4), 1);
    assert_eq!(labels.text_at(7), 3, "inside the second label");
    assert_eq!(labels.text_at(10), 4);
    for at in 0..=text.len() {
        assert_eq!(labels.text_at(labels.shown(at)), at, "round trip at {at}");
    }
}

#[test]
fn labels_map_offsets_like_a_walk_over_every_label() {
    let text = "ab\n".repeat(40);
    let rows: Vec<(u32, String)> = (0..40u32)
        .filter(|row| row % 3 != 1)
        .map(|row| (row, "x".repeat(1 + row as usize % 4)))
        .collect();
    let mut labels = Labels::default();
    labels.set(rows, &text);
    for at in 0..=text.len() {
        let walked: usize = labels
            .spots
            .iter()
            .take_while(|(spot, _)| *spot <= at)
            .map(|(_, label)| label.len())
            .sum();
        assert_eq!(labels.shown(at), at + walked, "shown {at}");
    }
    let shown_len = text.len() + labels.total();
    for shown in 0..=shown_len {
        let mut extra = 0;
        let mut want = None;
        for (spot, label) in &labels.spots {
            let start = spot + extra;
            if shown < start {
                break;
            }
            if shown < start + label.len() {
                want = Some(*spot);
                break;
            }
            extra += label.len();
        }
        assert_eq!(
            labels.text_at(shown),
            want.unwrap_or(shown - extra),
            "text_at {shown}"
        );
    }
}

#[test]
fn labels_move_with_their_lines_when_the_text_changes() {
    let mut labels = Labels::default();
    labels.set(vec![(0, "L0 ".into()), (1, "L1 ".into())], "ab\ncd\n");
    // A line typed at the top: the labels stay with their lines.
    labels.follow("x\nab\ncd\n", None, 1);
    assert_eq!(labels.insert("x\nab\ncd\n"), "x\nL0 ab\nL1 cd\n");
    // A typed letter inside a line moves the next line's label only.
    labels.follow("x\naYb\ncd\n", None, 2);
    assert_eq!(labels.insert("x\naYb\ncd\n"), "x\nL0 aYb\nL1 cd\n");
    // The labelled first line deleted: its label goes where the line was.
    labels.follow("x\ncd\n", None, 3);
    assert_eq!(labels.insert("x\ncd\n"), "x\nL0 L1 cd\n");
    // A delete in the middle of a line before a label.
    labels.set(vec![(1, "L1 ".into())], "abc\nde\n");
    labels.follow("ac\nde\n", None, 4);
    assert_eq!(labels.insert("ac\nde\n"), "ac\nL1 de\n");
}

#[test]
fn labels_follow_each_edit_of_a_change_with_many() {
    let start = std::time::Instant::now();
    let mut model = TextModel::with_text("ab\ncd\n");
    let mut labels = Labels::default();
    labels.set(vec![(0, "L0 ".into()), (1, "L1 ".into())], model.text());
    model.track_edits();
    // Tab on both lines: one change of two inserts, made back to front.
    let indent = |at: usize| crate::text::minimal::Edit {
        range: at..at,
        text: "    ".into(),
    };
    model.edit_all(&[indent(0), indent(3)], 0, 14, start);
    let edits = model.take_edits();
    labels.follow(model.text(), edits.as_deref(), model.version());
    assert_eq!(labels.insert(model.text()), "    L0 ab\n    L1 cd\n");
    // Undo takes both inserts out again.
    model.track_edits();
    assert!(model.undo());
    let edits = model.take_edits();
    labels.follow(model.text(), edits.as_deref(), model.version());
    assert_eq!(labels.insert(model.text()), "L0 ab\nL1 cd\n");
    // A whole new text: no edits are known, so one is guessed.
    model.track_edits();
    model.set_text("x\nab\ncd\n", start);
    let edits = model.take_edits();
    assert_eq!(edits, None);
    labels.follow(model.text(), edits.as_deref(), model.version());
    assert_eq!(labels.insert(model.text()), "x\nL0 ab\nL1 cd\n");
}

#[test]
fn labels_skip_a_sync_with_the_text_version_unchanged() {
    let mut labels = Labels::default();
    labels.set(vec![(1, "L1 ".into())], "ab\ncd\n");
    labels.follow("ab\ncd\n", None, 7);
    // The same version is the same text: nothing is guessed or copied, so a different text given
    // with it changes nothing.
    labels.follow("x\nab\ncd\n", None, 7);
    assert_eq!(labels.text, "ab\ncd\n");
    assert_eq!(labels.insert("ab\ncd\n"), "ab\nL1 cd\n");
    // A new version follows the text.
    labels.follow("x\nab\ncd\n", None, 8);
    assert_eq!(labels.insert("x\nab\ncd\n"), "x\nab\nL1 cd\n");
    // New labels are of a text of no known version.
    labels.set(vec![(0, "L0 ".into())], "ab\n");
    labels.follow("x\nab\n", None, 8);
    assert_eq!(labels.insert("x\nab\n"), "x\nL0 ab\n");
}

#[test]
fn a_guessed_edit_keeps_labels_on_whole_chars() {
    // á (C3 A1) and ạ (E1 BA A1) end with the same byte.
    let mut labels = Labels::default();
    let rows = vec![(0, "L0 ".into()), (1, "L1 ".into()), (2, "L2 ".into())];
    labels.set(rows, "ab\nlá\n");
    labels.follow("aạ\n", None, 1);
    assert_eq!(labels.insert("aạ\n"), "L0 aạL1 \nL2 ");
    let edit = input_edit("ab\nlá\n", "aạ\n");
    assert_eq!(
        (edit.start_byte, edit.old_end_byte, edit.new_end_byte),
        (1, 6, 4)
    );
}

#[test]
fn label_runs_split_the_runs_of_the_text() {
    let text = "ab\ncd";
    let mut labels = Labels::default();
    labels.set(vec![(0, "L ".into()), (1, "M ".into())], text);
    let run = |len: usize, color: Hsla| TextRun {
        len,
        font: gpui::font(".ZedMono"),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let (text_color, label_color) = (gpui::red(), gpui::blue());
    let out = labels.splice(
        vec![run(4, text_color), run(1, text_color)],
        &run(0, label_color),
    );
    let lens: Vec<(usize, bool)> = out
        .iter()
        .map(|run| (run.len, run.color == label_color))
        .collect();
    assert_eq!(
        lens,
        vec![(2, true), (3, false), (2, true), (1, false), (1, false)]
    );
}

#[gpui::test]
fn blame_labels_shift_their_rows_and_the_menu_opens_on_a_right_click(cx: &mut TestAppContext) {
    let (view, input, cx) = open("x.rs", "fn a() {}\nfn b() {}\n", cx);
    let row_start = 10;
    let x_of = |cx: &mut VisualTestContext| {
        input.update_in(cx, |input, window, cx| {
            let laid = input.layout(window, cx);
            laid.position(input.to_display(row_start), false).x
        })
    };
    let before = x_of(cx);
    cx.update(|_, cx| view.set_labels(vec![(1, "abc 2 days ago ".into())], cx));
    cx.run_until_parked();
    assert!(x_of(cx) > before, "the label pushes the row's text right");
    let shown = input.read_with(cx, |input, _| input.display().0.to_string());
    assert_eq!(shown, "fn a() {}\nabc 2 days ago fn b() {}\n");
    assert_eq!(cx.update(|_, cx| view.lines(cx)), 3);

    let asked: Rc<Cell<Option<Option<u32>>>> = Rc::default();
    let seen = asked.clone();
    let menu: RowMenu = Rc::new(
        move |row: Option<u32>, menu: ContextMenu, _: &mut Window, _: &mut App| -> ContextMenu {
            seen.set(Some(row));
            menu.entry("Close Annotations", None, |_, _| {})
        },
    );
    cx.update(|_, cx| view.set_menu(menu, cx));
    let (gutter_at, at) = input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.unwrap();
        let laid = input.layout(window, cx);
        let y = bounds.top() + laid.line_height * 1.5;
        (
            point(bounds.left() + px(2.), y),
            point(bounds.right() - px(40.), y),
        )
    });
    let right_press = |position, cx: &mut VisualTestContext| {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Right,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
    };
    // The line numbers open no menu, as in the Zed editor.
    right_press(gutter_at, cx);
    assert_eq!(asked.get(), None, "no menu from the gutter");
    right_press(at, cx);
    assert_eq!(asked.get(), Some(Some(1)), "the menu of the clicked row");
    let open = input.read_with(cx, |input, _| {
        input.code.as_ref().unwrap().open_menu.is_some()
    });
    assert!(open, "the menu is open");
    let head = input.read_with(cx, |input, _| input.model.head());
    assert!(head > row_start, "the caret went to the click");

    cx.update(|_, cx| view.clear_labels(cx));
    cx.run_until_parked();
    assert_eq!(x_of(cx), before, "the labels are gone");
}

/// T-0350: a file without blame (untracked, or before blame attaches) had no right-click menu,
/// so the mouse could not copy.
#[gpui::test]
fn a_right_press_opens_the_edit_menu_without_blame(cx: &mut TestAppContext) {
    let (_, input, cx) = open("x.rs", "fn a() {}\nfn b() {}\n", cx);
    input.update(cx, |input, cx| {
        // The whole first line: the press right of its end lands at its end, inside.
        input.model.select(0, 9);
        cx.notify();
    });
    cx.run_until_parked();
    let at = input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.unwrap();
        let laid = input.layout(window, cx);
        point(
            bounds.right() - px(40.),
            bounds.top() + laid.line_height * 0.5,
        )
    });
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    let (open, selection) = input.read_with(cx, |input, _| {
        let open = input.code.as_ref().unwrap().open_menu.is_some();
        (open, input.model.selection().range())
    });
    assert!(open, "the edit menu opens with no blame attached");
    assert_eq!(
        selection,
        0..9,
        "a press inside the selection keeps it for Copy"
    );
}

#[gpui::test]
fn the_editor_view_numbers_its_lines_in_a_gutter(cx: &mut TestAppContext) {
    let (_, input, cx) = open("x.rs", &lines(30), cx);
    let painted = paint(&input, cx);
    let gutter = painted.gutter.expect("the editor tab has a gutter");
    assert!(gutter.bounds.size.width > px(0.));
    let (first, number, active) = gutter.numbers[0];
    assert_eq!((number, active), (1, true), "the caret's line is active");
    assert!(first <= gutter.bounds.top() + px(1.));
    assert!(gutter.numbers[1..].iter().all(|(_, _, active)| !active));
    // The text starts right of the gutter.
    let inset = input.update_in(cx, |input, window, cx| input.layout(window, cx).inset);
    assert!(inset > gutter.bounds.size.width);
}

#[gpui::test]
fn a_view_that_does_not_wrap_scrolls_sideways_with_its_own_bar(cx: &mut TestAppContext) {
    let long = format!("{}\nshort\n", "word ".repeat(80));
    let (view, input, cx) = open("x.rs", &long, cx);
    let rows = |cx: &mut VisualTestContext| {
        input.update_in(cx, |input, window, cx| input.layout(window, cx).rows)
    };
    assert_eq!(rows(cx), 3, "a .rs file does not wrap");
    assert!(!cx.update(|_, cx| view.wraps(cx)));
    set_look(cx, |look| look.always_scrollbars = true);
    let hbar = paint(&input, cx)
        .hbar
        .expect("a line is wider than the view");
    assert!(hbar.0.thumb.size.width < hbar.0.track.size.width);

    cx.simulate_event(ScrollWheelEvent {
        position: hbar.0.track.center() - point(px(0.), px(40.)),
        delta: ScrollDelta::Pixels(point(px(-30.), px(0.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let x = input.read_with(cx, |input, _| input.scroll.x);
    assert_eq!(x, px(30.), "the wheel scrolls sideways");

    // The caret at the end of the long line is kept in view.
    input.update(cx, |input, cx| {
        input.model.move_to(399);
        input.changed(false, cx);
    });
    cx.run_until_parked();
    let (x, most) = input.update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        (input.scroll.x, input.code_scroll_x_limit(&laid))
    });
    assert!(x > px(30.) && x <= most, "{x:?} of {most:?}");

    cx.update(|_, cx| view.set_wrap(true, cx));
    cx.run_until_parked();
    assert!(rows(cx) > 3, "wrapped");
    assert!(cx.update(|_, cx| view.wraps(cx)));
    assert_eq!(input.read_with(cx, |input, _| input.scroll.x), px(0.));
    assert!(paint(&input, cx).hbar.is_none());
}

#[gpui::test]
fn the_sticky_rows_of_the_sticky_end_state(cx: &mut TestAppContext) {
    let (_, input, cx) = open("src/catalog_view.ts", &sticky_file(), cx);
    // The parse may finish on a background thread.
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let line_height = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), line_height * 125.5);
        cx.notify();
    });
    cx.run_until_parked();
    let painted = paint(&input, cx);
    let bounds = input.read_with(cx, |input, _| input.last_bounds.unwrap());
    let offsets: Vec<f32> = painted
        .sticky
        .iter()
        .map(|row| (row.top - bounds.top()) / line_height)
        .collect();
    assert_eq!(offsets.len(), 3, "{offsets:?}");
    for (offset, want) in offsets.iter().zip([0., 1., 1.5]) {
        assert!((offset - want).abs() < 0.001, "{offsets:?}");
    }
}

#[gpui::test]
fn the_guides_are_kept_until_the_text_changes(cx: &mut TestAppContext) {
    let (_, input, cx) = open("x.py", "def a():\n    b = 1\n    c = 2\n", cx);
    let guides = |line: usize, cx: &mut VisualTestContext| {
        input.read_with(cx, |input, _| input.cached_guides(line))
    };
    let (first, active) = guides(1, cx);
    let (again, _) = guides(1, cx);
    assert!(
        Rc::ptr_eq(&first, &again),
        "the same text gives the kept guides"
    );
    assert_eq!(first.len(), 1);
    assert_eq!(*active, vec![0]);
    input.update(cx, |input, cx| {
        input.replace_minimal("def a():\n    b = 1\n", cx)
    });
    cx.run_until_parked();
    let (after, _) = guides(1, cx);
    assert!(!Rc::ptr_eq(&first, &after), "an edit finds them again");
}

#[gpui::test]
fn the_colours_are_asked_for_the_lines_around_the_shown_ones(cx: &mut TestAppContext) {
    let (_, input, cx) = open("x.rs", &lines(1000), cx);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let asked = |cx: &mut VisualTestContext| {
        input.update_in(cx, |input, window, cx| {
            input.layout(window, cx).key.lines.clone()
        })
    };
    assert_eq!(asked(cx), Some(0..40), "the top two blocks");
    let line_height = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), line_height * 450.);
        cx.notify();
    });
    cx.run_until_parked();
    let lines = asked(cx).expect("a window");
    assert!(lines.start <= 450 && lines.end > 460, "{lines:?}");
    // A block before the first shown line, and whole blocks to one past the last shown one.
    assert_eq!(lines.start, 420, "{lines:?}");
    assert!(lines.end % 20 == 0 && lines.end <= 500, "{lines:?}");
}

/// Regression (S4.3c flip, `workspace-editor-long`): the pinned row of an item far above the
/// coloured lines showed in the plain text colour.
#[gpui::test]
fn the_head_line_of_a_pinned_item_far_above_is_coloured(cx: &mut TestAppContext) {
    let body: String = (0..1000).map(|n| format!("  a{n}: {n},\n")).collect();
    let text = format!("// rows\nexport const rows = {{\n{body}}}\n");
    let (_, input, cx) = open("x.ts", &text, cx);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let line_height = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), line_height * 900.);
        cx.notify();
    });
    cx.run_until_parked();
    let (lines, heads) = input.update_in(cx, |input, window, cx| {
        let laid = input.layout(window, cx);
        (laid.key.lines.clone(), laid.key.heads.clone())
    });
    assert!(lines.is_some_and(|lines| lines.start > 1));
    assert_eq!(heads, vec![1]);
    let painted = paint(&input, cx);
    assert_eq!(painted.sticky.len(), 1);
    let runs = input.read_with(cx, |input, cx| {
        let base = gpui::TextRun {
            len: 0,
            font: gpui::font(".ZedMono"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let len = input.model.text().len();
        input
            .code_runs(&base, len, input.query_lines(), &input.sticky_heads(), cx)
            .unwrap()
    });
    // `export` (bytes 8..14 of the text) has a colour of its own.
    let mut at = 0;
    let export = runs.iter().find(|run| {
        let start = at;
        at += run.len;
        start <= 8 && 8 < at
    });
    assert!(export.is_some_and(|run| run.color != Hsla::default()));
}

#[gpui::test]
fn a_view_with_nothing_to_colour_asks_for_no_lines(cx: &mut TestAppContext) {
    let (_, input, cx) = open("notes.txt", &lines(1000), cx);
    let asked = input.update_in(cx, |input, window, cx| {
        input.layout(window, cx).key.lines.clone()
    });
    assert_eq!(asked, None);
}

#[test]
fn the_deeper_pinned_row_takes_the_pointer_where_two_meet() {
    let tops = [px(0.), px(20.), px(30.)];
    let under = |x: f32, y: f32| {
        super::super::code_sticky::sticky_under(
            &tops,
            (px(0.), px(100.)),
            px(20.),
            point(px(x), px(y)),
        )
    };
    assert_eq!(under(50., 10.), Some(0));
    assert_eq!(under(50., 25.), Some(1));
    assert_eq!(under(50., 35.), Some(2), "rows 1 and 2 meet");
    assert_eq!(under(50., 60.), None, "below the rows");
    assert_eq!(under(150., 10.), None, "right of the rows");
}

#[gpui::test]
fn a_press_on_a_pinned_row_goes_to_its_line(cx: &mut TestAppContext) {
    let (_, input, cx) = open("src/catalog_view.ts", &sticky_file(), cx);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let line_height = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    for (place, offset) in [(0, 0.5), (2, 2.2)] {
        input.update(cx, |input, cx| {
            input.scroll = point(px(0.), line_height * 125.5);
            cx.notify();
        });
        cx.run_until_parked();
        let painted = paint(&input, cx);
        assert_eq!(painted.sticky.len(), 3);
        let (row, number) = (painted.sticky[place].row, painted.sticky[place].number);
        let bounds = input.read_with(cx, |input, _| input.last_bounds.unwrap());
        // Shift is held: a pinned row still gives a caret, as in Zed.
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let position = point(
            bounds.right() - px(60.),
            bounds.top() + line_height * offset,
        );
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: shift,
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: shift,
            click_count: 1,
        });
        cx.run_until_parked();
        let (line, empty, scroll) = input.read_with(cx, |input, _| {
            let text = input.model.text();
            let head = input.model.head();
            let line = text[..head].matches('\n').count();
            (
                line,
                input.model.selection().range().is_empty(),
                input.scroll.y,
            )
        });
        assert_eq!(
            line + 1,
            number,
            "row {place}: the caret is on the pinned line"
        );
        assert!(empty, "row {place}: a caret, no selection");
        assert_eq!(
            scroll,
            line_height * (row - place) as f32,
            "row {place}: the line shows where it was pinned"
        );
    }
}

#[gpui::test]
fn a_pinned_row_takes_no_right_press_and_is_tinted_under_the_pointer(cx: &mut TestAppContext) {
    let (view, input, cx) = open("src/catalog_view.ts", &sticky_file(), cx);
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let asked: Rc<Cell<Option<Option<u32>>>> = Rc::default();
    let seen = asked.clone();
    let menu: RowMenu = Rc::new(
        move |row: Option<u32>, menu: ContextMenu, _: &mut Window, _: &mut App| -> ContextMenu {
            seen.set(Some(row));
            menu.entry("Close Annotations", None, |_, _| {})
        },
    );
    cx.update(|_, cx| view.set_menu(menu, cx));
    let line_height = input.update_in(cx, |input, window, cx| input.layout(window, cx).line_height);
    input.update(cx, |input, cx| {
        input.scroll = point(px(0.), line_height * 125.5);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(paint(&input, cx).sticky.len(), 3);
    let bounds = input.read_with(cx, |input, _| input.last_bounds.unwrap());
    let x = bounds.right() - px(60.);
    let at = |rows: f32| point(x, bounds.top() + line_height * rows);

    let hovered = |position, cx: &mut VisualTestContext| {
        cx.simulate_mouse_move(position, None, Modifiers::default());
        input.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        input.read_with(cx, |input, _| input.sticky_hovered.get())
    };
    // Row 2 is pushed up half a row and starts at 1.5, so probe above it.
    assert_eq!(hovered(at(1.25), cx), Some(1), "the row under the pointer");
    assert_eq!(
        hovered(at(1.75), cx),
        Some(2),
        "the deeper row where two meet"
    );
    assert_eq!(hovered(at(4.5), cx), None, "none off the pinned rows");

    let right_press = |position, cx: &mut VisualTestContext| {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Right,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        input.read_with(cx, |input, _| {
            input.code.as_ref().unwrap().open_menu.is_some()
        })
    };
    assert!(!right_press(at(0.5), cx), "a pinned row opens no menu");
    assert_eq!(asked.get(), None, "no menu asked for on a pinned row");
    assert!(right_press(at(4.5), cx), "the text below opens the menu");
    assert!(asked.get().is_some(), "the menu of the clicked row");
}
