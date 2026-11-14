//! The scrollbar of the app's scroll areas: Zed's `ui` one, the same as the terminal's. Zed's
//! `Scrollbars::new` ignores the `scrollbar.show` setting, so this one asks `--shot` itself.

use std::panic::Location;

use gpui::{Div, Stateful};
use ui::prelude::*;
use ui::scrollbars::{ScrollbarVisibility, ShowScrollbar};
use ui::{ScrollAxes, ScrollableHandle, Scrollbars, WithScrollbar};

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

/// The config for `custom_scrollbars` over `axes` of the area `handle` tracks. Use it through
/// `scroll_area`.
fn scrollbars<T: ScrollableHandle>(axes: ScrollAxes, handle: &T) -> Scrollbars<T> {
    Scrollbars::for_settings::<AppScrollbar>()
        .show_along(axes)
        .tracked_scroll_handle(handle)
}

/// `area`, a scroll div that tracks `handle`, in a wrapper that draws its bar along `axes`.
/// Zed adds the bar as a child of the div it is called on: on the scroll div itself the bar
/// moves with the content, sits inside the padding and widens the scroll range (T-0286).
/// Give the wrapper the layout the area had in its parent (`flex_1`, `min_h_0`, ...).
#[track_caller]
pub fn scroll_area<T: ScrollableHandle>(
    area: impl IntoElement,
    axes: ScrollAxes,
    handle: &T,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    // Zed keys the bar's state by its caller, which would be this line for every area.
    let id = Location::caller();
    div()
        .child(area)
        .custom_scrollbars(scrollbars(axes, handle).id(id), window, cx)
}

#[cfg(test)]
#[path = "scrollbars_tests.rs"]
mod tests;
