//! Input method tests T-IME-1..10 and the table over the 100 words of `assets/ime-words.txt`
//! (plan 7.4, S3.2). They call the `EntityInputHandler` methods on the box's editor, as the
//! platform does for Telex, VNI, EVKey and OpenKey, so the own input must pass them unchanged.
//! Where the plan says "record", a test pins what Zed does today and prints it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use gpui::{EntityInputHandler as _, TestAppContext, VisualTestContext};

use super::text_box::TextBox;
use super::text_box_tests::{
    AREA, FILL, ONE, focus, focus_handle, load, press_as, setup, text_box,
};
use crate::app::test_driver::{selection_of, text_of, type_text};
use crate::ime::word_check::words;

/// `replace_and_mark_text_in_range`: the input method shows `text` as the word it is composing.
fn mark(text_box: &TextBox, range: Option<Range<usize>>, text: &str, cx: &mut VisualTestContext) {
    text_box.editor().update_in(cx, |editor, window, cx| {
        editor.replace_and_mark_text_in_range(range, text, None, window, cx);
    });
    cx.run_until_parked();
}

/// `replace_text_in_range`: the input method commits `text`.
fn commit(text_box: &TextBox, range: Option<Range<usize>>, text: &str, cx: &mut VisualTestContext) {
    text_box.editor().update_in(cx, |editor, window, cx| {
        editor.replace_text_in_range(range, text, window, cx);
    });
    cx.run_until_parked();
}

fn marked(text_box: &TextBox, cx: &mut VisualTestContext) -> Option<Range<usize>> {
    text_box.editor().update_in(cx, |editor, window, cx| {
        editor.marked_text_range(window, cx)
    })
}

/// The steps a Telex user sees while typing one syllable: each char shows in its base form, then
/// in its final form, so `việt` goes `v vi vie viê việ việt`.
fn steps(syllable: &str) -> Vec<String> {
    // Six forms of each vowel (none, sắc, huyền, hỏi, ngã, nặng), and the base letter of each.
    const FAMILIES: [(&str, char); 12] = [
        ("aáàảãạ", 'a'),
        ("ăắằẳẵặ", 'a'),
        ("âấầẩẫậ", 'a'),
        ("eéèẻẽẹ", 'e'),
        ("êếềểễệ", 'e'),
        ("iíìỉĩị", 'i'),
        ("oóòỏõọ", 'o'),
        ("ôốồổỗộ", 'o'),
        ("ơớờởỡợ", 'o'),
        ("uúùủũụ", 'u'),
        ("ưứừửữự", 'u'),
        ("yýỳỷỹỵ", 'y'),
    ];
    // The base letter, and the shape without a tone (`ê` for `ệ`), of a Vietnamese char.
    let parts = |c: char| -> (char, char) {
        let lower = c.to_lowercase().next().unwrap_or(c);
        let (base, shape) = if lower == 'đ' {
            ('d', 'đ')
        } else {
            FAMILIES
                .iter()
                .find(|(forms, _)| forms.contains(lower))
                .map_or((lower, lower), |(forms, base)| {
                    (*base, forms.chars().next().unwrap_or(lower))
                })
        };
        if c.is_uppercase() {
            (
                base.to_ascii_uppercase(),
                shape.to_uppercase().next().unwrap_or(shape),
            )
        } else {
            (base, shape)
        }
    };
    let mut out = Vec::new();
    let mut shown = String::new();
    for c in syllable.chars() {
        let (base, shape) = parts(c);
        if base != c {
            out.push(format!("{shown}{base}"));
            if shape != c && shape != base {
                out.push(format!("{shown}{shape}"));
            }
        }
        shown.push(c);
        out.push(shown.clone());
    }
    out
}

/// Types one syllable the way a composing input method does, then commits it.
fn compose(text_box: &TextBox, syllable: &str, cx: &mut VisualTestContext) {
    for step in steps(syllable) {
        mark(text_box, None, &step, cx);
    }
    commit(text_box, None, syllable, cx);
}

fn boxes_with_focus(
    cx: &mut TestAppContext,
    index: usize,
) -> (
    gpui::Entity<super::text_box_tests::Host>,
    TextBox,
    &mut VisualTestContext,
) {
    let (host, cx) = setup(cx, false, 300.);
    let text_box = text_box(&host, index, cx);
    focus(&text_box, cx);
    (host, text_box, cx)
}

