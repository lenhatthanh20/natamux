use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|n| n.to_string()).collect()
}

fn draft(text: &str, original: &str, base: Option<&str>) -> DocDraft {
    DocDraft {
        text: text.to_string(),
        original: original.to_string(),
        base: base.map(str::to_string),
    }
}

#[test]
fn starts_in_rendered_mode_when_the_stored_value_is_not_one_it_knows() {
    assert_eq!(ViewMode::parse("foo"), ViewMode::Rendered);
    assert_eq!(ViewMode::parse(""), ViewMode::Rendered);
    assert_eq!(ViewMode::parse("source"), ViewMode::Source);
    assert_eq!(ViewMode::default(), ViewMode::Rendered);
}

#[test]
fn stores_rendered_again_when_the_user_picks_it() {
    for mode in [ViewMode::Rendered, ViewMode::Source] {
        assert_eq!(ViewMode::parse(mode.name()), mode);
    }
}

#[test]
fn leaves_an_unchanged_edit_without_asking() {
    let unchanged = draft("# Top\n", "# Top\n", Some("t1"));
    assert!(!unchanged.is_dirty());
    assert_eq!(cancel_question("spec.md", unchanged.is_dirty()), None);
}

#[test]
fn asks_before_it_drops_an_edit_on_cancel() {
    let edited = draft("# Top\nmore", "# Top\n", Some("t1"));
    assert!(edited.is_dirty());
    assert_eq!(
        cancel_question("spec.md", edited.is_dirty()).as_deref(),
        Some("Drop your changes to spec.md?")
    );
}

#[test]
fn warns_and_keeps_the_draft_when_an_agent_changes_the_file_during_an_edit() {
    let edit = draft("# Top\nmine", "# Top\n", Some("2026-10-11T09:00:00.000Z"));
    assert!(!is_stale(&edit, Some("2026-10-11T09:00:00.000Z")));
    assert!(is_stale(&edit, Some("2026-10-11T09:30:00.000Z")));
    // The draft is not touched by the check.
    assert_eq!(edit.text, "# Top\nmine");
    assert_eq!(
        STALE_LINE,
        "This file changed after you began editing. Saving replaces the new text."
    );
    // Save asks first, only when the file changed.
    assert_eq!(
        save_question("spec.md", true).as_deref(),
        Some("spec.md changed after you began editing. Replace it with your text?")
    );
    assert_eq!(save_question("spec.md", false), None);
}

#[test]
fn renders_the_file_that_is_picked_whatever_its_name() {
    let list = names(&["notes.md", "spec.md"]);
    assert_eq!(
        current_doc(Some("spec.md"), &list, false).as_deref(),
        Some("spec.md")
    );
    assert_eq!(
        current_doc(Some("notes.md"), &list, false).as_deref(),
        Some("notes.md")
    );
}

#[test]
fn renders_the_file_the_progress_tab_opens() {
    let list = names(&["plan.md", "spec.md"]);
    assert_eq!(
        current_doc(Some("plan.md"), &list, false).as_deref(),
        Some("plan.md")
    );
}

#[test]
fn takes_the_first_file_when_the_pick_is_not_in_the_list() {
    let list = names(&["notes.md", "spec.md"]);
    assert_eq!(current_doc(None, &list, false).as_deref(), Some("notes.md"));
    assert_eq!(
        current_doc(Some("gone.md"), &list, true).as_deref(),
        Some("notes.md")
    );
}

#[test]
fn shows_the_empty_state_without_the_mode_toggle() {
    assert_eq!(current_doc(None, &[], false), None);
    // The open file is deleted: the pick is gone and nothing shows.
    assert_eq!(current_doc(Some("spec.md"), &[], false), None);
    assert!(EMPTY_TEXT.starts_with("No stage files yet"));
    assert_eq!(
        EMPTY_TEXT,
        "No stage files yet. The Context stage writes context.md, and each stage after it adds its own."
    );
}

#[test]
fn starts_context_md_after_write_context_md() {
    assert_eq!(current_doc(None, &[], true).as_deref(), Some(FIRST_FILE));
}

#[test]
fn lists_the_attachments_under_the_stage_files_with_their_sizes() {
    assert_eq!(attachment_meta(12_300, false), "12.3 KB");
    assert_eq!(attachment_meta(512, true), "512 B \u{b7} download");
}
