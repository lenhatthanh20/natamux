//! Workflow tests T1, T2, T4, T5, the button part of T6, and T8 (plan 7.5), on the text boxes
//! the forms make. They act through keys, `simulate_input` and the clipboard only, so they run
//! the same on the app's own text input later. They record what the boxes do today: a failure
//! lists every case that came out otherwise, with what came out.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::{Editor, EditorEvent};
use gpui::{
    AppContext as _, ClickEvent, Entity, FocusHandle, Focusable, KeyDownEvent, Modifiers,
    Subscription, TestAppContext, VisualTestContext,
};
use ui::prelude::*;

use super::button_style::action_button;
use super::form::{input_box, text_editor};
use crate::app::test_driver::{is_focused, paste, press, text_of, type_text};
use crate::app::test_views;

const MAC: bool = cfg!(target_os = "macos");

/// A form with one box: the keys that reach the form, the box's events as the form saw them,
/// a button beside the box, and a place to move the focus to.
struct Form {
    text_box: Entity<Editor>,
    keys: Vec<String>,
    events: Vec<String>,
    clicks: usize,
    other: FocusHandle,
    _events: Subscription,
}

impl Form {
    fn new(lines: Option<(usize, usize)>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text_box = text_editor("", lines, window, cx);
        let events = cx.subscribe(&text_box, |form, text_box, event: &EditorEvent, cx| {
            // Reading the box here panics if the event came while the box was being changed.
            let text = text_box.read(cx).text(cx);
            match event {
                EditorEvent::BufferEdited => form.events.push(format!("edited {text:?}")),
                EditorEvent::Blurred => form.events.push("blurred".into()),
                _ => {}
            }
        });
        Form {
            text_box,
            keys: Vec::new(),
            events: Vec::new(),
            clicks: 0,
            other: cx.focus_handle(),
            _events: events,
        }
    }
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button = action_button("form-button", "Save")
            .on_click(cx.listener(|form, _: &ClickEvent, _, _| form.clicks += 1));
        v_flex()
            .w(px(400.))
            .on_key_down(cx.listener(|form, event: &KeyDownEvent, _, _| {
                form.keys.push(event.keystroke.unparse());
            }))
            .child(input_box(&self.text_box, cx))
            .child(h_flex().child(div().debug_selector(|| "form-button".into()).child(button)))
            .child(div().track_focus(&self.other).child("other"))
    }
}

/// A form with one box (`lines` as `text_editor` takes them), the box focused.
fn form(
    cx: &mut TestAppContext,
    lines: Option<(usize, usize)>,
) -> (Entity<Form>, Entity<Editor>, &mut VisualTestContext) {
    let (form, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        cx.new(|cx| Form::new(lines, window, cx))
    });
    let text_box = form.read_with(cx, |form, _| form.text_box.clone());
    cx.update(|window, cx| window.focus(&text_box.focus_handle(cx), cx));
    cx.run_until_parked();
    (form, text_box, cx)
}

/// Empties the box and types `lines`, with Enter between them: the caret ends after the text.
fn fill(cx: &mut VisualTestContext, lines: &[&str]) {
    press(cx, "mod-a backspace");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            press(cx, "enter");
        }
        type_text(cx, line);
    }
}

/// Each case starts from `start`, presses its keys, then types `|`: the bar shows where the caret
/// ended, or stands in place of what was selected. Returns the cases that came out otherwise.
fn differing(
    cx: &mut VisualTestContext,
    text_box: &Entity<Editor>,
    start: &[&str],
    cases: &[(String, &str)],
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (keys, want) in cases {
        fill(cx, start);
        if !keys.is_empty() {
            press(cx, keys);
        }
        type_text(cx, "|");
        let got = text_of(text_box, cx);
        if got != *want {
            wrong.push(format!("{keys:?}: got {got:?}, want {want:?}"));
        }
    }
    wrong
}

fn cases(list: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    list.iter()
        .map(|(keys, want)| (keys.to_string(), *want))
        .collect()
}

