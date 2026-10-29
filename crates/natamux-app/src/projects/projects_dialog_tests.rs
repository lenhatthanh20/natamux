//! Browse… in the Projects dialog: what a picked folder does to its field (T-0225). The tests
//! call `took_folder` as the picker's answer would; the system picker itself is not run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use editor::Editor;
use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};
use natamux_client::DashboardEvent;

use super::{Pick, ProjectsDialog};
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;

fn dialog(
    cx: &mut TestAppContext,
) -> (
    Entity<Dashboard>,
    Entity<ProjectsDialog>,
    &mut VisualTestContext,
) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| ProjectsDialog::new(shown, window, cx));
    cx.run_until_parked();
    (dashboard, view, cx)
}

fn default_worktree(dashboard: &Entity<Dashboard>, dir: &str, cx: &mut VisualTestContext) {
    let event = DashboardEvent::Repos {
        repos: Vec::new(),
        worktrees_dir: Some(dir.to_owned()),
    };
    dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
    cx.run_until_parked();
}

fn texts(view: &Entity<ProjectsDialog>, cx: &mut VisualTestContext) -> (String, String) {
    view.read_with(cx, |view, cx| {
        let text = |editor: &Entity<Editor>| editor.read(cx).text(cx);
        (text(&view.folder), text(&view.worktree))
    })
}

#[gpui::test]
fn a_folder_pick_fills_the_folder_and_clears_its_error(cx: &mut TestAppContext) {
    let (_, view, cx) = dialog(cx);
    view.update_in(cx, |view, window, cx| {
        view.folder_error = Some("Not a git repository".into());
        view.picking = Some(Pick::Folder);
        view.took_folder(
            Pick::Folder,
            Some(PathBuf::from("/Users/x/repo")),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(texts(&view, cx).0, "/Users/x/repo");
    view.update_in(cx, |view, window, cx| {
        assert_eq!(view.folder_error, None);
        assert!(view.folder_touched);
        assert_eq!(view.picking, None);
        assert!(view.folder.focus_handle(cx).is_focused(window));
    });
}

#[gpui::test]
fn a_worktree_pick_stays_when_the_projects_change(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = dialog(cx);
    default_worktree(&dashboard, "/daemon/worktrees", cx);
    assert_eq!(texts(&view, cx).1, "/daemon/worktrees");

    view.update_in(cx, |view, window, cx| {
        view.worktree_error = Some("Use a full path".into());
        view.took_folder(
            Pick::Worktree,
            Some(PathBuf::from("/Users/x/trees")),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    default_worktree(&dashboard, "/daemon/other", cx);

    assert_eq!(texts(&view, cx).1, "/Users/x/trees");
    view.read_with(cx, |view, _| {
        assert_eq!(view.typed_worktree.as_deref(), Some("/Users/x/trees"));
        assert_eq!(view.worktree_error, None);
        assert!(view.worktree_touched);
    });
}

#[gpui::test]
fn a_cancel_changes_nothing(cx: &mut TestAppContext) {
    let (dashboard, view, cx) = dialog(cx);
    default_worktree(&dashboard, "/daemon/worktrees", cx);
    view.update_in(cx, |view, window, cx| {
        view.folder_error = Some("Not a git repository".into());
        view.picking = Some(Pick::Folder);
        view.took_folder(Pick::Folder, None, window, cx);
        view.took_folder(Pick::Worktree, None, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        texts(&view, cx),
        (String::new(), "/daemon/worktrees".into())
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.folder_error.as_deref(), Some("Not a git repository"));
        assert!(!view.folder_touched);
        assert_eq!(view.typed_worktree, None);
        assert_eq!(view.picking, None);
    });
}
