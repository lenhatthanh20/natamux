use super::*;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

// The cases of 'the open file' (`view-store.test.ts`).
#[test]
fn opens_the_workspace_of_the_task_on_the_editor_tab() {
    let mut files = FileViews::default();
    assert_eq!(files.open("T-0004", "src/a.ts"), WorkspaceTab::Editor);
    assert_eq!(files.editor_file("T-0004"), Some("src/a.ts"));
    files.open("T-0004", "src/b.ts");
    assert_eq!(files.editor_file("T-0004"), Some("src/b.ts"));
    assert_eq!(files.editor_file("T-0005"), None);
    assert!(files.editor_files("T-0005").is_empty());
}

#[test]
fn adds_a_tab_per_new_file_and_picks_an_open_one_again() {
    let mut files = FileViews::default();
    files.open("T-0004", "a.ts");
    files.open("T-0004", "b.ts");
    files.open("T-0004", "a.ts");
    assert_eq!(files.editor_files("T-0004"), paths(&["a.ts", "b.ts"]));
    assert_eq!(files.editor_file("T-0004"), Some("a.ts"));
}

#[test]
fn keeps_the_open_files_of_each_task() {
    let mut files = FileViews::default();
    files.open("T-0004", "x.ts");
    files.open("T-0004", "y.ts");
    files.open("T-0004", "x.ts");
    files.open("T-0005", "z.ts");
    assert_eq!(files.editor_files("T-0004"), paths(&["x.ts", "y.ts"]));
    assert_eq!(files.editor_file("T-0004"), Some("x.ts"));
    assert_eq!(files.editor_files("T-0005"), paths(&["z.ts"]));
}

#[test]
fn closing_the_active_file_picks_the_one_at_its_place_else_the_one_before() {
    let mut files = FileViews::default();
    for path in ["a.ts", "b.ts", "c.ts"] {
        files.open("T-0004", path);
    }
    files.open("T-0004", "b.ts");
    let editor = Some(WorkspaceTab::Editor);
    assert_eq!(files.close("T-0004", "b.ts", editor, None), None);
    assert_eq!(files.editor_file("T-0004"), Some("c.ts"));
    assert_eq!(files.close("T-0004", "c.ts", editor, None), None);
    assert_eq!(files.editor_file("T-0004"), Some("a.ts"));
}

#[test]
fn closing_another_file_keeps_the_active_one() {
    let mut files = FileViews::default();
    files.open("T-0004", "a.ts");
    files.open("T-0004", "b.ts");
    let back = files.close("T-0004", "a.ts", Some(WorkspaceTab::Editor), None);
    assert_eq!(back, None);
    assert_eq!(files.editor_files("T-0004"), paths(&["b.ts"]));
    assert_eq!(files.editor_file("T-0004"), Some("b.ts"));
    // A file that is not open: nothing changes.
    assert_eq!(files.close("T-0004", "z.ts", None, None), None);
    assert_eq!(files.editor_file("T-0004"), Some("b.ts"));
}

#[test]
fn closes_back_to_the_tab_the_user_picked_last_or_the_terminal() {
    let mut files = FileViews::default();
    files.open("T-0004", "src/a.ts");
    let back = files.close(
        "T-0004",
        "src/a.ts",
        Some(WorkspaceTab::Editor),
        Some(WorkspaceTab::Docs),
    );
    assert_eq!(back, Some(WorkspaceTab::Docs));
    assert_eq!(files.editor_file("T-0004"), None);

    files.open("T-0005", "b.ts");
    let back = files.close("T-0005", "b.ts", Some(WorkspaceTab::Editor), None);
    assert_eq!(back, Some(WorkspaceTab::Terminal));
    // Another tab shown, or another workspace open: the tab stays.
    files.open("T-0006", "c.ts");
    assert_eq!(
        files.close("T-0006", "c.ts", Some(WorkspaceTab::Docs), None),
        None
    );
    files.open("T-0007", "d.ts");
    assert_eq!(files.close("T-0007", "d.ts", None, None), None);
}

#[test]
fn gives_the_picked_stage_file_to_its_task_once() {
    let mut files = FileViews::default();
    files.request_doc("T-0001", "spec.md");
    assert_eq!(files.take_doc_request("T-0002"), None);
    assert_eq!(files.take_doc_request("T-0001"), Some("spec.md".into()));
    assert_eq!(files.take_doc_request("T-0001"), None);
}

#[test]
fn names_a_tab_by_its_file_and_adds_the_folder_for_a_shared_name() {
    let labels = tab_labels(&paths(&["src/git/mod.rs", "README.md", "src/files/mod.rs"]));
    assert_eq!(
        labels,
        vec![
            ("mod.rs".to_owned(), Some("src/git".to_owned())),
            ("README.md".to_owned(), None),
            ("mod.rs".to_owned(), Some("src/files".to_owned())),
        ]
    );
    // A file at the root shares a name with one in a folder: the root has no folder to show.
    let labels = tab_labels(&paths(&["mod.rs", "a/mod.rs"]));
    assert_eq!(labels[0], ("mod.rs".to_owned(), None));
    assert_eq!(labels[1], ("mod.rs".to_owned(), Some("a".to_owned())));
}
