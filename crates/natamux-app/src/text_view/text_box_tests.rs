//! Input tests T-KEY, T-KEY-P, T-CLIP-1, T-UNDO-1, T-EV-1, T-MASK-1 and T-LOOK-1 (plan 7.4, S3.2),
//! run through `TextBox` on Zed's `Editor`. They use keys, `simulate_input`, the clipboard and the
//! public `EntityInputHandler` only, so the own input (S3.3) must pass them unchanged. The key
//! tests run twice: after `text_boxes::bind(cx, true)` and after `bind(cx, false)`, so the macOS
//! keys are tried on Linux too. Where the plan says "record", a test pins what Zed does today.
//! T-WS-1 is `stages/stage_fields_tests.rs` (T12: key writes, the "Done when" placeholder) and
//! T-MIN-1 is `projects/json_tab_tests.rs` (T14: a text from outside keeps the caret, which is
//! what a Reset does) plus the `set_text` test below, and the `replace_minimal` test of the own
//! input (S3.3; the JSON tab and the file editor take it with the code view, S4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;
use std::time::Duration;

use gpui::{
    Bounds, Entity, EntityInputHandler as _, FocusHandle, Focusable, KeyDownEvent, Keystroke,
    Modifiers, Pixels, Subscription, TestAppContext, VisualTestContext,
};
use ui::prelude::*;

use super::text_box::{TextBox, TextEvent};
use crate::app::test_driver::{selection_of, text_of, type_text};
use crate::app::test_views;

pub(super) const ONE: usize = 0;
pub(super) const AREA: usize = 1;
pub(super) const FILL: usize = 2;
pub(super) const PLAIN: usize = 3;

/// One box of each kind, side by side in a column, with what reached their parent.
pub(super) struct Host {
    pub boxes: Vec<TextBox>,
    /// The (key, modifiers) of each key that reached the parent's `on_key_down`.
    pub keys: Vec<(String, Modifiers)>,
    pub events: Vec<(usize, TextEvent)>,
    /// The text of the box as the last handler read it: an event is an effect, so reading works.
    pub seen: String,
    pub other: FocusHandle,
    width: f32,
    _subscriptions: Vec<Subscription>,
}

impl Host {
    fn new(width: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let boxes = vec![
            TextBox::one_line("", window, cx),
            TextBox::area("", (2, 6), window, cx),
            TextBox::fill("", window, cx),
            TextBox::plain(false, window, cx),
        ];
        let subscriptions = boxes
            .iter()
            .enumerate()
            .map(|(index, text_box)| {
                text_box.subscribe(cx, move |host, text_box, event, cx| {
                    host.seen = text_box.text(cx);
                    host.events.push((index, event));
                })
            })
            .collect();
        Host {
            boxes,
            keys: Vec::new(),
            events: Vec::new(),
            seen: String::new(),
            other: cx.focus_handle(),
            width,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .on_key_down(cx.listener(|host, event: &KeyDownEvent, _, _| {
                let stroke = &event.keystroke;
                host.keys.push((stroke.key.clone(), stroke.modifiers));
            }))
            .children(
                self.boxes
                    .iter()
                    .map(|text_box| div().w(px(self.width)).h(px(100.)).child(text_box.clone())),
            )
            .child(div().track_focus(&self.other).child("other"))
    }
}

/// The window of the tests: a `Host` of boxes `width` px wide, with the keys of `text_boxes` bound
/// for macOS (`mac`) or for the others.
pub(super) fn setup(
    cx: &mut TestAppContext,
    mac: bool,
    width: f32,
) -> (Entity<Host>, &mut VisualTestContext) {
    let (host, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        cx.new(|cx| Host::new(width, window, cx))
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, mac));
    (host, cx)
}

pub(super) fn text_box(host: &Entity<Host>, index: usize, cx: &mut VisualTestContext) -> TextBox {
    host.read_with(cx, |host, _| host.boxes[index].clone())
}

pub(super) fn focus(handle: &impl Focusable, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(&handle.focus_handle(cx), cx));
    cx.run_until_parked();
}

/// Moves the focus to a bare focus handle (the host's `other` stand-in).
pub(super) fn focus_handle(handle: &FocusHandle, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(handle, cx));
    cx.run_until_parked();
}

/// Replaces the text with `text` and puts the caret at its end.
pub(super) fn load(text_box: &TextBox, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        text_box.set_text(text, window, cx);
        text_box.move_to_end(window, cx);
    });
    cx.run_until_parked();
}

