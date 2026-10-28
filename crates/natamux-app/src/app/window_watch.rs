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
    /// The bench's quit handling and the system look.
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
        cx.observe_window_activation(window, |root, window, cx| {
            let active = window.is_window_active();
            motion::set_active(active, cx);
            root.dashboard
                .update(cx, |dashboard, _| dashboard.set_feed_active(active));
            if active {
                root.workspace.update(cx, |view, cx| view.read_missed(cx));
                root.details.update(cx, |panel, cx| panel.read_missed(cx));
            }
        })
        .detach();
        // Follow the system's light or dark look, as Zed's workspace does.
        cx.observe_window_appearance(window, |_, window, cx| {
            *theme::SystemAppearance::global_mut(cx) =
                theme::SystemAppearance(window.appearance().into());
            theme_settings::reload_theme(cx);
        })
        .detach();
        if overlay::enabled(cx) {
            overlay::watch_frames(window);
        }
    }

    /// The overlay shows new numbers while it is on; off, nothing wakes for it.
    pub(crate) fn follow_overlay(&mut self, cx: &mut Context<Self>) {
        self.overlay_refresh = overlay::enabled(cx).then(|| {
            cx.spawn(async move |root, cx| {
                loop {
                    cx.background_executor().timer(OVERLAY_REFRESH).await;
                    if root.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            })
        });
    }
}
