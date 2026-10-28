//! `--count-draws`: counts the window's draws and prints them once a second, so CI can measure
//! what motion costs (`scripts/motion-cost.sh`). Unlike the perf overlay, it forces no frame.
//! `--open-task` opens a task for the same measure, with no shot: motion and the clock run.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{App, AppContext as _, WindowHandle};

use crate::app::window::Root;

/// What `--count-draws` prints.
static DRAWS: AtomicU64 = AtomicU64::new(0);
/// What `take` gives: a count of its own, so `--count-draws` and `--gate2` do not reset each
/// other's.
static TAKEN: AtomicU64 = AtomicU64::new(0);

/// How long `--open-task` waits for the dashboard to list the task.
const TASK_WAIT: Duration = Duration::from_secs(15);
const TASK_LOOK_EVERY: Duration = Duration::from_millis(100);

/// `Root` is not a cached view, so it renders once per window draw.
pub fn count() {
    DRAWS.fetch_add(1, Ordering::Relaxed);
    TAKEN.fetch_add(1, Ordering::Relaxed);
}

/// The draws since the last call, for `--gate2`'s hidden phase.
pub fn take() -> u64 {
    TAKEN.swap(0, Ordering::Relaxed)
}

/// The draws since the last print.
fn printed() -> u64 {
    DRAWS.swap(0, Ordering::Relaxed)
}

/// Prints `draws <n> active <true|false>` once a second.
pub fn start(handle: WindowHandle<Root>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let draws = printed();
            let active = cx
                .update_window(handle.into(), |_, window, _| window.is_window_active())
                .unwrap_or(false);
            println!("draws {draws} active {active}");
        }
    })
    .detach();
}

/// `--open-task`: opens the task once the dashboard has it. A task it never lists only prints a
/// line on stderr: the app goes on, and `motion-cost.sh` fails on that line.
pub fn open_task(handle: WindowHandle<Root>, task_id: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let started = Instant::now();
        while started.elapsed() < TASK_WAIT {
            let opened = handle.update(cx, |root, _, cx| {
                let dashboard = root.dashboard().clone();
                let known = dashboard.read(cx).data.task(&task_id).is_some();
                if known {
                    dashboard.update(cx, |dashboard, cx| dashboard.open(&task_id, cx));
                }
                known
            });
            if opened.unwrap_or(true) {
                return;
            }
            cx.background_executor().timer(TASK_LOOK_EVERY).await;
        }
        eprintln!("open-task: the dashboard has no task {task_id}");
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    // Review of T-0210: one counter for both made `--gate2 --count-draws` report about 1/20 of
    // the draws. Window tests may draw meanwhile, so the counts are at least, not exactly.
    #[test]
    fn the_printer_and_take_do_not_reset_each_other() {
        count();
        count();
        assert!(printed() >= 2);
        assert!(take() >= 2);
        count();
        count();
        assert!(take() >= 2);
        assert!(printed() >= 2);
    }
}
