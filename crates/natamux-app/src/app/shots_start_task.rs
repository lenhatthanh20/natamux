//! The Start a task `--shot` states of option B (T-0292): the dialog filled with files and an
//! edited branch (top and end), no worktree, One agent with no agent, and the daemon's error.
//! The runner's screen is 1024x768, so the states that show the lower cards scroll the body.

use std::time::Duration;

use gpui::{AsyncApp, WindowHandle};

use super::options::Shot;
use super::window::Root;
use crate::workspaces::new_workspace_dialog::{self, StartShot};

/// The dialog reads the project's branches and settings within this.
const READ: Duration = Duration::from_millis(600);
/// The filled dialog lays out within this, so its scroll end is known.
const LAYOUT: Duration = Duration::from_millis(300);

pub fn is_start_task_shot(shot: Shot) -> bool {
    fill_of(shot).is_some()
}

/// What the state puts in the dialog, and whether it scrolls to the end.
fn fill_of(shot: Shot) -> Option<(StartShot, bool)> {
    match shot {
        Shot::StartTaskFilled => Some((StartShot::Filled, false)),
        Shot::StartTaskFilledEnd => Some((StartShot::Filled, true)),
        Shot::StartTaskNoWorktree => Some((StartShot::NoWorktree, true)),
        Shot::StartTaskOneAgent => Some((StartShot::OneAgentNoAgent, true)),
        Shot::StartTaskError => Some((StartShot::Error, false)),
        _ => None,
    }
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let Some((fill, to_end)) = fill_of(shot) else {
        return;
    };
    let Ok(Some(dialog)) = handle.update(cx, |root, window, cx| {
        let dashboard = root.dashboard().clone();
        new_workspace_dialog::open_with(&dashboard, None, None, window, cx)
    }) else {
        return;
    };
    cx.background_executor().timer(READ).await;
    handle
        .update(cx, |_, window, cx| {
            dialog.update(cx, |dialog, cx| dialog.fill_for_shot(fill, window, cx))
        })
        .ok();
    if !to_end {
        return;
    }
    cx.background_executor().timer(LAYOUT).await;
    handle
        .update(cx, |_, _, cx| {
            dialog.update(cx, |dialog, cx| dialog.scroll_to_end(cx))
        })
        .ok();
}
