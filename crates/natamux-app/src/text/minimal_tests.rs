use super::*;

fn apply(old: &str, edits: &[Edit]) -> String {
    let mut text = old.to_string();
    for edit in edits.iter().rev() {
        text.replace_range(edit.range.clone(), &edit.text);
    }
    text
}

#[test]
fn the_edits_turn_the_old_text_into_the_new_one() {
    let cases = [
        ("", "abc"),
        ("abc", ""),
        ("one\ntwo\nthree\n", "one\nTWO\nthree\n"),
        ("a\nb\nc\nd\ne", "a\nB\nc\nd\nE"),
        ("{\n  \"x\": 1\n}", "{\n  \"x\": 2,\n  \"y\": 3\n}"),
        ("việt\nnam", "viết\nnam"),
        ("same", "same"),
        ("no end", "no end\n"),
    ];
    for (old, new) in cases {
        let edits = edits(old, new);
        assert_eq!(apply(old, &edits), new, "{old:?} -> {new:?}: {edits:?}");
    }
}

#[test]
fn only_the_changed_chars_are_replaced() {
    let edits = edits("one\ntwo\nthree\n", "one\ntwX\nthree\n");
    assert_eq!(
        edits,
        [Edit {
            range: 6..7,
            text: "X".into()
        }]
    );
}

#[test]
fn offsets_before_an_edit_stay_and_after_it_move() {
    let edits = [Edit {
        range: 4..6,
        text: "xyz".into(),
    }];
    assert_eq!(map_offset(&edits, 2), 2);
    assert_eq!(map_offset(&edits, 4), 4);
    assert_eq!(map_offset(&edits, 5), 5);
    assert_eq!(map_offset(&edits, 6), 7);
    assert_eq!(map_offset(&edits, 9), 10);
}
