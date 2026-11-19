//! A uniform_list lays each row out as a root: a commit row must still fit the list, so a long
//! subject truncates and the age stays in view, in the narrow panel and over main (T-0231).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui::TestAppContext;
use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit};
use ui::prelude::*;

use super::HistoryView;
use crate::app::test_views;

struct Frame {
    width: f32,
    in_main: bool,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let commit = GraphCommit {
            hash: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            author: "A long author name".to_owned(),
            date: "2026-10-30T00:00:00Z".to_owned(),
            subject: "Merge pull request #534 from someone/a-very-long-branch-name ".repeat(4),
            refs: vec![
                "natamux/t-0230-fix-bug-diff-view".to_owned(),
                "tag: v1.0.8".to_owned(),
            ],
            on_branch: true,
            ..GraphCommit::default()
        };
        let graph = GitGraph {
            head: commit.hash.clone(),
            commits: vec![commit],
            ..GitGraph::default()
        };
        let nothing = || Rc::new(|_: &mut Window, _: &mut App| {});
        div().w(px(self.width)).h(px(400.)).child(HistoryView {
            id: "T-0001".to_owned(),
            graph: Some(Rc::new(graph)),
            error: None,
            uncommitted: 0,
            in_main: self.in_main,
            main_shows: self.in_main,
            on_show_changes: nothing(),
            on_refresh: nothing(),
            on_toggle_main: nothing(),
        })
    }
}

fn row_fits(width: f32, in_main: bool, cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_frame, cx) = cx.add_window_view(|_, _| Frame { width, in_main });
    cx.run_until_parked();

    let age = cx
        .debug_bounds("history-age-0")
        .expect("the age was not drawn");
    let subject = cx
        .debug_bounds("history-subject-0")
        .expect("the subject was not drawn");
    // Inside the frame's 16 px padding: the list does not cut the age off.
    assert!(
        age.right() <= px(width - 16.),
        "age ends at {:?}",
        age.right()
    );
    assert!(
        subject.right() <= age.left(),
        "subject runs to {:?}",
        subject.right()
    );
}

#[gpui::test]
fn a_long_commit_fits_the_narrow_panel(cx: &mut TestAppContext) {
    row_fits(320., false, cx);
}

#[gpui::test]
fn a_long_commit_fits_main(cx: &mut TestAppContext) {
    row_fits(700., true, cx);
}

/// Lays the same graph out on two frames and keeps what each frame got.
struct Twice {
    graph: Rc<GitGraph>,
    seen: Rc<RefCell<Vec<Rc<GitGraph>>>>,
}

impl Render for Twice {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = super::layout_of("T-0002", &self.graph, window, cx);
        self.seen.borrow_mut().push(layout.graph);
        div()
    }
}

#[gpui::test]
fn the_layout_is_kept_by_pointer_between_frames(cx: &mut TestAppContext) {
    test_views::init(cx);
    let graph = Rc::new(GitGraph::default());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (_view, cx) = cx.add_window_view(|_, _| Twice {
        graph: graph.clone(),
        seen: seen.clone(),
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let seen = seen.borrow();
    assert!(seen.len() >= 2, "drew {} times", seen.len());
    // The kept layout holds the shown graph itself, not a copy of it.
    assert!(seen.iter().all(|kept| Rc::ptr_eq(kept, &graph)));
}
