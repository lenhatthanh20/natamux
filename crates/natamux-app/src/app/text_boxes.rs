//! Text box keys, as a web `<textarea>` and `<input>` have them. The app does not load a keymap
//! file (see `shortcuts.rs`), so each text box key is bound here.

use gpui::{Action, App, KeyBinding};

use super::commands::{command_by_id, keystroke};

const ANY: &str = "Editor";
/// A box of many lines (notes, a commit message) or a full editor (the editor tab, the Docs
/// edit). Single-line boxes leave Up, Down and Enter to the lists and forms around them.
const MANY_LINES: &str = "Editor && (mode == auto_height || mode == full)";
const FULL: &str = super::commands_editor::FULL_EDITOR;

pub fn bind(cx: &mut App, mac: bool) {
    cx.bind_keys(own_bindings(mac));
}

/// The key context entry of the own text input only.
pub const OWN_INPUT: &str = "own_input";

/// A key context narrowed to the own text input.
fn only_own(context: &str) -> String {
    format!("({context}) && {OWN_INPUT}")
}

/// The keys of the own text input (docs/plan-mit-license.md 6.3.1), each in its context narrowed
/// to the own input.
fn own_bindings(mac: bool) -> Vec<KeyBinding> {
    use crate::text_view::text_input::actions as own;
    let (any, many, full) = (only_own(ANY), only_own(MANY_LINES), only_own(FULL));
    let (any, many, full) = (Some(any.as_str()), Some(many.as_str()), Some(full.as_str()));
    let m = |keys: &str| keys.replace("mod", if mac { "cmd" } else { "ctrl" });
    let w = |keys: &str| keys.replace("word", if mac { "alt" } else { "ctrl" });
    let (start, end) = if mac {
        ("cmd-up", "cmd-down")
    } else {
        ("ctrl-home", "ctrl-end")
    };
    let mut keys = vec![
        KeyBinding::new("backspace", own::Backspace, any),
        KeyBinding::new("delete", own::Delete, any),
        KeyBinding::new("left", own::MoveLeft, any),
        KeyBinding::new("right", own::MoveRight, any),
        KeyBinding::new("shift-left", own::SelectLeft, any),
        KeyBinding::new("shift-right", own::SelectRight, any),
        KeyBinding::new("home", own::MoveToBeginningOfLine, any),
        KeyBinding::new("end", own::MoveToEndOfLine, any),
        KeyBinding::new("shift-home", own::SelectToBeginningOfLine, any),
        KeyBinding::new("shift-end", own::SelectToEndOfLine, any),
        KeyBinding::new(&w("word-left"), own::MoveToPreviousWordStart, any),
        KeyBinding::new(&w("word-right"), own::MoveToNextWordEnd, any),
        KeyBinding::new(&w("word-shift-left"), own::SelectToPreviousWordStart, any),
        KeyBinding::new(&w("word-shift-right"), own::SelectToNextWordEnd, any),
        KeyBinding::new(&w("word-backspace"), own::DeleteToPreviousWordStart, any),
        KeyBinding::new(&w("word-delete"), own::DeleteToNextWordEnd, any),
        KeyBinding::new(&m("mod-a"), own::SelectAll, any),
        KeyBinding::new(&m("mod-c"), own::Copy, any),
        KeyBinding::new(&m("mod-x"), own::Cut, any),
        KeyBinding::new(&m("mod-v"), own::Paste, any),
        KeyBinding::new(&m("mod-z"), own::Undo, any),
        KeyBinding::new(&m("mod-shift-z"), own::Redo, any),
        KeyBinding::new("enter", own::Newline, many),
        KeyBinding::new("up", own::MoveUp, many),
        KeyBinding::new("down", own::MoveDown, many),
        KeyBinding::new("shift-up", own::SelectUp, many),
        KeyBinding::new("shift-down", own::SelectDown, many),
        KeyBinding::new("pageup", own::MovePageUp, many),
        KeyBinding::new("pagedown", own::MovePageDown, many),
        KeyBinding::new(start, own::MoveToBeginning, many),
        KeyBinding::new(end, own::MoveToEnd, many),
        KeyBinding::new(&format!("shift-{start}"), own::SelectToBeginning, many),
        KeyBinding::new(&format!("shift-{end}"), own::SelectToEnd, many),
        KeyBinding::new("tab", own::Tab, full),
        KeyBinding::new("shift-tab", own::Backtab, full),
    ];
    if mac {
        keys.extend([
            KeyBinding::new("cmd-left", own::MoveToBeginningOfLine, any),
            KeyBinding::new("cmd-right", own::MoveToEndOfLine, any),
            KeyBinding::new("cmd-backspace", own::DeleteToBeginningOfLine, any),
            KeyBinding::new("cmd-shift-left", own::SelectToBeginningOfLine, any),
            KeyBinding::new("cmd-shift-right", own::SelectToEndOfLine, any),
        ]);
    }
    // The keys of the full boxes, with the keys and contexts of the registry.
    push_in(
        "editor.select-next",
        own::SelectNext,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.select-all-matches",
        own::SelectAllMatches,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.select-larger",
        own::SelectLargerSyntaxNode,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.select-smaller",
        own::SelectSmallerSyntaxNode,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.add-cursor-above",
        own::AddSelectionAbove,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.add-cursor-below",
        own::AddSelectionBelow,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.move-line-up",
        own::MoveLineUp,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.move-line-down",
        own::MoveLineDown,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.duplicate-line",
        own::DuplicateLineDown,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.delete-line",
        own::DeleteLine,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.toggle-comments",
        own::ToggleComments,
        mac,
        only_own,
        &mut keys,
    );
    push_in(
        "editor.toggle-hunk",
        own::ToggleSelectedDiffHunks,
        mac,
        only_own,
        &mut keys,
    );
    keys
}

