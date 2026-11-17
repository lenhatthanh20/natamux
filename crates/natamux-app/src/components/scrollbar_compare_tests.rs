//! The kit's own scrollbar against Zed's `ui` one (A35): the scroll areas the app builds, drawn
//! by each bar under the kit's metrics capture, through the same pointer steps (enter, hover the
//! thumb, press and drag it, let go, wheel, press the lane, leave). Every step must paint the same
//! frame and leave the same scroll offset. Shown bars only: a fading bar depends on the clock, so
//! the fade, the hide timing and the show rules are unit tests of the kit (`scrollbar_tests.rs`).
//! Zed's bar leaves with the `zed_ui` dependency: before that, freeze its rows into a golden file
//! and compare the own rows with it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{
    AnyWindowHandle, Axis, Hsla, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PlatformInput, Point, ScrollDelta, ScrollHandle, ScrollWheelEvent, TouchPhase,
    point, size,
};
use ui::prelude::*;
use ui::zed_scrollbars as zed;
use ui::{ScrollAxes, ScrollbarShow, Scrollbars, WithScrollbar};

use crate::components::kit_metrics::{CONFIGS, Config, context, redraw};

/// The area: 200 x 100 px, 16 px from the window's top left.
const PAD: f32 = 16.;
const WIDTH: f32 = 200.;
const HEIGHT: f32 = 100.;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Zed,
    Own,
}

/// The app's kinds of scroll area.
#[derive(Clone, Copy, Debug)]
enum Sample {
    /// `scroll_area` over a column of 20 rows: bars on both axes, one with something to scroll.
    Column,
    /// `scroll_area` over a row of 20 squares.
    Row,
    /// `scroll_area` over rows wider and taller than the area: two bars.
    Grid,
    /// The Markdown code block: its own scroll div in a frame, a track under a sideways bar.
    Track,
    /// The Markdown table: the bar's own handle makes the div it is put on scroll.
    OwnHandle,
    /// `scroll_area` while motion is still: no bar.
    Never,
}

impl Sample {
    const ALL: [Sample; 6] = [
        Sample::Column,
        Sample::Row,
        Sample::Grid,
        Sample::Track,
        Sample::OwnHandle,
        Sample::Never,
    ];

    /// The axis the pointer steps work on.
    fn axis(self) -> Axis {
        match self {
            Sample::Row | Sample::Track | Sample::OwnHandle => Axis::Horizontal,
            Sample::Column | Sample::Grid | Sample::Never => Axis::Vertical,
        }
    }
}

#[derive(Default)]
struct AlwaysShown;

impl zed::ScrollbarVisibility for AlwaysShown {
    fn visibility(&self, _: &App) -> zed::ShowScrollbar {
        zed::ShowScrollbar::Always
    }
}

#[derive(Default)]
struct NeverShown;

impl zed::ScrollbarVisibility for NeverShown {
    fn visibility(&self, _: &App) -> zed::ShowScrollbar {
        zed::ShowScrollbar::Never
    }
}

struct Area {
    sample: Sample,
    side: Side,
    handle: ScrollHandle,
}

fn squares(count: usize) -> Vec<Div> {
    (0..count)
        .map(|_| div().flex_none().size(px(20.)))
        .collect()
}

impl Area {
    /// What `scroll_area` builds: the scroll div in a wrapper that carries the bars.
    fn scroll_area(&self, area: Stateful<Div>, window: &mut Window, cx: &mut App) -> AnyElement {
        let never = matches!(self.sample, Sample::Never);
        let wrapper = div().child(area);
        let wrapper = match self.side {
            Side::Zed => {
                let bars = if never {
                    zed::Scrollbars::for_settings::<NeverShown>()
                } else {
                    zed::Scrollbars::for_settings::<AlwaysShown>()
                };
                let bars = bars
                    .show_along(zed::ScrollAxes::Vertical)
                    .tracked_scroll_handle(&self.handle)
                    .id("area-bar");
                zed::WithScrollbar::custom_scrollbars(wrapper, bars, window, cx)
            }
            Side::Own => {
                let show = if never {
                    ScrollbarShow::Never
                } else {
                    ScrollbarShow::Always
                };
                let bars = Scrollbars::shown(ScrollAxes::Both, show)
                    .tracked_scroll_handle(&self.handle)
                    .id("area-bar");
                WithScrollbar::custom_scrollbars(wrapper, bars, window, cx)
            }
        };
        wrapper.size_full().into_any_element()
    }

