//! The state of a scroll area's bars from frame to frame: how much of them shows, the thumb under
//! the pointer, the last frame's bars, and what the pointer does to them.

use std::ops::Range;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Along, App, Axis, Bounds, Context, EntityId, Hitbox, Hsla, MouseMoveEvent, Pixels, Point,
    ScrollHandle, Task, Window, px,
};

use crate::scrollbar::{Room, ScrollbarShow, Scrollbars, slot};
use crate::scrollbar_element::BarElement;

pub(crate) const THUMB: Pixels = px(6.);
pub(crate) const INSET: Pixels = px(4.);
const LEAST: Pixels = px(25.);
const SHOW_IN: Duration = Duration::from_millis(50);
const HIDE_AFTER: Duration = Duration::from_secs(1);
const FADE_OUT: Duration = Duration::from_millis(400);

/// How much of the bar is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Seen {
    Shown,
    /// Fading in (`showing`) or out, `done` of the way (0 to 1).
    Fading {
        showing: bool,
        done: f32,
    },
    Hidden,
    /// `Never`: no bar, no room, no pointer.
    Off,
}

impl Seen {
    pub(crate) fn first(show: ScrollbarShow) -> Self {
        match show {
            ScrollbarShow::Always => Seen::Shown,
            ScrollbarShow::Never => Seen::Off,
            ScrollbarShow::Auto => Seen::Fading {
                showing: true,
                done: 0.,
            },
        }
    }

    pub(crate) fn drawn(self) -> bool {
        matches!(self, Seen::Shown | Seen::Fading { .. })
    }

    /// Shown again: a hidden bar starts to show, one fading out turns back from where it is.
    pub(crate) fn again(self, show: ScrollbarShow) -> Self {
        match self {
            Seen::Hidden if show == ScrollbarShow::Auto => Seen::Fading {
                showing: true,
                done: 0.,
            },
            Seen::Hidden => Seen::Shown,
            Seen::Fading {
                showing: false,
                done,
            } => Seen::Fading {
                showing: true,
                done: 1. - done,
            },
            other => other,
        }
    }

    /// A fade `done` of the way; at the end it is shown or hidden.
    pub(crate) fn at(self, done: f32) -> Self {
        match self {
            Seen::Fading { showing, .. } if done >= 1. => {
                if showing {
                    Seen::Shown
                } else {
                    Seen::Hidden
                }
            }
            Seen::Fading { showing, .. } => Seen::Fading { showing, done },
            other => other,
        }
    }

    /// How long a fade takes.
    pub(crate) fn length(showing: bool) -> Duration {
        if showing { SHOW_IN } else { FADE_OUT }
    }
}

/// The thumb under the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Thumb {
    #[default]
    Idle,
    Hover(Axis),
    /// Held at `grab` px from its start.
    Drag(Axis, Pixels),
}

/// Where the pointer is against the area, from one move to the next.
enum Pointer {
    Entered,
    Within,
    Exited,
    Away,
}

/// One bar of the last frame.
pub(crate) struct Bar {
    pub(crate) axis: Axis,
    pub(crate) thumb: Bounds<Pixels>,
    /// The way the thumb moves in: the lane less its inset.
    pub(crate) way: Bounds<Pixels>,
    pub(crate) hitbox: Hitbox,
    pub(crate) room: Room,
    /// The whole lane and its colour, when a track is painted.
    pub(crate) track: Option<(Bounds<Pixels>, Hsla)>,
}

impl Bar {
    /// Same axis, thumb start (from the way's start) and thumb length.
    fn same_place(&self, other: &Bar) -> bool {
        let start = |bar: &Bar| bar.thumb.origin.along(bar.axis) - bar.way.origin.along(bar.axis);
        self.axis == other.axis
            && start(self) == start(other)
            && self.thumb.size.along(self.axis) == other.thumb.size.along(other.axis)
    }
}

/// The bars and the area's hitbox, as the last frame drew them.
pub(crate) struct Frame {
    pub(crate) area: Hitbox,
    pub(crate) bars: Vec<Bar>,
}

impl Frame {
    /// A thumb moved or changed length since `last` (or there was no last frame).
    pub(crate) fn moved_from(&self, last: Option<&Frame>) -> bool {
        last.is_none_or(|last| {
            self.bars.len() != last.bars.len()
                || self.bars.iter().zip(&last.bars).any(|(a, b)| !a.same_place(b))
        })
    }
}