/// Presses `keys` as the platform of `mac` has them: `mod` is Cmd on macOS and Ctrl elsewhere,
/// `word` is Alt on macOS and Ctrl elsewhere.
pub(super) fn press_as(cx: &mut VisualTestContext, keys: &str, mac: bool) {
    let (held, word) = if mac {
        ("cmd-", "alt-")
    } else {
        ("ctrl-", "ctrl-")
    };
    cx.simulate_keystrokes(&keys.replace("mod-", held).replace("word-", word));
}

/// The (key, modifiers) gpui reads from `keys` of `press_as`.
fn read_keys(keys: &str, mac: bool) -> (String, Modifiers) {
    let held = if mac { "cmd-" } else { "ctrl-" };
    let stroke = Keystroke::parse(&keys.replace("mod-", held)).expect("a key gpui reads");
    (stroke.key, stroke.modifiers)
}

/// Each case loads `start`, presses its keys, then types `|`: the bar shows where the caret
/// ended, or stands in place of what was selected. Returns the cases that came out otherwise.
fn differing(
    cx: &mut VisualTestContext,
    text_box: &TextBox,
    start: &str,
    cases: &[(&str, &str)],
    mac: bool,
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (keys, want) in cases {
        load(text_box, start, cx);
        if !keys.is_empty() {
            press_as(cx, keys, mac);
        }
        type_text(cx, "|");
        let got = text_of(text_box, cx);
        if got != *want {
            wrong.push(format!("mac {mac}, {keys:?}: got {got:?}, want {want:?}"));
        }
    }
    wrong
}

fn assert_none(wrong: Vec<String>) {
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// The line (0-based) of the first `|` in `text`.
fn caret_line(text: &str) -> usize {
    text.split('\n')
        .position(|line| line.contains('|'))
        .unwrap_or_else(|| panic!("no caret bar in {text:?}"))
}

/// Waits past the 300 ms of undo grouping and of the press-and-hold of a drag. The clock of the
/// test executor moves, and the real clock does too, so either one counts.
pub(super) fn pause(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(400));
    std::thread::sleep(Duration::from_millis(400));
    cx.run_until_parked();
}

// T-KEY

#[gpui::test]
fn t_key_word_moves_and_edits_of_a_one_line_box(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        let one = text_box(&host, ONE, cx);
        focus(&one, cx);
        let mut cases = vec![
            ("", "foo_bar  baz.qux|"),
            ("home", "|foo_bar  baz.qux"),
            ("home end", "foo_bar  baz.qux|"),
            ("home right", "f|oo_bar  baz.qux"),
            ("word-left", "foo_bar  baz.|qux"),
            ("word-left word-left", "foo_bar  baz|.qux"),
            ("word-left word-left word-left", "foo_bar  |baz.qux"),
            (
                "word-left word-left word-left word-left",
                "|foo_bar  baz.qux",
            ),
            ("home word-right", "foo_bar|  baz.qux"),
            ("home word-right word-right", "foo_bar  baz|.qux"),
            ("word-backspace", "foo_bar  baz.|"),
            ("word-backspace word-backspace", "foo_bar  baz|"),
            ("home word-delete", "|  baz.qux"),
            ("word-shift-left", "foo_bar  baz.|"),
            ("home word-shift-right", "|  baz.qux"),
            ("shift-home", "|"),
            ("home shift-end", "|"),
            ("shift-left shift-left shift-right", "foo_bar  baz.qu|"),
            // Mod+A then a typed char replaces everything.
            ("mod-a", "|"),
        ];
        if mac {
            cases.extend([
                ("cmd-left", "|foo_bar  baz.qux"),
                ("home cmd-right", "foo_bar  baz.qux|"),
                ("cmd-backspace", "|"),
                ("cmd-shift-left", "|"),
                ("home cmd-shift-right", "|"),
            ]);
        }
        assert_none(differing(cx, &one, "foo_bar  baz.qux", &cases, mac));
    }
}

#[gpui::test]
fn t_key_cmd_backspace_cuts_to_the_start_of_the_line_in_many_lines(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, true, 300.);
    for index in [AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        let cases = [
            ("cmd-backspace", "one two\n|"),
            ("up cmd-backspace", "|wo\nthree"),
        ];
        assert_none(differing(cx, &text_box, "one two\nthree", &cases, true));
    }
}