/// Binds `action` to each key of the registry entry `id`, each context passed through `context`.
fn push_in<A: Action + Clone>(
    id: &str,
    action: A,
    mac: bool,
    context: fn(&str) -> String,
    out: &mut Vec<KeyBinding>,
) {
    let Some(command) = command_by_id(id) else {
        return;
    };
    for key in command.keys {
        for registered in command.contexts {
            out.push(KeyBinding::new(
                &keystroke(key, mac),
                action.clone(),
                Some(context(registered).as_str()),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Context, Focusable as _, TestAppContext, Window};
    use ui::prelude::*;

    use crate::app::shortcuts;
    use crate::text_view::text_box::TextBox;

    /// A window with one text box, as the forms hold them.
    struct BoxView {
        text_box: TextBox,
    }

    impl Render for BoxView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.text_box.clone())
        }
    }

    /// The name of the action `name` as the box's input carries it: the registry names its
    /// actions `editor::...`, the own input has them in its own namespace (6.3.1).
    fn action_name(name: &str) -> String {
        name.replace("editor::", "text_input::")
    }

    /// The keys that run the action `name` in the box `make` gives, with the app's bindings.
    fn box_keys_for(
        cx: &mut TestAppContext,
        name: &str,
        make: impl FnOnce(&mut Window, &mut App) -> TextBox + 'static,
    ) -> Vec<String> {
        crate::app::test_views::init(cx);
        cx.update(shortcuts::bind);
        let (view, cx) = cx.add_window_view(|window, cx| BoxView {
            text_box: make(window, cx),
        });
        cx.run_until_parked();
        let text_box = view.read_with(cx, |view, _| view.text_box.clone());
        let name = action_name(name);
        cx.update(|window, cx| {
            let action = cx.build_action(&name, None).unwrap();
            window
                .bindings_for_action_in(action.as_ref(), &text_box.focus_handle(cx))
                .iter()
                .map(|binding| {
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

    fn one_line(window: &mut Window, cx: &mut App) -> TextBox {
        TextBox::one_line("", window, cx)
    }

    fn many_line_box(window: &mut Window, cx: &mut App) -> TextBox {
        TextBox::area("", (6, 12), window, cx)
    }

    fn plain_box(window: &mut Window, cx: &mut App) -> TextBox {
        TextBox::plain(false, window, cx)
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
        let keys = box_keys_for(cx, "editor::Newline", many_line_box);
        // The list can name the same key more than once.
        assert!(
            !keys.is_empty() && keys.iter().all(|key| key == "enter"),
            "{keys:?}"
        );
    }

    #[gpui::test]
    fn enter_is_left_to_the_form_in_a_one_line_box(cx: &mut TestAppContext) {
        let keys = box_keys_for(cx, "editor::Newline", one_line);
        assert!(keys.is_empty(), "{keys:?}");
    }

    /// T-0229: Up did nothing in the Notes box of Start a task.
    #[gpui::test]
    fn up_moves_the_cursor_in_a_box_of_many_lines(cx: &mut TestAppContext) {
        crate::app::test_views::init(cx);
        cx.update(shortcuts::bind);
        let (view, cx) = cx.add_window_view(|window, cx| BoxView {
            text_box: many_line_box(window, cx),
        });
        let text_box = view.read_with(cx, |view, _| view.text_box.clone());
        cx.update(|window, cx| {
            window.focus(&text_box.focus_handle(cx), cx);
            text_box.set_text("ab\ncd", window, cx);
        });
        let to_end = if cfg!(target_os = "macos") {
            "cmd-down"
        } else {
            "ctrl-end"
        };
        cx.simulate_keystrokes(&format!("{to_end} up"));
        cx.simulate_input("x");
        let text = cx.update(|_, cx| text_box.text(cx));
        assert_eq!(text, "abx\ncd");
    }

    #[gpui::test]
    fn up_is_left_to_the_list_in_a_one_line_box(cx: &mut TestAppContext) {
        let keys = box_keys_for(cx, "editor::MoveUp", one_line);
        assert!(keys.is_empty(), "{keys:?}");
    }

    #[gpui::test]
    fn word_keys_reach_a_one_line_box(cx: &mut TestAppContext) {
        let keys = box_keys_for(cx, "editor::SelectToPreviousWordStart", one_line);
        assert!(keys.contains(&word("shift-left")), "{keys:?}");
    }

    #[gpui::test]
    fn word_delete_reaches_a_box_of_many_lines(cx: &mut TestAppContext) {
        let keys = box_keys_for(cx, "editor::DeleteToPreviousWordStart", many_line_box);
        assert!(keys.contains(&word("backspace")), "{keys:?}");
    }

    /// T-0316: the keys of the full editors.
    const EDITOR_ACTIONS: &[&str] = &[
        "editor::SelectNext",
        "editor::SelectAllMatches",
        "editor::SelectLargerSyntaxNode",
        "editor::MoveLineUp",
        "editor::DuplicateLineDown",
        "editor::DeleteLine",
        "editor::ToggleComments",
        "editor::AddSelectionBelow",
    ];

    #[gpui::test]
    fn the_editor_keys_reach_a_full_box(cx: &mut TestAppContext) {
        let keys = box_keys_for(cx, "editor::DeleteLine", plain_box);
        let delete = if cfg!(target_os = "macos") {
            "cmd-shift-k"
        } else {
            "ctrl-shift-k"
        };
        // gpui writes the key of a Shift chord in capitals.
        assert!(
            keys.iter().any(|key| key.eq_ignore_ascii_case(delete)),
            "{keys:?}"
        );
        let keys = box_keys_for(cx, "editor::MoveLineUp", plain_box);
        assert!(keys.iter().any(|key| key == "alt-up"), "{keys:?}");
    }

    /// The keys of the full editors reach the Docs edit box: the own input binds and handles each
    /// of them (S3.3).
    #[gpui::test]
    fn the_editor_keys_reach_a_plain_box(cx: &mut TestAppContext) {
        let more = [
            "editor::SelectSmallerSyntaxNode",
            "editor::AddSelectionAbove",
            "editor::MoveLineDown",
        ];
        for name in EDITOR_ACTIONS.iter().chain(&more) {
            let keys = box_keys_for(cx, name, plain_box);
            assert!(!keys.is_empty(), "{name}");
        }
    }

    #[gpui::test]
    fn the_editor_keys_stay_out_of_a_one_line_box(cx: &mut TestAppContext) {
        for name in EDITOR_ACTIONS {
            let keys = box_keys_for(cx, name, one_line);
            assert!(keys.is_empty(), "{name}: {keys:?}");
        }
    }

    #[gpui::test]
    fn the_editor_keys_stay_out_of_a_box_of_many_lines(cx: &mut TestAppContext) {
        for name in EDITOR_ACTIONS {
            let keys = box_keys_for(cx, name, many_line_box);
            assert!(keys.is_empty(), "{name}: {keys:?}");
        }
    }

    /// A registry id that is renamed or removed must not silently drop its keys.
    #[test]
    fn every_registry_entry_of_the_own_input_builds_its_bindings() {
        use crate::app::commands::command_by_id;
        use crate::text_view::text_input::actions as own;
        use gpui::Action as _;
        let entries = [
            ("editor.delete-line", own::DeleteLine.name()),
            ("editor.toggle-comments", own::ToggleComments.name()),
            ("editor.toggle-hunk", own::ToggleSelectedDiffHunks.name()),
        ];
        for mac in [true, false] {
            let bindings = super::own_bindings(mac);
            for (id, action) in entries {
                let command = command_by_id(id).unwrap_or_else(|| panic!("{id} left the registry"));
                let want = command.keys.len() * command.contexts.len();
                assert!(want > 0, "{id} has no key");
                let got = bindings
                    .iter()
                    .filter(|binding| binding.action().name() == action)
                    .count();
                assert_eq!(got, want, "{id}, mac {mac}");
            }
        }
    }
}
