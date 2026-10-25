use super::*;

fn draft(text: &str, version: &str) -> FileDraft {
    FileDraft {
        text: text.to_string(),
        version: version.to_string(),
    }
}

fn pair(task: &str, path: &str) -> (String, String) {
    (task.to_string(), path.to_string())
}

#[test]
fn keeps_one_draft_per_task_and_file_and_drops_it() {
    let mut drafts = Drafts::default();
    drafts.set("T-0001", "a.ts", Some(draft("x", "v1")));
    drafts.set("T-0002", "a.ts", Some(draft("y", "v1")));
    drafts.set("T-0001", "a.ts", Some(draft("xx", "v1")));
    assert_eq!(drafts.get("T-0001", "a.ts"), Some(&draft("xx", "v1")));
    assert_eq!(drafts.get("T-0002", "a.ts"), Some(&draft("y", "v1")));
    assert_eq!(drafts.iter().count(), 2);
    assert_eq!(drafts.of_task("T-0001").count(), 1);
    drafts.set("T-0001", "a.ts", None);
    assert_eq!(drafts.unsaved(), vec![pair("T-0002", "a.ts")]);
}

#[test]
fn asks_before_the_page_closes_only_while_a_draft_is_not_saved() {
    let mut drafts = Drafts::default();
    assert!(!drafts.has_any());
    assert_eq!(quit_question(&drafts.unsaved(), &[]), None);
    drafts.set("T-0001", "a.ts", Some(draft("x", "v1")));
    assert!(drafts.has_any());
    let question = quit_question(&drafts.unsaved(), &[]).expect("a question");
    assert_eq!(question.title, "Quit and drop your changes?");
    assert_eq!(question.rows, vec!["T-0001 \u{b7} a.ts"]);
    drafts.set("T-0001", "a.ts", None);
    assert_eq!(quit_question(&drafts.unsaved(), &[]), None);
}

#[test]
fn lists_file_and_doc_drafts_and_at_most_five_rows() {
    let files: Vec<_> = (1..=4)
        .map(|n| pair("T-0001", &format!("f{n}.ts")))
        .collect();
    let docs = vec![pair("T-0002", "spec.md"), pair("T-0002", "plan.md")];
    let question = quit_question(&files, &docs).expect("a question");
    assert_eq!(question.rows.len(), 6);
    assert_eq!(question.rows[4], "T-0002 \u{b7} spec.md");
    assert_eq!(question.rows[5], "and 1 more");
    let docs_only = quit_question(&[], &docs).expect("a question");
    assert_eq!(docs_only.rows.len(), 2);
}

#[test]
fn saves_the_edit_with_mod_s_and_the_version_it_read() {
    // No draft: nothing to save. An edit makes one on the version read.
    assert!(!can_save(None, false, false));
    let edit = edit_draft("const a = 1\n", "v1", None, "const a = 1\nx").expect("a draft");
    assert_eq!(edit, draft("const a = 1\nx", "v1"));
    assert!(can_save(Some(&edit), false, false));
    assert!(!can_save(Some(&edit), false, true));
    assert!(!can_save(Some(&edit), true, false));
    // The save went through: the draft goes.
    assert_eq!(after_save(Some(&edit), "const a = 1\nx", "v2"), None);
}

#[test]
fn keeps_the_version_the_edit_began_on_and_drops_a_draft_that_is_back_on_disk() {
    let first = edit_draft("a", "v1", None, "ab").expect("a draft");
    // The file moved to v2 meanwhile: a further edit keeps v1.
    let next = edit_draft("agent", "v2", Some(&first), "abc").expect("a draft");
    assert_eq!(next.version, "v1");
    // The text on disk again, on the version of the disk: no draft.
    assert_eq!(edit_draft("a", "v1", None, "a"), None);
    // The same text as disk but on an older version stays a draft.
    assert!(edit_draft("agent", "v2", Some(&first), "agent").is_some());
}

#[test]
fn keeps_text_typed_while_the_save_ran_as_a_draft_on_the_saved_version() {
    let now = draft("abc", "v1");
    assert_eq!(after_save(Some(&now), "ab", "v2"), Some(draft("abc", "v2")));
    assert_eq!(after_save(None, "ab", "v2"), None);
}

#[test]
fn shows_a_new_text_from_disk_in_place_while_there_is_no_edit() {
    // No draft: never stale, so no banner; the editor takes the disk text.
    assert!(!is_stale(None, Some("v2")));
}

#[test]
fn asks_what_to_keep_when_the_file_changed_on_disk_under_an_edit() {
    let edit = draft("x", "v1");
    assert!(!is_stale(Some(&edit), Some("v1")));
    assert!(!is_stale(Some(&edit), None));
    assert!(is_stale(Some(&edit), Some("v2")));
    assert!(!can_save(Some(&edit), true, false));
    // Keep mine: the draft takes the disk version, the banner goes, the save sends v2.
    let kept = keep_mine(&edit, "v2");
    assert_eq!(kept, draft("x", "v2"));
    assert!(!is_stale(Some(&kept), Some("v2")));
    assert!(can_save(Some(&kept), false, false));
}

#[test]
fn drops_the_edit_for_the_text_on_disk_with_reload() {
    let mut drafts = Drafts::default();
    drafts.set("T-0001", "a.ts", Some(draft("x", "v1")));
    drafts.set("T-0001", "a.ts", None);
    assert_eq!(drafts.get("T-0001", "a.ts"), None);
    assert!(!is_stale(drafts.get("T-0001", "a.ts"), Some("v2")));
}

#[test]
fn shows_the_banner_when_the_save_finds_a_newer_file() {
    // A 409 reads the file again: the disk version differs from the draft's, so the banner shows.
    let edit = draft("x", "v1");
    assert!(is_stale(Some(&edit), Some("v2")));
}

#[test]
fn asks_before_close_drops_an_edit() {
    assert_eq!(close_question("a.ts"), "Discard your changes to a.ts?");
    assert_eq!(revert_question("a.ts"), "Drop your changes to a.ts?");
}

#[test]
fn shows_a_notice_instead_of_a_binary_or_a_too_large_file() {
    assert_eq!(
        body_notice(true, false, true),
        Some("Binary file. Not shown.")
    );
    assert_eq!(
        body_notice(false, true, true),
        Some("Larger than 1 MB. Not shown.")
    );
    assert_eq!(
        body_notice(false, false, true),
        Some("Larger than 1 MB. Not shown.")
    );
    assert_eq!(body_notice(false, false, false), None);
}
