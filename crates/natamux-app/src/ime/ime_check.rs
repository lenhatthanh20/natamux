//! Tools > IME Check (gate 1): type the fixed Vietnamese word list with Telex or VNI, in a text
//! box and in a terminal, and see which words came out wrong.

use std::sync::Arc;

use editor::Editor;
use gpui::{App, Entity, FocusHandle, Focusable, actions};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::word_check::{WordCheck, terminal_check_command, words};
use crate::app::source::Source;
use crate::components::button_style::quiet_button;
use crate::terminals::remote_terminal::{RemoteTerminal, Target};
use crate::terminals::terminal_screen::terminal_screen;

actions!(natamux, [SubmitWord]);

/// Next to the bench's, far from the terminals a task's stages use.
const TERMINAL: &str = "991";
const TYPED_FILE: &str = "/tmp/natamux-ime.txt";

pub struct ImeCheck {
    check: WordCheck,
    input: Entity<Editor>,
    last: Option<(String, bool)>,
    terminal: Option<Entity<RemoteTerminal>>,
}

impl ImeCheck {
    pub fn new(
        source: Option<Arc<Source>>,
        task: Option<Task>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Type the word above, then Enter", window, cx);
            editor
        });
        let terminal = match (source, task) {
            (Some(source), Some(task)) if task.has_live_terminal() => {
                let target = Target {
                    task_id: task.id.clone(),
                    terminal: TERMINAL.to_string(),
                };
                Some(cx.new(|cx| RemoteTerminal::new(source, target, window, cx)))
            }
            _ => None,
        };
        ImeCheck {
            check: WordCheck::new(words()),
            input,
            last: None,
            terminal,
        }
    }

    fn submit(&mut self, _: &SubmitWord, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.input.read(cx).text(cx);
        if let Some(expected) = self.check.expected() {
            let right = self.check.submit(&typed);
            self.last = Some((format!("{expected} ← {typed}"), right));
        }
        self.input
            .update(cx, |input, cx| input.set_text("", window, cx));
        cx.notify();
    }

    fn type_in_terminal(&self, text: &str, cx: &mut App) {
        if let Some(terminal) = &self.terminal {
            terminal.update(cx, |terminal, _| terminal.type_text(text));
        }
    }

    fn render_terminal_half(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(terminal) = &self.terminal else {
            return Label::new(
                "Pick a running task first: the terminal half uses one of its shells.",
            )
            .color(Color::Muted)
            .into_any_element();
        };
        v_flex()
            .size_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(quiet_button("ime-start", "1. Start").on_click(cx.listener(
                        |check, _, _, cx| {
                            check.type_in_terminal(&format!("clear; cat > {TYPED_FILE}\r"), cx);
                        },
                    )))
                    .child(
                        Label::new("2. Type every word below, one per line.").color(Color::Muted),
                    )
                    .child(quiet_button("ime-check", "3. Check").on_click(cx.listener(
                        |check, _, _, cx| {
                            // Ctrl+D ends `cat`, then the shell compares the files.
                            check.type_in_terminal("\x04", cx);
                            check.type_in_terminal(
                                &terminal_check_command(&words(), TYPED_FILE),
                                cx,
                            );
                        },
                    ))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(terminal_screen(terminal, false, cx)),
            )
            .into_any_element()
    }
}

impl Focusable for ImeCheck {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for ImeCheck {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let expected = self
            .check
            .expected()
            .map_or("All words typed.".to_string(), |word| {
                format!("Type: {word}")
            });
        let last = self.last.as_ref().map(|(text, right)| {
            Label::new(format!("{} {text}", if *right { "✓" } else { "✗" })).color(if *right {
                Color::Success
            } else {
                Color::Error
            })
        });
        let failed = self.check.failed().iter().map(|(expected, typed)| {
            Label::new(format!("{expected} ← {typed:?}"))
                .size(LabelSize::Small)
                .color(Color::Error)
        });
        v_flex()
            .size_full()
            .p_3()
            .gap_3()
            .bg(colors.editor_background)
            .child(Headline::new("IME check").size(HeadlineSize::Small))
            .child(
                v_flex()
                    .key_context("ImeCheck")
                    .on_action(cx.listener(Self::submit))
                    .gap_2()
                    .child(Label::new("Text box").color(Color::Muted))
                    .child(Label::new(expected).size(LabelSize::Large))
                    .child(
                        div()
                            .w(px(360.))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border_variant)
                            .child(self.input.clone()),
                    )
                    .children(last)
                    .child(Label::new(self.check.progress()).color(Color::Muted))
                    .children(failed),
            )
            .child(Label::new("Terminal").color(Color::Muted))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_terminal_half(cx)),
            )
            .child(
                Label::new(words().join("  "))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }
}
