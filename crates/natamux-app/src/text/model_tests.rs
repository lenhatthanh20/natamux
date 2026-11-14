use std::time::{Duration, Instant};

use super::*;

fn later(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn typed(model: &mut TextModel, text: &str, at: Instant) {
    let range = model.selection().range();
    model.edit(range, text, EditKind::Typing, at);
}

#[test]
fn fast_typing_is_one_step_and_a_pause_splits_it() {
    let start = Instant::now();
    let mut model = TextModel::default();
    typed(&mut model, "a", start);
    typed(&mut model, "b", later(start, 100));
    typed(&mut model, "c", later(start, 200));
    assert!(model.undo());
    assert_eq!(model.text(), "");
    assert!(model.redo());
    assert_eq!(model.text(), "abc");
    assert_eq!(model.selection(), Selection::caret(3));

    let mut model = TextModel::default();
    typed(&mut model, "ab", start);
    typed(&mut model, "c", later(start, 400));
    model.undo();
    assert_eq!(model.text(), "ab");
    model.undo();
    assert_eq!(model.text(), "");
    assert!(!model.undo());
    model.redo();
    model.redo();
    assert_eq!(model.text(), "abc");
}

#[test]
fn set_text_is_its_own_step_and_the_same_text_changes_nothing() {
    let start = Instant::now();
    let mut model = TextModel::default();
    assert!(model.set_text("first", start));
    typed(&mut model, "!", later(start, 10));
    assert!(model.set_text("second", later(start, 20)));
    assert!(!model.set_text("second", later(start, 30)));
    model.undo();
    assert_eq!(model.text(), "first!");
    model.undo();
    assert_eq!(model.text(), "first");
    model.undo();
    assert_eq!(model.text(), "");
}

#[test]
fn a_composition_is_one_step_whatever_its_length() {
    let start = Instant::now();
    let mut model = TextModel::default();
    let mut at = 0;
    for step in ["v", "vi", "vie", "viê", "việ"] {
        let range = model.marked().unwrap_or_else(|| model.selection().range());
        model.edit(range.clone(), step, EditKind::Composing, later(start, at));
        model.set_marked(Some(range.start..range.start + step.len()));
        at += 1000;
    }
    let range = model.marked().unwrap_or_else(|| model.selection().range());
    model.edit(range, "việt", EditKind::Composing, later(start, at));
    model.set_marked(None);
    model.seal();
    typed(&mut model, " ", later(start, at + 10));
    model.undo();
    assert_eq!(model.text(), "việt");
    model.undo();
    assert_eq!(model.text(), "");
}

#[test]
fn line_ends_are_stored_as_line_feeds() {
    let mut model = TextModel::default();
    model.edit(0..0, "a\r\nb\rc", EditKind::Other, Instant::now());
    assert_eq!(model.text(), "a\nb\nc");
}

#[test]
fn replace_minimal_keeps_the_caret_and_undoes_in_one_step() {
    let start = Instant::now();
    let mut model = TextModel::default();
    model.set_text("one\ntwo\nthree\nfour", start);
    model.move_to(10);
    assert!(model.replace_minimal("ONE\ntwo\nthree\nfour!", later(start, 1000)));
    assert_eq!(model.text(), "ONE\ntwo\nthree\nfour!");
    assert_eq!(model.selection(), Selection::caret(10));
    assert!(model.undo());
    assert_eq!(model.text(), "one\ntwo\nthree\nfour");
    assert!(model.redo());
    assert_eq!(model.text(), "ONE\ntwo\nthree\nfour!");
    // A caret after a line that grew moves with it.
    model.move_to(12);
    model.replace_minimal("ONE\ntwo two\nthree\nfour!", later(start, 2000));
    assert_eq!(model.selection(), Selection::caret(16));
}

#[test]
fn a_drop_moves_or_copies_the_text_in_one_step() {
    let start = Instant::now();
    let text = "hello world foo bar";
    let mut model = TextModel::default();
    model.set_text(text, start);
    assert!(model.drop_text(6..11, 15, false, later(start, 1000)));
    assert_eq!(model.text(), "hello  fooworld bar");
    assert_eq!(model.selection().range(), 10..15);
    model.undo();
    assert_eq!(model.text(), text);
    assert!(model.drop_text(6..11, 15, true, later(start, 2000)));
    assert_eq!(model.text(), "hello world fooworld bar");
    model.undo();
    assert!(model.drop_text(6..11, 0, false, later(start, 3000)));
    assert_eq!(model.text(), "worldhello  foo bar");
    model.undo();
    assert_eq!(model.text(), text);
    // A drop inside the text it moves does nothing.
    assert!(!model.drop_text(6..11, 8, false, later(start, 4000)));
}

#[test]
fn a_selection_reads_front_to_back_and_knows_its_direction() {
    let mut model = TextModel::default();
    model.set_text("hello", Instant::now());
    model.select(4, 1);
    assert_eq!(model.selection().range(), 1..4);
    assert!(model.selection().reversed());
    // Offsets inside a char go back to its start.
    model.set_text("đa", Instant::now());
    model.move_to(1);
    assert_eq!(model.head(), 0);
}

#[test]
fn the_mark_moves_with_an_edit_before_it_and_goes_with_one_over_it() {
    let start = Instant::now();
    let mut model = TextModel::default();
    model.set_text("ab việt", start);
    model.set_marked(Some(3..model.text().len()));
    model.edit(0..0, "xyz", EditKind::Other, later(start, 10));
    assert_eq!(model.marked(), Some(6..model.text().len()));
    assert_eq!(&model.text()[model.marked().unwrap()], "việt");
    // An edit after the mark leaves it.
    let end = model.text().len();
    model.edit(end..end, "!", EditKind::Other, later(start, 20));
    assert_eq!(&model.text()[model.marked().unwrap()], "việt");
    // An edit over it ends it.
    model.edit(5..7, "", EditKind::Other, later(start, 30));
    assert_eq!(model.marked(), None);
    // A mark that ends inside a char is clipped to the char's start.
    model.set_text("việt", later(start, 40));
    model.set_marked(Some(0..3));
    assert_eq!(model.marked(), Some(0..2));
}

#[test]
fn a_paste_starts_a_step_that_the_next_quick_edits_join() {
    let start = Instant::now();
    let mut model = TextModel::default();
    typed(&mut model, "ab", start);
    let range = model.selection().range();
    model.edit(range, "cd", EditKind::Paste, start);
    let head = model.head();
    model.edit(head - 1..head, "", EditKind::Deleting, start);
    assert_eq!(model.text(), "abc");
    model.undo();
    assert_eq!(model.text(), "ab");
    model.undo();
    assert_eq!(model.text(), "");
}

#[test]
fn edit_all_is_one_step_and_puts_the_selection() {
    let start = Instant::now();
    let mut model = TextModel::default();
    model.set_text("one\ntwo", start);
    let edits = [
        minimal::Edit {
            range: 0..0,
            text: "  ".into(),
        },
        minimal::Edit {
            range: 4..4,
            text: "  ".into(),
        },
    ];
    model.edit_all(&edits, 0, 11, later(start, 1000));
    assert_eq!(model.text(), "  one\n  two");
    assert_eq!(model.selection().range(), 0..11);
    model.undo();
    assert_eq!(model.text(), "one\ntwo");
    model.redo();
    assert_eq!(model.text(), "  one\n  two");
}
