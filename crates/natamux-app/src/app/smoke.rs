//! `--smoke`: CI checks that the app draws and takes typing, on each OS it builds for. With
//! `--fake-daemon` it also checks that the terminal grid of a workspace does not scroll when its
//! panes fit (T-0162), that a click on the status bar's sidebar and details toggles hides and
//! shows them (T-0175), and that the pipeline chart of Project settings scrolls sideways to its
//! last step (T-0163).

use std::time::Duration;

use editor::Editor;
use gpui::{
    App, AppContext as _, AsyncApp, Entity, FocusHandle, Focusable, Keystroke, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Window, WindowHandle,
    point, px,
};

use super::settings_rules::SettingsPage;
use super::shots::{self, RUNNING};
use super::shots_stages::{PROJECT, READ};
use super::status_bar::STATUS_BAR_HEIGHT;
use super::window::Root;

const TYPED: &str = "abc";
/// The fake's terminal list comes at once; the grid is drawn well within this.
const GRID_WAIT: Duration = Duration::from_secs(5);
const GRID_POLL: Duration = Duration::from_millis(100);
/// The grid draws its maximized pane within this.
const GRID_SETTLE: Duration = Duration::from_millis(500);
/// A toggle's panel is drawn, or gone, within this.
const TOGGLE_SETTLE: Duration = Duration::from_millis(300);
/// From the window's edge to a point inside the first or last toggle of the status bar, in the
/// px of `rems_from_px`: the bar's 6 px padding, then about half of the 22 px wide toggle.
const TOGGLE_INSET: f32 = 16.;

type FocusOf = fn(&Root, &App) -> FocusHandle;
type Shown = fn(&Root, &App) -> bool;

/// The end of the status bar a toggle is at.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// Waits for the first frame, types into the text box through the window's own key dispatch, and
/// checks the text on the next frame. `grid`: then checks the terminal grid and the pipeline
/// chart. Exits with 1 on any failure. `quit`: quit once it passed.
pub fn run(handle: WindowHandle<Root>, quit: bool, grid: bool, cx: &mut App) {
    // Read through the editor, not the window: inside a frame callback the window is in use.
    let result = handle.update(cx, |root, window, cx| {
        let filter = root.text_box(cx);
        window.on_next_frame(move |window, cx| {
            type_and_check(filter, handle, quit, grid, window, cx)
        });
    });
    if let Err(error) = result {
        fail(&format!("the window is gone: {error}"));
    }
}

fn type_and_check(
    filter: Entity<Editor>,
    handle: WindowHandle<Root>,
    quit: bool,
    grid: bool,
    window: &mut Window,
    cx: &mut App,
) {
    println!("smoke: first frame drawn");
    for key in TYPED.chars() {
        match Keystroke::parse(&key.to_string()) {
            Ok(keystroke) => {
                window.dispatch_keystroke(keystroke, cx);
            }
            Err(error) => fail(&format!("cannot make the key {key:?}: {error}")),
        }
    }
    window.on_next_frame(move |_, cx| {
        let text = filter.read(cx).text(cx);
        if text != TYPED {
            fail(&format!("the text box has {text:?}, not {TYPED:?}"));
        }
        println!("smoke: typed {text:?} into the text box");
        if grid {
            cx.spawn(async move |cx| check_grid(handle, quit, cx).await)
                .detach();
        } else {
            pass(quit, cx);
        }
    });
}

/// Opens the running task's workspace and fails if its terminal grid can scroll while its panes
/// fit: a wheel over a terminal then moved the whole grid (T-0162). Terminal 1 is maximized
/// first, so the one pane fits at any window size.
async fn check_grid(handle: WindowHandle<Root>, quit: bool, cx: &mut AsyncApp) {
    let opened = handle.update(cx, |root, _, cx| {
        shots::open(root.dashboard(), RUNNING, cx);
        root.workspace_view().read(cx).panes().clone()
    });
    let panes = match opened {
        Ok(panes) => panes,
        Err(error) => fail(&format!("the window is gone: {error}")),
    };
    let mut waited = Duration::ZERO;
    while handle
        .update(cx, |_, _, cx| panes.read(cx).has_rows())
        .is_ok_and(|drawn| !drawn)
    {
        if waited >= GRID_WAIT {
            fail(&format!("the terminal grid of {RUNNING} was not drawn"));
        }
        cx.background_executor().timer(GRID_POLL).await;
        waited += GRID_POLL;
    }
    let maximized = handle.update(cx, |_, _, cx| {
        panes.update(cx, |panes, cx| panes.toggle_maximized("1", cx))
    });
    if let Err(error) = maximized {
        fail(&format!("the window is gone: {error}"));
    }
    cx.background_executor().timer(GRID_SETTLE).await;
    let result = handle.update(cx, |_, _, cx| {
        let range = panes.read(cx).scroll_range();
        if range > px(0.) {
            fail(&format!(
                "the terminal grid scrolls by {} px with nothing to scroll",
                range / px(1.)
            ));
        }
        println!("smoke: the terminal grid of {RUNNING} does not scroll");
    });
    if let Err(error) = result {
        fail(&format!("the window is gone: {error}"));
    }
    check_toggles(handle, cx).await;
    check_chart(handle, quit, cx).await;
}

