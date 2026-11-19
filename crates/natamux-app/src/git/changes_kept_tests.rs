//! The file panel keeps its rows between frames and draws only the rows in view. What it keeps
//! must match a fresh build after a mode change, a fold, a pick and a new list.

use gpui::{Entity, TestAppContext, VisualTestContext, WeakEntity};

use super::*;
use crate::app::test_views;

const TASK: &str = "T-0143";
const FILES: usize = 2_000;

fn entry(path: String) -> ChangeEntry {
    ChangeEntry {
        path,
        status: ChangeStatus::Modified,
        original_path: None,
        added: Some(1),
        removed: Some(0),
    }
}

fn many(count: usize) -> Vec<ChangeEntry> {
    (0..count)
        .map(|n| entry(format!("src/part{}/file{n}.ts", n % 20)))
        .collect()
}

/// Draws the file panel of the view, as the details panel does.
struct Panel {
    changes: Entity<ChangesView>,
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let files = self
            .changes
            .update(cx, |changes, cx| changes.render_panel(window, cx));
        div().size_full().child(files)
    }
}

fn panel(cx: &mut TestAppContext) -> (Entity<ChangesView>, &mut VisualTestContext) {
    let (panel, _, cx) = test_views::init_app(cx, |dashboard, _, cx| {
        let workspace = WeakEntity::new_invalid();
        let changes = cx.new(|cx| ChangesView::new(dashboard, workspace, cx));
        cx.new(|_| Panel { changes })
    });
    let changes = panel.read_with(cx, |panel, _| panel.changes.clone());
    changes.update(cx, |changes, cx| {
        changes.set_list(TASK, Some(many(FILES)), None, cx)
    });
    cx.run_until_parked();
    (changes, cx)
}

/// One more frame: the row builds so far.
fn frame(changes: &Entity<ChangesView>, cx: &mut VisualTestContext) -> usize {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    changes.read_with(cx, |changes, _| changes.kept.builds)
}

fn shape(rows: &[RowFacts]) -> Vec<(String, usize, bool)> {
    rows.iter()
        .map(|row| (row.key.clone(), row.depth, row.expanded))
        .collect()
}

fn fresh_tree(list: &[ChangeEntry], folded: &[&str]) -> Vec<(String, usize, bool)> {
    let folded: BTreeSet<String> = folded.iter().map(|key| key.to_string()).collect();
    flatten_tree(&build_change_tree(list, true), &folded)
        .into_iter()
        .map(|row| (row.key, row.level - 1, row.expanded))
        .collect()
}

fn kept_rows(
    changes: &Entity<ChangesView>,
    cx: &mut VisualTestContext,
) -> Vec<(String, usize, bool)> {
    changes.update(cx, |changes, cx| shape(&changes.rows(cx)))
}

fn kept_order(changes: &Entity<ChangesView>, cx: &mut VisualTestContext) -> Vec<String> {
    changes.update(cx, |changes, cx| changes.order(cx).to_vec())
}

#[gpui::test]
fn the_panel_builds_the_rows_once(cx: &mut TestAppContext) {
    let (changes, cx) = panel(cx);
    for _ in 0..3 {
        assert_eq!(
            frame(&changes, cx),
            1,
            "the rows were built again for a frame"
        );
    }
}

#[gpui::test]
fn the_kept_rows_match_a_fresh_build(cx: &mut TestAppContext) {
    let (changes, cx) = panel(cx);
    let dashboard = changes.read_with(cx, |changes, _| changes.dashboard.clone());
    let list = many(FILES);
    assert_eq!(frame(&changes, cx), 1);
    let listed: Vec<_> = list_rows(&list)
        .into_iter()
        .map(|row| (row.key, row.level - 1, row.expanded))
        .collect();
    assert_eq!(kept_rows(&changes, cx), listed);
    assert_eq!(
        kept_order(&changes, cx),
        file_order(&list, FileViewMode::List)
    );

    dashboard.update(cx, |dashboard, cx| {
        dashboard.set_changes_view(FileViewMode::Tree, cx)
    });
    assert_eq!(frame(&changes, cx), 2, "a new mode builds the rows");
    assert_eq!(kept_rows(&changes, cx), fresh_tree(&list, &[]));
    assert_eq!(
        kept_order(&changes, cx),
        file_order(&list, FileViewMode::Tree)
    );

    changes.update(cx, |changes, _| {
        let folded = BTreeSet::from(["d:src/part3".to_owned()]);
        changes.folded.insert(TASK.to_owned(), folded);
    });
    assert_eq!(frame(&changes, cx), 3, "a fold builds the rows");
    assert_eq!(kept_rows(&changes, cx), fresh_tree(&list, &["d:src/part3"]));

    // The picked file's folders stay open.
    changes.update(cx, |changes, _| {
        changes.picked = Some("src/part3/file3.ts".to_owned());
    });
    assert_eq!(frame(&changes, cx), 4, "a pick builds the rows");
    assert_eq!(kept_rows(&changes, cx), fresh_tree(&list, &[]));

    let fewer = many(50);
    changes.update(cx, |changes, cx| {
        changes.set_list(TASK, Some(fewer.clone()), None, cx)
    });
    // A new list drops the kept state, its build count too.
    assert_eq!(frame(&changes, cx), 1, "a new list builds the rows");
    assert_eq!(kept_rows(&changes, cx), fresh_tree(&fewer, &[]));
    assert_eq!(
        kept_order(&changes, cx),
        file_order(&fewer, FileViewMode::Tree)
    );

    changes.update(cx, |changes, cx| {
        changes.set_list(TASK, Some(fewer.clone()), None, cx)
    });
    assert_eq!(frame(&changes, cx), 1, "the same list keeps the rows");
}

#[test]
fn a_folder_row_finds_its_folder_in_the_tree() {
    let list: Vec<ChangeEntry> = ["a/b/c/x", "a/b/c/y", "a/d", "e/f/g", "h"]
        .into_iter()
        .map(|path| entry(path.to_owned()))
        .collect();
    let tree = build_change_tree(&list, true);
    let rows = flatten_tree(&tree, &BTreeSet::new());
    let mut folders = 0;
    for row in &rows {
        if let TreeNode::Folder(folder) = row.node.as_ref() {
            folders += 1;
            let found = find_folder(&tree, &folder.key);
            assert_eq!(found, Some(row.node.as_ref()), "{}", folder.key);
            assert_eq!(
                found.map(entries_under),
                Some(entries_under(row.node.as_ref()))
            );
        }
    }
    assert_eq!(folders, 3, "a, b/c and e/f");
}
