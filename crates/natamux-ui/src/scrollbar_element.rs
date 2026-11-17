//! The element that lays out, fades and paints a scroll area's bars, and takes the pointer for
//! them. It covers the whole area, drawn after the area's content.

use std::time::Instant;

use gpui::{
    Along, App, Axis, BorderStyle, Bounds, ContentMask, Corners, CursorStyle, DispatchPhase, Edges,
    Element, ElementId, Entity, GlobalElementId, HitboxBehavior, Hsla, InspectorElementId,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Position, ScrollWheelEvent, Style, Window, ease_in_out, point, px, quad, relative, size,
};
use theme::ActiveTheme;

use crate::scrollbar::Room;
use crate::scrollbar_state::{Bar, BarState, Frame, INSET, Seen, THUMB, Thumb};

/// When the thumb is not hovered and lies on no track, the colour under it shows at most this
/// much.
const QUIET_ALPHA: f32 = 0.7;

pub(crate) struct BarElement {
    state: Entity<BarState>,
}

impl BarElement {
    pub(crate) fn new(state: Entity<BarState>) -> Self {
        BarElement { state }
    }
}

impl IntoElement for BarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The lane of a bar along `axis`: as long as the area, at its bottom or right edge.
fn lane(area: Bounds<Pixels>, axis: Axis) -> Bounds<Pixels> {
    let across = THUMB + INSET * 2.;
    match axis {
        Axis::Horizontal => Bounds::new(
            point(area.left(), area.bottom() - across),
            size(area.size.width, across),
        ),
        Axis::Vertical => Bounds::new(
            point(area.right() - across, area.top()),
            size(across, area.size.height),
        ),
    }
}

impl Element for BarElement {
    type RequestLayoutState = ();
    /// The frame's bars and how far they are faded out (None: not fading).
    type PrepaintState = Option<(Frame, Option<f32>)>;

    fn id(&self) -> Option<ElementId> {
        Some(("scrollbar-fade", self.state.entity_id()).into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        style.inset = Edges::all(px(0.).into());
        style.size = size(relative(1.).into(), relative(1.).into());
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let bar = self.state.read(cx);
        if bar.seen == Seen::Off {
            return None;
        }
        let spans = bar.spans();
        // With two bars each one stops a thumb short of the corner.
        let corner = if spans.len() == 2 { THUMB } else { Pixels::ZERO };
        let track = bar.track;
        let bars = spans
            .into_iter()
            .map(|(axis, span, room)| {
                let whole = lane(bounds, axis);
                let way = whole.dilate(-INSET);
                let length = way.size.along(axis) - corner;
                let (start, end) = (span.start * length, span.end * length);
                let thumb = Bounds::new(
                    way.origin.apply_along(axis, |origin| origin + start),
                    way.size.apply_along(axis, |_| end - start),
                );
                let hit = if room == Room::Track { way } else { thumb };
                Bar {
                    axis,
                    thumb,
                    way,
                    hitbox: window.insert_hitbox(hit, HitboxBehavior::BlockMouseExceptScroll),
                    room,
                    track: track
                        .filter(|_| room == Room::Track)
                        .map(|colour| (whole, colour)),
                }
            })
            .collect();
        let frame = Frame {
            area: window.insert_hitbox(bounds, HitboxBehavior::Normal),
            bars,
        };
        if frame.moved_from(self.state.read(cx).last.as_ref()) {
            self.state.update(cx, |bar, cx| bar.reveal(window, cx));
        }
        let seen = self.state.read(cx).seen;
        let fade = match seen {
            Seen::Fading { showing, done } => Some(self.fade(id, showing, done, window, cx)),
            _ => None,
        };
        Some((frame, fade))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((frame, fade)) = prepaint.take() else {
            return;
        };
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let bar = self.state.read(cx);
            let drawn = bar.seen.drawn();
            let dragging = matches!(bar.thumb, Thumb::Drag(..));
            // A held thumb takes the pointer before anything under it.
            let phase = if drawn && dragging {
                DispatchPhase::Capture
            } else {
                DispatchPhase::Bubble
            };
            if drawn {
                for one in &frame.bars {
                    paint_bar(one, bar.thumb, fade, window, cx);
                    if dragging {
                        window.set_window_cursor_style(CursorStyle::Arrow);
                    } else {
                        window.set_cursor_style(CursorStyle::Arrow, &one.hitbox);
                    }
                }
            }
            self.state.update(cx, |bar, _| bar.last = Some(frame));
            self.listen(phase, window);
        });
    }
}

