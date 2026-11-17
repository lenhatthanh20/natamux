//! The bar's host entity, which runs the state against the scroll handle and the hide wait, and
//! the element that lays the bars out, paints them and passes the pointer to the host. The
//! element covers the whole area and is drawn after the area's content.

use std::time::Instant;

use gpui::{
    Along, App, Axis, BorderStyle, Bounds, ContentMask, Context, Corners, CursorStyle,
    DispatchPhase, Edges, Element, ElementId, Entity, EntityId, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Position, ScrollHandle, ScrollWheelEvent, Style,
    Task, Window, px, quad, relative, size,
};
use theme::ActiveTheme;

use crate::scrollbar::Scrollbars;
use crate::scrollbar_layout::{Hit, Lane, Lanes, Layout, Piece};
use crate::scrollbar_state::{Grip, HIDE_AFTER, Input, Model, Sight};

/// A thumb that is not hovered and lies on no track lets the colour under it show at most this
/// much.
const QUIET_ALPHA: f32 = 0.7;

/// What the last frame drew, for the pointer to hit.
pub(crate) struct Drawn {
    layout: Layout,
    targets: Vec<Hitbox>,
    area: Hitbox,
}

pub(crate) struct BarHost {
    model: Model,
    handle: ScrollHandle,
    /// The handle is the bar's own, not one the caller scrolls with.
    owns_handle: bool,
    pub(crate) lanes: Lanes,
    pub(crate) track: Option<Hsla>,
    /// The view that draws the bar. The host is no view, so its own notify reaches no cached
    /// view: every change of the bar notifies this one.
    view: EntityId,
    last: Option<Drawn>,
    wait: Option<(u32, Task<()>)>,
}

impl BarHost {
    pub(crate) fn new(bars: Scrollbars, view: EntityId) -> Self {
        BarHost {
            model: Model::new(bars.show),
            owns_handle: bars.handle.is_none(),
            handle: bars.handle.unwrap_or_default(),
            lanes: bars.lanes,
            track: bars.track,
            view,
            last: None,
            wait: None,
        }
    }

    pub(crate) fn own_handle(&self) -> Option<&ScrollHandle> {
        self.owns_handle.then_some(&self.handle)
    }

    /// A tracked axis takes its lane's room from the area while it can scroll.
    pub(crate) fn keeps_room(&self, axis: Axis) -> bool {
        !self.model.off()
            && self.lanes.of(axis) == Lane::Tracked
            && self.handle.max_offset().along(axis) != Pixels::ZERO
    }

    /// Gives the state one input and acts on the result: redraw on a change, start or drop the
    /// hide wait. True when the event should go no further.
    fn feed(&mut self, input: Input, cx: &mut Context<Self>) -> bool {
        let seen = (self.model.look, self.model.grip);
        let stop = self.model.apply(input, Instant::now());
        self.sync_wait(cx);
        if seen != (self.model.look, self.model.grip) {
            redraw_view(self.view, cx);
        }
        stop
    }

    fn sync_wait(&mut self, cx: &mut Context<Self>) {
        if !self.model.waiting {
            self.wait = None;
            return;
        }
        let round = self.model.round;
        if self
            .wait
            .as_ref()
            .is_some_and(|(running, _)| *running == round)
        {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDE_AFTER).await;
            this.update(cx, |host, cx| host.feed(Input::Waited { round }, cx))
                .ok();
        });
        self.wait = Some((round, task));
    }

    /// The bar whose thumb is under `at`, when its hitbox is the one hit.
    fn over(&self, at: Point<Pixels>, window: &Window) -> Option<Axis> {
        let last = self.last.as_ref()?;
        let index = last.layout.thumb_at(at)?;
        let hovered = last.targets.get(index)?.is_hovered(window);
        hovered.then(|| last.layout.pieces[index].axis)
    }

    fn inside(&self, window: &Window) -> bool {
        self.last
            .as_ref()
            .is_some_and(|last| last.area.is_hovered(window))
    }

    fn scroll(&mut self, axis: Axis, to: Pixels, cx: &mut Context<Self>) {
        let offset = self.handle.offset().apply_along(axis, |_| to);
        self.handle.set_offset(offset);
        redraw_view(self.view, cx);
    }

    fn on_down(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let hit = self
            .last
            .as_ref()
            .and_then(|last| last.layout.press(at, self.handle.max_offset()));
        match hit {
            Some(Hit::Grab { axis, grab }) => {
                self.feed(Input::Grab(axis, grab), cx);
            }
            Some(Hit::Jump { axis, offset }) => self.scroll(axis, offset, cx),
            None => return,
        }
        cx.stop_propagation();
    }

    fn on_move(&mut self, event: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        if let Grip::Held(axis, grab) = self.model.grip
            && event.dragging()
        {
            let max = self.handle.max_offset().along(axis);
            let to = self
                .last
                .as_ref()
                .and_then(|last| last.layout.along(axis))
                .map(|piece| piece.offset_at(event.position, grab, max));
            if let Some(to) = to {
                self.scroll(axis, to, cx);
                cx.stop_propagation();
            }
            return;
        }
        let input = Input::Pointer {
            inside: self.inside(window),
            button: event.pressed_button.is_some(),
            over: self.over(event.position, window),
        };
        if self.feed(input, cx) {
            cx.stop_propagation();
        }
    }

    fn on_up(&mut self, at: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let input = Input::Release {
            inside: self.inside(window),
            over: self.over(at, window),
        };
        self.feed(input, cx);
    }

    fn on_wheel(&mut self, at: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        if self.inside(window) {
            let over = self.over(at, window);
            self.feed(Input::Wheel { over }, cx);
        }
    }
}