    /// The Markdown code block: a sideways bar with a track, put on the frame (a plain div).
    fn code_block(&self, track: Hsla, window: &mut Window, cx: &mut App) -> AnyElement {
        let frame = div().relative().size_full().rounded_lg();
        let frame = match self.side {
            Side::Zed => {
                let bars = zed::Scrollbars::always_visible(zed::ScrollAxes::Horizontal)
                    .id("code-bar")
                    .tracked_scroll_handle(&self.handle)
                    .with_track_along(zed::ScrollAxes::Horizontal, track)
                    .notify_content();
                zed::WithScrollbar::custom_scrollbars(frame, bars, window, cx)
            }
            Side::Own => {
                let bars = Scrollbars::always_visible(ScrollAxes::Horizontal)
                    .id("code-bar")
                    .tracked_scroll_handle(&self.handle)
                    .with_track_along(ScrollAxes::Horizontal, track)
                    .notify_content();
                WithScrollbar::custom_scrollbars(frame, bars, window, cx)
            }
        };
        let body = div()
            .id("code")
            .flex()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&self.handle)
            .children(squares(30));
        // No padding top or bottom, so the room the track keeps shows.
        frame
            .px(px(12.))
            .border_1()
            .border_color(track)
            .child(body)
            .into_any_element()
    }

    /// The Markdown table: a sideways bar on a scroll div of its own, with its own handle.
    fn table(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let grid = div()
            .id("table")
            .size_full()
            .flex()
            .restrict_scroll_to_axis();
        let grid = match self.side {
            Side::Zed => {
                let bars = zed::Scrollbars::always_visible(zed::ScrollAxes::Horizontal)
                    .id("table-bar")
                    .notify_content();
                zed::WithScrollbar::custom_scrollbars(grid, bars, window, cx)
            }
            Side::Own => {
                let bars = Scrollbars::always_visible(ScrollAxes::Horizontal)
                    .id("table-bar")
                    .notify_content();
                WithScrollbar::custom_scrollbars(grid, bars, window, cx)
            }
        };
        grid.children(squares(30)).into_any_element()
    }
}

impl Render for Area {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (background, track) = {
            let colours = cx.theme().colors();
            (colours.background, colours.editor_background)
        };
        let scroll_div = || {
            div()
                .id("area")
                .size_full()
                .flex()
                .px_4()
                .py_3()
                .track_scroll(&self.handle)
        };
        let shown = match self.sample {
            Sample::Column | Sample::Never => {
                let area = scroll_div().flex_col().overflow_y_scroll().children(squares(20));
                self.scroll_area(area, window, cx)
            }
            Sample::Row => {
                let area = scroll_div().flex_row().overflow_x_scroll().children(squares(20));
                self.scroll_area(area, window, cx)
            }
            Sample::Grid => {
                let rows = (0..20).map(|_| div().flex_none().w(px(400.)).h(px(20.)));
                let area = scroll_div().flex_col().overflow_scroll().children(rows);
                self.scroll_area(area, window, cx)
            }
            Sample::Track => self.code_block(track, window, cx),
            Sample::OwnHandle => self.table(window, cx),
        };
        div()
            .size_full()
            .bg(background)
            .p(px(PAD))
            .child(div().w(px(WIDTH)).h(px(HEIGHT)).child(shown))
    }
}

/// A point along `axis` at `along` px from the area's start, in the middle of its bar's lane
/// across.
fn on_bar(axis: Axis, along: f32) -> Point<Pixels> {
    match axis {
        // The lane is 14 px; with a 1 px frame border (code block) its middle is 1 px higher.
        Axis::Horizontal => point(px(PAD + along), px(PAD + HEIGHT - 7.5)),
        Axis::Vertical => point(px(PAD + WIDTH - 7.), px(PAD + along)),
    }
}

fn mouse_move(at: Point<Pixels>, pressed: Option<MouseButton>, window: &mut Window, cx: &mut App) {
    let event = MouseMoveEvent {
        position: at,
        pressed_button: pressed,
        modifiers: Modifiers::default(),
    };
    let _ = window.dispatch_event(PlatformInput::MouseMove(event), cx);
}

fn mouse_down(at: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let event = MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    };
    let _ = window.dispatch_event(PlatformInput::MouseDown(event), cx);
}

fn mouse_up(at: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let event = MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    let _ = window.dispatch_event(PlatformInput::MouseUp(event), cx);
}

fn wheel(at: Point<Pixels>, axis: Axis, window: &mut Window, cx: &mut App) {
    let delta = match axis {
        Axis::Horizontal => point(px(-20.), px(0.)),
        Axis::Vertical => point(px(0.), px(-20.)),
    };
    let event = ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(delta),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    };
    let _ = window.dispatch_event(PlatformInput::ScrollWheel(event), cx);
}