#[gpui::test]
fn t_ime_1_telex_steps_then_a_commit_leave_no_mark_and_one_undo_step(cx: &mut TestAppContext) {
    for index in [ONE, AREA, FILL] {
        let (_, text_box, cx) = boxes_with_focus(cx, index);
        load(&text_box, "", cx);
        for step in ["v", "vi", "vie", "viê", "việ"] {
            mark(&text_box, None, step, cx);
            assert_eq!(text_of(&text_box, cx), step, "kind {index}");
            assert_eq!(marked(&text_box, cx), Some(0..step.encode_utf16().count()));
        }
        commit(&text_box, None, "việt", cx);
        assert_eq!(text_of(&text_box, cx), "việt", "kind {index}");
        assert_eq!(marked(&text_box, cx), None, "kind {index}");
        press_as(cx, "mod-z", false);
        assert_eq!(text_of(&text_box, cx), "", "kind {index}: one undo step");
    }
}

#[gpui::test]
fn t_ime_2_a_relative_replace_inside_the_mark(cx: &mut TestAppContext) {
    let (_, text_box, cx) = boxes_with_focus(cx, ONE);
    load(&text_box, "", cx);
    type_text(cx, "ab ");
    mark(&text_box, None, "viet", cx);
    assert_eq!(marked(&text_box, cx), Some(3..7));
    // The range counts from the start of the marked text.
    mark(&text_box, Some(2..3), "ê", cx);
    assert_eq!(text_of(&text_box, cx), "ab viêt");
    // Zed marks only the replacement.
    assert_eq!(marked(&text_box, cx), Some(5..6));
}

#[gpui::test]
fn t_ime_3_an_evkey_absolute_replace(cx: &mut TestAppContext) {
    for index in [ONE, AREA] {
        let (_, text_box, cx) = boxes_with_focus(cx, index);
        load(&text_box, "", cx);
        // EVKey types plain letters, then replaces the word at its absolute place.
        type_text(cx, "xin viet");
        assert_eq!(marked(&text_box, cx), None);
        commit(&text_box, Some(4..8), "việt", cx);
        assert_eq!(text_of(&text_box, cx), "xin việt", "kind {index}");
        assert_eq!(marked(&text_box, cx), None);
        assert_eq!(selection_of(&text_box, cx), 8..8, "kind {index}");
    }
}

#[gpui::test]
fn t_ime_4_a_fast_backspace_and_text_for_50_words(cx: &mut TestAppContext) {
    let list = words();
    assert!(list.len() >= 50);
    let (_, text_box, cx) = boxes_with_focus(cx, ONE);
    for word in &list[..50] {
        load(&text_box, "", cx);
        let first = word.chars().next().unwrap();
        // The engine shows the first char, takes it back, and writes the word.
        commit(&text_box, None, &first.to_string(), cx);
        press_as(cx, "backspace", false);
        commit(&text_box, None, word, cx);
        assert_eq!(text_of(&text_box, cx), *word);
        assert_eq!(marked(&text_box, cx), None, "{word}");
    }
}

#[gpui::test]
fn t_ime_5_an_empty_range_inserts_at_the_caret(cx: &mut TestAppContext) {
    for index in [ONE, AREA] {
        let (_, text_box, cx) = boxes_with_focus(cx, index);
        load(&text_box, "ac", cx);
        press_as(cx, "left", false);
        commit(&text_box, Some(1..1), "b", cx);
        assert_eq!(text_of(&text_box, cx), "abc", "kind {index}");
        assert_eq!(selection_of(&text_box, cx), 2..2, "kind {index}");
        commit(&text_box, None, "X", cx);
        assert_eq!(text_of(&text_box, cx), "abXc", "kind {index}");
    }
}

