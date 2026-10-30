//! The input driver of the flow tests: it acts only through keystrokes and `simulate_input`, as a
//! person at the keyboard does, so a test runs the same on any text box behind it.

use editor::Editor;
use gpui::{ClipboardItem, Entity, Focusable, VisualTestContext};

/// Types `text` into the focused view, as an input method commits it.
pub fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
}

/// Presses keys in gpui's form, `"left shift-right"`. `mod` is Cmd on macOS and Ctrl elsewhere,
/// as the app binds it.
pub fn press(cx: &mut VisualTestContext, keys: &str) {
    let held = if cfg!(target_os = "macos") {
        "cmd-"
    } else {
        "ctrl-"
    };
    cx.simulate_keystrokes(&keys.replace("mod-", held));
}

/// Puts `text` on the clipboard and presses the paste key.
pub fn paste(cx: &mut VisualTestContext, text: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    press(cx, "mod-v");
}

pub fn text_of(text_box: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| text_box.read(cx).text(cx))
}

pub fn is_focused<V: Focusable>(view: &Entity<V>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| view.focus_handle(cx).is_focused(window))
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use theme::ActiveTheme;

    use super::*;
    use crate::app::test_views;
    use crate::components::dialog::DialogLayer;

    #[gpui::test]
    fn types_presses_and_pastes_in_a_focused_box(cx: &mut TestAppContext) {
        let (text_box, _, cx) = test_views::init_app(cx, |_, window, cx| {
            cx.new(|cx| Editor::single_line(window, cx))
        });
        assert!(!is_focused(&text_box, cx));
        cx.update(|window, cx| window.focus(&text_box.focus_handle(cx), cx));
        assert!(is_focused(&text_box, cx));

        type_text(cx, "ac");
        press(cx, "left");
        type_text(cx, "b");
        press(cx, "end");
        paste(cx, "d");
        assert_eq!(text_of(&text_box, cx), "abcd");
        press(cx, "mod-a backspace");
        assert_eq!(text_of(&text_box, cx), "");
    }

    #[gpui::test]
    fn the_app_window_has_natamux_dark_and_the_layers(cx: &mut TestAppContext) {
        let (_, _, cx) = test_views::init_app(cx, |_, window, cx| {
            cx.new(|cx| Editor::single_line(window, cx))
        });
        cx.update(|_, cx| {
            assert_eq!(cx.theme().name.as_ref(), "Natamux Dark");
            assert!(!DialogLayer::is_open(cx));
        });
    }
}
