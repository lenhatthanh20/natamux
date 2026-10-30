//! Text box keys, as a web `<textarea>` and `<input>` have them. The app does not load Zed's
//! keymap (see `shortcuts.rs`), so each editor key is bound here.

use std::rc::Rc;

use editor::actions as edit;
use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};
use serde_json::{Value, json};

const ANY: &str = "Editor";
/// A box of many lines (notes, a commit message) or a full editor (the editor tab, the Docs
/// edit). Single-line boxes leave Up, Down and Enter to the lists and forms around them.
const MANY_LINES: &str = "Editor && (mode == auto_height || mode == full)";
const FULL: &str = "Editor && mode == full";

pub fn bind(cx: &mut App, mac: bool) {
    let any = Some(ANY);
    let many = Some(MANY_LINES);
    let full = Some(FULL);
    let m = |keys: &str| keys.replace("mod", if mac { "cmd" } else { "ctrl" });
    // Word keys: Alt on macOS, Ctrl elsewhere.
    let w = |keys: &str| keys.replace("word", if mac { "alt" } else { "ctrl" });
    // A textarea's Home and End stop at the edge of the shown row of a wrapped line.
    let line_start = edit::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: false,
    };
    let line_end = edit::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    };
    let (start, end) = if mac {
        ("cmd-up", "cmd-down")
    } else {
        ("ctrl-home", "ctrl-end")
    };
    cx.bind_keys([
        KeyBinding::new("backspace", edit::Backspace, any),
        KeyBinding::new("delete", edit::Delete, any),
        KeyBinding::new("left", edit::MoveLeft, any),
        KeyBinding::new("right", edit::MoveRight, any),
        KeyBinding::new("shift-left", edit::SelectLeft, any),
        KeyBinding::new("shift-right", edit::SelectRight, any),
        KeyBinding::new("home", line_start.clone(), any),
        KeyBinding::new("end", line_end.clone(), any),
        KeyBinding::new(&w("word-left"), edit::MoveToPreviousWordStart, any),
        KeyBinding::new(&w("word-right"), edit::MoveToNextWordEnd, any),
        KeyBinding::new(&w("word-shift-left"), edit::SelectToPreviousWordStart, any),
        KeyBinding::new(&w("word-shift-right"), edit::SelectToNextWordEnd, any),
        KeyBinding::new(
            &w("word-backspace"),
            edit::DeleteToPreviousWordStart::default(),
            any,
        ),
        KeyBinding::new(&w("word-delete"), edit::DeleteToNextWordEnd::default(), any),
        KeyBinding::new(&m("mod-a"), edit::SelectAll, any),
        KeyBinding::new(&m("mod-c"), edit::Copy, any),
        KeyBinding::new(&m("mod-x"), edit::Cut, any),
        KeyBinding::new(&m("mod-v"), edit::Paste, any),
        KeyBinding::new(&m("mod-z"), edit::Undo, any),
        KeyBinding::new(&m("mod-shift-z"), edit::Redo, any),
        // The forms around a box of many lines submit on Mod+Enter.
        KeyBinding::new("enter", edit::Newline, many),
        KeyBinding::new("shift-up", edit::SelectUp, many),
        KeyBinding::new("shift-down", edit::SelectDown, many),
        KeyBinding::new("pageup", edit::MovePageUp::default(), many),
        KeyBinding::new("pagedown", edit::MovePageDown::default(), many),
        KeyBinding::new(start, edit::MoveToBeginning, many),
        KeyBinding::new(end, edit::MoveToEnd, many),
        KeyBinding::new(&format!("shift-{start}"), edit::SelectToBeginning, many),
        KeyBinding::new(&format!("shift-{end}"), edit::SelectToEnd, many),
        // In a box of many lines Tab moves the focus, as in a textarea.
        KeyBinding::new("tab", edit::Tab, full),
        KeyBinding::new("shift-tab", edit::Backtab, full),
    ]);
    if mac {
        cx.bind_keys([
            KeyBinding::new("cmd-left", line_start, any),
            KeyBinding::new("cmd-right", line_end, any),
            KeyBinding::new(
                "cmd-backspace",
                edit::DeleteToBeginningOfLine::default(),
                any,
            ),
        ]);
    }
    let wrap = || Some(json!({ "stop_at_soft_wraps": true }));
    let mut named = vec![
        ("up", "editor::MoveUp", None, MANY_LINES),
        ("down", "editor::MoveDown", None, MANY_LINES),
        ("shift-home", "editor::SelectToBeginningOfLine", wrap(), ANY),
        ("shift-end", "editor::SelectToEndOfLine", wrap(), ANY),
    ];
    if mac {
        named.push((
            "cmd-shift-left",
            "editor::SelectToBeginningOfLine",
            wrap(),
            ANY,
        ));
        named.push(("cmd-shift-right", "editor::SelectToEndOfLine", wrap(), ANY));
    }
    let bindings = named
        .into_iter()
        .filter_map(|(keys, name, data, context)| by_name(cx, keys, name, data, context))
        .collect::<Vec<_>>();
    cx.bind_keys(bindings);
}

