//! What Root follows in the window: the close button, the app's quit, the system look and the
//! perf overlay's refresh.

use std::time::Duration;

use ui::prelude::*;

use super::Root;
use crate::components::motion;
use crate::perf::overlay;

/// How often the perf overlay shows new numbers.
const OVERLAY_REFRESH: Duration = Duration::from_millis(500);

impl Root {
    /// The bench's quit handling, the system look and the perf overlay's refresh.
    pub(super) fn watch_window(window: &mut Window, cx: &mut Context<Self>) {
        // Unsaved edits ask first; a running bench quits the app (`window_files.rs`).
        let weak_root = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak_root
                .update(cx, |root, cx| root.window_may_close(window, cx))
                .unwrap_or(true)
        });
        // A bench left running would flood the daemon after the app is gone.
        cx.on_app_quit(|root, cx| {
            let stop = root.take_bench_stop(cx);
            async move {
                if let Some(stop) = stop {
                    stop.await;
                }
            }
        })
        .detach();
        // Motion runs only while the window has focus, as the web's `lib/window-focus.ts`.
        motion::set_active(window.is_window_active(), cx);
        cx.observe_window_activation(window, |_, window, cx| {
            motion::set_active(window.is_window_active(), cx);
        })
        .detach();
        // Follow the system's light or dark look, as Zed's workspace does.
        cx.observe_window_appearance(window, |_, window, cx| {
            *theme::SystemAppearance::global_mut(cx) =
                theme::SystemAppearance(window.appearance().into());
            theme_settings::reload_theme(cx);
        })
        .detach();
        cx.spawn(async move |root, cx| {
            loop {
                cx.background_executor().timer(OVERLAY_REFRESH).await;
                let refreshed = root.update(cx, |_, cx| {
                    if overlay::enabled(cx) {
                        cx.notify();
                    }
                });
                if refreshed.is_err() {
                    break;
                }
            }
        })
        .detach();
        if overlay::enabled(cx) {
            overlay::watch_frames(window);
        }
    }
}