#[gpui::test]
fn t_ime_6_enter_while_composing_and_mod_enter_after(cx: &mut TestAppContext) {
    let (host, text_box, cx) = boxes_with_focus(cx, ONE);
    load(&text_box, "", cx);
    mark(&text_box, None, "việt", cx);
    host.update(cx, |host, _| host.keys.clear());
    press_as(cx, "enter", false);
    let reached = host.read_with(cx, |host, _| host.keys.clone());
    // Today a one-line box has no Enter binding: the key reaches the parent, as in T-KEY-P. The
    // plan's rule (Enter commits the word and stops) is for S3.3; this prints what happens now.
    eprintln!(
        "T-IME-6 Enter while composing: text {:?}, mark {:?}, parent heard {reached:?}",
        text_of(&text_box, cx),
        marked(&text_box, cx)
    );
    assert!(
        reached.iter().any(|(key, _)| key == "enter"),
        "the parent heard {reached:?}"
    );
    // Mod+Enter reaches the parent, and the parent reads the committed text.
    load(&text_box, "", cx);
    commit(&text_box, None, "việt", cx);
    host.update(cx, |host, _| host.keys.clear());
    press_as(cx, "mod-enter", false);
    let reached = host.read_with(cx, |host, _| host.keys.len());
    assert_eq!(reached, 1);
    assert!(text_of(&text_box, cx).starts_with("việt"));
}

#[gpui::test]
fn t_ime_7_nfd_backspace_and_left_are_recorded(cx: &mut TestAppContext) {
    // `ệ` as e + dot below + circumflex.
    let nfd = "Vie\u{323}\u{302}t";
    let (_, text_box, cx) = boxes_with_focus(cx, ONE);
    load(&text_box, nfd, cx);
    press_as(cx, "backspace", false);
    let after_backspace = text_of(&text_box, cx);
    press_as(cx, "backspace", false);
    let after_two = text_of(&text_box, cx);
    load(&text_box, nfd, cx);
    press_as(cx, "left", false);
    let after_left = selection_of(&text_box, cx);
    press_as(cx, "left", false);
    let after_two_lefts = selection_of(&text_box, cx);
    eprintln!(
        "T-IME-7 NFD {nfd:?}: Backspace {after_backspace:?}, again {after_two:?}; Left at {after_left:?}, again {after_two_lefts:?}"
    );
    // Whatever the rule, the keys only shorten the text from its end and only move left.
    assert!(nfd.starts_with(&after_backspace) && after_backspace.len() < nfd.len());
    assert!(after_backspace.starts_with(&after_two) && after_two.len() < after_backspace.len());
    let end = nfd.encode_utf16().count();
    assert!(after_left.end < end && after_two_lefts.end < after_left.end);
}

#[gpui::test]
fn t_ime_8_utf16_ranges_with_d_bar_emoji_and_cjk(cx: &mut TestAppContext) {
    // UTF-16 lengths: đ 1, 👍 2, 日 1, 本 1.
    let text = "đ👍日本";
    for index in [ONE, AREA] {
        let (_, text_box, cx) = boxes_with_focus(cx, index);
        load(&text_box, text, cx);
        press_as(cx, "mod-a", false);
        assert_eq!(selection_of(&text_box, cx), 0..5, "kind {index}");
        press_as(cx, "home right right", false);
        assert_eq!(
            selection_of(&text_box, cx),
            3..3,
            "kind {index}: after the emoji"
        );
        let (whole, adjusted) = text_box.editor().update_in(cx, |editor, window, cx| {
            let mut adjusted = None;
            let got = editor.text_for_range(1..3, &mut adjusted, window, cx);
            (got, adjusted)
        });
        assert_eq!(whole.as_deref(), Some("👍"), "kind {index}");
        eprintln!("T-IME-8 kind {index}: text_for_range(1..3) adjusted {adjusted:?}");
        // A range that cuts the emoji in two clips to char boundaries.
        let (clipped, adjusted) = text_box.editor().update_in(cx, |editor, window, cx| {
            let mut adjusted = None;
            let got = editor.text_for_range(2..3, &mut adjusted, window, cx);
            (got, adjusted)
        });
        eprintln!(
            "T-IME-8 kind {index}: text_for_range(2..3) gave {clipped:?}, adjusted {adjusted:?}"
        );
        commit(&text_box, Some(1..3), "x", cx);
        assert_eq!(text_of(&text_box, cx), "đx日本", "kind {index}");
        commit(&text_box, Some(2..4), "!", cx);
        assert_eq!(text_of(&text_box, cx), "đx!", "kind {index}");
    }
}