/// Zed's MoveUp and MoveDown live in a crate the app does not take, and the line-select actions
/// keep `stop_at_soft_wraps` private: these bind by name.
fn by_name(
    cx: &App,
    keys: &str,
    name: &str,
    data: Option<Value>,
    context: &str,
) -> Option<KeyBinding> {
    let action = cx.build_action(name, data).ok()?;
    let predicate = KeyBindingContextPredicate::parse(context).ok()?;
    KeyBinding::load(
        keys,
        action,
        Some(Rc::new(predicate)),
        false,
        None,
        &DummyKeyboardMapper,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use editor::Editor;
    use gpui::{Context, Focusable as _, TestAppContext, Window};

    use crate::app::shortcuts;

    /// The keys that run the action `name` in the editor `make` gives, with the app's bindings.
    fn keys_for(
        cx: &mut TestAppContext,
        name: &str,
        make: impl FnOnce(&mut Window, &mut Context<Editor>) -> Editor + 'static,
    ) -> Vec<String> {
        crate::app::test_views::init(cx);
        cx.update(shortcuts::bind);
        let (editor, cx) = cx.add_window_view(make);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let action = cx.build_action(name, None).unwrap();
            window
                .bindings_for_action_in(action.as_ref(), &editor.focus_handle(cx))
                .iter()
                .map(|binding| {
                    // `unparse`, not `to_string`: on macOS the display form is glyphs (⌥⇧←).
                    binding
                        .keystrokes()
                        .iter()
                        .map(|keystroke| keystroke.unparse())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
    }

    fn many_lines(window: &mut Window, cx: &mut Context<Editor>) -> Editor {
        Editor::auto_height(6, 12, window, cx)
    }

    /// The platform's word key: Alt on macOS, Ctrl elsewhere.
    fn word(keys: &str) -> String {
        let held = if cfg!(target_os = "macos") {
            "alt"
        } else {
            "ctrl"
        };
        format!("{held}-{keys}")
    }

    /// T-0186: Enter did nothing in the Notes box of Start a task.
    #[gpui::test]
    fn enter_starts_a_line_in_a_box_of_many_lines(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::Newline", many_lines);
        // The list can name the same key more than once.
        assert!(
            !keys.is_empty() && keys.iter().all(|key| key == "enter"),
            "{keys:?}"
        );
    }

    #[gpui::test]
    fn enter_is_left_to_the_form_in_a_one_line_box(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::Newline", Editor::single_line);
        assert!(keys.is_empty(), "{keys:?}");
    }

    /// T-0229: Up did nothing in the Notes box of Start a task.
    #[gpui::test]
    fn up_moves_the_cursor_in_a_box_of_many_lines(cx: &mut TestAppContext) {
        crate::app::test_views::init(cx);
        cx.update(shortcuts::bind);
        let (editor, cx) = cx.add_window_view(many_lines);
        cx.update(|window, cx| {
            window.focus(&editor.focus_handle(cx), cx);
            editor.update(cx, |editor, cx| editor.set_text("ab\ncd", window, cx));
        });
        let to_end = if cfg!(target_os = "macos") {
            "cmd-down"
        } else {
            "ctrl-end"
        };
        cx.simulate_keystrokes(&format!("{to_end} up"));
        cx.simulate_input("x");
        let text = cx.update(|_, cx| editor.read(cx).text(cx));
        assert_eq!(text, "abx\ncd");
    }

    #[gpui::test]
    fn up_is_left_to_the_list_in_a_one_line_box(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::MoveUp", Editor::single_line);
        assert!(keys.is_empty(), "{keys:?}");
    }

    #[gpui::test]
    fn word_keys_reach_a_one_line_box(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::SelectToPreviousWordStart", Editor::single_line);
        assert!(keys.contains(&word("shift-left")), "{keys:?}");
    }

    #[gpui::test]
    fn word_delete_reaches_a_box_of_many_lines(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::DeleteToPreviousWordStart", many_lines);
        assert!(keys.contains(&word("backspace")), "{keys:?}");
    }
}
