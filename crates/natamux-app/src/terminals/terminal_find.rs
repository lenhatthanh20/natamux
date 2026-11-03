//! The find bar of a terminal pane (docs/ui-spec.md 4.23, `features/terminals/TerminalFind.tsx`):
//! Mod+F in a focused terminal opens it at the top right of that pane. It searches as the user
//! types or flips Match case or Use regular expression; Enter and Shift+Enter step through the
//! matches, Esc closes it and gives the keyboard back to the terminal.

use editor::{Editor, EditorEvent};
use gpui::{ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyContext, Subscription};
use ui::{Tooltip, prelude::*};

use super::find_rules::{FindResults, find_count_label, find_pattern};
use super::pane_header::icon_button;
use super::remote_terminal::RemoteTerminal;
use super::terminal_cache::TerminalCache;
use crate::app::commands::TERMINAL_FIND;
use crate::app::shortcuts::{Back, FindNext, FindPrevious};
use crate::components::chips::tip_with_keys;

/// The bar closed: the pane gives the keyboard back to its terminal.
pub struct FindClosed;

/// Mod+F in a pane: opens its find bar, or focuses the open one. Closing it gives the keyboard
/// back to the terminal.
pub fn open_find<V: 'static>(
    key: String,
    terminal: Entity<RemoteTerminal>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let find = match TerminalCache::find(&key, cx) {
        Some(find) => find,
        None => {
            let find = cx.new(|cx| TerminalFind::new(terminal.clone(), window, cx));
            let back = terminal.clone();
            let closed = key.clone();
            cx.subscribe_in(&find, window, move |_, _, _: &FindClosed, window, cx| {
                TerminalCache::set_find(&closed, None, cx);
                let focus = back.read(cx).view.focus_handle(cx);
                window.focus(&focus, cx);
                cx.notify();
            })
            .detach();
            TerminalCache::set_find(&key, Some(find.clone()), cx);
            find
        }
    };
    find.update(cx, |find, cx| find.focus(window, cx));
    cx.notify();
}

pub struct TerminalFind {
    /// Not Zed's terminal: a snapshot can swap the shown screen (T-0210).
    terminal: Entity<RemoteTerminal>,
    field: Entity<Editor>,
    match_case: bool,
    regex: bool,
    results: Option<FindResults>,
    /// Bumped on each new search, so a slow answer of an old one is dropped.
    search: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindClosed> for TerminalFind {}

impl TerminalFind {
    pub fn new(
        terminal: Entity<RemoteTerminal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Find", window, cx);
            editor
        });
        let edits = cx.subscribe(&field, |find, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                find.run(cx);
            }
        });
        TerminalFind {
            terminal,
            field,
            match_case: false,
            regex: false,
            results: None,
            search: 0,
            _subscriptions: vec![edits],
        }
    }

    /// Opening again keeps the term and selects it, as the web's bar does on Mod+F.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.field.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx)
        });
        let focus = self.field.focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Types a term, as the user would: the `terminal-find` shot.
    pub fn set_term(&mut self, term: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.field
            .update(cx, |editor, cx| editor.set_text(term, window, cx));
    }

    fn term(&self, cx: &App) -> String {
        self.field.read(cx).text(cx)
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        self.search += 1;
        let search = self.search;
        let pattern = find_pattern(&self.term(cx), self.match_case, self.regex);
        let found = (!pattern.is_empty())
            .then(|| {
                self.terminal
                    .update(cx, |terminal, cx| terminal.find(&pattern, cx))
            })
            .flatten();
        let Some(found) = found else {
            // No term, or a regex that does not parse: nothing matches.
            self.terminal
                .update(cx, |terminal, cx| terminal.clear_matches(cx));
            self.results = (!pattern.is_empty()).then_some((None, 0));
            cx.notify();
            return;
        };
        cx.spawn(async move |find, cx| {
            let matches = found.await;
            find.update(cx, |find, cx| {
                if find.search != search {
                    return;
                }
                let count = matches.len();
                find.terminal
                    .update(cx, |terminal, cx| terminal.set_matches(matches, cx));
                find.results = Some(((count > 0).then_some(0), count));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some((index, count)) = self.results else {
            return;
        };
        if count == 0 || self.term(cx).is_empty() {
            return;
        }
        let next = match (index, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        self.terminal
            .update(cx, |terminal, cx| terminal.activate_match(next, cx));
        self.results = Some((Some(next), count));
        cx.notify();
    }

    fn next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    fn previous(&mut self, _: &FindPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(false, cx);
    }

    fn close(&mut self, _: &Back, _: &mut Window, cx: &mut Context<Self>) {
        self.shut(cx);
    }

    fn shut(&mut self, cx: &mut Context<Self>) {
        self.search += 1;
        self.terminal
            .update(cx, |terminal, cx| terminal.clear_matches(cx));
        cx.emit(FindClosed);
    }

    fn toggle(&mut self, regex: bool, cx: &mut Context<Self>) {
        if regex {
            self.regex = !self.regex;
        } else {
            self.match_case = !self.match_case;
        }
        self.run(cx);
    }
}

impl Focusable for TerminalFind {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl Render for TerminalFind {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = find_count_label(&self.term(cx), self.results);
        let toggle = |id: &'static str, icon: &str, tip: &'static str, on: bool, regex: bool| {
            icon_button(id, icon)
                .toggle_state(on)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(move |find, _: &ClickEvent, _, cx| find.toggle(regex, cx)))
        };
        let mut context = KeyContext::new_with_defaults();
        context.add(TERMINAL_FIND);
        h_flex()
            .key_context(context)
            .on_action(cx.listener(Self::next))
            .on_action(cx.listener(Self::previous))
            .on_action(cx.listener(Self::close))
            .occlude()
            .gap_1()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .elevation_2(cx)
            .child(
                h_flex()
                    .w(px(200.))
                    .h(px(24.))
                    .px_1p5()
                    .gap_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    .child(div().flex_1().min_w_0().child(self.field.clone()))
                    .child(
                        Label::new(count)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
            .child(toggle(
                "find-case",
                "case-sensitive",
                "Match case",
                self.match_case,
                false,
            ))
            .child(toggle(
                "find-regex",
                "regex",
                "Use regular expression",
                self.regex,
                true,
            ))
            .child(
                icon_button("find-previous", "chevron-up")
                    .tooltip(tip_with_keys("Previous match", "Shift+Enter"))
                    .on_click(cx.listener(|find, _: &ClickEvent, _, cx| find.step(false, cx))),
            )
            .child(
                icon_button("find-next", "chevron-down")
                    .tooltip(tip_with_keys("Next match", "Enter"))
                    .on_click(cx.listener(|find, _: &ClickEvent, _, cx| find.step(true, cx))),
            )
            .child(
                icon_button("find-close", "x")
                    .tooltip(tip_with_keys("Close find", "Escape"))
                    .on_click(cx.listener(|find, _: &ClickEvent, _, cx| find.shut(cx))),
            )
    }
}

#[cfg(test)]
#[path = "terminal_find_tests.rs"]
mod tests;
