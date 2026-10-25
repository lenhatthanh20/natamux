use super::*;

fn entry(path: &str) -> ChangeEntry {
    with_status(path, ChangeStatus::Modified)
}

fn with_status(path: &str, status: ChangeStatus) -> ChangeEntry {
    ChangeEntry {
        path: path.into(),
        status,
        original_path: None,
        added: None,
        removed: None,
    }
}

fn counted(
    path: &str,
    status: ChangeStatus,
    added: Option<u64>,
    removed: Option<u64>,
) -> ChangeEntry {
    ChangeEntry {
        added,
        removed,
        ..with_status(path, status)
    }
}

fn folder(node: &TreeNode) -> &TreeFolder {
    match node {
        TreeNode::Folder(folder) => folder,
        TreeNode::File(_) => panic!("Expected a folder."),
    }
}

fn names(nodes: &[TreeNode]) -> Vec<&str> {
    nodes.iter().map(TreeNode::name).collect()
}

fn keys<'a>(rows: &'a [TreeRow]) -> Vec<&'a str> {
    rows.iter().map(|row| row.key.as_str()).collect()
}

fn collapsed(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

#[test]
fn puts_a_file_at_the_top_as_one_file_node() {
    let tree = build_change_tree(&[entry("a.ts")], true);
    assert_eq!(tree.len(), 1);
    match &tree[0] {
        TreeNode::File(file) => {
            assert_eq!(
                (file.key.as_str(), file.name.as_str(), file.depth),
                ("a.ts", "a.ts", 0)
            );
        }
        TreeNode::Folder(_) => panic!("Expected a file."),
    }
}

#[test]
fn joins_a_chain_of_single_folders_into_one_row() {
    let tree = build_change_tree(
        &[
            entry("packages/web/src/a.ts"),
            entry("packages/web/src/b.ts"),
        ],
        true,
    );
    assert_eq!(tree.len(), 1);
    let src = folder(&tree[0]);
    assert_eq!(src.key, "packages/web/src");
    assert_eq!(src.name, "packages/web/src");
    assert_eq!(src.depth, 0);
    assert_eq!(names(&src.children), ["a.ts", "b.ts"]);
    assert_eq!(src.children[0].depth(), 1);
}

#[test]
fn stops_the_chain_at_a_folder_that_holds_a_file() {
    let tree = build_change_tree(&[entry("src/x.ts"), entry("src/a/b/y.ts")], true);
    let src = folder(&tree[0]);
    assert_eq!((src.key.as_str(), src.name.as_str()), ("src", "src"));
    assert_eq!(names(&src.children), ["a/b", "x.ts"]);
    let ab = folder(&src.children[0]);
    assert_eq!((ab.key.as_str(), ab.depth), ("src/a/b", 1));
    assert_eq!(names(&ab.children), ["y.ts"]);
}

#[test]
fn gives_one_row_per_folder_without_compaction() {
    let tree = build_change_tree(&[entry("a/b/c.ts")], false);
    let a = folder(&tree[0]);
    let b = folder(&a.children[0]);
    assert_eq!(
        [
            a.key.as_str(),
            a.name.as_str(),
            b.key.as_str(),
            b.name.as_str()
        ],
        ["a", "a", "a/b", "b"]
    );
    match &b.children[0] {
        TreeNode::File(file) => assert_eq!((file.key.as_str(), file.depth), ("a/b/c.ts", 2)),
        TreeNode::Folder(_) => panic!("Expected a file."),
    }
}

#[test]
fn puts_folders_first_then_sorts_naturally_and_without_case() {
    let top = |paths: &[&str]| -> Vec<String> {
        let list: Vec<ChangeEntry> = paths.iter().map(|path| entry(path)).collect();
        names(&build_change_tree(&list, true))
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(top(&["z.ts", "lib/a.ts"]), ["lib", "z.ts"]);
    assert_eq!(top(&["file10.ts", "file2.ts"]), ["file2.ts", "file10.ts"]);
    assert_eq!(top(&["b.ts", "A.ts"]), ["A.ts", "b.ts"]);
    assert_eq!(top(&["a.ts", "A.ts"]), top(&["A.ts", "a.ts"]));
}

#[test]
fn gives_a_folder_the_strongest_status_below_it() {
    use ChangeStatus::{Added, Conflict, Deleted, Modified, Untracked};
    let status_of = |statuses: &[ChangeStatus]| -> ChangeStatus {
        let list: Vec<ChangeEntry> = statuses
            .iter()
            .enumerate()
            .map(|(index, status)| with_status(&format!("d/{index}.ts"), *status))
            .collect();
        folder(&build_change_tree(&list, true)[0]).status
    };
    assert_eq!(status_of(&[Modified, Conflict]), Conflict);
    assert_eq!(status_of(&[Deleted, Modified]), Deleted);
    assert_eq!(status_of(&[Untracked, Added]), Added);
}

#[test]
fn counts_the_files_and_sums_the_lines_below_a_folder() {
    use ChangeStatus::Modified;
    let tree = build_change_tree(
        &[
            counted("src/a.ts", Modified, Some(3), Some(1)),
            counted("src/lib/b.ts", Modified, Some(2), None),
            counted("src/c.png", Modified, None, None),
            entry("src/d.ts"),
        ],
        true,
    );
    let src = folder(&tree[0]);
    assert_eq!(
        (src.file_count, src.added, src.removed),
        (4, Some(5), Some(1))
    );

    let unknown = build_change_tree(
        &[
            entry("src/a.ts"),
            counted("src/b.png", Modified, None, None),
        ],
        true,
    );
    let src = folder(&unknown[0]);
    assert_eq!((src.file_count, src.added, src.removed), (2, None, None));
}

#[test]
fn places_a_renamed_file_at_its_new_path() {
    let renamed = ChangeEntry {
        original_path: Some("old/old.ts".into()),
        ..with_status("src/new.ts", ChangeStatus::Renamed)
    };
    let tree = build_change_tree(&[renamed], true);
    assert_eq!(names(&tree), ["src"]);
    assert_eq!(folder(&tree[0]).children[0].key(), "src/new.ts");
}

fn sample() -> Vec<TreeNode> {
    build_change_tree(
        &[entry("src/a.ts"), entry("src/lib/b.ts"), entry("README.md")],
        true,
    )
}

#[test]
fn lists_the_rows_with_their_place_in_the_tree() {
    let tree = sample();
    let rows = flatten_tree(&tree, &collapsed(&["d:src/lib"]));
    assert_eq!(
        keys(&rows),
        ["d:src", "d:src/lib", "f:src/a.ts", "f:README.md"]
    );
    let shape = |row: &TreeRow| {
        (
            row.parent.clone(),
            row.level,
            row.pos_in_set,
            row.set_size,
            row.expandable,
            row.expanded,
        )
    };
    assert_eq!(shape(&rows[0]), (None, 1, 1, 2, true, true));
    assert_eq!(
        shape(&rows[1]),
        (Some("d:src".into()), 2, 1, 2, true, false)
    );
    assert_eq!(
        shape(&rows[2]),
        (Some("d:src".into()), 2, 2, 2, false, false)
    );
    assert_eq!(
        (rows[3].parent.clone(), rows[3].level, rows[3].pos_in_set),
        (None, 1, 2)
    );
    assert_eq!(rows[3].set_size, 2);
}

#[test]
fn leaves_out_everything_below_a_collapsed_folder() {
    let tree = sample();
    let rows = flatten_tree(&tree, &collapsed(&["d:src"]));
    assert_eq!(keys(&rows), ["d:src", "f:README.md"]);
}

#[test]
fn gives_every_folder_key() {
    assert_eq!(all_folder_keys(&sample()), ["d:src", "d:src/lib"]);
}

#[test]
fn keeps_the_order_given_all_at_the_first_level() {
    let rows = list_rows(&[entry("b/z.ts"), entry("a.ts")]);
    let shape: Vec<_> = rows
        .iter()
        .map(|row| (row.key.as_str(), row.level, row.pos_in_set, row.set_size))
        .collect();
    assert_eq!(shape, [("f:b/z.ts", 1, 1, 2), ("f:a.ts", 1, 2, 2)]);
    assert_eq!(rows[0].node.name(), "z.ts");
}

fn order_list() -> Vec<ChangeEntry> {
    ["z.ts", "src/b.ts", "src/lib/a.ts", "a.ts"]
        .iter()
        .map(|path| entry(path))
        .collect()
}

#[test]
fn goes_depth_first_folders_first_in_tree_mode() {
    assert_eq!(
        file_order(&order_list(), FileViewMode::Tree),
        ["src/lib/a.ts", "src/b.ts", "a.ts", "z.ts"]
    );
}

#[test]
fn keeps_the_order_given_in_list_mode() {
    assert_eq!(
        file_order(&order_list(), FileViewMode::List),
        ["z.ts", "src/b.ts", "src/lib/a.ts", "a.ts"]
    );
}

#[test]
fn gives_the_compacted_folders_around_a_path_outermost_first() {
    let tree = build_change_tree(
        &[
            entry("packages/web/src/a.ts"),
            entry("packages/web/src/lib/b.ts"),
            entry("packages/web/src/lib/c.ts"),
        ],
        true,
    );
    assert_eq!(
        ancestor_keys(&tree, "packages/web/src/lib/b.ts"),
        ["d:packages/web/src", "d:packages/web/src/lib"]
    );
    assert!(ancestor_keys(&tree, "other.ts").is_empty());
}

#[test]
fn sums_the_entries_that_have_counts() {
    use ChangeStatus::{Modified, Untracked};
    let total = total_counts(&[
        counted("a.ts", Modified, Some(4), Some(2)),
        entry("b.ts"),
        counted("c.ts", Untracked, Some(1), Some(0)),
    ]);
    assert_eq!(
        total,
        Some(LineCounts {
            added: 5,
            removed: 2
        })
    );
    assert_eq!(total_counts(&[entry("a.ts")]), None);
}

#[test]
fn counts_a_missing_removed_as_zero_and_a_missing_added_as_unknown() {
    let only_added = counted("a.ts", ChangeStatus::Modified, Some(2), None);
    assert_eq!(
        entry_counts(&only_added),
        Some(LineCounts {
            added: 2,
            removed: 0
        })
    );
    assert_eq!(
        entry_counts(&counted("a.ts", ChangeStatus::Modified, None, Some(3))),
        None
    );
}

#[test]
fn compares_names_naturally_without_case_and_by_code_point_on_a_tie() {
    assert_eq!(compare_names("file2", "file10"), Ordering::Less);
    assert_eq!(compare_names("b", "A"), Ordering::Greater);
    assert_eq!(compare_names("A.ts", "a.ts"), Ordering::Less);
    assert_eq!(compare_names("a", "a"), Ordering::Equal);
    assert_eq!(compare_names("v2.1", "v2.10"), Ordering::Less);
    assert_eq!(compare_names("a1", "a01"), Ordering::Greater);
    assert_eq!(compare_names("ab", "abc"), Ordering::Less);
}

#[test]
fn reads_any_view_value_but_tree_as_the_list() {
    assert_eq!(FileViewMode::parse("tree"), FileViewMode::Tree);
    assert_eq!(FileViewMode::parse("list"), FileViewMode::List);
    assert_eq!(FileViewMode::parse("grid"), FileViewMode::List);
    assert_eq!(FileViewMode::Tree.name(), "tree");
    assert_eq!(FileViewMode::List.name(), "list");
}

#[test]
fn gives_the_files_a_row_stands_for() {
    let tree = sample();
    let src = folder(&tree[0]);
    let paths: Vec<String> = entries_under(&tree[0])
        .into_iter()
        .map(|e| e.path)
        .collect();
    assert_eq!(paths, ["src/lib/b.ts", "src/a.ts"]);
    assert_eq!(src.file_count, 2);
    assert_eq!(entries_under(&src.children[1]).len(), 1);
}