#[gpui::test]
fn t_key_many_lines_moves_selects_and_pages(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        let (start, end) = if mac {
            ("cmd-up", "cmd-down")
        } else {
            ("ctrl-home", "ctrl-end")
        };
        let to_start_shift = format!("shift-{start}");
        let to_end_shift = format!("{start} shift-{end}");
        let start_down = format!("{start} down");
        let start_end = format!("{start} {end}");
        let cases = [
            ("", "one two\nthree\nfour five|"),
            ("up", "one two\nthree|\nfour five"),
            ("up up", "one two|\nthree\nfour five"),
            ("home", "one two\nthree\n|four five"),
            ("enter", "one two\nthree\nfour five\n|"),
            ("shift-up", "one two\nthree|"),
            ("shift-up shift-down", "one two\nthree\nfour five|"),
            ("mod-a", "|"),
            (start, "|one two\nthree\nfour five"),
            (start_end.as_str(), "one two\nthree\nfour five|"),
            (start_down.as_str(), "one two\n|three\nfour five"),
            (to_start_shift.as_str(), "|"),
            (to_end_shift.as_str(), "|"),
        ];
        for index in [AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            assert_none(differing(
                cx,
                &text_box,
                "one two\nthree\nfour five",
                &cases,
                mac,
            ));
        }
    }
}

#[gpui::test]
fn t_key_page_up_and_down_move_by_a_page(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..60).map(|n| format!("line {n}")).collect();
    let text = lines.join("\n");
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            let top = if mac { "cmd-up" } else { "ctrl-home" };
            load(&text_box, &text, cx);
            press_as(cx, "pageup", mac);
            type_text(cx, "|");
            let up = caret_line(&text_of(&text_box, cx));
            load(&text_box, &text, cx);
            press_as(cx, top, mac);
            press_as(cx, "pagedown", mac);
            type_text(cx, "|");
            let down = caret_line(&text_of(&text_box, cx));
            // Record: the distance of a page is pinned once CI has printed it.
            eprintln!(
                "T-KEY-PAGE mac {mac}, kind {index}: Page Up from 59 to {up}, Page Down from 0 to {down}"
            );
            assert!(
                up < 59,
                "mac {mac}, kind {index}: Page Up went down to line {up}"
            );
            assert!(
                down > 0,
                "mac {mac}, kind {index}: Page Down went to line {down}"
            );
        }
    }
}

#[gpui::test]
fn t_key_home_and_end_stop_at_the_edge_of_a_wrapped_row(cx: &mut TestAppContext) {
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ";
    let text = words.repeat(6);
    for mac in [true, false] {
        // A narrow box, so the one line of text wraps in many rows.
        let (host, cx) = setup(cx, mac, 120.);
        let area = text_box(&host, AREA, cx);
        focus(&area, cx);
        let top = if mac { "cmd-up" } else { "ctrl-home" };
        let bottom = if mac { "cmd-down" } else { "ctrl-end" };
        load(&area, &text, cx);
        press_as(cx, top, mac);
        press_as(cx, "end", mac);
        let first_row_end = selection_of(&area, cx).end;
        assert!(
            first_row_end > 0 && first_row_end < text.len(),
            "mac {mac}: End from the start went to {first_row_end} of {}",
            text.len()
        );
        press_as(cx, bottom, mac);
        press_as(cx, "home", mac);
        let last_row_start = selection_of(&area, cx).start;
        assert!(
            last_row_start > 0 && last_row_start < text.len(),
            "mac {mac}: Home from the end went to {last_row_start}"
        );
        // Mod+Up / Ctrl+Home still go to the real start of the text.
        press_as(cx, top, mac);
        assert_eq!(selection_of(&area, cx), 0..0, "mac {mac}");
        // Shift+End selects to the same row edge.
        press_as(cx, "shift-end", mac);
        assert_eq!(selection_of(&area, cx), 0..first_row_end, "mac {mac}");
    }
}

#[gpui::test]
fn t_key_select_all_then_type_replaces_the_text_in_every_kind(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            load(&text_box, "one two\nthree", cx);
            press_as(cx, "mod-a", mac);
            type_text(cx, "x");
            assert_eq!(text_of(&text_box, cx), "x", "mac {mac}, kind {index}");
        }
    }
}

#[gpui::test]
fn t_key_delete_and_backspace_by_char_in_every_kind(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    for index in [ONE, AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        load(&text_box, "abc", cx);
        press_as(cx, "backspace", false);
        assert_eq!(text_of(&text_box, cx), "ab");
        press_as(cx, "left delete", false);
        assert_eq!(text_of(&text_box, cx), "a");
        press_as(cx, "home shift-right", false);
        assert_eq!(selection_of(&text_box, cx), 0..1);
    }
}

