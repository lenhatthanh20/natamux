//! The scrollbars of a code view (plan 6.4): Zed's editor values, the markers and the drag of
//! the vertical bar, and the horizontal bar of a view whose lines do not wrap.

use std::time::{Duration, Instant};

use gpui::{App, Bounds, Context, Hsla, Pixels, Task, Window, point, px};
use theme::ActiveTheme;

use super::element::Layout;
use super::text_input::{Mode, TextInput};
use crate::app::appearance::Appearance;

/// The width of the editor's scrollbar.
pub const BAR_WIDTH: Pixels = px(15.);
/// The shortest thumb.
const MIN_THUMB: Pixels = px(25.);
/// The height of a cursor marker on the scrollbar.
const CURSOR_MARKER: Pixels = px(2.);
/// The shortest marker of a selected-text place.
const MIN_MARKER: Pixels = px(5.);
/// How long the scrollbar stays after a scroll.
const BAR_SHOWS_FOR: Duration = Duration::from_secs(1);

/// The state of a code view's scrollbar.
#[derive(Default)]
pub(super) struct Bar {
    scrolled_at: Option<Instant>,
    hovered: bool,
    /// While the thumb is dragged: where it was taken, from its top.
    grab: Option<Pixels>,
    /// The same for the horizontal bar, from the thumb's left.
    hhovered: bool,
    hgrab: Option<Pixels>,
    _hide: Option<Task<()>>,
}

/// Where the scrollbar is drawn.
pub(super) struct BarBounds {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
}

