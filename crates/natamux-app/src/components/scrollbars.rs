//! The scrollbar of the app's scroll areas: Zed's `ui` one, the same as the terminal's. Zed's
//! `Scrollbars::new` ignores the `scrollbar.show` setting, so this one asks `--shot` itself.

use std::panic::Location;

use gpui::{Div, Stateful};
use ui::prelude::*;
use ui::scrollbars::{ScrollbarVisibility, ShowScrollbar};
use ui::{ScrollAxes, ScrollableHandle, Scrollbars, WithScrollbar};

use super::motion::is_still;
use crate::app::appearance::Inputs;
use crate::app::appearance_rules::shows_scrollbars;

/// Auto-hide in the app. A `--shot` shows it only in a `-scrollbar` state, so other shots do not
/// catch a fading thumb.
#[derive(Default)]
pub struct AppScrollbar;

impl ScrollbarVisibility for AppScrollbar {
    fn visibility(&self, cx: &App) -> ShowScrollbar {
        if shot_shows_bars(cx) {
            ShowScrollbar::Always
        } else if is_still(cx) {
            ShowScrollbar::Never
        } else {
            ShowScrollbar::Auto
        }
    }
}

/// A `-scrollbar` shot state is on.
fn shot_shows_bars(cx: &App) -> bool {
    let shot = cx.try_global::<Inputs>().and_then(|inputs| inputs.shot);
    shot.is_some_and(shows_scrollbars)
}

/// Zed's `Scrollbars::new(axes)`, which shows and hides on its own, always shown in a
/// `-scrollbar` shot state.
pub fn plain_scrollbars(axes: ScrollAxes, cx: &App) -> Scrollbars {
    if shot_shows_bars(cx) {
        Scrollbars::always_visible(axes)
    } else {
        Scrollbars::new(axes)
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