#[gpui::test]
fn t_key_up_and_down_leave_a_selection_from_its_edge(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        let start = if mac { "cmd-up" } else { "ctrl-home" };
        let down_from_start = format!("{start} right shift-down shift-down up");
        let cases = [
            // Up starts from the start of the selection: on the first row it goes to the start.
            (down_from_start.as_str(), "|one two\nthree\nfour five"),
            // Down starts from its end: on the last row it goes to the end.
            ("shift-up shift-up down", "one two\nthree\nfour five|"),
        ];
        for index in [AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            assert_none(differing(
                cx,
                &text_box,
                "one two\nthree\nfour five",
                &cases,
                mac,
            ));
        }
    }
}

#[gpui::test]
fn t_key_tab_with_a_selection_indents_its_lines_and_shift_tab_outdents_them(
    cx: &mut TestAppContext,
) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            load(&text_box, "one\ntwo", cx);
            // From "tw|o" up to "on|e": both lines are touched, and the text stays.
            press_as(cx, "left shift-up tab", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "    one\n    two",
                "mac {mac}, kind {index}: tab"
            );
            assert_eq!(selection_of(&text_box, cx), 6..14, "mac {mac}, kind {index}");
            press_as(cx, "shift-tab", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "one\ntwo",
                "mac {mac}, kind {index}: shift-tab"
            );
            assert_eq!(selection_of(&text_box, cx), 2..6, "mac {mac}, kind {index}");
            press_as(cx, "mod-a tab", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "    one\n    two",
                "mac {mac}, kind {index}: tab over all"
            );
            // The selection start moves past the new spaces, also from the text start.
            assert_eq!(selection_of(&text_box, cx), 4..15, "mac {mac}, kind {index}");
            // Each line goes to the next tab stop of its own indent.
            load(&text_box, "  a\nb", cx);
            press_as(cx, "mod-a tab", mac);
            assert_eq!(text_of(&text_box, cx), "    a\n    b", "mac {mac}, kind {index}");
            // On one line the spaces go to the next tab stop, in front of the selection.
            load(&text_box, "  foo", cx);
            press_as(cx, "left shift-left shift-left tab", mac);
            assert_eq!(text_of(&text_box, cx), "    foo", "mac {mac}, kind {index}");
            assert_eq!(selection_of(&text_box, cx), 4..6, "mac {mac}, kind {index}");
            // Shift+Tab goes back to the tab stop before the indent.
            load(&text_box, "      foo", cx);
            press_as(cx, "shift-tab", mac);
            assert_eq!(text_of(&text_box, cx), "    foo", "mac {mac}, kind {index}");
            assert_eq!(selection_of(&text_box, cx), 7..7, "mac {mac}, kind {index}");
            // And takes a leading tab off.
            load(&text_box, "\tfoo", cx);
            press_as(cx, "shift-tab", mac);
            assert_eq!(text_of(&text_box, cx), "foo", "mac {mac}, kind {index}");
            assert_eq!(selection_of(&text_box, cx), 3..3, "mac {mac}, kind {index}");
        }
    }
}

#[gpui::test]
fn t_key_the_full_box_keys_move_copy_and_delete_lines(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        let cases = [
            ("alt-up", "one two\nfour five|\nthree"),
            ("up alt-down", "one two\nfour five\nthree|"),
            ("alt-shift-down", "one two\nthree\nfour five\nfour five|"),
            ("up mod-shift-k", "one two\nfour |five"),
            // With nothing selected, Select Next takes the word at the caret.
            ("home mod-d", "one two\nthree\n| five"),
            // Plain text has no syntax and no comments.
            ("ctrl-shift-up", "one two\nthree\nfour five|"),
            ("mod-/", "one two\nthree\nfour five|"),
        ];
        for index in [FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            assert_none(differing(
                cx,
                &text_box,
                "one two\nthree\nfour five",
                &cases,
                mac,
            ));
        }
    }
}

// T-KEY-P

#[gpui::test]
fn t_key_p_a_one_line_box_passes_its_keys_to_the_parent(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        let one = text_box(&host, ONE, cx);
        focus(&one, cx);
        for keys in [
            "enter",
            "up",
            "down",
            "tab",
            "shift-tab",
            "escape",
            "mod-enter",
        ] {
            load(&one, "abc", cx);
            host.update(cx, |host, _| host.keys.clear());
            press_as(cx, keys, mac);
            let reached = host.read_with(cx, |host, _| host.keys.clone());
            assert_eq!(reached, [read_keys(keys, mac)], "mac {mac}, {keys}");
            assert!(
                one.is_focused_in(cx),
                "mac {mac}, {keys}: the focus left the box"
            );
        }
    }
}