pub(crate) struct BarState {
    pub(crate) show: ScrollbarShow,
    pub(crate) seen: Seen,
    pub(crate) thumb: Thumb,
    pub(crate) handle: ScrollHandle,
    /// The caller scrolls its own div with `handle`.
    pub(crate) tracked: bool,
    pub(crate) rooms: [Room; 2],
    pub(crate) track: Option<Hsla>,
    notify: Option<EntityId>,
    pointer_in: bool,
    pub(crate) last: Option<Frame>,
    hide: Option<Task<()>>,
}

impl BarState {
    pub(crate) fn new(bars: Scrollbars, view: EntityId) -> Self {
        BarState {
            show: bars.show,
            seen: Seen::first(bars.show),
            thumb: Thumb::Idle,
            tracked: bars.handle.is_some(),
            handle: bars.handle.unwrap_or_default(),
            rooms: bars.rooms,
            track: bars.track,
            notify: bars.notify.then_some(view),
            pointer_in: true,
            last: None,
            hide: None,
        }
    }

    pub(crate) fn room_kept(&self, axis: Axis) -> Option<Pixels> {
        let keeps = self.rooms[slot(axis)] == Room::Track
            && self.handle.max_offset().along(axis) != Pixels::ZERO;
        (self.seen != Seen::Off && keeps).then_some(THUMB + INSET * 2.)
    }

    /// Each axis with a bar and something to scroll: its thumb as a part of the viewport.
    pub(crate) fn spans(&self) -> Vec<(Axis, Range<f32>, Room)> {
        let max = self.handle.max_offset();
        let viewport = self.handle.bounds().size;
        let offset = self.handle.offset();
        [Axis::Horizontal, Axis::Vertical]
            .into_iter()
            .filter(|&axis| self.rooms[slot(axis)] != Room::None)
            .filter_map(|axis| {
                let span = thumb_span(max.along(axis), viewport.along(axis), offset.along(axis))?;
                Some((axis, span, self.rooms[slot(axis)]))
            })
            .collect()
    }

    pub(crate) fn set_seen(&mut self, seen: Seen, cx: &mut Context<Self>) {
        if self.seen != seen {
            self.seen = seen;
            cx.notify();
        }
    }

