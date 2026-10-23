use super::*;

fn entry(name: &str, kind: EntryKind) -> TreeEntry {
    TreeEntry {
        name: name.to_string(),
        kind,
    }
}

fn folders() -> HashMap<String, Vec<TreeEntry>> {
    HashMap::from([
        (
            String::new(),
            vec![
                entry("src", EntryKind::Folder),
                entry("docs", EntryKind::Folder),
                entry("README.md", EntryKind::File),
            ],
        ),
        (
            "src".to_string(),
            vec![
                entry("lib", EntryKind::Folder),
                entry("a.ts", EntryKind::File),
            ],
        ),
        ("src/lib".to_string(), vec![entry("b.ts", EntryKind::File)]),
    ])
}

fn open(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

fn change(path: &str, status: ChangeStatus) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        status,
        original_path: None,
        added: None,
        removed: None,
    }
}

fn row<'a>(rows: &'a [ExplorerRow], key: &str) -> &'a ExplorerRow {
    rows.iter().find(|r| r.key == key).expect("row")
}

#[test]
fn shows_the_root_and_the_children_of_the_open_folders_that_are_loaded() {
    let rows = explorer_rows(&folders(), &open(&["src", "docs"]));
    let got: Vec<(&str, usize, bool)> = rows
        .iter()
        .map(|r| (r.key.as_str(), r.level, r.expanded))
        .collect();
    assert_eq!(
        got,
        vec![
            ("src", 1, true),
            ("src/lib", 2, false),
            ("src/a.ts", 2, false),
            // Open, but not read yet: no children.
            ("docs", 1, true),
            ("README.md", 1, false),
        ]
    );
}

#[test]
fn gives_each_row_its_parent_and_place_for_the_tree_keys_and_screen_readers() {
    let rows = explorer_rows(&folders(), &open(&["src", "src/lib"]));
    let b = row(&rows, "src/lib/b.ts");
    assert_eq!(b.name, "b.ts");
    assert_eq!(b.kind, EntryKind::File);
    assert_eq!(b.parent.as_deref(), Some("src/lib"));
    assert_eq!((b.level, b.pos_in_set, b.set_size), (3, 1, 1));
    assert!(!b.expandable);
    let readme = row(&rows, "README.md");
    assert_eq!(readme.parent, None);
    assert_eq!((readme.pos_in_set, readme.set_size), (3, 3));
    assert!(row(&rows, "src").expandable);
}

#[test]
fn is_empty_until_the_root_is_read() {
    assert!(explorer_rows(&HashMap::new(), &BTreeSet::new()).is_empty());
}

#[test]
fn marks_changed_files_with_their_status_and_every_folder_above_them() {
    let marks = change_marks(&[
        change("src/lib/b.ts", ChangeStatus::Modified),
        change("new.ts", ChangeStatus::Untracked),
    ]);
    assert_eq!(
        marks.files.get("src/lib/b.ts"),
        Some(&ChangeStatus::Modified)
    );
    assert_eq!(marks.files.get("new.ts"), Some(&ChangeStatus::Untracked));
    let mut folders: Vec<&str> = marks.folders.iter().map(String::as_str).collect();
    folders.sort();
    assert_eq!(folders, vec!["src", "src/lib"]);
}

#[test]
fn is_the_path_and_a_space_a_folder_with_a_slash() {
    assert_eq!(
        drop_text("packages/web/a.ts", EntryKind::File),
        "packages/web/a.ts "
    );
    assert_eq!(
        drop_text("packages/web", EntryKind::Folder),
        "packages/web/ "
    );
    assert_eq!(
        drop_text(".github/@types/a+b-c_d.ts", EntryKind::File),
        ".github/@types/a+b-c_d.ts "
    );
}

#[test]
fn quotes_a_path_that_a_shell_would_split_or_expand() {
    assert_eq!(
        drop_text("my docs/a.md", EntryKind::File),
        "'my docs/a.md' "
    );
    assert_eq!(drop_text("it$s.md", EntryKind::File), "'it$s.md' ");
    assert_eq!(drop_text("it's.md", EntryKind::File), "'it'\\''s.md' ");
}

#[test]
fn names_the_task_in_the_type_in_lower_case_so_a_drop_target_can_check_it_before_the_drop() {
    let drag = DraggedPath {
        task_id: "T-0118".to_string(),
        path: "a.ts".to_string(),
        kind: EntryKind::File,
    };
    assert!(drag.accepts("T-0118"));
    assert!(drag.accepts("t-0118"));
    assert!(!drag.accepts("T-0119"));
}

#[test]
fn reads_the_root_first_and_a_folder_only_when_it_opens() {
    assert_eq!(shown_folders(&open(&[])), vec![""]);
    assert_eq!(shown_folders(&open(&["src"])), vec!["", "src"]);
    // An open folder inside a closed one waits.
    assert_eq!(shown_folders(&open(&["src/lib"])), vec![""]);
    assert_eq!(
        shown_folders(&open(&["src", "src/lib"])),
        vec!["", "src", "src/lib"]
    );
}

#[test]
fn opens_the_folders_around_the_open_file() {
    assert_eq!(folders_above("src/lib/b.ts"), vec!["src", "src/lib"]);
    assert!(folders_above("README.md").is_empty());
}

#[test]
fn moves_and_opens_with_the_keys_of_a_tree() {
    let rows = explorer_rows(&folders(), &open(&["src"]));
    // Right on a closed folder opens it; on an open one it goes to the first child.
    assert!(row(&rows, "src").expandable && row(&rows, "src").expanded);
    assert_eq!(first_child_key(&rows, "src").as_deref(), Some("src/lib"));
    // A file has no child.
    assert_eq!(first_child_key(&rows, "README.md"), None);
    // Left goes to the parent; a root row has none.
    assert_eq!(parent_key(&rows, "src/lib").as_deref(), Some("src"));
    assert_eq!(parent_key(&rows, "src"), None);
    assert_eq!(parent_key(&rows, "missing"), None);
}