#[gpui::test]
fn t_key_p_a_box_of_many_lines_passes_escape_and_mod_enter_only(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            for keys in ["escape", "mod-enter"] {
                host.update(cx, |host, _| host.keys.clear());
                press_as(cx, keys, mac);
                let reached = host.read_with(cx, |host, _| host.keys.clone());
                assert_eq!(
                    reached,
                    [read_keys(keys, mac)],
                    "mac {mac}, kind {index}, {keys}"
                );
            }
            // Enter, Up and Down are the box's own keys: the parent does not hear them.
            for keys in ["enter", "up", "down"] {
                host.update(cx, |host, _| host.keys.clear());
                press_as(cx, keys, mac);
                let reached = host.read_with(cx, |host, _| host.keys.clone());
                assert!(
                    reached.is_empty(),
                    "mac {mac}, kind {index}, {keys}: {reached:?}"
                );
            }
        }
    }
}

trait FocusedIn {
    fn is_focused_in(&self, cx: &mut VisualTestContext) -> bool;
}

impl FocusedIn for TextBox {
    fn is_focused_in(&self, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, cx| self.is_focused(window, cx))
    }
}

// T-CLIP-1

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn t_clip_copy_and_cut_with_an_empty_selection_take_the_line(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            // D10: the whole line plus a line end.
            load(&text_box, "abc", cx);
            press_as(cx, "left", mac);
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("old".into()));
            press_as(cx, "mod-c", mac);
            assert_eq!(
                clipboard(cx).as_deref(),
                Some("abc\n"),
                "mac {mac}, copy, kind {index}"
            );
            assert_eq!(text_of(&text_box, cx), "abc");
            press_as(cx, "mod-x", mac);
            // Today a cut of the last line has no line end; a copy has one.
            assert_eq!(
                clipboard(cx).as_deref(),
                Some("abc"),
                "mac {mac}, cut, kind {index}"
            );
            assert_eq!(text_of(&text_box, cx), "", "mac {mac}, cut, kind {index}");
        }
        // In a box of many lines only the line of the caret goes.
        let area = text_box(&host, AREA, cx);
        load(&area, "one\ntwo\nthree", cx);
        press_as(cx, "up mod-x", mac);
        assert_eq!(clipboard(cx).as_deref(), Some("two\n"), "mac {mac}");
        assert_eq!(text_of(&area, cx), "one\nthree", "mac {mac}");
    }
}

#[gpui::test]
fn t_clip_a_copied_line_pastes_above_the_line_of_the_caret(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            // D10: a Copy with nothing selected takes the line; its paste goes in above the line
            // and the caret stays in its text.
            load(&text_box, "abc", cx);
            press_as(cx, "left mod-c mod-v", mac);
            type_text(cx, "|");
            assert_eq!(
                text_of(&text_box, cx),
                "abc\nab|c",
                "mac {mac}, kind {index}: one line"
            );
            if index == ONE {
                continue;
            }
            load(&text_box, "one\ntwo", cx);
            press_as(cx, "left mod-x", mac);
            press_as(cx, "mod-v", mac);
            type_text(cx, "|");
            assert_eq!(
                text_of(&text_box, cx),
                "one\ntwo|",
                "mac {mac}, kind {index}: cut then paste"
            );
            load(&text_box, "one\ntwo", cx);
            press_as(cx, "up mod-c down mod-v", mac);
            type_text(cx, "|");
            assert_eq!(
                text_of(&text_box, cx),
                "one\none\ntwo|",
                "mac {mac}, kind {index}: copy then paste on the next line"
            );
        }
    }
}

#[gpui::test]
fn t_clip_copy_and_cut_of_a_selection_and_paste_over_it(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            load(&text_box, "abcd", cx);
            press_as(cx, "home right shift-right shift-right", mac);
            press_as(cx, "mod-c", mac);
            assert_eq!(
                clipboard(cx).as_deref(),
                Some("bc"),
                "mac {mac}, kind {index}"
            );
            press_as(cx, "mod-x", mac);
            assert_eq!(text_of(&text_box, cx), "ad", "mac {mac}, kind {index}");
            press_as(cx, "mod-v", mac);
            assert_eq!(text_of(&text_box, cx), "abcd", "mac {mac}, kind {index}");
            // Paste replaces the selection.
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("Z".into()));
            press_as(cx, "home right shift-right shift-right mod-v", mac);
            assert_eq!(text_of(&text_box, cx), "aZd", "mac {mac}, kind {index}");
        }
    }
}

