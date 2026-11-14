//! The own terminal's model on its own, with no view: typing, paste, copy, clear, links, search
//! and the copy of the screen a sync makes. The matrix rows of 7.3 that the model answers alone
//! (T-E1, T-Q1, T-R1, T-F1) are in `term_behaviour_tests.rs`, next to the old ones.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::{Cell as Count, RefCell};
use std::rc::Rc;
use std::time::Duration;

use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui::{AppContext as _, Bounds, Entity, Pixels, TestAppContext, point, px, size};

use super::{Search, TermBounds, TermEvent, TermModel};
use crate::terminals::term::writer::TermWriter;

/// The cell of the test font (7.8 x 16.9 px), as the old tests measure it.
const CELL: (f32, f32) = (7.8, 16.9);

/// A grid of `columns` x `lines` cells, with half a cell to spare each way.
pub(crate) fn bounds(columns: usize, lines: usize) -> TermBounds {
    let extent = size(
        px((columns as f32 + 0.5) * CELL.0),
        px((lines as f32 + 0.5) * CELL.1),
    );
    TermBounds::new(
        px(CELL.1),
        px(CELL.0),
        Bounds::new(point(px(0.), px(0.)), extent),
    )
}

/// A model, the parser's writer, and what reached the shell.
pub(crate) struct Screen {
    pub(crate) model: Entity<TermModel>,
    pub(crate) writer: TermWriter,
    input: UnboundedReceiver<Vec<u8>>,
}

impl Screen {
    /// A writer for the screen shown now, as the parser gets one after a replace.
    pub(crate) fn renew_writer(&mut self, cx: &mut TestAppContext) {
        self.writer = self
            .model
            .read_with(cx, |model, _| model.display_only_writer().unwrap());
    }
}

/// A model of `columns` x `lines` under the test theme, with 1,000 lines of history.
pub(crate) fn screen(cx: &mut TestAppContext, (columns, lines): (usize, usize)) -> Screen {
    crate::app::test_views::init(cx);
    let grid = bounds(columns, lines);
    let model = cx.new(|cx| TermModel::new(Some(1000), Some(grid), cx));
    let (sink, input) = unbounded();
    let writer = model.update(cx, |model, _| {
        model.set_display_only_input(sink).unwrap();
        model.display_only_writer().unwrap()
    });
    Screen {
        model,
        writer,
        input,
    }
}

/// Runs the event loop past its pace, then syncs as a frame would.
pub(crate) fn settle(model: &Entity<TermModel>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    model.update(cx, |model, cx| model.sync(cx));
}

/// Output of the program, through the parser's writer.
pub(crate) fn feed(screen: &mut Screen, bytes: &str, cx: &mut TestAppContext) {
    screen.writer.write(bytes.as_bytes());
    settle(&screen.model, cx);
}

/// What reached the shell since the last look, joined.
pub(crate) fn sent(screen: &mut Screen) -> String {
    let mut text = String::new();
    while let Ok(bytes) = screen.input.try_recv() {
        text.push_str(&String::from_utf8_lossy(&bytes));
    }
    text
}

fn change(screen: &Screen, cx: &mut TestAppContext, f: impl FnOnce(&mut TermModel)) {
    screen.model.update(cx, |model, _| f(model));
    settle(&screen.model, cx);
}

fn lines(count: usize) -> String {
    (1..=count).map(|line| format!("line {line}\r\n")).collect()
}

fn at(line: i32, column: usize) -> Point {
    Point::new(Line(line), Column(column))
}

/// A window position on shown row `line`, `column` cells from the left (2.5 is the middle of
/// the third cell). The tests' bounds start at 0, 0.
fn pixel(line: usize, column: f32) -> gpui::Point<Pixels> {
    point(px(column * CELL.0), px((line as f32 + 0.5) * CELL.1))
}

