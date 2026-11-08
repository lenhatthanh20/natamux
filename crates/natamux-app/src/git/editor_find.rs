//! The find bar of the diff and the editor tab (Mod+F, T-0316), built as the terminal's
//! (`terminals/terminal_find.rs`): it searches the text of one editor as the user types, flips
//! Match case or Use regular expression, or the text changes. Matches are drawn with the theme's
//! search colours; Enter and Shift+Enter select the next or the previous one, starting after the
//! cursor; Esc closes the bar and gives the keyboard back to the editor.

use std::ops::Range;

use editor::display_map::HighlightKey;
use editor::scroll::Autoscroll;
use editor::{Anchor, Editor, EditorEvent, MultiBufferOffset, SelectionEffects};
use gpui::{ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyContext, Subscription};
use text::Point;
use ui::{Tooltip, prelude::*};

use super::editor_find_rules::{count_label, find_ranges, step_match};
use crate::app::commands_editor::EDITOR_FIND;
use crate::app::shortcuts::{Back, FindNext, FindPrevious};
use crate::components::chips::tip_with_keys;
use crate::components::form::input_editor;
use crate::terminals::pane_header::icon_button;

/// The bar closed: its owner drops it.
pub struct FindClosed;

pub struct EditorFind {
    /// The editor searched: the focused side of the diff, or the editor tab's.
    target: Entity<Editor>,
    field: Entity<Editor>,
    match_case: bool,
    regex: bool,
    invalid: bool,
    matches: Vec<Range<Anchor>>,
    active: Option<usize>,
    /// The first match after the cursor, where Enter goes first.
    first: usize,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindClosed> for EditorFind {}

impl EditorFind {
    /// A bar on `target`. A selection within one line becomes the term.
    pub fn new(target: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            let mut editor = input_editor(None, window, cx);
            editor.set_placeholder_text("Find", window, cx);
            editor
        });
        let picked = one_line_selection(&target, cx);
        if let Some(picked) = picked {
            field.update(cx, |editor, cx| editor.set_text(picked, window, cx));
        }
        let subscriptions = vec![
            cx.subscribe(&field, |find, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    find.run(cx);
                }
            }),
            cx.subscribe(&target, |find, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    find.run(cx);
                }
            }),
        ];
        let mut find = EditorFind {
            target,
            field,
            match_case: false,
            regex: false,
            invalid: false,
            matches: Vec::new(),
            active: None,
            first: 0,
            _subscriptions: subscriptions,
        };
        find.run(cx);
        find
    }

    pub fn target(&self) -> &Entity<Editor> {
        &self.target
    }

    /// Mod+F again selects the term, as the terminal's bar does.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.field.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx)
        });
        window.focus(&self.field.focus_handle(cx), cx);
    }

    /// Types a term, as the user would: the find shots.
    pub fn set_term(&mut self, term: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.field
            .update(cx, |editor, cx| editor.set_text(term, window, cx));
    }

    fn term(&self, cx: &App) -> String {
        self.field.read(cx).text(cx)
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        let text = self.target.read(cx).text(cx);
        let found = find_ranges(&text, &term, self.match_case, self.regex);
        self.invalid = found.is_err();
        let found = found.unwrap_or_default();
        let (matches, first) = self.target.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let cursor = editor.selections.newest_anchor().head();
            let matches: Vec<Range<Anchor>> = found
                .into_iter()
                .map(|range| {
                    snapshot.anchor_after(MultiBufferOffset(range.start))
                        ..snapshot.anchor_before(MultiBufferOffset(range.end))
                })
                .collect();
            let first = matches
                .iter()
                .position(|range| range.start.cmp(&cursor, &snapshot).is_ge())
                .unwrap_or(0);
            (matches, first)
        });
        self.matches = matches;
        self.first = first;
        self.active = None;
        self.draw(cx);
    }

    /// The matches in the theme's search colour, the active one stronger.
    fn draw(&mut self, cx: &mut Context<Self>) {
        let active = self.active;
        let ranges = self.matches.clone();
        self.target.update(cx, |editor, cx| {
            editor.highlight_background(
                HighlightKey::BufferSearchHighlights,
                &ranges,
                move |index, theme| {
                    if Some(*index) == active {
                        theme.colors().search_active_match_background
                    } else {
                        theme.colors().search_match_background
                    }
                },
                cx,
            );
        });
        cx.notify();
    }

    fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = step_match(self.active, self.first, self.matches.len(), forward) else {
            return;
        };
        self.active = Some(next);
        let range = self.matches[next].clone();
        self.target.update(cx, |editor, cx| {
            let effects = SelectionEffects::scroll(Autoscroll::center());
            editor.change_selections(effects, window, cx, |selections| {
                selections.select_anchor_ranges([range])
            });
        });
        self.draw(cx);
    }

    fn next(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        self.step(true, window, cx);
    }

    fn previous(&mut self, _: &FindPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.step(false, window, cx);
    }

    fn close(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        self.shut(window, cx);
    }

    /// Takes the matches away; the searched editor gets the keyboard back.
    pub fn shut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear(cx);
        window.focus(&self.target.read(cx).focus_handle(cx), cx);
        cx.emit(FindClosed);
    }

    /// The matches go, without moving the focus: another bar replaces this one.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.target.update(cx, |editor, cx| {
            editor.clear_background_highlights(HighlightKey::BufferSearchHighlights, cx);
        });
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

