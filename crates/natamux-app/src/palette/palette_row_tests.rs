#[test]
fn matched_task_title_stays_regular_while_other_matches_keep_their_weight() {
    let source = include_str!("palette_row.rs");
    assert!(source.contains("font_weight: bold_matches.then_some(FontWeight::SEMIBOLD)"));
    assert!(source.contains("highlighted(&task.title, title_ranges, ink, false)"));
    assert!(source.contains("highlighted(&command.title, ranges, ink, true)"));
    assert!(source.contains("highlighted(&task.id, id_ranges, ink, true)"));
    assert!(source.contains("highlighted(&repo.name, ranges, ink, true)"));
}
