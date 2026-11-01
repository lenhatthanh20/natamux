//! The connectors of the Progress node flow (T-0246): the straight link between two steps, the fork and join curves of a pair, the join dot and the dot grid of
//! the canvas. Lines are painted on a `canvas` with `PathBuilder`, as the git graph does. Nothing
//! here moves (owner, T-0164): only the running stage's `StateGlyph` blinks.

// The Progress view uses them; this allow keeps a build green while it is wired in parts.
#![cfg_attr(not(test), allow(dead_code))]

use gpui::{Hsla, PathBuilder, Pixels, Point, canvas, point};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::flow_rules::{
    CARET, CURVE_H, CaretAt, GRID, JOIN_DOT, LINK_H, LinkStyle, PAIR_GAP, PORT_OUT, STROKE,
    Stroke, caret_x, pair_centres,
};
use crate::components::colors::state_colors;

/// The px of one rem when the UI size is 100%; the flow's sizes are given at this size.
const BASE_REM: f32 = 16.;

/// The colour of a connector or a port of that style.
pub fn link_color(style: LinkStyle, cx: &App) -> Hsla {
    let muted = cx.theme().colors().text_muted;
    match style {
        LinkStyle::Done => state_colors(TaskState::Done, cx).fg,
        LinkStyle::Current(state) => state_colors(state, cx).fg,
        LinkStyle::Upcoming => muted.opacity(0.55),
        LinkStyle::NotTaken | LinkStyle::Skipped => muted.opacity(0.35),
    }
}

/// What a canvas needs to stroke one connector; read from the theme before painting.
#[derive(Clone, Copy)]
struct Pen {
    color: Hsla,
    stroke: Stroke,
}

fn pen(style: LinkStyle, cx: &App) -> Pen {
    Pen {
        color: link_color(style, cx),
        stroke: style.stroke(),
    }
}

/// The UI size: canvas sizes are in px, the layout around them in rems.
fn scale(window: &Window) -> f32 {
    f32::from(window.rem_size()) / BASE_REM
}

/// Strokes a straight line, or a curve when `controls` are given.
fn draw(
    window: &mut Window,
    pen: Pen,
    scale: f32,
    from: Point<Pixels>,
    to: Point<Pixels>,
    controls: Option<(Point<Pixels>, Point<Pixels>)>,
) {
    let builder = PathBuilder::stroke(px(STROKE));
    let mut path = match pen.stroke {
        Stroke::Solid => builder,
        Stroke::Dashed => builder.dash_array(&[px(3. * scale), px(4. * scale)]),
        Stroke::Dotted => builder.dash_array(&[px(STROKE), px(3. * scale)]),
    };
    path.move_to(from);
    match controls {
        Some((first, second)) => path.cubic_bezier_to(to, first, second),
        None => path.line_to(to),
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, pen.color);
    }
}

/// The straight connector between two steps, down the middle of the column. By default it starts
/// below the output port of the step above, so it does not draw over the ring.
#[derive(IntoElement)]
pub struct FlowLink {
    style: LinkStyle,
    grow: bool,
    from_top: bool,
}

impl FlowLink {
    pub fn new(style: LinkStyle) -> Self {
        Self {
            style,
            grow: false,
            from_top: false,
        }
    }

    /// Takes the height its parent gives (`flex_1`), at least `LINK_H`.
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Starts at the very top: no port above it (a join dot, a group pill).
    pub fn at_top(mut self) -> Self {
        self.from_top = true;
        self
    }
}

impl RenderOnce for FlowLink {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pen = pen(self.style, cx);
        let start = if self.from_top { 0. } else { PORT_OUT };
        let line = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let scale = scale(window);
                let x = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.;
                let top = f32::from(bounds.origin.y);
                let from = point(px(x), px(top + start * scale));
                let to = point(px(x), px(top + f32::from(bounds.size.height)));
                draw(window, pen, scale, from, to, None);
            },
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .w_full()
            .min_h(rems_from_px(LINK_H))
            .when(self.grow, |this| this.flex_1())
            .when(!self.grow, |this| this.flex_none().h(rems_from_px(LINK_H)))
            .child(line)
    }
}

/// The caret on top of a details card, pointing up at the selected node (mockup A of T-0257). Put
/// it as a child of the `relative()` card: it stands on the card's top border and covers the
/// border under its base, in the card's colours.
#[derive(IntoElement)]
pub struct Caret {
    at: CaretAt,
}

impl Caret {
    pub fn new(at: CaretAt) -> Self {
        Self { at }
    }
}

impl RenderOnce for Caret {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (fill, rim) = (colors.elevated_surface_background, colors.border);
        let at = self.at;
        let shape = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let scale = scale(window);
                let width = f32::from(bounds.size.width);
                let x = f32::from(bounds.origin.x) + caret_x(at, width, PAIR_GAP * scale);
                // The base is 1 px into the card, over its top border.
                let base = f32::from(bounds.origin.y) + f32::from(bounds.size.height);
                let half = CARET * scale;
                let tip = point(px(x), px(base - 1. - half));
                let (left, right) = (point(px(x - half), px(base)), point(px(x + half), px(base)));
                let mut body = PathBuilder::fill();
                body.move_to(left);
                body.line_to(tip);
                body.line_to(right);
                body.close();
                if let Ok(path) = body.build() {
                    window.paint_path(path, fill);
                }
                let mut edge = PathBuilder::stroke(px(1.));
                edge.move_to(left);
                edge.line_to(tip);
                edge.line_to(right);
                if let Ok(path) = edge.build() {
                    window.paint_path(path, rim);
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(rems_from_px(-CARET))
            .h(rems_from_px(CARET + 1.))
            .child(shape)
    }
}

