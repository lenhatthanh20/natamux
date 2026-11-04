//! The scrollbar of the app's scroll areas: Zed's `ui` one, the same as the terminal's. Zed's
//! `Scrollbars::new` ignores the `scrollbar.show` setting, so this one asks `--shot` itself.

use ui::prelude::*;
use ui::scrollbars::{ScrollbarVisibility, ShowScrollbar};
use ui::{ScrollAxes, ScrollableHandle, Scrollbars};

use super::motion::is_still;

/// Auto-hide in the app, never in a `--shot`, so shots do not catch a fading thumb.
#[derive(Default)]
pub struct AppScrollbar;

impl ScrollbarVisibility for AppScrollbar {
    fn visibility(&self, cx: &App) -> ShowScrollbar {
        if is_still(cx) {
            ShowScrollbar::Never
        } else {
            ShowScrollbar::Auto
        }
    }
}

/// The config for `custom_scrollbars` over `axes` of the area `handle` tracks.
pub fn scrollbars<T: ScrollableHandle>(axes: ScrollAxes, handle: &T) -> Scrollbars<T> {
    Scrollbars::for_settings::<AppScrollbar>()
        .show_along(axes)
        .tracked_scroll_handle(handle)
}