/// `Context::notify` notifies its own entity only, so another view goes through `App`.
fn redraw_view(view: EntityId, cx: &mut App) {
    cx.notify(view);
}

pub(crate) struct BarElement {
    host: Entity<BarHost>,
}

impl BarElement {
    pub(crate) fn new(host: Entity<BarHost>) -> Self {
        BarElement { host }
    }
}

impl IntoElement for BarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// A frame's layout with its hitboxes, and how it is seen.
pub(crate) struct Prepainted {
    drawn: Drawn,
    sight: Sight,
}

impl Element for BarElement {
    type RequestLayoutState = ();
    /// None for a `Never` bar.
    type PrepaintState = Option<Prepainted>;

    fn id(&self) -> Option<ElementId> {
        None
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
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let host = self.host.read(cx);
        if host.model.off() {
            return None;
        }
        let layout = Layout::measure(bounds, host.lanes, &host.handle);
        let moved = host
            .last
            .as_ref()
            .is_none_or(|last| !last.layout.same_thumbs(&layout));
        // The bars' hitboxes go in before the area's, so the area's lies on top and still sees
        // the pointer over a bar.
        let targets = layout
            .pieces
            .iter()
            .map(|piece| {
                window.insert_hitbox(piece.target(), HitboxBehavior::BlockMouseExceptScroll)
            })
            .collect();
        let area = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let still = cx.reduce_motion();
        let (sight, animate) = self.host.update(cx, |host, cx| {
            host.feed(Input::Drawn { moved }, cx);
            host.model.frame(Instant::now(), still)
        });
        if animate {
            window.request_animation_frame();
        }
        Some(Prepainted {
            drawn: Drawn {
                layout,
                targets,
                area,
            },
            sight,
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepainted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(Prepainted { drawn, sight }) = prepainted.take() else {
            return;
        };
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let host = self.host.read(cx);
            let grip = host.model.grip;
            let held = matches!(grip, Grip::Held(..));
            if sight.drawn {
                let colours = Colours::of(host.track, cx);
                for (piece, target) in drawn.layout.pieces.iter().zip(&drawn.targets) {
                    paint_piece(piece, grip, sight.alpha, &colours, window);
                    if held {
                        window.set_window_cursor_style(CursorStyle::Arrow);
                    } else {
                        window.set_cursor_style(CursorStyle::Arrow, target);
                    }
                }
            }
            // A held thumb gets the pointer before anything under it.
            let phase = if sight.drawn && held {
                DispatchPhase::Capture
            } else {
                DispatchPhase::Bubble
            };
            self.host.update(cx, |host, _| host.last = Some(drawn));
            self.listen(phase, window);
        });
    }
}

impl BarElement {
    fn listen(&self, phase: DispatchPhase, window: &mut Window) {
        let host = self.host.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, at, _, cx| {
            if at == phase && event.button == MouseButton::Left {
                host.update(cx, |host, cx| host.on_down(event.position, cx));
            }
        });
        let host = self.host.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, at, window, cx| {
            if at == phase {
                host.update(cx, |host, cx| host.on_move(event, window, cx));
            }
        });
        let host = self.host.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, at, window, cx| {
            if at == phase {
                host.update(cx, |host, cx| host.on_up(event.position, window, cx));
            }
        });
        let host = self.host.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, at, window, cx| {
            if at.capture() {
                host.update(cx, |host, cx| host.on_wheel(event.position, window, cx));
            }
        });
    }
}

/// The theme colours a bar is painted with.
struct Colours {
    rest: Hsla,
    hover: Hsla,
    held: Hsla,
    surface: Hsla,
    track: Option<Hsla>,
}

impl Colours {
    fn of(track: Option<Hsla>, cx: &App) -> Colours {
        let theme = cx.theme().colors();
        Colours {
            rest: theme.scrollbar_thumb_background,
            hover: theme.scrollbar_thumb_hover_background,
            held: theme.scrollbar_thumb_active_background,
            surface: theme.surface_background,
            track,
        }
    }
}

/// One bar: the track when it has one, then the thumb. The thumb's colour is laid over the
/// track's (else the surface's); a hovered thumb does not fade.
fn paint_piece(
    piece: &Piece,
    grip: Grip,
    alpha: Option<f32>,
    colours: &Colours,
    window: &mut Window,
) {
    let tracked = piece.lane == Lane::Tracked;
    let track = colours.track.filter(|_| tracked);
    let (thumb, hovered) = match grip {
        Grip::Held(axis, _) if axis == piece.axis => (colours.held, false),
        Grip::Over(axis) if axis == piece.axis => (colours.hover, true),
        _ => (colours.rest, false),
    };
    let mut under = track.unwrap_or(colours.surface);
    if !hovered && !tracked {
        under.a = under.a.min(QUIET_ALPHA);
    }
    let fill = match alpha {
        Some(alpha) if !hovered => under.blend(thumb).opacity(alpha),
        _ => under.blend(thumb),
    };
    if let Some(track) = track {
        let track = alpha.map_or(track, |alpha| track.opacity(alpha));
        window.paint_quad(quad(
            piece.strip,
            Corners::default(),
            track,
            Edges::default(),
            Hsla::transparent_black(),
            BorderStyle::Solid,
        ));
    }
    let round = Corners::all(Pixels::MAX).clamp_radii_for_quad_size(piece.thumb.size);
    window.paint_quad(quad(
        piece.thumb,
        round,
        fill,
        Edges::default(),
        Hsla::transparent_black(),
        BorderStyle::default(),
    ));
}