/// Clicks the sidebar toggle, then the details toggle, twice each: hide, then show again. Once
/// with the focus inside the panel that hides, once with no focus. The clicks did nothing when
/// the focused element was gone or there was none (T-0175).
async fn check_toggles(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    let sidebar: Shown = |root, cx| root.dashboard().read(cx).view().sidebar_open;
    let details: Shown = |root, cx| root.dashboard().read(cx).view().details_open;
    let in_filter: FocusOf = |root, cx| root.filter(cx).focus_handle(cx);
    let in_details: FocusOf = |root, cx| root.details_panel().focus_handle(cx);
    let cases = [
        ("sidebar", Side::Left, Some(in_filter), sidebar),
        ("sidebar", Side::Left, None, sidebar),
        ("details", Side::Right, Some(in_details), details),
        ("details", Side::Right, None, details),
    ];
    for (name, side, focus, shown) in cases {
        click_twice(handle, name, side, focus, shown, cx).await;
    }
}

async fn click_twice(
    handle: WindowHandle<Root>,
    name: &str,
    side: Side,
    focus: Option<FocusOf>,
    shown: Shown,
    cx: &mut AsyncApp,
) {
    let start = handle.update(cx, |root, window, cx| {
        match focus {
            Some(focus) => {
                let target = focus(root, cx);
                window.focus(&target, cx);
            }
            None => window.blur(cx),
        }
        shown(root, cx)
    });
    match start {
        Ok(true) => {}
        Ok(false) => fail(&format!("the {name} is not shown at the start")),
        Err(error) => fail(&format!("the window is gone: {error}")),
    }
    let place = if focus.is_some() {
        "in the panel"
    } else {
        "nowhere"
    };
    cx.background_executor().timer(TOGGLE_SETTLE).await;
    for want in [false, true] {
        click_toggle(handle, side, cx);
        cx.background_executor().timer(TOGGLE_SETTLE).await;
        match handle.update(cx, |root, _, cx| shown(root, cx)) {
            Ok(now) if now == want => {}
            Ok(_) => {
                let verb = if want { "show" } else { "hide" };
                fail(&format!(
                    "a click on the {name} toggle did not {verb} the {name}, focus {place}"
                ));
            }
            Err(error) => fail(&format!("the window is gone: {error}")),
        }
    }
    println!("smoke: a click on the {name} toggle hides and shows the {name}, focus {place}");
}

/// A left click through the window's own mouse dispatch, inside the first or the last toggle
/// of the status bar.
fn click_toggle(handle: WindowHandle<Root>, side: Side, cx: &mut AsyncApp) {
    // Root is not leased here: the toggle's click updates it.
    let clicked = cx.update_window(handle.into(), |_, window, cx| {
        let unit = f32::from(window.rem_size()) / 16.;
        let size = window.viewport_size();
        let inset = px(TOGGLE_INSET * unit);
        let x = match side {
            Side::Left => inset,
            Side::Right => size.width - inset,
        };
        let position = point(x, size.height - px(STATUS_BAR_HEIGHT / 2. * unit));
        let modifiers = Modifiers::default();
        let (button, click_count) = (MouseButton::Left, 1);
        let events = [
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers,
            }),
            PlatformInput::MouseDown(MouseDownEvent {
                button,
                position,
                modifiers,
                click_count,
                first_mouse: false,
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button,
                position,
                modifiers,
                click_count,
            }),
        ];
        for event in events {
            window.dispatch_event(event, cx);
        }
    });
    if let Err(error) = clicked {
        fail(&format!("the window is gone: {error}"));
    }
}

/// Opens Project settings of the fake project, whose pipeline is wider than the page, and fails
/// if the chart cannot scroll to its last step: the right part was cut off (T-0163).
async fn check_chart(handle: WindowHandle<Root>, quit: bool, cx: &mut AsyncApp) {
    let opened = handle.update(cx, |root, window, cx| {
        root.show_settings(SettingsPage::Project(PROJECT.to_owned()), window, cx)
    });
    let dialog = match opened {
        Ok(Some(dialog)) => dialog,
        Ok(None) => fail("Project settings did not open"),
        Err(error) => fail(&format!("the window is gone: {error}")),
    };
    cx.background_executor().timer(READ).await;
    let result = handle.update(cx, |_, _, cx| {
        let scroll = dialog.read(cx).preview_scroll().clone();
        let count = scroll.children_count();
        let Some(last) = count
            .checked_sub(1)
            .and_then(|ix| scroll.bounds_for_item(ix))
        else {
            fail("the pipeline chart of Project settings was not drawn");
        };
        let range = scroll.max_offset().x;
        if range <= px(0.) {
            fail("the pipeline chart cannot scroll sideways, yet the fake pipeline is wider");
        }
        let short = last.right() - scroll.bounds().right() - range;
        if short > px(1.) {
            fail(&format!(
                "the pipeline chart cannot scroll to its last step ({} px out of reach)",
                short / px(1.)
            ));
        }
        println!("smoke: the pipeline chart scrolls to its last step");
        pass(quit, cx);
    });
    if let Err(error) = result {
        fail(&format!("the window is gone: {error}"));
    }
}

fn pass(quit: bool, cx: &mut App) {
    println!("smoke: ok");
    if quit {
        cx.quit();
    }
}

fn fail(message: &str) -> ! {
    eprintln!("smoke: {message}");
    std::process::exit(1);
}