const STEPS: [&str; 11] = [
    "rest", "enter", "hover", "press", "drag", "release", "wheel", "lane", "lane-up", "leave",
    "back",
];

/// Does one pointer step of `STEPS` on a bar along `axis`.
fn step(name: &str, axis: Axis, window: &mut Window, cx: &mut App) {
    let inside = point(px(PAD + 60.), px(PAD + 40.));
    let thumb = on_bar(axis, 10.);
    let dragged = on_bar(axis, 40.);
    let lane_end = on_bar(axis, if axis == Axis::Vertical { 94. } else { 194. });
    match name {
        "rest" => {}
        "enter" | "back" => mouse_move(inside, None, window, cx),
        "hover" => mouse_move(thumb, None, window, cx),
        "press" => mouse_down(thumb, window, cx),
        "drag" => mouse_move(dragged, Some(MouseButton::Left), window, cx),
        "release" => mouse_up(dragged, window, cx),
        "wheel" => wheel(inside, axis, window, cx),
        "lane" => mouse_down(lane_end, window, cx),
        "lane-up" => mouse_up(lane_end, window, cx),
        "leave" => mouse_move(point(px(400.), px(380.)), None, window, cx),
        other => panic!("no step {other}"),
    }
    window.refresh();
}

/// Each step's frame and scroll offset for `sample` drawn by `side`.
fn rows(config: &Config, sample: Sample, side: Side) -> Vec<String> {
    let (mut cx, capture, _) = context(config);
    let handle = ScrollHandle::new();
    let shared = handle.clone();
    let window: AnyWindowHandle = cx
        .open_window(size(px(480.), px(400.)), move |_, cx| {
            cx.new(|_| Area {
                sample,
                side,
                handle: shared,
            })
        })
        .expect("a window")
        .into();
    redraw(&mut cx, window);
    let scale = cx
        .update_window(window, |_, window, _| window.scale_factor())
        .expect("the window is open");
    {
        let mut capture = capture.borrow_mut();
        capture.origin = (0., 0.);
        capture.scale = scale;
    }
    let mut rows = Vec::new();
    for name in STEPS {
        cx.update_window(window, |_, window, cx| step(name, sample.axis(), window, cx))
            .expect("the window is open");
        cx.run_until_parked();
        redraw(&mut cx, window);
        cx.capture_screenshot(window).expect("the recorder captures");
        let painted = capture.borrow_mut().painted.take().expect("a captured frame");
        let offset = handle.offset();
        rows.push(format!(
            "{} {sample:?} {name}: offset {:?} {:?}, painted {painted}",
            config.name,
            f32::from(offset.x),
            f32::from(offset.y),
        ));
    }
    cx.update_window(window, |_, window, _| window.remove_window())
        .expect("the window is open");
    cx.run_until_parked();
    rows
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_own_scrollbar_draws_and_acts_as_zeds() {
    let mut differ = Vec::new();
    for config in &CONFIGS[..2] {
        for sample in Sample::ALL {
            let zed = rows(config, sample, Side::Zed);
            let own = rows(config, sample, Side::Own);
            assert_eq!(zed.len(), own.len());
            differ.extend(
                zed.iter()
                    .zip(&own)
                    .filter(|(zed, own)| zed != own)
                    .map(|(zed, own)| format!("zed: {zed}\nown: {own}")),
            );
        }
    }
    assert!(
        differ.is_empty(),
        "{} steps differ; first ones:\n{}",
        differ.len(),
        differ.iter().take(4).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// The offset and the frame of a row, apart.
fn parts(row: &str) -> (&str, &str) {
    let (head, painted) = row.split_once(", painted ").unwrap();
    (head.split_once(": ").unwrap().1, painted)
}

/// The steps must reach the bars, else the compare above proves nothing: the hover paints
/// another thumb, the drag scrolls, and a press on a lane scrolls only where a track is drawn.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_pointer_steps_reach_the_bars() {
    let column = rows(&CONFIGS[0], Sample::Column, Side::Own);
    let at = |name: &str| STEPS.iter().position(|step| *step == name).unwrap();
    let (rest, rest_painted) = parts(&column[at("rest")]);
    assert_eq!(rest, "offset 0.0 0.0");
    assert_ne!(rest_painted, parts(&column[at("hover")]).1);
    assert_ne!(parts(&column[at("drag")]).0, rest);
    let row = rows(&CONFIGS[0], Sample::Row, Side::Own);
    assert_eq!(parts(&row[at("lane")]).0, parts(&row[at("wheel")]).0);
    let track = rows(&CONFIGS[0], Sample::Track, Side::Own);
    assert_ne!(parts(&track[at("lane")]).0, parts(&track[at("wheel")]).0);
}
