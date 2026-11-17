//! The scrollbar of a scroll area (A35 of the MIT move). A bar per axis lies over the area's far
//! edge: a 14 px lane with a round 6 px thumb 4 px in, the thumb at least 25 px long. It shows on
//! a scroll, a new size or the pointer coming into the area, fades in over 50 ms, and on `Auto`
//! fades out over 400 ms a second after the last of these while the thumb is not under the
//! pointer. The thumb drags; a press on a drawn track puts the thumb's middle at the pointer.

use std::panic::Location;

use gpui::prelude::*;
use gpui::{App, Axis, Div, ElementId, Entity, Hsla, ScrollHandle, Stateful, Window};

use crate::scrollbar_state::BarState;

/// The axes a bar is drawn along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxes {
    Horizontal,
    Vertical,
    Both,
}

/// When a bar shows. Read once, when the bar is first drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarShow {
    #[default]
    Auto,
    Always,
    Never,
}

/// What a bar holds on its axis: nothing, a thumb, or a thumb on a painted track that also takes
/// presses and the room for itself while the area scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Room {
    #[default]
    None,
    Thumb,
    Track,
}

/// Index of an axis in a `[Room; 2]`.
pub(crate) fn slot(axis: Axis) -> usize {
    match axis {
        Axis::Horizontal => 0,
        Axis::Vertical => 1,
    }
}

impl ScrollAxes {
    fn give(self, rooms: &mut [Room; 2], room: Room) {
        if self != ScrollAxes::Vertical {
            rooms[0] = room;
        }
        if self != ScrollAxes::Horizontal {
            rooms[1] = room;
        }
    }
}

/// How a scroll area's bars look and act; hand it to `custom_scrollbars`.
#[derive(Clone)]
pub struct Scrollbars {
    pub(crate) id: Option<ElementId>,
    pub(crate) show: ScrollbarShow,
    pub(crate) handle: Option<ScrollHandle>,
    pub(crate) rooms: [Room; 2],
    pub(crate) track: Option<Hsla>,
    pub(crate) notify: bool,
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
        let mut rooms = [Room::None; 2];
        axes.give(&mut rooms, Room::Thumb);
        Scrollbars {
            id: None,
            show,
            handle: None,
            rooms,
            track: None,
            notify: false,
        }
    }

    /// The key of the bar's state; else the caller of `custom_scrollbars`.
    pub fn id(self, id: impl Into<ElementId>) -> Self {
        Scrollbars {
            id: Some(id.into()),
            ..self
        }
    }

    /// Draws the view again when the bar scrolls the area.
    pub fn notify_content(self) -> Self {
        Scrollbars {
            notify: true,
            ..self
        }
    }

    /// The bars follow `handle`, which the caller puts on its own scroll div. Without one the
    /// div the bars are put on scrolls by a handle of the bar's own.
    pub fn tracked_scroll_handle(self, handle: &ScrollHandle) -> Self {
        Scrollbars {
            handle: Some(handle.clone()),
            ..self
        }
    }

    /// A track in `background` under the thumb along `axes`.
    pub fn with_track_along(mut self, axes: ScrollAxes, background: Hsla) -> Self {
        axes.give(&mut self.rooms, Room::Track);
        self.track = Some(background);
        self
    }
}

/// Puts a bar over a div.
pub trait WithScrollbar: Sized {
    fn custom_scrollbars(self, bars: Scrollbars, window: &mut Window, cx: &mut App)
    -> Stateful<Div>;
}

impl WithScrollbar for Div {
    #[track_caller]
    fn custom_scrollbars(
        self,
        bars: Scrollbars,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        let state = bar_state(bars, Location::caller(), window, cx);
        let area = self.id(("scroll-area", state.entity_id()));
        put_on(state, area, cx)
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
        let state = bar_state(bars, Location::caller(), window, cx);
        put_on(state, self, cx)
    }
}

/// The bar's state of the last frames, made at its first frame. The track colour follows every
/// frame; the rest of `bars` counts only at the start.
fn bar_state(
    bars: Scrollbars,
    caller: &'static Location<'static>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<BarState> {
    let key = bars.id.clone().unwrap_or_else(|| caller.into());
    let track = bars.track;
    let view = window.current_view();
    let state = window.use_keyed_state(key, cx, |_, _| BarState::new(bars, view));
    state.update(cx, |state, _| state.track = track);
    state
}

/// The area with its bar on top. A bar with its own handle makes the area scroll; a track keeps
/// its room at the area's edge while there is something to scroll.
fn put_on(state: Entity<BarState>, area: Stateful<Div>, cx: &App) -> Stateful<Div> {
    let bar = state.read(cx);
    let mut area = area;
    if !bar.tracked {
        area = area.track_scroll(&bar.handle);
        area = match (bar.rooms[0], bar.rooms[1]) {
            (Room::None, Room::None) => area,
            (Room::None, _) => area.overflow_y_scroll(),
            (_, Room::None) => area.overflow_x_scroll(),
            _ => area.overflow_scroll(),
        };
    }
    if let Some(room) = bar.room_kept(Axis::Horizontal) {
        area = area.pb(room);
    }
    if let Some(room) = bar.room_kept(Axis::Vertical) {
        area = area.pr(room);
    }
    area.child(state.clone())
}