fn hovered(screen: &Screen, cx: &mut TestAppContext) -> Option<(String, Point, Point)> {
    screen.model.read_with(cx, |model, _| {
        let word = model.last_content().last_hovered_word.clone()?;
        Some((word.word, *word.range.start(), *word.range.end()))
    })
}

#[gpui::test]
fn typing_scrolls_to_the_bottom_and_drops_the_selection(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    feed(&mut screen, &lines(60), cx);
    let read = |screen: &Screen, cx: &mut TestAppContext| {
        screen.model.read_with(cx, |model, _| {
            let content = model.last_content();
            (
                content.display_offset,
                content.selection_text.clone(),
                content.scrolled_to_top,
                content.scrolled_to_bottom,
            )
        })
    };
    change(&screen, cx, |model| {
        model.scroll_up_by(5);
        model.select_all();
    });
    let (offset, selection, top, bottom) = read(&screen, cx);
    assert_eq!(offset, 5);
    assert!(selection.unwrap().starts_with("line 1\n"));
    assert!(!top && !bottom);
    change(&screen, cx, TermModel::scroll_to_top);
    let history = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        assert!(content.scrolled_to_top);
        content.total_lines - content.screen_lines
    });
    assert_eq!(history, 51);
    change(&screen, cx, |model| model.scroll_down_by(2));
    assert_eq!(read(&screen, cx).0, 49);
    change(&screen, cx, |model| model.type_bytes("x"));
    let (offset, selection, _, bottom) = read(&screen, cx);
    assert_eq!((offset, selection, bottom), (0, None, true));
    assert_eq!(sent(&mut screen), "x");
    change(&screen, cx, |model| model.scroll_up_by(3));
    change(&screen, cx, TermModel::scroll_to_bottom);
    assert_eq!(read(&screen, cx).0, 0);
}

#[gpui::test]
fn pastes_with_the_marks_only_when_the_program_asks(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    change(&screen, cx, |model| model.paste("a\r\nb\nc\x1b"));
    assert_eq!(sent(&mut screen), "a\rb\rc\x1b");
    feed(&mut screen, "\x1b[?2004h", cx);
    change(&screen, cx, |model| model.paste("x\x1by\n"));
    assert_eq!(sent(&mut screen), "\x1b[200~xy\n\x1b[201~");
}

#[gpui::test]
fn copy_keeps_the_selection(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    feed(&mut screen, "hello world", cx);
    change(&screen, cx, |model| {
        let start = Selection::new(SelectionType::Simple, at(0, 0), Side::Left);
        model.set_selection(Some(start));
        model.update_selection(pixel(0, 4.75));
        model.copy();
    });
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("hello"));
    let (selection, text) = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (content.selection, content.selection_text.clone())
    });
    assert_eq!(text.as_deref(), Some("hello"));
    assert_eq!(selection.unwrap().end, at(0, 4));
}

#[gpui::test]
fn a_selection_update_after_a_queued_scroll_takes_the_new_lines(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    feed(&mut screen, &lines(60), cx);
    change(&screen, cx, |model| {
        let start = Selection::new(SelectionType::Simple, at(-8, 0), Side::Left);
        model.set_selection(Some(start));
        // A drag below the pane: a scroll, then the end of the selection, in one mouse move.
        model.scroll_up_by(5);
        model.update_selection(pixel(2, 3.75));
    });
    let (offset, selection) = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (content.display_offset, content.selection.unwrap())
    });
    assert_eq!(offset, 5);
    assert_eq!((selection.start, selection.end), (at(-8, 0), at(-3, 3)));
}

// The old row is `clear_keeps_the_line_of_the_cursor` in term_behaviour_tests.rs.
#[gpui::test]
fn clear_keeps_only_the_line_of_the_cursor(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    feed(&mut screen, &format!("{}prompt$ ab", lines(60)), cx);
    change(&screen, cx, TermModel::clear);
    let (text, history, cursor) = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (
            model.get_content(),
            content.total_lines - content.screen_lines,
            content.cursor.point,
        )
    });
    assert_eq!(text.trim_end(), "prompt$ ab");
    assert_eq!((history, cursor), (0, at(0, 10)));
    assert_eq!(sent(&mut screen), "", "the shell was told");
}

