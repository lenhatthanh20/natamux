//! The scrollbar of the app's scroll areas: the kit's own (A35), drawn as Zed's `ui` one was
//! (`scrollbar_golden_tests.rs`).
//! It reads no setting, so this asks `--shot` and the still motion setting itself.

use std::panic::Location;

use gpui::{Div, ScrollHandle, Stateful};
use ui::prelude::*;
use ui::{ScrollAxes, ScrollbarShow, Scrollbars, WithScrollbar};

use super::motion::is_still;
use crate::app::appearance::Inputs;
use crate::app::appearance_rules::shows_scrollbars;

/// Auto-hide in the app, none while motion is still. A `--shot` shows it only in a `-scrollbar`
/// state, so other shots do not catch a fading thumb.
fn app_show(cx: &App) -> ScrollbarShow {
    if shot_shows_bars(cx) {
        ScrollbarShow::Always
    } else if is_still(cx) {
        ScrollbarShow::Never
    } else {
        ScrollbarShow::Auto
    }
}

/// A `-scrollbar` shot state is on.
fn shot_shows_bars(cx: &App) -> bool {
    let shot = cx.try_global::<Inputs>().and_then(|inputs| inputs.shot);
    shot.is_some_and(shows_scrollbars)
}

/// Bars along `axes` that show and hide on their own, always shown in a `-scrollbar` shot state.
pub fn plain_scrollbars(axes: ScrollAxes, cx: &App) -> Scrollbars {
    if shot_shows_bars(cx) {
        Scrollbars::always_visible(axes)
    } else {
        Scrollbars::new(axes)
    }
}

/// `area`, a scroll div that tracks `handle`, in a wrapper that draws its bars, on both axes.
/// A bar on the scroll div itself moved with the content, sat inside the padding and widened
/// the scroll range (T-0286). Give the wrapper the layout the area had in its parent (`flex_1`,
/// `min_h_0`, ...).
#[track_caller]
pub fn scroll_area(
    area: impl IntoElement,
    handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    // The bar's state is keyed by its caller, else it would be this line for every area.
    let bars = Scrollbars::shown(ScrollAxes::Both, app_show(cx))
        .tracked_scroll_handle(handle)
        .id(Location::caller());
    div().child(area).custom_scrollbars(bars, window, cx)
}

#[cfg(test)]
#[path = "scrollbars_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scrollbar_golden_tests.rs"]
mod golden_tests;
