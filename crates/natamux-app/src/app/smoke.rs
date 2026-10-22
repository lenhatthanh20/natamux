//! `--smoke`: CI checks that the app draws and takes typing, on each OS it builds for.

use editor::Editor;
use gpui::{App, Entity, Keystroke, Window, WindowHandle};

use super::window::Root;

const TYPED: &str = "abc";

/// Waits for the first frame, types into the text box through the window's own key dispatch, and
/// checks the text on the next frame. Exits with 1 on any failure. `quit`: quit once it passed.
pub fn run(handle: WindowHandle<Root>, quit: bool, cx: &mut App) {
    // Read through the editor, not the window: inside a frame callback the window is in use.
    let result = handle.update(cx, |root, window, cx| {
        let filter = root.filter(cx);
        window.on_next_frame(move |window, cx| type_and_check(filter, quit, window, cx));
    });
    if let Err(error) = result {
        fail(&format!("the window is gone: {error}"));
    }
}

fn type_and_check(filter: Entity<Editor>, quit: bool, window: &mut Window, cx: &mut App) {
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
        println!("smoke: ok");
        if quit {
            cx.quit();
        }
    });
}

fn fail(message: &str) {
    eprintln!("smoke: {message}");
    std::process::exit(1);
}