#[gpui::test]
fn t_clip_pasted_line_ends_become_line_feeds_in_a_one_line_box_and_an_area(
    cx: &mut TestAppContext,
) {
    let (host, cx) = setup(cx, false, 300.);
    // T4 pins the same cases through the forms; this one pins them through the bare boxes.
    for index in [ONE, AREA] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        for (pasted, want) in [("a\r\nb", "a\nb"), ("a\rb", "a\nb"), ("a\nb\n", "a\nb\n")] {
            load(&text_box, "", cx);
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(pasted.into()));
            press_as(cx, "mod-v", false);
            assert_eq!(text_of(&text_box, cx), want, "kind {index}, {pasted:?}");
        }
    }
}

// T-UNDO-1

#[gpui::test]
fn t_undo_fast_typing_is_one_step_and_a_pause_splits_it(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            load(&text_box, "", cx);
            // Nothing before `abc` can join its step.
            pause(cx);
            type_text(cx, "a");
            type_text(cx, "b");
            type_text(cx, "c");
            press_as(cx, "mod-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "",
                "mac {mac}, kind {index}: fast typing"
            );
            press_as(cx, "mod-shift-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "abc",
                "mac {mac}, kind {index}: redo"
            );

            load(&text_box, "", cx);
            pause(cx);
            type_text(cx, "ab");
            pause(cx);
            type_text(cx, "c");
            press_as(cx, "mod-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "ab",
                "mac {mac}, kind {index}: after a pause"
            );
            press_as(cx, "mod-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "",
                "mac {mac}, kind {index}: the first step"
            );
            press_as(cx, "mod-shift-z mod-shift-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "abc",
                "mac {mac}, kind {index}: redo twice"
            );
        }
    }
}

#[gpui::test]
fn t_undo_set_text_can_be_undone(cx: &mut TestAppContext) {
    for mac in [true, false] {
        let (host, cx) = setup(cx, mac, 300.);
        for index in [ONE, AREA, FILL, PLAIN] {
            let text_box = text_box(&host, index, cx);
            focus(&text_box, cx);
            load(&text_box, "first", cx);
            pause(cx);
            load(&text_box, "second", cx);
            press_as(cx, "mod-z", mac);
            assert_eq!(text_of(&text_box, cx), "first", "mac {mac}, kind {index}");
            press_as(cx, "mod-shift-z", mac);
            assert_eq!(
                text_of(&text_box, cx),
                "second",
                "mac {mac}, kind {index}: redo"
            );
        }
    }
}

// T-EV-1

fn events_of(host: &Entity<Host>, index: usize, cx: &mut VisualTestContext) -> Vec<TextEvent> {
    host.read_with(cx, |host, _| {
        host.events
            .iter()
            .filter(|(from, _)| *from == index)
            .map(|(_, event)| *event)
            .collect()
    })
}

fn forget_events(host: &Entity<Host>, cx: &mut VisualTestContext) {
    host.update(cx, |host, _| host.events.clear());
}

#[gpui::test]
fn t_ev_the_events_of_typing_paste_undo_and_set_text(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    for index in [ONE, AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        load(&text_box, "", cx);
        forget_events(&host, cx);
        let edited = |count: usize| vec![TextEvent::Edited; count];

        type_text(cx, "ab");
        assert_eq!(
            events_of(&host, index, cx),
            edited(2),
            "kind {index}: typing"
        );
        // The handler read the text while the event was an effect, not inside the change.
        assert_eq!(host.read_with(cx, |host, _| host.seen.clone()), "ab");

        forget_events(&host, cx);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("cd".into()));
        press_as(cx, "mod-v", false);
        assert_eq!(
            events_of(&host, index, cx),
            edited(1),
            "kind {index}: paste"
        );

        forget_events(&host, cx);
        press_as(cx, "mod-z", false);
        assert_eq!(events_of(&host, index, cx), edited(1), "kind {index}: undo");

        forget_events(&host, cx);
        load(&text_box, "other", cx);
        assert_eq!(
            events_of(&host, index, cx),
            edited(1),
            "kind {index}: set_text, other text"
        );
        assert_eq!(host.read_with(cx, |host, _| host.seen.clone()), "other");

        // The same text again: record whether it tells, but it may tell at most once.
        forget_events(&host, cx);
        load(&text_box, "other", cx);
        let same = events_of(&host, index, cx).len();
        eprintln!("T-EV-1 kind {index}: set_text with the same text gave {same} Edited");
        assert!(
            same <= 1,
            "kind {index}: set_text with the same text gave {same}"
        );

        forget_events(&host, cx);
        cx.update(|window, cx| text_box.insert_at_caret("!", window, cx));
        assert_eq!(
            events_of(&host, index, cx),
            edited(1),
            "kind {index}: insert_at_caret"
        );
        forget_events(&host, cx);
        cx.update(|window, cx| text_box.clear(window, cx));
        assert_eq!(
            events_of(&host, index, cx),
            edited(1),
            "kind {index}: clear"
        );
    }
}