    /// Shows the bar and starts the wait before it hides again.
    pub(crate) fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_seen(self.seen.again(self.show), cx);
        self.hide = None;
        self.hide_later(window, cx);
    }

    /// On `Auto`, hides the bar a second from now unless a wait already runs or the thumb is
    /// under the pointer by then.
    pub(crate) fn hide_later(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hide.is_some() || !self.seen.drawn() || self.show != ScrollbarShow::Auto {
            return;
        }
        self.hide = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(HIDE_AFTER).await;
            this.update(cx, |bar, cx| {
                if bar.thumb == Thumb::Idle {
                    let fade = Seen::Fading {
                        showing: false,
                        done: 0.,
                    };
                    bar.set_seen(fade, cx);
                }
                bar.hide = None;
            })
            .ok();
        }));
    }

    fn set_thumb(&mut self, thumb: Thumb, window: &mut Window, cx: &mut Context<Self>) {
        if self.thumb == thumb {
            return;
        }
        if thumb == Thumb::Idle {
            self.hide_later(window, cx);
        } else {
            self.set_seen(self.seen.again(self.show), cx);
            self.hide = None;
        }
        self.thumb = thumb;
        cx.notify();
    }

    /// The thumb under `at`, if its hitbox is the one the pointer is over.
    pub(crate) fn hover_at(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let under = self.last.as_ref().and_then(|frame| {
            frame.bars.iter().find(|bar| bar.thumb.contains(&at))
        });
        let thumb = match under {
            Some(bar) if bar.hitbox.is_hovered(window) => Thumb::Hover(bar.axis),
            _ => Thumb::Idle,
        };
        self.set_thumb(thumb, window, cx);
    }

    pub(crate) fn area_hovered(&self, window: &Window) -> bool {
        self.last
            .as_ref()
            .is_some_and(|frame| frame.area.is_hovered(window))
    }

    fn pointer(&mut self, window: &Window) -> Pointer {
        let was_in = self.pointer_in;
        self.pointer_in = self.area_hovered(window);
        match (was_in, self.pointer_in) {
            (false, true) => Pointer::Entered,
            (true, true) => Pointer::Within,
            (true, false) => Pointer::Exited,
            (false, false) => Pointer::Away,
        }
    }

    pub(crate) fn scroll_to(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        self.handle.set_offset(offset);
        if let Some(view) = self.notify {
            notify_view(cx, view);
        }
        cx.notify();
    }

    /// A left press: on a thumb it starts a drag; elsewhere in a track it scrolls so the thumb's
    /// middle is at the pointer. Either way the press goes no further.
    pub(crate) fn press(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let hit = self.last.as_ref().and_then(|frame| {
            frame.bars.iter().find(|bar| match bar.room {
                Room::Track => bar.way.contains(&at),
                _ => bar.thumb.contains(&at),
            })
        });
        let Some(&Bar {
            axis, thumb, way, ..
        }) = hit
        else {
            return;
        };
        if thumb.contains(&at) {
            let grab = at.along(axis) - thumb.origin.along(axis);
            self.set_thumb(Thumb::Drag(axis, grab), window, cx);
        } else {
            let length = thumb.size.along(axis);
            let max = self.handle.max_offset().along(axis);
            let to = offset_for(way, length, at, length / 2., max, axis);
            let offset = self.handle.offset().apply_along(axis, |_| to);
            self.scroll_to(offset, cx);
        }
        cx.stop_propagation();
    }

    /// A move with the thumb held: the area follows the thumb.
    pub(crate) fn drag(&mut self, at: Point<Pixels>, axis: Axis, grab: Pixels, cx: &mut Context<Self>) -> bool {
        let Some(bar) = self
            .last
            .as_ref()
            .and_then(|frame| frame.bars.iter().find(|bar| bar.axis == axis))
        else {
            return false;
        };
        let max = self.handle.max_offset().along(axis);
        let to = offset_for(bar.way, bar.thumb.size.along(axis), at, grab, max, axis);
        let offset = self.handle.offset().apply_along(axis, |_| to);
        self.scroll_to(offset, cx);
        true
    }

    /// A move with no thumb held: coming into the area shows the bar; over it the thumb under
    /// the pointer is hovered, and a move over a thumb goes no further; leaving lets go.
    pub(crate) fn moved(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.pointer(window) {
            pointer @ (Pointer::Entered | Pointer::Within) if event.pressed_button.is_none() => {
                if matches!(pointer, Pointer::Entered) {
                    self.reveal(window, cx);
                }
                self.hover_at(event.position, window, cx);
                if self.thumb != Thumb::Idle {
                    cx.stop_propagation();
                }
            }
            Pointer::Exited => self.set_thumb(Thumb::Idle, window, cx),
            _ => {}
        }
    }

    /// A release: outside the area the bar may hide; inside the thumb under it is hovered.
    pub(crate) fn released(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.area_hovered(window) {
            self.hover_at(at, window, cx);
        } else {
            self.hide_later(window, cx);
        }
    }
}

/// `Context::notify` takes no id, so the view goes through `App`.
fn notify_view(cx: &mut App, view: EntityId) {
    cx.notify(view);
}

impl Render for BarState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        BarElement::new(cx.entity())
    }
}

/// The thumb over a viewport of `viewport` that scrolls up to `max` (offset `offset`, 0 or
/// below), as the part of the viewport it covers; None when nothing scrolls or the thumb would
/// not fit.
pub(crate) fn thumb_span(max: Pixels, viewport: Pixels, offset: Pixels) -> Option<Range<f32>> {
    if max == Pixels::ZERO || viewport == Pixels::ZERO {
        return None;
    }
    let length = (viewport * (viewport / (viewport + max))).max(LEAST);
    if length > viewport {
        return None;
    }
    let scrolled = offset.max(-max).min(Pixels::ZERO).abs();
    let start = (scrolled / max) * (viewport - length);
    Some(start / viewport..(start + length) / viewport)
}

/// The offset (0 or below) that puts the thumb, held `grab` px from its start, at `at` within
/// `way`.
pub(crate) fn offset_for(
    way: Bounds<Pixels>,
    length: Pixels,
    at: Point<Pixels>,
    grab: Pixels,
    max: Pixels,
    axis: Axis,
) -> Pixels {
    let room = way.size.along(axis) - length;
    let start = (at.along(axis) - way.origin.along(axis) - grab)
        .max(px(0.))
        .min(room);
    let part = if room > px(0.) { start / room } else { 0. };
    -max * part
}

#[cfg(test)]
#[path = "scrollbar_tests.rs"]
mod tests;
