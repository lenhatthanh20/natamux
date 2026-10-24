//! `--smoke`: CI checks that the app draws and takes typing, on each OS it builds for. With
//! `--fake-daemon` it also checks that the terminal grid of a workspace does not scroll when its
//! panes fit (T-0162).

use std::time::Duration;

use editor::Editor;
use gpui::{App, AsyncApp, Entity, Keystroke, Window, WindowHandle, px};

use super::shots::{self, RUNNING};
use super::window::Root;

const TYPED: &str = "abc";
/// The fake's terminal list comes at once; the grid is drawn well within this.
const GRID_WAIT: Duration = Duration::from_secs(5);
const GRID_POLL: Duration = Duration::from_millis(100);
/// The grid draws its maximized pane within this.
const GRID_SETTLE: Duration = Duration::from_millis(500);

/// Waits for the first frame, types into the text box through the window's own key dispatch, and
/// checks the text on the next frame. `grid`: then checks the terminal grid. Exits with 1 on any
/// failure. `quit`: quit once it passed.
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