#[gpui::test]
fn t_ime_9_blur_keeps_the_text_and_unmarks(cx: &mut TestAppContext) {
    let (host, text_box, cx) = boxes_with_focus(cx, ONE);
    load(&text_box, "", cx);
    mark(&text_box, None, "viet", cx);
    assert!(marked(&text_box, cx).is_some());
    let other = host.read_with(cx, |host, _| host.other.clone());
    focus_handle(&other, cx);
    assert_eq!(text_of(&text_box, cx), "viet");
    // Today the mark survives a blur; the plan's rule (a blur ends it) is for S3.3.
    assert_eq!(marked(&text_box, cx), Some(0..4));
    // `unmark_text` on its own keeps the text and ends the mark.
    text_box
        .editor()
        .update_in(cx, |editor, window, cx| editor.unmark_text(window, cx));
    assert_eq!(marked(&text_box, cx), None);
    assert_eq!(text_of(&text_box, cx), "viet");
}

#[gpui::test]
fn t_ime_10_bounds_for_range_in_one_line_area_and_full(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx, false, 300.);
    let rect = |text_box: &TextBox, range: Range<usize>, cx: &mut VisualTestContext| {
        text_box.editor().update_in(cx, |editor, window, cx| {
            let area = *editor.last_bounds().expect("the box was drawn");
            editor
                .bounds_for_range(range, area, window, cx)
                .map(|bounds| (area, bounds))
        })
    };

    // One line, scrolled: the caret at the end of a long text is inside the box.
    let one = text_box(&host, ONE, cx);
    focus(&one, cx);
    let long = "chữ dài ".repeat(30);
    load(&one, &long, cx);
    let end = long.encode_utf16().count();
    let (area, last) = rect(&one, end - 2..end - 1, cx).expect("the last char is shown");
    eprintln!("T-IME-10 one-line scrolled: box {area:?}, last char {last:?}");
    let (_, first) = rect(&one, 0..1, cx).unwrap_or((area, last));
    // Zed gives the start of the range with the size of one em and one line.
    assert_eq!(first.size, last.size);
    eprintln!("T-IME-10 one-line scrolled: first char {first:?}");

    // Area and full: row 3 is three line heights below row 0.
    for index in [AREA, FILL] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        load(&text_box, "r0\nr1\nr2\nr3", cx);
        press_as(cx, "ctrl-home", false);
        let (_, row0) = rect(&text_box, 0..1, cx).expect("row 0");
        let (_, row3) = rect(&text_box, 9..10, cx).expect("row 3");
        let step = (row3.origin.y - row0.origin.y) / 3.;
        eprintln!("T-IME-10 kind {index}: row 0 {row0:?}, row 3 {row3:?}");
        assert!(f32::from(step) > 0.);
        assert!(
            (f32::from(step) - f32::from(row0.size.height)).abs() <= 0.5,
            "kind {index}"
        );
        assert!(
            (f32::from(row3.origin.x) - f32::from(row0.origin.x)).abs() <= 0.5,
            "kind {index}"
        );
    }
}

#[gpui::test]
fn t_ime_words_every_word_of_the_list_composes_and_commits(cx: &mut TestAppContext) {
    let list = words();
    assert_eq!(list.len(), 100);
    let (host, cx) = setup(cx, false, 300.);
    for index in [ONE, AREA] {
        let text_box = text_box(&host, index, cx);
        focus(&text_box, cx);
        for word in &list {
            load(&text_box, "", cx);
            for (position, syllable) in word.split(' ').enumerate() {
                if position > 0 {
                    commit(&text_box, None, " ", cx);
                }
                compose(&text_box, syllable, cx);
            }
            assert_eq!(text_of(&text_box, cx), *word, "kind {index}");
            assert_eq!(marked(&text_box, cx), None, "kind {index}: {word}");
            let units = word.encode_utf16().count();
            assert_eq!(
                selection_of(&text_box, cx),
                units..units,
                "kind {index}: {word}"
            );
        }
    }
}

#[test]
fn the_steps_of_a_syllable_show_each_char_in_its_base_form_first() {
    assert_eq!(steps("việt"), ["v", "vi", "vie", "viê", "việ", "việt"]);
    assert_eq!(steps("Đà"), ["D", "Đ", "Đa", "Đà"]);
}
