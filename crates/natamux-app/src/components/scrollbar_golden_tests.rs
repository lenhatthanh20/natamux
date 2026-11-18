//! The metrics golden of the scrollbar (A35): the scroll areas the app builds, drawn under the
//! kit's metrics capture, through pointer steps (enter, hover the thumb, press and drag it, let
//! go, wheel, press the lane, leave). Each step's frame and scroll offset must equal the golden,
//! which `Tool: golden` made from Zed's `ui` bar before it left (S8.4). A fading bar depends on
//! the wall clock, so the `Auto` rows hold only the frames after a fade has ended; the hide wait
//! runs on the test clock. The fade itself and the show rules are unit tests of the kit
//! (`scrollbar_state_tests.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyWindowHandle, Axis, HeadlessAppContext, Hsla, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PlatformInput, Point, ScrollDelta, ScrollHandle,
    ScrollWheelEvent, Stateful, TouchPhase, point, size,
};
use ui::prelude::*;
use ui::{ScrollAxes, ScrollbarShow, Scrollbars, WithScrollbar};

use crate::components::kit_metrics::{CONFIGS, Capture, Config, context, redraw};

/// The area: 200 x 100 px, 16 px from the window's top left.
const PAD: f32 = 16.;
const WIDTH: f32 = 200.;
const HEIGHT: f32 = 100.;

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

struct Area {
    sample: Sample,
    handle: ScrollHandle,
    /// `scroll_area` shows and hides on its own, as in the app.
    auto: bool,
}

fn squares(count: usize) -> Vec<Div> {
    (0..count)
        .map(|_| div().flex_none().size(px(20.)))
        .collect()
}

impl Area {
    /// What `scroll_area` builds: the scroll div in a wrapper that carries the bars.
    fn scroll_area(&self, area: Stateful<Div>, window: &mut Window, cx: &mut App) -> AnyElement {
        let show = if matches!(self.sample, Sample::Never) {
            ScrollbarShow::Never
        } else if self.auto {
            ScrollbarShow::Auto
        } else {
            ScrollbarShow::Always
        };
        let bars = Scrollbars::shown(ScrollAxes::Both, show)
            .tracked_scroll_handle(&self.handle)
            .id("area-bar");
        div()
            .child(area)
            .custom_scrollbars(bars, window, cx)
            .size_full()
            .into_any_element()
    }

