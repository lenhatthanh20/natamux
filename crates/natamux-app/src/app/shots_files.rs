//! Screen 6's `--shot` states: the Files tab, the Docs tab and the editor tab of the reviewed
//! task, and a drop target over a terminal. Drafts go into the draft store as an edit would put
//! them; the fake moves a file on disk for the conflict.

use std::time::Duration;

use gpui::{App, AsyncApp, Entity, WindowHandle};

use super::dashboard::Dashboard;
use super::fake_daemon;
use super::options::Shot;
use super::view_rules::DetailsView;
use super::window::Root;
use crate::files::draft_store::DraftStore;
use crate::files::file_drafts::FileDraft;
use crate::files::file_reads::keep_open_folders;
use crate::terminals::pane_drop::set_drop_target;
use crate::workspaces::docs_rules::DocDraft;

/// The reviewed task: its worktree, stage files and attachments are in `fake_files.json`.
const REVIEWED: &str = "T-0143";
/// The task of the terminals: Terminal 1 shows the drop outline.
const RUNNING: &str = "T-0146";
const DROP_TERMINAL: &str = "1";
const README: &str = "README.md";
const SOURCE: &str = "src/checkout.ts";
const SPEC: &str = "spec.md";
const TABS: [&str; 11] = [
    README,
    ".gitignore",
    ".natamux.json",
    "package.json",
    ".claude/settings.json",
    ".codex/config.toml",
    ".github/workflows/ci.yml",
    // Shares its name with the root README.md, so its tab shows its folder.
    ".github/README.md",
    "src/cart.ts",
    SOURCE,
    "src/format.ts",
];
/// The edits of the draft shots.
const SOURCE_EDIT: &str = "\n// Keep the total at zero or more, also with two coupons.\n";
const SPEC_EDIT: &str = "\n- A coupon equal to the cart total makes the total zero.\n";
/// The file is read and drawn within this.
const READ: Duration = Duration::from_millis(800);

pub fn is_files_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::DetailsFiles
            | Shot::WorkspaceDocs
            | Shot::WorkspaceDocsEdit
            | Shot::WorkspaceEditor
            | Shot::WorkspaceEditorSource
            | Shot::WorkspaceEditorDraft
            | Shot::WorkspaceEditorConflict
            | Shot::WorkspaceEditorTabs
            | Shot::TerminalDropTarget
    )
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let entered = handle.update(cx, |root, _, cx| {
        let dashboard = root.dashboard().clone();
        enter(shot, &dashboard, cx);
    });
    if entered.is_err() || shot != Shot::WorkspaceEditorConflict {
        return;
    }
    // The editor reads the draft's version first; then the agent writes the file.
    cx.background_executor().timer(READ).await;
    fake_daemon::shared().set_changed_on_disk(REVIEWED, SOURCE);
}

fn enter(shot: Shot, dashboard: &Entity<Dashboard>, cx: &mut App) {
    match shot {
        Shot::DetailsFiles => {
            keep_open_folders(REVIEWED, &["src".to_owned()].into(), cx);
            files_panel(dashboard, REVIEWED, cx);
        }
        Shot::WorkspaceDocs => {
            dashboard.update(cx, |dashboard, cx| {
                dashboard.open(REVIEWED, cx);
                dashboard.show_view(DetailsView::Docs, cx);
            });
        }
        Shot::WorkspaceDocsEdit => {
            doc_draft(REVIEWED, SPEC, cx);
            panel(dashboard, REVIEWED, DetailsView::Progress, cx);
            dashboard.update(cx, |dashboard, cx| {
                dashboard.open_stage_file(REVIEWED, SPEC, cx)
            });
        }
        Shot::WorkspaceEditor => {
            panel(dashboard, REVIEWED, DetailsView::Progress, cx);
            open_file(dashboard, README, cx);
        }
        Shot::WorkspaceEditorSource => {
            panel(dashboard, REVIEWED, DetailsView::Details, cx);
            open_file(dashboard, SOURCE, cx);
        }
        Shot::WorkspaceEditorDraft | Shot::WorkspaceEditorConflict => {
            file_draft(REVIEWED, SOURCE, cx);
            keep_open_folders(REVIEWED, &["src".to_owned()].into(), cx);
            files_panel(dashboard, REVIEWED, cx);
            open_file(dashboard, SOURCE, cx);
        }
        // T-0220: more tabs than one row holds, the active one with a draft.
        Shot::WorkspaceEditorTabs => {
            file_draft(REVIEWED, SOURCE, cx);
            panel(dashboard, REVIEWED, DetailsView::Progress, cx);
            for path in TABS {
                open_file(dashboard, path, cx);
            }
            open_file(dashboard, SOURCE, cx);
        }
        Shot::TerminalDropTarget => {
            set_drop_target(Some((RUNNING.to_owned(), DROP_TERMINAL.to_owned())), cx);
            files_panel(dashboard, RUNNING, cx);
        }
        _ => {}
    }
}

/// The task's workspace with its panel open on `view`.
fn panel(dashboard: &Entity<Dashboard>, id: &str, view: DetailsView, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(id, cx);
        dashboard.set_details_view(view, cx);
        dashboard.set_details_open(true, cx);
    });
}

fn files_panel(dashboard: &Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(id, cx);
        dashboard.show_files(id, cx);
    });
}

fn open_file(dashboard: &Entity<Dashboard>, path: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| dashboard.open_file(REVIEWED, path, cx));
}

/// An edit of the file on the version the fake has now.
fn file_draft(task: &str, path: &str, cx: &mut App) {
    let Ok(file) = fake_daemon::shared().read_workspace_file(task, path) else {
        return;
    };
    let draft = FileDraft {
        text: format!("{}{SOURCE_EDIT}", file.text.unwrap_or_default()),
        version: file.version,
    };
    let store = DraftStore::global(cx);
    store.update(cx, |store, cx| store.set_file(task, path, Some(draft), cx));
}

/// An edit of the stage file that began on its time now, so no stale line shows.
fn doc_draft(task: &str, name: &str, cx: &mut App) {
    let fake = fake_daemon::shared();
    let (Ok(file), Ok(list)) = (
        fake.read_stage_file(task, name),
        fake.list_stage_files(task),
    ) else {
        return;
    };
    let base = list
        .into_iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.modified_at);
    let draft = DocDraft {
        text: format!("{}{SPEC_EDIT}", file.text),
        original: file.text,
        base,
    };
    let store = DraftStore::global(cx);
    store.update(cx, |store, cx| store.set_doc(task, name, Some(draft), cx));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shots_name_files_the_fake_has() {
        let fake = fake_daemon::FakeDaemon::default();
        assert!(fake.read_workspace_file(REVIEWED, README).is_ok());
        let source = fake.read_workspace_file(REVIEWED, SOURCE).unwrap();
        assert!(source.text.is_some());
        assert!(fake.read_stage_file(REVIEWED, SPEC).is_ok());
        assert!(
            fake.list_stage_files(REVIEWED)
                .unwrap()
                .iter()
                .any(|file| file.name == SPEC)
        );
        assert!(fake.list_folder(REVIEWED, "src").is_ok());
        assert!(fake.list_folder(RUNNING, "").is_ok());
    }
}