#[gpui::test]
fn t_ev_blurred_comes_when_the_focus_leaves(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    for index in [ONE, AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        forget_events(&host, cx);
        let other = host.read_with(cx, |host, _| host.other.clone());
        focus_handle(&other, cx);
        assert_eq!(
            events_of(&host, index, cx),
            [TextEvent::Blurred],
            "kind {index}"
        );
        // Other boxes heard nothing.
        let all = host.read_with(cx, |host, _| host.events.len());
        assert_eq!(all, 1, "kind {index}");
    }
}

// T-MASK-1

#[gpui::test]
fn t_mask_the_real_text_stays_and_toggling_keeps_the_caret(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    let one = text_box(&host, ONE, cx);
    focus(&one, cx);
    cx.update(|_, cx| one.set_masked(true, cx));
    type_text(cx, "abc");
    assert_eq!(text_of(&one, cx), "abc");
    press_as(cx, "left", false);
    assert_eq!(selection_of(&one, cx), 2..2);
    cx.update(|_, cx| one.set_masked(false, cx));
    assert_eq!(selection_of(&one, cx), 2..2);
    cx.update(|_, cx| one.set_masked(true, cx));
    assert_eq!(selection_of(&one, cx), 2..2);
    // Typing into the masked box inserts at the caret.
    type_text(cx, "X");
    assert_eq!(text_of(&one, cx), "abXc");
    // Copy of a masked box must not matter to the text: select all keeps the real text.
    press_as(cx, "mod-a", false);
    assert_eq!(selection_of(&one, cx), 0..4);
    assert_eq!(text_of(&one, cx), "abXc");
}

// T-LOOK-1

/// The box of `text_box` on screen, and the window rect of the chars `range` (UTF-16 units), as
/// the input method asks for them.
fn char_rect(
    text_box: &TextBox,
    range: Range<usize>,
    cx: &mut VisualTestContext,
) -> Bounds<Pixels> {
    text_box.editor().update_in(cx, |editor, window, cx| {
        let area = *editor.last_bounds().expect("the box was drawn");
        editor
            .bounds_for_range(range, area, window, cx)
            .expect("the chars are shown")
    })
}

fn close(a: Pixels, b: Pixels, within: f32) -> bool {
    (f32::from(a) - f32::from(b)).abs() <= within
}

#[gpui::test]
fn t_look_the_line_box_the_caret_and_the_selection_of_a_one_line_box(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    let one = text_box(&host, ONE, cx);
    focus(&one, cx);
    type_text(cx, "abc");
    let a = char_rect(&one, 0..1, cx);
    let b = char_rect(&one, 1..2, cx);
    let c = char_rect(&one, 2..3, cx);
    let bc = char_rect(&one, 1..3, cx);
    eprintln!("T-LOOK-1 one-line: a {a:?}, b {b:?}, c {c:?}, bc {bc:?}");
    // Zed gives the start of the range with the size of one em and one line: the end of the range
    // does not count, every rect has the same size and y, and x grows with the index.
    assert!(f32::from(a.size.height) > 0. && f32::from(a.size.width) > 0.);
    for rect in [b, c, bc] {
        assert_eq!(rect.size, a.size);
        assert!(close(rect.origin.y, a.origin.y, 0.5));
    }
    assert!(a.origin.x < b.origin.x && b.origin.x < c.origin.x);
    assert!(close(bc.origin.x, b.origin.x, 0.5));
}