    /// The Markdown code block: a sideways bar with a track, put on the frame (a plain div).
    fn code_block(&self, track: Hsla, window: &mut Window, cx: &mut App) -> AnyElement {
        let bars = Scrollbars::always_visible(ScrollAxes::Horizontal)
            .id("code-bar")
            .tracked_scroll_handle(&self.handle)
            .with_track_along(ScrollAxes::Horizontal, track)
            .notify_content();
        let frame = div()
            .relative()
            .size_full()
            .rounded_lg()
            .custom_scrollbars(bars, window, cx);
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
        let bars = Scrollbars::always_visible(ScrollAxes::Horizontal)
            .id("table-bar")
            .notify_content();
        div()
            .id("table")
            .size_full()
            .flex()
            .restrict_scroll_to_axis()
            .custom_scrollbars(bars, window, cx)
            .children(squares(30))
            .into_any_element()
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
                let area = scroll_div()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(squares(20));
                self.scroll_area(area, window, cx)
            }
            Sample::Row => {
                let area = scroll_div()
                    .flex_row()
                    .overflow_x_scroll()
                    .children(squares(20));
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

/// Each step's frame and scroll offset for `sample`.
fn rows(config: &Config, sample: Sample) -> Vec<String> {
    let (mut cx, capture, _) = context(config);
    let handle = ScrollHandle::new();
    let shared = handle.clone();
    let window: AnyWindowHandle = cx
        .open_window(size(px(480.), px(400.)), move |_, cx| {
            cx.new(|_| Area {
                sample,
                handle: shared,
                auto: false,
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
        cx.update_window(window, |_, window, cx| {
            step(name, sample.axis(), window, cx)
        })
        .expect("the window is open");
        cx.run_until_parked();
        redraw(&mut cx, window);
        cx.capture_screenshot(window)
            .expect("the recorder captures");
        let painted = capture
            .borrow_mut()
            .painted
            .take()
            .expect("a captured frame");
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

/// The offset and the frame of a row, apart.
fn parts(row: &str) -> (&str, &str) {
    let (head, painted) = row.split_once(", painted ").unwrap();
    (head.split_once(": ").unwrap().1, painted)
}

/// The steps must reach the bars, else the golden proves nothing: the hover paints
/// another thumb, the drag scrolls, and a press on a lane scrolls only where a track is drawn.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_pointer_steps_reach_the_bars() {
    let column = rows(&CONFIGS[0], Sample::Column);
    let at = |name: &str| STEPS.iter().position(|step| *step == name).unwrap();
    let (rest, rest_painted) = parts(&column[at("rest")]);
    assert_eq!(rest, "offset 0.0 0.0");
    assert_ne!(rest_painted, parts(&column[at("hover")]).1);
    assert_ne!(parts(&column[at("drag")]).0, rest);
    let row = rows(&CONFIGS[0], Sample::Row);
    assert_eq!(parts(&row[at("lane")]).0, parts(&row[at("wheel")]).0);
    let track = rows(&CONFIGS[0], Sample::Track);
    assert_ne!(parts(&track[at("lane")]).0, parts(&track[at("wheel")]).0);
}

/// Draws one frame and returns what it painted.
fn frame(
    cx: &mut HeadlessAppContext,
    capture: &Rc<RefCell<Capture>>,
    window: AnyWindowHandle,
) -> String {
    redraw(cx, window);
    cx.capture_screenshot(window)
        .expect("the recorder captures");
    let painted = capture
        .borrow_mut()
        .painted
        .take()
        .expect("a captured frame");
    painted.to_string()
}

/// Frames until a fade of at most `fade` has ended. A fade counts from the frame that starts it
/// on the wall clock, and a bar may show a fade one frame late, so: a frame, the wait, two more.
fn settle(
    cx: &mut HeadlessAppContext,
    capture: &Rc<RefCell<Capture>>,
    window: AnyWindowHandle,
    fade: Duration,
) -> String {
    frame(cx, capture, window);
    std::thread::sleep(fade + Duration::from_millis(50));
    frame(cx, capture, window);
    frame(cx, capture, window)
}

const AUTO_STEPS: [&str; 6] = [
    "shown",
    "waited",
    "enter",
    "hover-waited",
    "left-waited",
    "wheel",
];

/// The column area on `Auto`, through the steps of `AUTO_STEPS`: each one's offset and the frame
/// once its fade has ended.
fn auto_rows(config: &Config) -> Vec<String> {
    let rise = Duration::from_millis(50);
    let fall = Duration::from_millis(400);
    // The hide wait is 1 s on the test clock.
    let wait = Duration::from_millis(1100);
    let (mut cx, capture, _) = context(config);
    let handle = ScrollHandle::new();
    let shared = handle.clone();
    let window: AnyWindowHandle = cx
        .open_window(size(px(480.), px(400.)), move |_, cx| {
            cx.new(|_| Area {
                sample: Sample::Column,
                handle: shared,
                auto: true,
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
    let inside = point(px(PAD + 60.), px(PAD + 40.));
    let outside = point(px(400.), px(380.));
    let thumb = on_bar(Axis::Vertical, 10.);
    let mut rows = Vec::new();
    for name in AUTO_STEPS {
        let pointer = |at: Point<Pixels>, cx: &mut HeadlessAppContext| {
            cx.update_window(window, |_, window, cx| mouse_move(at, None, window, cx))
                .expect("the window is open");
            cx.run_until_parked();
        };
        let wait_out = |cx: &mut HeadlessAppContext| {
            cx.advance_clock(wait);
            cx.run_until_parked();
        };
        let fade = match name {
            // The first frames fade the bar in.
            "shown" => rise,
            "waited" => {
                wait_out(&mut cx);
                fall
            }
            "enter" => {
                pointer(outside, &mut cx);
                pointer(inside, &mut cx);
                rise
            }
            "hover-waited" => {
                pointer(thumb, &mut cx);
                wait_out(&mut cx);
                fall
            }
            "left-waited" => {
                pointer(outside, &mut cx);
                wait_out(&mut cx);
                fall
            }
            "wheel" => {
                cx.update_window(window, |_, window, cx| {
                    wheel(inside, Axis::Vertical, window, cx)
                })
                .expect("the window is open");
                cx.run_until_parked();
                rise
            }
            other => panic!("no step {other}"),
        };
        let painted = settle(&mut cx, &capture, window, fade);
        let offset = handle.offset();
        rows.push(format!(
            "{} auto {name}: offset {:?} {:?}, painted {painted}",
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

/// On `Auto` (the app's mode) the bar shows at first, hides a second after the last show, shows
/// again when the pointer comes in, stays while its thumb is hovered, hides after the pointer
/// leaves, and shows on a scroll. The golden holds the frames; this checks the steps reach it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_auto_steps_reach_the_bar() {
    let own = auto_rows(&CONFIGS[0]);
    let at = |name: &str| AUTO_STEPS.iter().position(|step| *step == name).unwrap();
    let painted = |name: &str| parts(&own[at(name)]).1.to_string();
    // The steps reach the bar: hidden after each wait unless hovered, shown again otherwise.
    assert_ne!(painted("shown"), painted("waited"));
    assert_eq!(painted("shown"), painted("enter"));
    assert_ne!(painted("hover-waited"), painted("waited"));
    assert_ne!(painted("hover-waited"), painted("enter"));
    assert_eq!(painted("left-waited"), painted("waited"));
    assert_ne!(painted("wheel"), painted("waited"));
    assert_ne!(parts(&own[at("wheel")]).0, parts(&own[at("shown")]).0);
}

/// Every row: the pointer steps of each sample under the first two configs, then the `Auto`
/// steps. One row per line.
fn golden_rows() -> String {
    let mut out = Vec::new();
    for config in &CONFIGS[..2] {
        for sample in Sample::ALL {
            out.extend(rows(config, sample));
        }
    }
    out.extend(auto_rows(&CONFIGS[0]));
    out.join("\n") + "\n"
}

/// The committed golden of this OS.
fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/kit")
        .join(std::env::consts::OS)
        .join("scrollbar.txt")
}

/// Writes the rows to `$NATAMUX_GOLDEN_OUT/kit/scrollbar.txt` (`Tool: golden`, plan 8.4). The
/// committed file was made on Zed's bar.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "makes the golden file; run by the Tool: golden workflow"]
fn golden_kit_scrollbar() {
    let out = std::env::var("NATAMUX_GOLDEN_OUT").expect("NATAMUX_GOLDEN_OUT is set");
    let dir = std::path::Path::new(&out).join("kit");
    std::fs::create_dir_all(&dir).expect("the output folder");
    std::fs::write(dir.join("scrollbar.txt"), golden_rows()).expect("the golden file");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_scrollbar_draws_and_acts_as_the_golden_holds() {
    let path = golden_path();
    let golden = std::fs::read_to_string(&path).expect("the golden file of this OS");
    let now = golden_rows();
    let differ: Vec<String> = golden
        .lines()
        .zip(now.lines())
        .filter(|(want, got)| want != got)
        .take(4)
        .map(|(want, got)| format!("golden: {want}\nnow:    {got}"))
        .collect();
    assert!(
        differ.is_empty() && golden.lines().count() == now.lines().count(),
        "{} differs ({} rows in the golden, {} now); first rows that differ:\n{}",
        path.display(),
        golden.lines().count(),
        now.lines().count(),
        differ.join("\n")
    );
}

#[path = "scrollbar_cached_tests.rs"]
mod cached;
