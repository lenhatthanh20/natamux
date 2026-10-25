use super::*;

#[test]
fn waits_while_the_message_is_drafted_first() {
    assert_eq!(
        commit_reason("", true, false, false),
        Some("Drafting the message…")
    );
    assert_eq!(
        commit_reason("Fix it", true, false, false),
        Some("Drafting the message…")
    );
}

#[test]
fn asks_for_a_message_when_it_is_blank() {
    assert_eq!(
        commit_reason("", false, false, false),
        Some("Write a commit message first")
    );
    assert_eq!(
        commit_reason(" \n ", false, false, false),
        Some("Write a commit message first")
    );
    assert_eq!(commit_reason("Fix it", false, false, false), None);
}

#[test]
fn waits_for_another_git_action_but_not_for_its_own() {
    assert_eq!(
        commit_reason("Fix it", false, true, false),
        Some("Another git action is running")
    );
    assert_eq!(commit_reason("Fix it", false, true, true), None);
}

#[test]
fn submits_only_when_nothing_is_in_the_way() {
    assert!(can_submit("Fix it", false, false, false));
    assert!(!can_submit("  ", false, false, false));
    assert!(!can_submit("Fix it", true, false, false));
    assert!(!can_submit("Fix it", false, true, false));
    assert!(!can_submit("Fix it", false, true, true));
    assert!(!can_submit("Fix it", false, false, true));
}

#[test]
fn replaces_the_text_with_the_draft_only_when_it_did_not_change_meanwhile() {
    assert_eq!(draft_result("", "", "Add coupons"), "Add coupons");
    assert_eq!(draft_result("wip", "wip", "Add coupons"), "Add coupons");
    assert_eq!(
        draft_result("", "My own words", "Add coupons"),
        "My own words"
    );
}

#[test]
fn says_how_many_files_were_committed() {
    assert_eq!(committed_text(1), "Committed 1 file.");
    assert_eq!(committed_text(3), "Committed 3 files.");
    assert_eq!(committed_text(0), "Committed 0 files.");
}

#[test]
fn says_how_many_files_were_rolled_back() {
    assert_eq!(rolled_back_text(1), "Rolled back 1 file.");
    assert_eq!(rolled_back_text(2), "Rolled back 2 files.");
}

#[test]
fn says_what_was_pushed() {
    assert_eq!(
        pushed_text(Some("natamux/t-1")),
        "Pushed natamux/t-1 to origin."
    );
    assert_eq!(pushed_text(None), "Pushed the branch to origin.");
}

#[test]
fn labels_the_changes_tab_with_the_files_to_commit() {
    assert_eq!(tab_count_label(1), "1 file to commit");
    assert_eq!(tab_count_label(5), "5 files to commit");
}

#[test]
fn names_the_agent_in_the_tooltip_of_generate() {
    assert_eq!(draft_tip("Claude Code"), "Draft a message with Claude Code");
}

#[test]
fn keeps_the_toast_texts_of_the_web_client() {
    assert_eq!(COMMIT_FAILED, "Could not commit.");
    assert_eq!(PUSH_FAILED, "Could not push.");
    assert_eq!(PULL_REQUEST_FAILED, "Could not open the pull request.");
    assert_eq!(ROLLBACK_FAILED, "Could not roll back.");
    assert_eq!(UNDO_FAILED, "Could not undo the rollback.");
    assert_eq!(ROLLBACK_UNDONE, "Rollback undone.");
    assert_eq!(PULL_REQUEST_OPENED, "Pull request opened.");
    assert_eq!(DRAFT_FAILED, "Could not draft the commit message.");
    assert_eq!(
        DRAFT_FALLBACK,
        "The agent could not draft the message. A simple draft is in the box."
    );
}