/// The two curves of a pair: a fork from the middle down to the two nodes, or a join from their
/// output ports back to the middle. Its width must be the pair row's width (same parent, same
/// side insets), and the row must space its two `flex_1` nodes `PAIR_GAP` apart.
#[derive(IntoElement)]
pub struct PairCurves {
    left: LinkStyle,
    right: LinkStyle,
    join: bool,
}

impl PairCurves {
    /// Above the pair; each style is the connector into that node.
    pub fn fork(left: LinkStyle, right: LinkStyle) -> Self {
        Self {
            left,
            right,
            join: false,
        }
    }

    /// Below the pair; each style is that node's way out (`join_style`).
    pub fn join(left: LinkStyle, right: LinkStyle) -> Self {
        Self {
            left,
            right,
            join: true,
        }
    }
}

impl RenderOnce for PairCurves {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pens = [pen(self.left, cx), pen(self.right, cx)];
        let join = self.join;
        let curves = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let scale = scale(window);
                let left = f32::from(bounds.origin.x);
                let top = f32::from(bounds.origin.y);
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let (first, second) = pair_centres(width, PAIR_GAP * scale);
                let middle = width / 2.;
                let at = |x: f32, y: f32| point(px(left + x), px(top + y));
                for (pen, x) in pens.into_iter().zip([first, second]) {
                    // A join starts below the output port, so it does not draw over the ring.
                    let ((x0, y0), (x1, y1)) = if join {
                        ((x, PORT_OUT * scale), (middle, height))
                    } else {
                        ((middle, 0.), (x, height))
                    };
                    let span = y1 - y0;
                    let controls = (at(x0, y0 + span * 0.6), at(x1, y0 + span * 0.4));
                    draw(window, pen, scale, at(x0, y0), at(x1, y1), Some(controls));
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .w_full()
            .flex_none()
            .h(rems_from_px(CURVE_H))
            .child(curves)
    }
}

/// The dot where a group joins back, with its note at the right ("Review starts when both are
/// finished"). With `below`, the connector to the next step hangs under the dot and grows with a
/// note of two lines, so the caller adds no `FlowLink` after it.
#[derive(IntoElement)]
pub struct JoinDot {
    style: LinkStyle,
    note: Option<SharedString>,
    below: Option<LinkStyle>,
}

impl JoinDot {
    /// `style`: `join_style` of the group's status. Filled once the group is done.
    pub fn new(style: LinkStyle) -> Self {
        Self {
            style,
            note: None,
            below: None,
        }
    }

    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn below(mut self, style: LinkStyle) -> Self {
        self.below = Some(style);
        self
    }
}

impl RenderOnce for JoinDot {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = link_color(self.style, cx);
        let fill = if self.style == LinkStyle::Done {
            color
        } else {
            cx.theme().colors().panel_background
        };
        let dot = div()
            .flex_none()
            .w(rems_from_px(JOIN_DOT))
            .h(rems_from_px(JOIN_DOT))
            .rounded_full()
            .border_1()
            .border_color(color)
            .bg(fill);
        let middle = v_flex()
            .flex_none()
            .w(rems_from_px(JOIN_DOT))
            .items_center()
            .child(dot)
            .when_some(self.below, |this, below| {
                this.min_h(rems_from_px(JOIN_DOT + LINK_H))
                    .child(FlowLink::new(below).at_top().grow())
            });
        // Equal flex_1 sides keep the dot on the middle line whatever the note's length.
        let note = div()
            .flex_1()
            .min_w_0()
            .pl(rems_from_px(6.0_f32))
            .mt(rems_from_px(-4.0_f32))
            .when_some(self.note, |this, note| {
                this.child(Label::new(note).size(LabelSize::XSmall).color(Color::Muted))
            });
        div()
            .flex()
            .w_full()
            .child(div().flex_1())
            .child(middle)
            .child(note)
    }
}

/// The faint dot grid behind the flow: put it as the first child of a `relative()` parent. Only
/// the dots inside the visible area (the content mask) are painted, about 20 x 40 small quads for
/// a 320 px panel, once per frame the window draws; a still panel draws no frames. The rows are
/// fixed to the window, so the flow scrolls over them.
pub fn dot_grid(cx: &App) -> impl IntoElement {
    let color = cx.theme().colors().border_variant;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let step = GRID * scale(window);
            if step < 1. {
                return;
            }
            let seen = bounds.intersect(&window.content_mask().bounds);
            let left = f32::from(bounds.origin.x);
            let (x0, y0) = (f32::from(seen.origin.x), f32::from(seen.origin.y));
            let x1 = x0 + f32::from(seen.size.width);
            let y1 = y0 + f32::from(seen.size.height);
            // Rows line up with the window, not this step: each step paints its own grid, and
            // steps of any height then meet without a seam. Columns start half a step in.
            let first = |start: f32, edge: f32| {
                edge + step / 2. + ((start - edge - step / 2.) / step).ceil().max(0.) * step
            };
            let dot = gpui::size(px(1.), px(1.));
            let mut y = first(y0, 0.);
            while y < y1 {
                let mut x = first(x0, left);
                while x < x1 {
                    let origin = point(px(x), px(y));
                    window.paint_quad(gpui::fill(gpui::Bounds { origin, size: dot }, color));
                    x += step;
                }
                y += step;
            }
        },
    )
    .absolute()
    .inset_0()
}
