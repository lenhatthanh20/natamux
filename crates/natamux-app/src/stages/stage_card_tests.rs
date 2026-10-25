/// The source of `finished_card`, so the layout can be checked without a window.
fn finished_card_source() -> &'static str {
    let source = include_str!("stage_card.rs");
    let start = source.find("fn finished_card(").expect("finished_card");
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("end of finished_card");
    &rest[..end]
}

#[test]
fn a_finished_card_has_its_chevron_on_the_title_line() {
    // Beside a top-aligned head the chevron sat above the stage name (T-0169).
    let source = finished_card_source();
    assert!(source.contains("title_row(input, Some(chevron), cx)"));
    assert!(!source.contains("items_start"));
}
