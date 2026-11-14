//! Text box keys, as a web `<textarea>` and `<input>` have them. The app does not load Zed's
//! keymap (see `shortcuts.rs`), so each editor key is bound here.

use std::rc::Rc;

use editor::actions as edit;
use gpui::{Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};
use serde_json::{Value, json};

use super::commands::{command_by_id, keystroke};

const ANY: &str = "Editor";
/// A box of many lines (notes, a commit message) or a full editor (the editor tab, the Docs
/// edit). Single-line boxes leave Up, Down and Enter to the lists and forms around them.
const MANY_LINES: &str = "Editor && (mode == auto_height || mode == full)";
const FULL: &str = super::commands_editor::FULL_EDITOR;

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
    let bindings = named(mac)
        .into_iter()
        .filter_map(|(keys, name, data, context)| by_name(cx, keys, name, data, context))
        .collect::<Vec<_>>();
    cx.bind_keys(bindings);
    cx.bind_keys(editor_bindings(mac));
    #[cfg(feature = "own-input")]
    cx.bind_keys(own_bindings(mac));
}

/// The same keys for the own text input (docs/plan-mit-license.md 6.3.1), in the same contexts.
/// A Zed `Editor` (the code and diff views) takes Zed's action and leaves the own one; the own
/// input takes the own one. gpui tries each binding of a key until one is handled.
#[cfg(feature = "own-input")]
fn own_bindings(mac: bool) -> Vec<KeyBinding> {
    use crate::text_view::text_input::actions as own;
    let any = Some(ANY);
    let many = Some(MANY_LINES);
    let full = Some(FULL);
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
    keys
}

/// The keys that bind by name: (keys, action name, action data, context).
type Named = (&'static str, &'static str, Option<Value>, &'static str);

fn named(mac: bool) -> Vec<Named> {
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
    named
}

/// The Zed actions of the full editors, with the keys and contexts of their registry entries
/// (`commands_editor.rs`), so the shortcut sheet shows what is bound.
pub fn editor_bindings(mac: bool) -> Vec<KeyBinding> {
    let mut out = Vec::new();
    push(
        "editor.select-next",
        edit::SelectNext::default(),
        mac,
        &mut out,
    );
    push(
        "editor.select-all-matches",
        edit::SelectAllMatches,
        mac,
        &mut out,
    );
    push(
        "editor.select-larger",
        edit::SelectLargerSyntaxNode,
        mac,
        &mut out,
    );
    push(
        "editor.select-smaller",
        edit::SelectSmallerSyntaxNode,
        mac,
        &mut out,
    );
    push(
        "editor.add-cursor-above",
        edit::AddSelectionAbove {
            skip_soft_wrap: true,
        },
        mac,
        &mut out,
    );
    push(
        "editor.add-cursor-below",
        edit::AddSelectionBelow {
            skip_soft_wrap: true,
        },
        mac,
        &mut out,
    );
    push("editor.move-line-up", edit::MoveLineUp, mac, &mut out);
    push("editor.move-line-down", edit::MoveLineDown, mac, &mut out);
    push(
        "editor.duplicate-line",
        edit::DuplicateLineDown,
        mac,
        &mut out,
    );
    push("editor.delete-line", edit::DeleteLine, mac, &mut out);
    push(
        "editor.toggle-comments",
        edit::ToggleComments::default(),
        mac,
        &mut out,
    );
    push("editor.toggle-fold", edit::ToggleFold, mac, &mut out);
    push("editor.fold-all", edit::FoldAll, mac, &mut out);
    push("editor.unfold-all", edit::UnfoldAll, mac, &mut out);
    push(
        "editor.toggle-hunk",
        edit::ToggleSelectedDiffHunks,
        mac,
        &mut out,
    );
    out
}

fn push<A: Action + Clone>(id: &str, action: A, mac: bool, out: &mut Vec<KeyBinding>) {
    let Some(command) = command_by_id(id) else {
        return;
    };
    for key in command.keys {
        for context in command.contexts {
            out.push(KeyBinding::new(
                &keystroke(key, mac),
                action.clone(),
                Some(context),
            ));
        }
    }
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

    /// The name of Zed's editor action `name` as the box's input carries it: the own input has
    /// the same names in its own namespace (6.3.1).
    fn action_name(name: &str) -> String {
        if cfg!(feature = "own-input") {
            name.replace("editor::", "text_input::")
        } else {
            name.to_string()
        }
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

    /// T-0316: the Zed keys of the full editors.
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
    fn the_editor_keys_reach_a_full_editor(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::DeleteLine", Editor::multi_line);
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
        let keys = keys_for(cx, "editor::MoveLineUp", Editor::multi_line);
        assert!(keys.iter().any(|key| key == "alt-up"), "{keys:?}");
    }

    #[gpui::test]
    fn the_editor_keys_stay_out_of_a_one_line_box(cx: &mut TestAppContext) {
        for name in EDITOR_ACTIONS {
            let keys = keys_for(cx, name, Editor::single_line);
            assert!(keys.is_empty(), "{name}: {keys:?}");
        }
    }

    #[gpui::test]
    fn the_editor_keys_stay_out_of_a_box_of_many_lines(cx: &mut TestAppContext) {
        for name in EDITOR_ACTIONS {
            let keys = keys_for(cx, name, many_lines);
            assert!(keys.is_empty(), "{name}: {keys:?}");
        }
    }

    /// Folds are for the editor tab: a plain full editor (the Docs box, the diff) has none.
    #[gpui::test]
    fn fold_keys_only_in_the_editor_tab(cx: &mut TestAppContext) {
        let keys = keys_for(cx, "editor::FoldAll", Editor::multi_line);
        assert!(keys.is_empty(), "{keys:?}");
    }

    /// T-BIND-1: `by_name` drops a name it cannot build with no error, so a renamed action would
    /// silently lose its key. Every entry must build.
    #[gpui::test]
    fn every_by_name_entry_builds_a_binding(cx: &mut TestAppContext) {
        for mac in [true, false] {
            let list = super::named(mac);
            let built = cx.update(|cx| {
                list.iter()
                    .filter(|(keys, name, data, context)| {
                        super::by_name(cx, keys, name, data.clone(), context).is_some()
                    })
                    .count()
            });
            assert_eq!(built, list.len(), "mac {mac}: {list:?}");
        }
    }

    #[test]
    fn binds_every_editor_entry_of_the_registry() {
        use crate::app::commands::EDITOR_COMMANDS;
        let documented: usize = EDITOR_COMMANDS
            .iter()
            .filter(|command| command.documents_only)
            .map(|command| command.keys.len() * command.contexts.len())
            .sum();
        for mac in [true, false] {
            assert_eq!(super::editor_bindings(mac).len(), documented);
        }
    }
}
