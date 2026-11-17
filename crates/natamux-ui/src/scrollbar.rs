//! The scrollbar of a scroll area (A35 of the MIT move). A bar per axis lies over the area's far
//! edge: a 14 px lane with a round 6 px thumb 4 px in, the thumb at least 25 px long. It shows on
//! a scroll, a new size or the pointer coming into the area, fades in over 50 ms, and on `Auto`
//! fades out over 400 ms a second after the last of these while the thumb is not under the
//! pointer. The thumb drags; a press on a drawn track puts the thumb's middle at the pointer.
//!
//! `scrollbar_layout.rs` places the bars, `scrollbar_state.rs` keeps what they are doing, and
//! `scrollbar_element.rs` draws them and feeds the pointer to the state.

use std::panic::Location;

use gpui::prelude::*;
use gpui::{App, Axis, Div, ElementId, Entity, Hsla, ScrollHandle, Stateful, Window};

use crate::scrollbar_element::{BarElement, BarHost};
use crate::scrollbar_layout::{LANE, Lane, Lanes};

/// The axes a bar is drawn along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxes {
    Horizontal,
    Vertical,
    Both,
}

impl ScrollAxes {
    fn has(self, axis: Axis) -> bool {
        match self {
            ScrollAxes::Both => true,
            ScrollAxes::Horizontal => axis == Axis::Horizontal,
            ScrollAxes::Vertical => axis == Axis::Vertical,
        }
    }

    fn mark(self, lanes: &mut Lanes, lane: Lane) {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if self.has(axis) {
                lanes.set(axis, lane);
            }
        }
    }
}

/// When a bar shows. Read once, when the bar is first drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarShow {
    #[default]
    Auto,
    Always,
    Never,
}

/// How a scroll area's bars look and act; hand it to `custom_scrollbars`.
#[derive(Clone)]
pub struct Scrollbars {
    pub(crate) key: Option<ElementId>,
    pub(crate) show: ScrollbarShow,
    pub(crate) handle: Option<ScrollHandle>,
    pub(crate) lanes: Lanes,
    pub(crate) track: Option<Hsla>,
}

impl Scrollbars {
    /// Bars along `axes` that show and hide on their own.
    pub fn new(axes: ScrollAxes) -> Self {
        Self::shown(axes, ScrollbarShow::Auto)
    }

    pub fn always_visible(axes: ScrollAxes) -> Self {
        Self::shown(axes, ScrollbarShow::Always)
    }

    pub fn shown(axes: ScrollAxes, show: ScrollbarShow) -> Self {
        let mut lanes = Lanes::default();
        axes.mark(&mut lanes, Lane::Bare);
        Scrollbars {
            key: None,
            show,
            handle: None,
            lanes,
            track: None,
        }
    }

    /// The key of the bar's state; else the caller of `custom_scrollbars`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.key = Some(id.into());
        self
    }

    /// Draws the view again when the bar scrolls the area. Every bar does that now: the call
    /// stays so the callers read as Zed's bar does.
    pub fn notify_content(self) -> Self {
        self
    }

    /// The bars follow `handle`, which the caller puts on its own scroll div. Without one the
    /// div the bars are put on scrolls by a handle of the bar's own.
    pub fn tracked_scroll_handle(mut self, handle: &ScrollHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// A track in `background` under the thumb along `axes`.
    pub fn with_track_along(mut self, axes: ScrollAxes, background: Hsla) -> Self {
        axes.mark(&mut self.lanes, Lane::Tracked);
        self.track = Some(background);
        self
    }
}

/// Puts a bar over a div.
pub trait WithScrollbar: Sized {
    fn custom_scrollbars(
        self,
        bars: Scrollbars,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div>;
}

impl WithScrollbar for Div {
    #[track_caller]
    fn custom_scrollbars(
        self,
        bars: Scrollbars,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        let host = host_of(bars, Location::caller(), window, cx);
        let area = self.id(("scroll-area", host.entity_id()));
        dress(area, host, cx)
    }
}

impl WithScrollbar for Stateful<Div> {
    #[track_caller]
    fn custom_scrollbars(
        self,
        bars: Scrollbars,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        let host = host_of(bars, Location::caller(), window, cx);
        dress(self, host, cx)
    }
}

/// The bar's state, made at its first frame and kept while the bar is drawn. Only the track
/// colour of later `bars` counts.
fn host_of(
    bars: Scrollbars,
    caller: &'static Location<'static>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<BarHost> {
    let key = bars.key.clone().unwrap_or_else(|| caller.into());
    let track = bars.track;
    let view = window.current_view();
    let host = window.use_keyed_state(key, cx, |_, _| BarHost::new(bars, view));
    host.update(cx, |host, _| host.track = track);
    host
}

/// The area with the bars drawn over it. With no handle of the caller's the area scrolls by the
/// bar's own; a track takes its room from the area while there is something to scroll.
fn dress(area: Stateful<Div>, host: Entity<BarHost>, cx: &App) -> Stateful<Div> {
    let bar = host.read(cx);
    let lanes = bar.lanes;
    let mut area = area;
    if let Some(handle) = bar.own_handle() {
        let along = |axis| lanes.of(axis) != Lane::Off;
        area = area.track_scroll(handle);
        area = match (along(Axis::Horizontal), along(Axis::Vertical)) {
            (true, true) => area.overflow_scroll(),
            (true, false) => area.overflow_x_scroll(),
            (false, true) => area.overflow_y_scroll(),
            (false, false) => area,
        };
    }
    if bar.keeps_room(Axis::Horizontal) {
        area = area.pb(LANE);
    }
    if bar.keeps_room(Axis::Vertical) {
        area = area.pr(LANE);
    }
    area.child(BarElement::new(host))
}

#[cfg(test)]
#[path = "scrollbar_tests.rs"]
mod tests;