impl BarElement {
    /// How far a fading bar is faded out this frame. The fade moves on by the time since the
    /// last frame (none on the first), and frames keep coming until it ends.
    fn fade(
        &self,
        id: Option<&GlobalElementId>,
        showing: bool,
        done: f32,
        window: &mut Window,
        cx: &mut App,
    ) -> f32 {
        let id = match id {
            Some(id) if !cx.reduce_motion() => id,
            // Reduced motion: the fade ends at once.
            _ => {
                self.state.update(cx, |bar, _| bar.seen = bar.seen.at(1.));
                return if showing { 0. } else { 1. };
            }
        };
        window.with_element_state(id, |last: Option<Instant>, window| {
            let now = Instant::now();
            let since = now - last.unwrap_or(now);
            let next = (done + since.div_duration_f32(Seen::length(showing))).min(1.);
            self.state.update(cx, |bar, _| bar.seen = bar.seen.at(next));
            window.request_animation_frame();
            // This frame shows the fade where the last frame left it.
            let gone = ease_in_out(if showing { 1. - done } else { done });
            (gone, now)
        })
    }

    fn listen(&self, phase: DispatchPhase, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, now, window, cx| {
            if now == phase && event.button == MouseButton::Left {
                state.update(cx, |bar, cx| bar.press(event.position, window, cx));
            }
        });
        let state = self.state.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, now, window, cx| {
            state.update(cx, |bar, cx| {
                if now.capture() && bar.area_hovered(window) {
                    bar.hover_at(event.position, window, cx);
                }
            });
        });
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, now, window, cx| {
            if now != phase {
                return;
            }
            let thumb = state.read(cx).thumb;
            match thumb {
                Thumb::Drag(axis, grab) if event.dragging() => {
                    let held = state.update(cx, |bar, cx| bar.drag(event.position, axis, grab, cx));
                    if held {
                        cx.stop_propagation();
                    }
                }
                _ => state.update(cx, |bar, cx| bar.moved(event, window, cx)),
            }
        });
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, now, window, cx| {
            if now == phase {
                state.update(cx, |bar, cx| bar.released(event.position, window, cx));
            }
        });
    }
}

/// One bar: its track when it has one, then the thumb in the colour of its state, laid over the
/// track's colour (else the surface's), faded with the bar unless it is hovered.
fn paint_bar(bar: &Bar, thumb: Thumb, fade: Option<f32>, window: &mut Window, cx: &App) {
    let colours = cx.theme().colors();
    let (colour, hovered) = match thumb {
        Thumb::Drag(axis, _) if axis == bar.axis => (colours.scrollbar_thumb_active_background, false),
        Thumb::Hover(axis) if axis == bar.axis => (colours.scrollbar_thumb_hover_background, true),
        _ => (colours.scrollbar_thumb_background, false),
    };
    let under = bar.track.map_or(colours.surface_background, |(_, track)| track);
    let under = if hovered || bar.room == Room::Track {
        under
    } else {
        Hsla {
            a: under.a.min(QUIET_ALPHA),
            ..under
        }
    };
    let mut fill = under.blend(colour);
    if !hovered && let Some(fade) = fade {
        fill.fade_out(fade);
    }
    if let Some((whole, mut track)) = bar.track {
        if let Some(fade) = fade {
            track.fade_out(fade);
        }
        let clear = Hsla::transparent_black();
        window.paint_quad(quad(
            whole,
            Corners::default(),
            track,
            Edges::default(),
            clear,
            BorderStyle::Solid,
        ));
    }
    let round = Corners::all(Pixels::MAX).clamp_radii_for_quad_size(bar.thumb.size);
    window.paint_quad(quad(
        bar.thumb,
        round,
        fill,
        Edges::default(),
        Hsla::transparent_black(),
        BorderStyle::default(),
    ));
}
