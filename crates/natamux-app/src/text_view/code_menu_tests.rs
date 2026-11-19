use super::*;

fn enabled(selected: bool, read_only: bool) -> Vec<(&'static str, bool)> {
    edit_items(selected, read_only)
        .iter()
        .map(|item| (item.label, item.enabled))
        .collect()
}

#[test]
fn an_editable_text_with_a_selection_offers_every_item() {
    assert_eq!(
        enabled(true, false),
        [
            ("Cut", true),
            ("Copy", true),
            ("Paste", true),
            ("Select All", true)
        ]
    );
}

#[test]
fn no_selection_turns_off_cut_and_copy() {
    assert_eq!(
        enabled(false, false),
        [
            ("Cut", false),
            ("Copy", false),
            ("Paste", true),
            ("Select All", true)
        ]
    );
}

#[test]
fn a_read_only_text_only_copies_and_selects() {
    assert_eq!(
        enabled(true, true),
        [
            ("Cut", false),
            ("Copy", true),
            ("Paste", false),
            ("Select All", true)
        ]
    );
    assert_eq!(
        enabled(false, true),
        [
            ("Cut", false),
            ("Copy", false),
            ("Paste", false),
            ("Select All", true)
        ]
    );
}
