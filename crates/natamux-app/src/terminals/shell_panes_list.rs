//! The list read that confirms a listed shell before its pane makes a terminal (T-0211). The web
//! grid waits for a read after mount for the same reason (`isFetchedAfterMount`).

use std::time::{Duration, Instant};

use ui::prelude::*;

use super::ShellPanes;
use crate::terminals::terminal_cache::{TerminalCache, terminal_key};
use crate::terminals::terminal_tabs::unconfirmed_shells;

/// The grid draws right after an answer while it is on screen. An answer older than this came
/// while another view filled the window, and its list may be stale again by now.
const ANSWER_FRESH: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReadState {
    #[default]
    Idle,
    /// The read with this number runs.
    Reading(u64),
    /// The answer came at this time, or the read failed: the grid goes on with the list it has.
    Answered(Instant),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ListRead {
    /// Reads started, so an answer knows whether the grid still waits for it.
    started: u64,
    state: ReadState,
}

impl ListRead {
    /// No shell waits, or another task: a read still out counts for nothing.
    pub(super) fn reset(&mut self) {
        self.state = ReadState::Idle;
    }
}

impl ShellPanes {
    /// The list read while another task was shown may still have a shell the daemon closed
    /// meanwhile, such as a stage that ends its shell. A new terminal for it would start a new
    /// shell under its name, so the grid reads the list once more and waits for the answer.
    pub(super) fn waits_for_list(
        &mut self,
        task_id: &str,
        visible: &[String],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(list) = self.list(cx) else {
            return false;
        };
        let unconfirmed = {
            let app: &App = cx;
            unconfirmed_shells(&list, visible, |name| {
                TerminalCache::cached(&terminal_key(task_id, name), app).is_some()
            })
        };
        let now = cx.background_executor().now();
        match (unconfirmed.is_empty(), self.list_read.state) {
            (true, _) => {
                self.list_read.reset();
                false
            }
            (false, ReadState::Reading(_)) => true,
            (false, ReadState::Answered(at))
                if now.saturating_duration_since(at) <= ANSWER_FRESH =>
            {
                self.list_read.reset();
                false
            }
            (false, ReadState::Idle | ReadState::Answered(_)) => {
                self.confirm_list(task_id, cx);
                true
            }
        }
    }

    fn confirm_list(&mut self, task_id: &str, cx: &mut Context<Self>) {
        self.list_read.started += 1;
        let id = self.list_read.started;
        self.list_read.state = ReadState::Reading(id);
        let read = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.read_terminals(task_id, cx));
        cx.spawn(async move |panes, cx| {
            let _ = read.await;
            panes
                .update(cx, |panes, cx| {
                    // Only the read the grid still waits for counts. Once it stopped waiting (no
                    // shell to confirm, another task), this list may be older than the next close.
                    if panes.list_read.state == ReadState::Reading(id) {
                        panes.list_read.state = ReadState::Answered(cx.background_executor().now());
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
    }
}