#[gpui::test]
fn a_link_comes_from_its_osc8_cells_or_from_its_line(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (20, 10));
    feed(
        &mut screen,
        "\x1b]8;;https://x.y/\x1b\\ab\x1b]8;;\x1b\\ cd\r\ngo https://example.com/a/b/c/d end",
        cx,
    );
    let link = |screen: &Screen, point: Point, cx: &mut TestAppContext| {
        screen.model.read_with(cx, |model, _| model.link_at(point))
    };
    let osc = link(&screen, at(0, 1), cx).unwrap();
    assert_eq!(osc.word, "https://x.y/");
    assert_eq!(osc.range, at(0, 0)..=at(0, 1));
    assert_eq!(link(&screen, at(0, 4), cx), None);
    // The URL wraps onto the next row; either row finds all of it.
    let wrapped = link(&screen, at(2, 3), cx).unwrap();
    assert_eq!(wrapped.word, "https://example.com/a/b/c/d");
    assert_eq!(wrapped.range, at(1, 3)..=at(2, 9));
    assert_eq!(link(&screen, at(1, 5), cx), Some(wrapped));

    change(&screen, cx, |model| model.find_link(pixel(0, 0.5)));
    let hovered = screen.model.read_with(cx, |model, _| {
        model.last_content().last_hovered_word.clone()
    });
    assert_eq!(hovered, Some(osc));
    change(&screen, cx, |model| model.open_link(pixel(0, 1.5)));
    assert_eq!(cx.opened_url().as_deref(), Some("https://x.y/"));
    change(&screen, cx, TermModel::clear_hovered_word);
    let hovered = screen.model.read_with(cx, |model, _| {
        model.last_content().last_hovered_word.clone()
    });
    assert_eq!(hovered, None);
}

#[gpui::test]
fn a_scroll_looks_for_the_link_under_the_pointer_again(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 5));
    feed(&mut screen, &lines(10), cx);
    feed(&mut screen, "see https://example.com/z", cx);
    let wakes = Rc::new(Count::new(0));
    let counted = wakes.clone();
    cx.update(|cx| {
        cx.subscribe(&screen.model, move |_, event: &TermEvent, _| {
            if *event == TermEvent::Wakeup {
                counted.set(counted.get() + 1);
            }
        })
        .detach();
    });
    let hovered = |screen: &Screen, cx: &mut TestAppContext| {
        screen.model.read_with(cx, |model, _| {
            let content = model.last_content();
            content
                .last_hovered_word
                .as_ref()
                .map(|word| word.word.clone())
        })
    };
    change(&screen, cx, |model| model.find_link(pixel(4, 6.5)));
    assert_eq!(
        hovered(&screen, cx).as_deref(),
        Some("https://example.com/z")
    );
    wakes.set(0);
    change(&screen, cx, |model| model.scroll_up_by(1));
    assert_eq!(wakes.get(), 1, "no wake to look again");
    settle(&screen.model, cx);
    assert_eq!(hovered(&screen, cx), None, "the old link stayed");
}

#[gpui::test]
fn output_or_a_resize_looks_for_the_link_under_the_pointer_again(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 5));
    feed(&mut screen, &lines(10), cx);
    feed(&mut screen, "see https://example.com/z", cx);
    let url = "https://example.com/z".to_owned();
    change(&screen, cx, |model| model.find_link(pixel(4, 6.5)));
    assert_eq!(
        hovered(&screen, cx),
        Some((url.clone(), at(4, 4), at(4, 24)))
    );
    // At the bottom, new output moves the lines up under the pointer.
    feed(&mut screen, "\r\nmore", cx);
    settle(&screen.model, cx);
    assert_eq!(hovered(&screen, cx), None, "output kept the old link");

    feed(&mut screen, "\r\nsee https://example.com/z", cx);
    change(&screen, cx, |model| model.find_link(pixel(4, 6.5)));
    assert!(hovered(&screen, cx).is_some());
    // At 20 columns the line wraps: the pointer is past its second row.
    change(&screen, cx, |model| model.set_size(bounds(20, 5)));
    settle(&screen.model, cx);
    assert_eq!(hovered(&screen, cx), None, "a resize kept the old link");
}