#[gpui::test]
fn t1_editing_keys_of_a_one_line_box(cx: &mut TestAppContext) {
    let (_, text_box, cx) = form(cx, None);
    let mut list = cases(&[
        ("", "foo_bar  baz.qux|"),
        ("left", "foo_bar  baz.qu|x"),
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
        ("backspace", "foo_bar  baz.qu|"),
        ("home delete", "|oo_bar  baz.qux"),
        ("word-backspace", "foo_bar  baz.|"),
        ("home word-delete", "|  baz.qux"),
        ("shift-left shift-left", "foo_bar  baz.q|"),
        ("home shift-right", "|oo_bar  baz.qux"),
        ("shift-home", "|"),
        ("home shift-end", "|"),
        ("word-shift-left", "foo_bar  baz.|"),
        ("home word-shift-right", "|  baz.qux"),
        ("mod-a", "|"),
        ("up", "foo_bar  baz.qux|"),
        ("down", "foo_bar  baz.qux|"),
        // Today a one-line box takes the text of Enter and Tab; the forms stop Enter first.
        ("enter", "foo_bar  baz.qux\n|"),
        ("tab", "foo_bar  baz.qux\t|"),
        ("pageup", "foo_bar  baz.qux|"),
        // Cut with no selection cuts the whole line (D10).
        ("mod-x", "|"),
    ]);
    if MAC {
        list.extend(cases(&[
            ("cmd-left", "|foo_bar  baz.qux"),
            ("home cmd-right", "foo_bar  baz.qux|"),
            ("cmd-backspace", "|"),
            ("cmd-shift-left", "|"),
            ("home cmd-shift-right", "|"),
        ]));
    }
    let wrong = differing(cx, &text_box, &["foo_bar  baz.qux"], &list);
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t1_editing_keys_of_a_box_of_many_lines(cx: &mut TestAppContext) {
    let (_, text_box, cx) = form(cx, Some((4, 8)));
    let (start, end) = if MAC {
        ("cmd-up", "cmd-down")
    } else {
        ("ctrl-home", "ctrl-end")
    };
    let mut list = cases(&[
        ("", "one two\nthree\nfour five|"),
        ("up", "one two\nthree|\nfour five"),
        ("up up", "one two|\nthree\nfour five"),
        ("home", "one two\nthree\n|four five"),
        ("home up", "one two\n|three\nfour five"),
        ("enter", "one two\nthree\nfour five\n|"),
        ("shift-up", "one two\nthree|"),
        // Page Up keeps the column, on the first line.
        ("pageup", "one two|\nthree\nfour five"),
        ("home backspace", "one two\nthree|four five"),
        ("word-left", "one two\nthree\nfour |five"),
        ("word-backspace", "one two\nthree\nfour |"),
        ("tab", "one two\nthree\nfour five\t|"),
        ("escape", "one two\nthree\nfour five|"),
        ("mod-a", "|"),
    ]);
    list.extend([
        (start.to_string(), "|one two\nthree\nfour five"),
        (format!("{start} {end}"), "one two\nthree\nfour five|"),
        (format!("{start} down"), "one two\n|three\nfour five"),
        (format!("{start} shift-down"), "|three\nfour five"),
        (format!("{start} pagedown"), "one two\nthree\n|four five"),
        (format!("shift-{start}"), "|"),
        (format!("{start} shift-{end}"), "|"),
    ]);
    let wrong = differing(cx, &text_box, &["one two", "three", "four five"], &list);
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t1_undo_and_redo(cx: &mut TestAppContext) {
    let (_, text_box, cx) = form(cx, None);
    type_text(cx, "abc");
    press(cx, "mod-z");
    let undone = text_of(&text_box, cx);
    press(cx, "mod-shift-z");
    let redone = text_of(&text_box, cx);
    assert_eq!((undone.as_str(), redone.as_str()), ("", "abc"));
}

#[gpui::test]
fn t2_a_one_line_box_leaves_the_form_keys_to_the_form(cx: &mut TestAppContext) {
    let (form, text_box, cx) = form(cx, None);
    let mut wrong = Vec::new();
    // Each key reaches the form. Today the box also takes the text of Enter and Tab: a form
    // that keeps Enter for itself stops it (Add a task does).
    let keys = [
        ("enter", "abc\n"),
        ("tab", "abc\t"),
        ("escape", "abc"),
        ("mod-enter", "abc"),
        ("up", "abc"),
        ("down", "abc"),
    ];
    for (keys, want_text) in keys {
        fill(cx, &["abc"]);
        form.update(cx, |form, _| form.keys.clear());
        press(cx, keys);
        let reached = form.read_with(cx, |form, _| form.keys.clone());
        let held = if MAC { "cmd-" } else { "ctrl-" };
        let want = keys.replace("mod-", held);
        let text = text_of(&text_box, cx);
        let focused = is_focused(&text_box, cx);
        if reached != [want.clone()] || text != want_text || !focused {
            wrong.push(format!(
                "{keys}: the form got {reached:?}, the box has {text:?}, focused {focused}"
            ));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// Pastes each text into the empty box; returns the pastes that came out otherwise.
fn pasted(cx: &mut VisualTestContext, text_box: &Entity<Editor>, many: bool) -> Vec<String> {
    let nfc = "Vi\u{1ec7}t";
    let nfd = "Vie\u{323}\u{302}t";
    // (pasted, in one line, in many lines)
    let cases = [
        ("a\r\nb", "a\nb", "a\nb"),
        ("a\rb", "a\nb", "a\nb"),
        ("ab\n", "ab\n", "ab\n"),
        ("a\tb", "a\tb", "a\tb"),
        ("👍🏽 ok", "👍🏽 ok", "👍🏽 ok"),
        (nfc, nfc, nfc),
        (nfd, nfd, nfd),
    ];
    let mut wrong = Vec::new();
    for (text, one, area) in cases {
        let want = if many { area } else { one };
        fill(cx, &[]);
        paste(cx, text);
        let got = text_of(text_box, cx);
        if got != want {
            wrong.push(format!("{text:?}: got {got:?}, want {want:?}"));
        }
    }
    let big = "0123456789abcdef".repeat(6400);
    fill(cx, &[]);
    paste(cx, &big);
    let got = text_of(text_box, cx);
    if got != big {
        wrong.push(format!("100 KB: got {} bytes", got.len()));
    }
    wrong
}

#[gpui::test]
fn t4_paste_into_a_one_line_box(cx: &mut TestAppContext) {
    let (_, text_box, cx) = form(cx, None);
    let wrong = pasted(cx, &text_box, false);
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t4_paste_into_a_box_of_many_lines(cx: &mut TestAppContext) {
    let (_, text_box, cx) = form(cx, Some((4, 8)));
    let wrong = pasted(cx, &text_box, true);
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t5_the_form_gets_the_box_events_as_effects_with_no_timer(cx: &mut TestAppContext) {
    let (form, text_box, cx) = form(cx, None);
    // No clock moves in this test: every event comes from the keys alone.
    type_text(cx, "ab");
    paste(cx, "cd");
    press(cx, "backspace");
    press(cx, "mod-z");
    text_box.update_in(cx, |text_box, window, cx| {
        text_box.set_text("new", window, cx)
    });
    cx.run_until_parked();
    let other = form.read_with(cx, |form, _| form.other.clone());
    cx.update(|window, cx| window.focus(&other, cx));
    cx.run_until_parked();
    let events = form.read_with(cx, |form, _| form.events.clone());
    assert_eq!(
        events,
        [
            "edited \"a\"",
            "edited \"ab\"",
            "edited \"abcd\"",
            "edited \"abc\"",
            // One undo takes back the paste and the Backspace: no clock moved between them.
            "edited \"ab\"",
            "edited \"new\"",
            "blurred",
        ]
    );
}

#[gpui::test]
fn t6_a_click_on_a_button_keeps_the_focus_in_the_box(cx: &mut TestAppContext) {
    let (form, text_box, cx) = form(cx, None);
    let button = cx
        .debug_bounds("form-button")
        .expect("the button was not drawn");
    cx.simulate_click(button.center(), Modifiers::none());
    assert_eq!(form.read_with(cx, |form, _| form.clicks), 1);
    assert!(is_focused(&text_box, cx));
}

/// Every kind of box the forms make, by `text_editor`'s lines: framed by `input_box`, and two
/// without the frame, so the frame's padding shows.
struct Kinds {
    framed: Vec<(&'static str, Entity<Editor>)>,
    bare: Vec<(&'static str, Entity<Editor>)>,
}

const KINDS: [(&str, Option<(usize, usize)>); 8] = [
    ("one-line", None),
    ("area-4-8", Some((4, 8))),
    ("area-6-12", Some((6, 12))),
    ("area-8-20", Some((8, 20))),
    ("area-9-24", Some((9, 24))),
    ("area-3-8", Some((3, 8))),
    ("area-3-3", Some((3, 3))),
    ("area-1-4", Some((1, 4))),
];

impl Render for Kinds {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let framed = self.framed.iter().map(|(name, text_box)| {
            let name = *name;
            div()
                .debug_selector(move || name.into())
                .child(input_box(text_box, cx))
        });
        let bare = self.bare.iter().map(|(name, text_box)| {
            let name = *name;
            div()
                .debug_selector(move || name.into())
                .child(text_box.clone())
        });
        v_flex().w(px(400.)).children(framed).children(bare)
    }
}

#[gpui::test]
fn t8_the_size_of_each_kind_of_box(cx: &mut TestAppContext) {
    let (kinds, _, cx) = test_views::init_app(cx, |_, window, cx| {
        let framed = KINDS.map(|(name, lines)| (name, text_editor("", lines, window, cx)));
        let bare = [("bare-one-line", None), ("bare-area-4-8", Some((4, 8)))]
            .map(|(name, lines)| (name, text_editor("", lines, window, cx)));
        cx.new(|_| Kinds {
            framed: framed.into(),
            bare: bare.into(),
        })
    });
    let names = KINDS
        .iter()
        .map(|(name, _)| *name)
        .chain(["bare-one-line", "bare-area-4-8"]);
    let mut got: Vec<(String, f32, f32)> = names
        .map(|name| {
            let bounds = cx.debug_bounds(name).expect("drawn");
            (
                name.to_string(),
                bounds.size.width.into(),
                bounds.size.height.into(),
            )
        })
        .collect();
    // An area grows with its lines up to its max, then stops.
    let area = kinds.read_with(cx, |kinds, _| kinds.framed[1].1.clone());
    for lines in [6, 20] {
        let text = vec!["line"; lines].join("\n");
        area.update_in(cx, |area, window, cx| area.set_text(text, window, cx));
        cx.run_until_parked();
        let bounds = cx.debug_bounds("area-4-8").expect("drawn");
        let name = format!("area-4-8 with {lines} lines");
        got.push((name, bounds.size.width.into(), bounds.size.height.into()));
    }
    // A line is 23 px; the frame adds 6 px of padding and a 1 px border at the top and bottom.
    let want: Vec<(String, f32, f32)> = [
        ("one-line", 37.),
        ("area-4-8", 106.),
        ("area-6-12", 152.),
        ("area-8-20", 198.),
        ("area-9-24", 221.),
        ("area-3-8", 83.),
        ("area-3-3", 83.),
        ("area-1-4", 37.),
        ("bare-one-line", 23.),
        ("bare-area-4-8", 92.),
        ("area-4-8 with 6 lines", 152.),
        ("area-4-8 with 20 lines", 198.),
    ]
    .iter()
    .map(|(name, height)| (name.to_string(), 400., *height))
    .collect();
    assert_eq!(got, want);
}
