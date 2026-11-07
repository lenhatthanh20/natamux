use super::*;

#[test]
fn a_separator_starts_where_the_text_of_the_rows_starts() {
    // Plain rows: the 12 px row padding. Tile or chevron rows: past the 24 px tile and the
    // 8 px gap. Checkbox + tile rows: past both.
    assert_eq!(SeparatorInset::Text.px(), 12.);
    assert_eq!(SeparatorInset::Tile.px(), 44.);
    assert_eq!(SeparatorInset::CheckTile.px(), 68.);
    assert_eq!(SeparatorInset::default(), SeparatorInset::Text);
}

#[test]
fn a_card_puts_one_separator_between_two_rows_only() {
    assert!(!separator_after(0, 1));
    assert!(separator_after(0, 3));
    assert!(separator_after(1, 3));
    assert!(!separator_after(2, 3));
}

#[test]
fn a_dialog_group_title_sits_closer_to_the_top() {
    assert_eq!(group_top_margin(false), 20.);
    assert_eq!(group_top_margin(true), 12.);
}