#[gpui::test]
fn output_while_scrolled_up_moves_the_hovered_link_with_its_line(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 5));
    feed(
        &mut screen,
        &format!("see https://example.com/z\r\n{}", lines(10)),
        cx,
    );
    change(&screen, cx, |model| model.scroll_up_by(7));
    change(&screen, cx, |model| model.find_link(pixel(0, 6.5)));
    let url = "https://example.com/z".to_owned();
    assert_eq!(
        hovered(&screen, cx),
        Some((url.clone(), at(-7, 4), at(-7, 24)))
    );
    // The view keeps its lines; the link goes one line further into the history.
    feed(&mut screen, "x\r\n", cx);
    let offset = screen
        .model
        .read_with(cx, |model, _| model.last_content().display_offset);
    assert_eq!(offset, 8);
    assert_eq!(hovered(&screen, cx), Some((url, at(-8, 4), at(-8, 24))));
}

#[gpui::test]
fn finds_in_the_history_and_selects_a_match(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 5));
    feed(&mut screen, &lines(40), cx);
    assert!(Search::new("(").is_none());
    let search = Search::new("line 1[0-9]").unwrap();
    let task = screen
        .model
        .read_with(cx, |model, cx| model.find_matches(search, cx));
    let found = Rc::new(RefCell::new(None));
    let out = found.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| *out.borrow_mut() = Some(task.await))
            .detach()
    });
    cx.run_until_parked();
    let matches = found.borrow_mut().take().unwrap();
    assert_eq!(matches.len(), 10);
    change(&screen, cx, |model| {
        model.matches = matches;
        model.activate_match(2);
    });
    let (text, offset) = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (content.selection_text.clone(), content.display_offset)
    });
    assert_eq!(text.as_deref(), Some("line 12"));
    assert!(offset > 0, "the match was not scrolled into view");
}

#[gpui::test]
fn a_sync_copies_the_screen_and_the_program_state(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 5));
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    cx.update(|cx| {
        cx.subscribe(&screen.model, move |_, event: &TermEvent, _| {
            seen.borrow_mut().push(*event)
        })
        .detach();
    });
    feed(&mut screen, "\x1b]0;my title\x07\x07ab\x1b[1 q", cx);
    screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        let first = content.cells.first().unwrap();
        assert_eq!((first.point, first.cell.c), (at(0, 0), 'a'));
        assert_eq!(content.cells.len(), 40 * 5);
        assert_eq!((content.columns, content.screen_lines), (40, 5));
        assert_eq!(content.cursor_char, ' ');
        assert!(!content.bottom_row_used);
        assert_eq!(model.title(), Some("my title"));
        assert!(model.bell());
        assert!(model.cursor_blinks());
        assert_eq!(model.history(), 1000);
        assert_eq!(content.terminal_bounds, bounds(40, 5));
    });
    assert!(events.borrow().contains(&TermEvent::BlinkChanged));
    change(&screen, cx, TermModel::clear_bell);
    assert!(!screen.model.read_with(cx, |model, _| model.bell()));

    feed(&mut screen, "\r\n\r\n\r\n\r\nlast", cx);
    assert!(
        screen
            .model
            .read_with(cx, |model, _| model.last_content().bottom_row_used)
    );
    screen
        .model
        .update(cx, |model, cx| model.write_output(b" more", cx));
    settle(&screen.model, cx);
    assert!(
        screen
            .model
            .read_with(cx, |model, _| model.get_content().contains("last more"))
    );

    events.borrow_mut().clear();
    screen
        .model
        .update(cx, |model, cx| model.replace_display_output(b"fresh", cx))
        .unwrap();
    settle(&screen.model, cx);
    let text = screen.model.read_with(cx, |model, _| model.get_content());
    assert!(text.contains("fresh") && !text.contains("last"), "{text}");
    for event in [
        TermEvent::OutputReplaced,
        TermEvent::BlinkChanged,
        TermEvent::Wakeup,
    ] {
        assert!(events.borrow().contains(&event), "no {event:?}");
    }
    assert!(
        screen
            .model
            .read_with(cx, |model, _| model.title().is_none())
    );
    // The old writer writes to the old screen; a new one to the new screen.
    feed(&mut screen, " old", cx);
    screen.renew_writer(cx);
    feed(&mut screen, " new", cx);
    let text = screen.model.read_with(cx, |model, _| model.get_content());
    assert!(
        text.contains("fresh new") && !text.contains("old"),
        "{text}"
    );
}