impl TextInput {
    /// A wheel or bar scroll of a code view: the scrollbar shows and hides again 1 s later.
    pub(super) fn code_scrolled(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(code) = self.code.as_mut() else {
            return;
        };
        code.bar.scrolled_at = Some(now);
        code.bar._hide = Some(cx.spawn(async move |input, cx| {
            cx.background_executor().timer(BAR_SHOWS_FOR).await;
            input.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    /// Whether the scrollbars of a code view show: always in a `-scrollbar` shot, never in
    /// other shots, else while one is dragged, for 1 s after a scroll, while the caret is out of
    /// view, and while the selected text has other places.
    fn bars_show(&self, laid: &Layout, bounds: Bounds<Pixels>, cx: &App) -> bool {
        let Some(code) = self.code.as_ref() else {
            return false;
        };
        if !matches!(self.mode, Mode::Full { .. }) {
            return false;
        }
        let look = cx.try_global::<Appearance>();
        if look.is_some_and(|look| look.hide_scrollbars) {
            return false;
        }
        let height = bounds.size.height;
        let caret = laid
            .position(self.to_display(self.model.head()), self.end_bias)
            .y;
        let caret_hidden =
            caret + laid.line_height <= self.scroll.y || caret >= self.scroll.y + height;
        let recent = code
            .bar
            .scrolled_at
            .is_some_and(|at| self.now(cx).saturating_duration_since(at) < BAR_SHOWS_FOR);
        let always = look.is_some_and(|look| look.always_scrollbars);
        let occurrences = code
            .other_occurrences(self.model.selection().range())
            .next()
            .is_some();
        let held = code.bar.grab.is_some() || code.bar.hgrab.is_some();
        always || recent || caret_hidden || occurrences || held
    }

    /// Where the vertical scrollbar of a code view is, when it shows (`bars_show`).
    pub(super) fn bar_bounds(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) -> Option<BarBounds> {
        if !self.bars_show(laid, bounds, cx) {
            return None;
        }
        let height = bounds.size.height;
        let limit = self.code_scroll_limit(laid).unwrap_or_default();
        let content = limit + height;
        let thumb_height = (height * (height / content)).max(MIN_THUMB).min(height);
        let top = if limit > px(0.) {
            (height - thumb_height) * (self.scroll.y / limit)
        } else {
            px(0.)
        };
        let track = Bounds::new(
            point(bounds.right() - BAR_WIDTH, bounds.top()),
            gpui::size(BAR_WIDTH, height),
        );
        let thumb = Bounds::new(
            point(track.left(), track.top() + top),
            gpui::size(BAR_WIDTH, thumb_height),
        );
        Some(BarBounds { track, thumb })
    }

    /// The most a code view whose lines do not wrap scrolls sideways: until the end of its
    /// widest line is at the right of the text's room. Zero for every other view.
    pub(super) fn code_scroll_x_limit(&self, laid: &Layout) -> Pixels {
        if self.scrolls_sideways() {
            (laid.width - laid.key.view).max(px(0.))
        } else {
            px(0.)
        }
    }

    /// The lines do not wrap: a code view with wrap off.
    pub(super) fn scrolls_sideways(&self) -> bool {
        matches!(self.mode, Mode::Full { .. }) && self.code.as_ref().is_some_and(|code| !code.wrap)
    }

    /// Where the horizontal scrollbar is, when it shows: with the vertical one, while a line is
    /// wider than the view. It runs from the gutter to the vertical bar, along the bottom.
    pub(super) fn hbar_bounds(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) -> Option<BarBounds> {
        let most = self.code_scroll_x_limit(laid);
        if most <= px(0.) || !self.bars_show(laid, bounds, cx) {
            return None;
        }
        let left = bounds.left() + laid.key.gutter;
        let width = (bounds.right() - BAR_WIDTH - left).max(px(0.));
        let content = laid.width.max(px(1.));
        let thumb_width = (width * (laid.key.view / content)).max(MIN_THUMB).min(width);
        let x = (width - thumb_width) * (self.scroll.x / most);
        let track = Bounds::new(
            point(left, bounds.bottom() - BAR_WIDTH),
            gpui::size(width, BAR_WIDTH),
        );
        let thumb = Bounds::new(
            point(left + x, track.top()),
            gpui::size(thumb_width, BAR_WIDTH),
        );
        Some(BarBounds { track, thumb })
    }

    pub(super) fn hbar_hovered(&self) -> bool {
        self.code.as_ref().is_some_and(|code| code.bar.hhovered)
    }

    pub(super) fn hbar_dragged(&self) -> bool {
        self.code
            .as_ref()
            .is_some_and(|code| code.bar.hgrab.is_some())
    }

    pub(super) fn bar_hovered(&self) -> bool {
        self.code.as_ref().is_some_and(|code| code.bar.hovered)
    }

    pub(super) fn bar_dragged(&self) -> bool {
        self.code
            .as_ref()
            .is_some_and(|code| code.bar.grab.is_some())
    }

    /// A press on the scrollbar: on the thumb it takes it, on the track the thumb jumps there
    /// first. True when the press was on the bar.
    pub(super) fn bar_press(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bounds) = self.last_bounds.filter(|_| self.code.is_some()) else {
            return false;
        };
        let laid = self.layout(window, cx);
        if let Some(hbar) = self.hbar_bounds(&laid, bounds, cx)
            && hbar.track.contains(&position)
        {
            let grab = if hbar.thumb.contains(&position) {
                position.x - hbar.thumb.left()
            } else {
                hbar.thumb.size.width / 2.
            };
            if let Some(code) = self.code.as_mut() {
                code.bar.hgrab = Some(grab);
            }
            self.bar_drag(position, window, cx);
            return true;
        }
        let Some(bar) = self.bar_bounds(&laid, bounds, cx) else {
            return false;
        };
        if !bar.track.contains(&position) {
            return false;
        }
        let grab = if bar.thumb.contains(&position) {
            position.y - bar.thumb.top()
        } else {
            bar.thumb.size.height / 2.
        };
        if let Some(code) = self.code.as_mut() {
            code.bar.grab = Some(grab);
        }
        self.bar_drag(position, window, cx);
        true
    }

    /// Moves a taken thumb with the pointer, and keeps the hover state. True while the thumb is
    /// taken.
    pub(super) fn bar_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bounds) = self.last_bounds.filter(|_| self.code.is_some()) else {
            return false;
        };
        let laid = self.layout(window, cx);
        let bar = self.bar_bounds(&laid, bounds, cx);
        let hbar = self.hbar_bounds(&laid, bounds, cx);
        let on_thumb = |bar: &Option<BarBounds>| {
            bar.as_ref()
                .is_some_and(|bar| bar.thumb.contains(&position))
        };
        let (hovered, hhovered) = (on_thumb(&bar), on_thumb(&hbar));
        let most_x = self.code_scroll_x_limit(&laid);
        let Some(code) = self.code.as_mut() else {
            return false;
        };
        if (code.bar.hovered, code.bar.hhovered) != (hovered, hhovered) {
            code.bar.hovered = hovered;
            code.bar.hhovered = hhovered;
            cx.notify();
        }
        if let (Some(grab), Some(hbar)) = (code.bar.hgrab, hbar) {
            let room = hbar.track.size.width - hbar.thumb.size.width;
            if room > px(0.) {
                let left = (position.x - grab - hbar.track.left()).clamp(px(0.), room);
                self.scroll.x = most_x * (left / room);
            }
            self.clamp_scroll(&laid, bounds);
            self.code_scrolled(cx);
            cx.notify();
            return true;
        }
        let (Some(grab), Some(bar)) = (code.bar.grab, bar) else {
            return false;
        };
        let limit = self.code_scroll_limit(&laid).unwrap_or_default();
        let room = bar.track.size.height - bar.thumb.size.height;
        if room > px(0.) {
            let top = (position.y - grab - bar.track.top()).clamp(px(0.), room);
            self.scroll.y = limit * (top / room);
        }
        self.clamp_scroll(&laid, bounds);
        self.code_scrolled(cx);
        cx.notify();
        true
    }

    /// Lets go of the thumb. True when it was taken.
    pub(super) fn bar_release(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(code) = self.code.as_mut() else {
            return false;
        };
        let taken = code.bar.grab.take().is_some() | code.bar.hgrab.take().is_some();
        if taken {
            cx.notify();
        }
        taken
    }

    /// The markers on the scrollbar `bar`, as Zed's editor paints them: each other place of the
    /// selected text in the middle third, then the caret over the full width.
    pub(super) fn bar_markers(
        &self,
        laid: &Layout,
        bar: &BarBounds,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, Hsla)> {
        let Some(code) = self.code.as_ref() else {
            return Vec::new();
        };
        let line_height = laid.line_height;
        let scroll_rows = laid.rows.saturating_sub(1) as f32;
        let unit = if scroll_rows > 0. {
            (bar.track.size.height - bar.thumb.size.height) / scroll_rows
        } else {
            line_height
        };
        let row = |at: usize| (laid.position(self.to_display(at), false).y / line_height).round();
        let border = px(1.);
        let column = ((BAR_WIDTH - border) / 3.).floor();
        let (left, top) = (bar.track.left(), bar.track.top());
        let mut markers: Vec<(Bounds<Pixels>, Hsla)> = Vec::new();
        let info = cx.theme().status().info;
        let selected = self.model.selection().range();
        for range in code.other_occurrences(selected) {
            let start = top + unit * row(range.start);
            let end = top + unit * row(range.end) + unit.max(MIN_MARKER);
            // Touching places merge into one marker.
            if let Some((last, _)) = markers.last_mut()
                && last.bottom() >= start - px(1.)
            {
                last.size.height = end.max(last.bottom()) - last.top();
                continue;
            }
            markers.push((
                Bounds::from_corners(
                    point(left + border + column, start),
                    point(left + border + column * 2., end),
                ),
                info,
            ));
        }
        let head = top + unit * row(self.model.head());
        markers.push((
            Bounds::from_corners(
                point(left + border, head),
                point(left + BAR_WIDTH, head + CURSOR_MARKER),
            ),
            cx.theme().players().local().cursor,
        ));
        markers
    }
}
