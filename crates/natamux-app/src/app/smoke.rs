//! `--smoke`: CI checks that the app draws and takes typing, on each OS it builds for. With
//! `--fake-daemon` it also checks that the pipeline chart of Project settings scrolls sideways to
//! its last step (T-0163).

use editor::Editor;
use gpui::{App, AsyncApp, Entity, Keystroke, Window, WindowHandle, px};

use super::shots_stages::{PROJECT, READ};
use super::window::Root;
use crate::projects::project_settings_dialog;

const TYPED: &str = "abc";

/// Waits for the first frame, types into the text box through the window's own key dispatch, and
/// checks the text on the next frame. `fake`: then checks the fake's screens. Exits with 1 on any
/// failure. `quit`: quit once it passed.
pub fn run(handle: WindowHandle<Root>, quit: bool, fake: bool, cx: &mut App) {
    // Read through the editor, not the window: inside a frame callback the window is in use.
    let result = handle.update(cx, |root, window, cx| {
        let filter = root.text_box(cx);
        window.on_next_frame(move |window, cx| {
            type_and_check(filter, handle, quit, fake, window, cx)
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
    fake: bool,
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
        if fake {
            cx.spawn(async move |cx| check_chart(handle, quit, cx).await)
                .detach();
        } else {
            pass(quit, cx);
        }
    });
}

/// Opens Project settings of the fake project, whose pipeline is wider than the dialog, and fails
/// if the chart cannot scroll to its last step: the right part was cut off (T-0163).
async fn check_chart(handle: WindowHandle<Root>, quit: bool, cx: &mut AsyncApp) {
    let opened = handle.update(cx, |root, window, cx| {
        project_settings_dialog::open(root.dashboard(), PROJECT, window, cx)
    });
    let dialog = match opened {
        Ok(dialog) => dialog,
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