#[gpui::test]
fn a_size_asked_for_before_a_replace_is_the_new_screens_size(cx: &mut TestAppContext) {
    let screen = screen(cx, (79, 20));
    screen.model.update(cx, |model, cx| {
        model.set_size(bounds(48, 11));
        model.replace_display_output(b"fresh", cx).unwrap();
    });
    settle(&screen.model, cx);
    let grid = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (
            content.columns,
            content.screen_lines,
            content.terminal_bounds,
        )
    });
    assert_eq!(grid, (48, 11, bounds(48, 11)));
}

#[gpui::test]
fn a_moved_pane_keeps_its_grid_and_takes_the_new_origin(cx: &mut TestAppContext) {
    let mut screen = screen(cx, (40, 10));
    let mut moved = bounds(40, 10);
    moved.bounds.origin = point(px(120.), px(30.));
    change(&screen, cx, |model| model.set_size(moved));
    let (grid, stored) = screen.model.read_with(cx, |model, _| {
        let content = model.last_content();
        (
            (content.columns, content.screen_lines),
            content.terminal_bounds,
        )
    });
    assert_eq!((grid, stored), ((40, 10), moved));
    assert_eq!(sent(&mut screen), "");
    // Points are taken from the new origin.
    feed(&mut screen, "see https://example.com/z", cx);
    change(&screen, cx, |model| {
        model.find_link(point(px(120. + 6.5 * CELL.0), px(30. + 0.5 * CELL.1)))
    });
    assert!(hovered(&screen, cx).is_some(), "the old origin was used");
}

#[gpui::test]
fn write_output_turns_a_bare_lf_into_cr_lf(cx: &mut TestAppContext) {
    let screen = screen(cx, (40, 10));
    screen.model.update(cx, |model, cx| {
        model.write_output(b"built\nin 2.1 s\r", cx);
        model.write_output(b"\ndone\r\n", cx);
    });
    settle(&screen.model, cx);
    let text = screen.model.read_with(cx, |model, _| model.get_content());
    let rows: Vec<&str> = text.lines().take(3).map(str::trim_end).collect();
    assert_eq!(rows, ["built", "in 2.1 s", "done"]);
}

#[test]
fn a_point_past_the_grid_takes_the_whole_cell() {
    // 10 columns and 5 lines of 8 x 10 px.
    let grid = TermBounds::new(
        px(10.),
        px(8.),
        Bounds::new(point(px(0.), px(0.)), size(px(80.), px(50.))),
    );
    let at = |x: f32, y: f32| grid.shown_cell(point(px(x), px(y)), 10, 5);
    assert_eq!(at(5., 1.), (0, Column(0), Side::Right));
    assert_eq!(at(3., 1.), (0, Column(0), Side::Left));
    // Right of the last column and below the last row: the right side.
    assert_eq!(at(81., 1.), (0, Column(9), Side::Right));
    assert_eq!(at(1., 55.), (4, Column(0), Side::Right));
    // Above the top: the line above, left side.
    assert_eq!(at(7., -12.), (-1, Column(0), Side::Left));
}
