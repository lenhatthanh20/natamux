use ChangeStatus::{Added, Conflict, Deleted, Modified, Renamed, Untracked};

use super::*;

fn entry(path: &str, status: ChangeStatus) -> ChangeEntry {
    ChangeEntry {
        path: path.into(),
        status,
        original_path: None,
        added: None,
        removed: None,
    }
}

fn files() -> Vec<ChangeEntry> {
    vec![
        entry("src/app.ts", Modified),
        entry("notes.md", Untracked),
        entry("src/new.ts", Added),
    ]
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

#[test]
fn checks_every_file_and_warns_about_new_files() {
    let list = files();
    let checked = first_checks(&list);
    assert_eq!(checked, set(&["src/app.ts", "notes.md", "src/new.ts"]));
    assert_eq!(row_name(&list[0]), "src/app.ts, Modified");
    assert_eq!(row_name(&list[1]), "notes.md, New file");
    assert_eq!(all_state(&list, &checked).tri, Tri::Checked);
    assert_eq!(
        confirm_label(picked(&list, &checked).len()),
        "Roll back 3 files"
    );
    assert_eq!(
        new_file_text(new_file_count(&list, &checked)).as_deref(),
        Some("2 new files will be deleted.")
    );
}

#[test]
fn rolls_back_the_checked_files_only() {
    let list = files();
    let mut checked = first_checks(&list);

    checked = toggle(&checked, "notes.md", false);
    assert_eq!(all_state(&list, &checked).tri, Tri::Mixed);
    assert_eq!(
        new_file_text(new_file_count(&list, &checked)).as_deref(),
        Some("1 new file will be deleted.")
    );
    checked = toggle(&checked, "src/new.ts", false);
    assert_eq!(new_file_text(new_file_count(&list, &checked)), None);
    assert_eq!(
        confirm_label(picked(&list, &checked).len()),
        "Roll back 1 file"
    );
    assert_eq!(picked(&list, &checked), ["src/app.ts"]);
}

#[test]
fn does_nothing_with_no_file_checked() {
    let list = files();
    let all = first_checks(&list);
    let none = toggle_all(&list, all_state(&list, &all).tri);
    assert!(none.is_empty());
    assert!(!row_checked(&list[0], &none));
    assert_eq!(
        confirm_reason(picked(&list, &none).len(), false),
        Some("Check a file first")
    );

    let again = toggle_all(&list, all_state(&list, &none).tri);
    assert_eq!(again, all);
}

#[test]
fn checks_everything_when_the_all_box_is_mixed() {
    let list = files();
    let some = set(&["src/app.ts"]);
    assert_eq!(
        toggle_all(&list, all_state(&list, &some).tri),
        first_checks(&list)
    );
}

#[test]
fn shows_a_conflict_without_letting_it_be_checked() {
    let list = vec![entry("both.ts", Conflict), entry("src/app.ts", Modified)];
    let checked = first_checks(&list);
    assert_eq!(checked, set(&["src/app.ts"]));
    assert!(!row_checked(&list[0], &set(&["both.ts"])));
    assert!(row_off(&list[0]));
    assert_eq!(row_tip(&list[0]), Some("Resolve the conflict first"));
    assert_eq!(row_tip(&list[1]), None);
    assert_eq!(row_name(&list[0]), "both.ts, Conflict");
    assert_eq!(
        picked(&list, &set(&["both.ts", "src/app.ts"])),
        ["src/app.ts"]
    );
}

#[test]
fn turns_the_all_box_off_when_only_conflicts_are_left() {
    let only = vec![entry("both.ts", Conflict)];
    let state = all_state(&only, &BTreeSet::new());
    assert_eq!(
        state,
        AllState {
            tri: Tri::Unchecked,
            off: true
        }
    );
    assert!(all_state(&[], &BTreeSet::new()).off);
    assert!(!all_state(&files(), &BTreeSet::new()).off);
}

#[test]
fn waits_while_another_git_action_runs() {
    assert_eq!(
        confirm_reason(3, true),
        Some("Another git action is running")
    );
    assert_eq!(confirm_reason(0, true), Some("Check a file first"));
    assert_eq!(confirm_reason(1, false), None);
}

#[test]
fn counts_added_untracked_and_renamed_files_as_new() {
    let list = vec![
        entry("a", Added),
        entry("b", Untracked),
        entry("c", Renamed),
        entry("d", Modified),
        entry("e", Deleted),
    ];
    let all = first_checks(&list);
    assert_eq!(new_file_count(&list, &all), 3);
    assert_eq!(new_file_count(&list, &BTreeSet::new()), 0);
}

#[test]
fn offers_the_files_of_a_row_without_the_conflicts() {
    let offer = rollback_offer(vec![entry("a.ts", Modified), entry("b.ts", Conflict)]);
    assert_eq!(offer.entries, [entry("a.ts", Modified)]);
    assert_eq!(offer.off_reason, None);
}

#[test]
fn turns_the_rollback_item_off_when_only_conflicts_are_left() {
    let offer = rollback_offer(vec![entry("b.ts", Conflict)]);
    assert!(offer.entries.is_empty());
    assert_eq!(offer.off_reason, Some("Resolve the conflict first"));
    assert_eq!(
        rollback_offer(Vec::new()).off_reason,
        Some("Resolve the conflict first")
    );
}

#[test]
fn drops_the_files_that_stopped_being_changed() {
    let offer = vec![entry("a.ts", Modified), entry("b.ts", Modified)];
    let list = vec![entry("b.ts", Deleted), entry("c.ts", Added)];
    assert_eq!(still_changed(&offer, &list), [entry("b.ts", Deleted)]);
    assert!(still_changed(&offer, &[]).is_empty());
}

#[test]
fn names_one_file_or_many_on_the_confirm_button() {
    assert_eq!(confirm_label(1), "Roll back 1 file");
    assert_eq!(confirm_label(2), "Roll back 2 files");
    assert_eq!(confirm_label(0), "Roll back 0 files");
}