#[gpui::test]
fn t_look_a_box_of_many_lines_wraps_and_steps_by_the_line_height(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    for index in [AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        load(&text_box, "row 0\nrow 1\nrow 2\nrow 3", cx);
        // UTF-16 starts of the four rows: 0, 6, 12, 18.
        let rows: Vec<Bounds<Pixels>> = [0, 6, 12, 18]
            .into_iter()
            .map(|at| char_rect(&text_box, at..at + 1, cx))
            .collect();
        let step = rows[1].origin.y - rows[0].origin.y;
        eprintln!("T-LOOK-1 kind {index}: rows {rows:?}");
        assert!(f32::from(step) > 0.);
        for pair in rows.windows(2) {
            assert!(
                close(pair[1].origin.y - pair[0].origin.y, step, 0.5),
                "kind {index}"
            );
            assert!(
                close(pair[1].origin.x, pair[0].origin.x, 0.5),
                "kind {index}"
            );
        }
        // The row height is the line height: a row's box is as high as the step down.
        assert!(close(rows[0].size.height, step, 0.5), "kind {index}");
    }
}

#[gpui::test]
fn t_look_the_wrap_row_count_at_a_fixed_width(cx: &mut TestAppContext) {
    let text = "wordy ".repeat(12);
    let (host, cx) = setup(cx, false, 200.);
    let area = text_box(&host, AREA, cx);
    focus(&area, cx);
    load(&area, &text, cx);
    press_as(cx, "ctrl-home", false);
    let first = char_rect(&area, 0..1, cx);
    let last = char_rect(&area, text.len() - 2..text.len() - 1, cx);
    let below = f32::from(last.origin.y - first.origin.y) / f32::from(first.size.height);
    eprintln!(
        "T-LOOK-1 wrap: {} chars in 200 px, last row is {below} below the first",
        text.len()
    );
    // 72 chars do not fit one row of 200 px: the text wrapped.
    assert!(
        below >= 1.,
        "the text did not wrap: {below} rows below the first"
    );
}

#[gpui::test]
fn t_min_set_text_puts_the_caret_where_zed_does(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..60).map(|n| format!("line {n}")).collect();
    let text = lines.join("\n");
    let (host, cx) = setup(cx, false, 300.);
    let area = text_box(&host, AREA, cx);
    focus(&area, cx);
    load(&area, &text, cx);
    press_as(cx, "ctrl-home down down down", false);
    let before = selection_of(&area, cx);
    cx.update(|window, cx| area.set_text(text.as_str(), window, cx));
    cx.run_until_parked();
    let after = selection_of(&area, cx);
    // Record: `set_text` is not a minimal replace; S3.3's `replace_minimal` must keep the caret.
    eprintln!("T-MIN-1 set_text of the same text: caret {before:?} before, {after:?} after");
    assert_eq!(text_of(&area, cx), text);
    assert_eq!(before, 21..21);
    // Known gap: the caret after `set_text` and the scroll row are not pinned; CI never printed
    // them. The caret must stay inside the text; S3.3 adds the exact test on `replace_minimal`.
    assert!(after.start == after.end && after.end <= text.len());
}

#[cfg(feature = "own-input")]
#[gpui::test]
fn t_min_replace_minimal_keeps_the_caret_the_scroll_and_the_undo(cx: &mut TestAppContext) {
    let lines: Vec<String> = (0..60).map(|n| format!("line {n}")).collect();
    let text = lines.join("\n");
    let (host, cx) = setup(cx, false, 300.);
    for index in [AREA, FILL, PLAIN] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        load(&text_box, &text, cx);
        pause(cx);
        // The caret on line 40, scrolled into view.
        press_as(cx, "ctrl-home", false);
        for _ in 0..40 {
            press_as(cx, "down", false);
        }
        // A key the box leaves alone: the window draws first, so the scroll follows the caret.
        press_as(cx, "escape", false);
        let before = selection_of(&text_box, cx);
        let row = char_rect(&text_box, before.start..before.start + 1, cx).origin.y;
        // A change above the caret, one in its line after it, and one at the end.
        let changed = text
            .replacen("line 3\n", "line three\n", 1)
            .replacen("line 40", "line 40!", 1)
            + "\nline 60";
        cx.update(|window, cx| text_box.replace_minimal(&changed, window, cx));
        cx.run_until_parked();
        assert_eq!(text_of(&text_box, cx), changed, "kind {index}");
        let grown = "line three".len() - "line 3".len();
        assert_eq!(
            selection_of(&text_box, cx),
            before.start + grown..before.end + grown,
            "kind {index}: the caret moved with the text above it"
        );
        let after = char_rect(&text_box, before.start + grown..before.start + grown + 1, cx);
        assert!(
            close(after.origin.y, row, 0.5),
            "kind {index}: the caret row moved on screen"
        );
        press_as(cx, "ctrl-z", false);
        assert_eq!(text_of(&text_box, cx), text, "kind {index}: one undo step");
    }
}
