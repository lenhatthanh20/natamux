//! The board builds only the cards on screen (T-0223, P2).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, ListOffset, TestAppContext, px};
use natamux_proto::tasks::task::{Task, TaskState};

use super::TaskBoard;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;

fn running(n: usize) -> Task {
    Task {
        id: format!("T-{n:04}"),
        title: format!("Task {n}"),
        state: TaskState::Running,
        ..Task::default()
    }
}

#[gpui::test]
fn a_column_of_two_hundred_tasks_builds_few_cards_and_keeps_its_scroll_top(
    cx: &mut TestAppContext,
) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (board, cx) = cx.add_window_view(move |_, cx| TaskBoard::new(dashboard, cx));
    let mut tasks: Vec<Task> = (0..200).map(running).collect();
    board.update(cx, |board, cx| board.set_rows(tasks.clone(), cx));
    cx.run_until_parked();
    let built = board.read_with(cx, |board, _| board.built.get());
    assert!(built > 0 && built < 100, "built {built} cards");

    let list = board
        .read_with(cx, |board, _| board.list_for(TaskState::Running))
        .expect("the Running column has a list");
    list.scroll_to(ListOffset {
        item_ix: 50,
        offset_in_item: px(0.),
    });
    cx.run_until_parked();
    tasks.push(running(200));
    board.update(cx, |board, cx| board.set_rows(tasks, cx));
    cx.run_until_parked();
    assert_eq!(list.item_count(), 201);
    assert_eq!(list.logical_scroll_top().item_ix, 50);
}