/// The selected text when it lies within one line, for the term.
fn one_line_selection(target: &Entity<Editor>, cx: &mut App) -> Option<String> {
    target.update(cx, |editor, cx| {
        let display = editor.display_snapshot(cx);
        let picked = editor.selections.newest::<Point>(&display);
        if picked.is_empty() || picked.start.row != picked.end.row {
            return None;
        }
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        Some(snapshot.text_for_range(picked.start..picked.end).collect())
    })
}

impl Focusable for EditorFind {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl Render for EditorFind {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let results = Some((self.active, self.matches.len()));
        let count = count_label(&self.term(cx), results, self.invalid);
        let toggle = |id: &'static str, icon: &str, tip: &'static str, on: bool, regex: bool| {
            icon_button(id, icon)
                .toggle_state(on)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(move |find, _: &ClickEvent, _, cx| find.toggle(regex, cx)))
        };
        let mut context = KeyContext::new_with_defaults();
        context.add(EDITOR_FIND);
        h_flex()
            .key_context(context)
            .on_action(cx.listener(Self::next))
            .on_action(cx.listener(Self::previous))
            .on_action(cx.listener(Self::close))
            .flex_none()
            .w_full()
            .gap_1()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .flex_1()
                    .max_w(px(360.))
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
                "editor-find-case",
                "case-sensitive",
                "Match case",
                self.match_case,
                false,
            ))
            .child(toggle(
                "editor-find-regex",
                "regex",
                "Use regular expression",
                self.regex,
                true,
            ))
            .child(
                icon_button("editor-find-previous", "chevron-up")
                    .tooltip(tip_with_keys("Previous match", "Shift+Enter"))
                    .on_click(
                        cx.listener(|find, _: &ClickEvent, window, cx| {
                            find.step(false, window, cx)
                        }),
                    ),
            )
            .child(
                icon_button("editor-find-next", "chevron-down")
                    .tooltip(tip_with_keys("Next match", "Enter"))
                    .on_click(
                        cx.listener(|find, _: &ClickEvent, window, cx| find.step(true, window, cx)),
                    ),
            )
            .child(
                icon_button("editor-find-close", "x")
                    .tooltip(tip_with_keys("Close find", "Escape"))
                    .on_click(
                        cx.listener(|find, _: &ClickEvent, window, cx| find.shut(window, cx)),
                    ),
            )
    }
}

/// Mod+F in a view with editors: the bar on `target` takes the focus again, or a new bar
/// replaces the one on another editor. The owner keeps the bar in `slot` and drops it on close.
pub fn open_editor_find<V: 'static>(
    slot: &mut Option<Entity<EditorFind>>,
    target: Entity<Editor>,
    window: &mut Window,
    cx: &mut Context<V>,
    on_close: fn(&mut V) -> &mut Option<Entity<EditorFind>>,
) {
    if let Some(open) = slot
        .as_ref()
        .filter(|open| open.read(cx).target() == &target)
    {
        open.update(cx, |find, cx| find.focus(window, cx));
        return;
    }
    if let Some(old) = slot.take() {
        old.update(cx, |find, cx| find.clear(cx));
    }
    let find = cx.new(|cx| EditorFind::new(target, window, cx));
    cx.subscribe(&find, move |owner, _, _: &FindClosed, cx| {
        *on_close(owner) = None;
        cx.notify();
    })
    .detach();
    find.update(cx, |find, cx| find.focus(window, cx));
    *slot = Some(find);
    cx.notify();
}
