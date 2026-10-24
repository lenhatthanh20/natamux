//! `--count-draws`: counts the window's draws and prints them once a second, so CI can measure
//! what motion costs (`scripts/motion-cost.sh`). Unlike the perf overlay, it forces no frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::{App, AppContext as _, WindowHandle};

use crate::app::window::Root;

static DRAWS: AtomicU64 = AtomicU64::new(0);

/// `Root` is not a cached view, so it renders once per window draw.
pub fn count() {
    DRAWS.fetch_add(1, Ordering::Relaxed);
}

/// Prints `draws <n> active <true|false>` once a second.
pub fn start(handle: WindowHandle<Root>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let draws = DRAWS.swap(0, Ordering::Relaxed);
            let active = cx
                .update_window(handle.into(), |_, window, _| window.is_window_active())
                .unwrap_or(false);
            println!("draws {draws} active {active}");
        }
    })
    .detach();
}
