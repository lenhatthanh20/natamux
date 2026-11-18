//! The find bar of the diff and the editor tab (Mod+F, T-0316), built as the terminal's
//! (`terminals/terminal_find.rs`): it searches the text of one editor as the user types, flips
//! Match case or Use regular expression, or the text changes. Matches are drawn with the theme's
//! search colours; Enter and Shift+Enter select the next or the previous one, starting after the
//! cursor; Esc closes the bar and gives the keyboard back to the editor.

use std::ops::Range;
use std::rc::Rc;

use gpui::{ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyContext, Subscription};
use ui::{Tooltip, prelude::*};

use super::code_target::FindTarget;
use super::editor_find_rules::{count_label, find_ranges, first_after, step_match};
use crate::app::commands_editor::EDITOR_FIND;
use crate::app::shortcuts::{Back, FindNext, FindPrevious};
use crate::components::chips::tip_with_keys;
use crate::components::form::text_editor;
use crate::terminals::pane_header::icon_button;
use crate::text_view::text_box::{TextBox, TextEvent};

/// The bar closed: its owner drops it.
pub struct FindClosed;

pub struct EditorFind {
    /// The text searched: the focused side of the diff, or the editor tab's.
    target: Rc<dyn FindTarget>,
    field: TextBox,
    match_case: bool,
    regex: bool,
    invalid: bool,
    matches: Vec<Range<usize>>,
    active: Option<usize>,
    /// The first match after the cursor, where Enter goes first.
    first: usize,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindClosed> for EditorFind {}

impl EditorFind {
    /// A bar on `target`. A selection within one line becomes the term.
    pub fn new(target: Rc<dyn FindTarget>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = text_editor("Find", None, window, cx);
        let picked = target.one_line_selection(cx);
        if let Some(picked) = picked {
            field.set_text(picked, window, cx);
        }
        let this = cx.weak_entity();
        let subscriptions = vec![
            field.subscribe(cx, |find, _, event, cx| {
                if event == TextEvent::Edited {
                    find.run(cx);
                }
            }),
            target.on_edited(
                Box::new(move |cx: &mut App| {
                    this.update(cx, |find, cx| find.run(cx)).ok();
                }),
                cx,
            ),
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

    pub fn target(&self) -> &Rc<dyn FindTarget> {
        &self.target
    }

    /// Mod+F again selects the term, as the terminal's bar does.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.field.select_all(window, cx);
        window.focus(&self.field.focus_handle(cx), cx);
    }

    /// Types a term, as the user would: the find shots.
    pub fn set_term(&mut self, term: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.field.set_text(term, window, cx);
    }

    fn term(&self, cx: &App) -> String {
        self.field.text(cx)
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let term = self.term(cx);
        let text = self.target.text(cx);
        let found = find_ranges(&text, &term, self.match_case, self.regex);
        self.invalid = found.is_err();
        let matches = found.unwrap_or_default();
        self.first = first_after(&matches, self.target.caret(cx));
        self.matches = matches;
        self.active = None;
        self.draw(cx);
    }

    /// The matches in the theme's search colour, the active one stronger.
    fn draw(&mut self, cx: &mut Context<Self>) {
        self.target.show_matches(&self.matches, self.active, cx);
        cx.notify();
    }

    fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = step_match(self.active, self.first, self.matches.len(), forward) else {
            return;
        };
        self.active = Some(next);
        let range = self.matches[next].clone();
        self.target.select(range, window, cx);
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
        window.focus(&self.target.focus_handle(cx), cx);
        cx.emit(FindClosed);
    }

    /// The matches go, without moving the focus: another bar replaces this one.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.target.clear_matches(cx);
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
    target: Rc<dyn FindTarget>,
    window: &mut Window,
    cx: &mut Context<V>,
    on_close: fn(&mut V) -> &mut Option<Entity<EditorFind>>,
) {
    if let Some(open) = slot
        .as_ref()
        .filter(|open| open.read(cx).target().id() == target.id())
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
