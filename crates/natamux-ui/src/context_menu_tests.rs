use super::*;

fn rows() -> Vec<Row> {
    // 0 heading, 1 line, 2 entry, 3 entry that is off, 4 text, 5 entry, 6 line.
    vec![
        Row::Heading("Head".into()),
        Row::Line,
        Row::Entry(ContextMenuEntry::new("First")),
        Row::Entry(ContextMenuEntry::new("Off").disabled(true)),
        Row::Text("Note".into()),
        Row::Entry(ContextMenuEntry::new("Last")),
        Row::Line,
    ]
}

#[test]
fn down_skips_rows_that_cannot_be_picked_and_wraps() {
    let rows = rows();
    assert_eq!(row_below(&rows, None), Some(2));
    assert_eq!(row_below(&rows, Some(2)), Some(5));
    assert_eq!(row_below(&rows, Some(5)), Some(2));
}

#[test]
fn up_skips_rows_that_cannot_be_picked_and_wraps() {
    let rows = rows();
    assert_eq!(row_above(&rows, None), Some(5));
    assert_eq!(row_above(&rows, Some(5)), Some(2));
    assert_eq!(row_above(&rows, Some(2)), Some(5));
}

#[test]
fn a_menu_with_no_pickable_row_picks_nothing() {
    let rows = [Row::Line, Row::Heading("Head".into())];
    assert_eq!(row_below(&rows, None), None);
    assert_eq!(row_above(&rows, Some(1)), None);
}

#[test]
fn only_a_checked_entry_that_is_on_counts_as_checked() {
    let on = Row::Entry(ContextMenuEntry::new("On").toggleable(IconPosition::Start, true));
    let off = Row::Entry(ContextMenuEntry::new("Off").toggleable(IconPosition::Start, false));
    assert!(on.checked());
    assert!(!off.checked());
    assert!(!Row::Line.checked());
}
